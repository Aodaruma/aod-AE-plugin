use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct TimeSettings {
    pub delta: f32,
    pub origin: Vec2,
    pub axis: IndexAxis,
    pub offset: i32,
    pub edge: SampleEdge,
}

#[derive(Clone, Copy, Debug)]
pub struct Timeline {
    current: i32,
    step: i32,
    duration: i32,
}

impl Timeline {
    pub fn new(in_data: InData) -> Self {
        Self {
            current: in_data.current_time(),
            step: in_data.time_step(),
            duration: in_data.as_ref().total_time,
        }
    }

    pub fn at(self, index: i32, settings: TimeSettings) -> i32 {
        if settings.delta == 0.0 {
            return self.current;
        }
        let value = self.current as f64
            + (index as f64 + settings.offset as f64) * settings.delta as f64 * self.step as f64;
        let duration = self.duration.max(1) as f64;
        let last = (duration - self.step.unsigned_abs().max(1) as f64).max(0.0);
        let value = match settings.edge {
            SampleEdge::Transparent => value,
            SampleEdge::Clamp => value.clamp(0.0, last),
            SampleEdge::Tile => value.rem_euclid(duration),
            SampleEdge::Mirror => {
                if last == 0.0 {
                    0.0
                } else {
                    let phase = value.rem_euclid(2.0 * last);
                    phase.min(2.0 * last - phase)
                }
            }
        };
        value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }
}

#[derive(Clone, Debug)]
pub struct FrameRequest {
    pub time: i32,
    pub id: i32,
    pub rect: ae::Rect,
}

#[derive(Clone, Debug, Default)]
pub struct PreRenderData {
    pub layout: RenderLayout,
    pub frames: Vec<FrameRequest>,
}

pub struct SourceFrame {
    pub pixels: Vec<ae::PixelF32>,
    pub bounds: BufferBounds,
}

impl SourceFrame {
    pub fn read(layer: &Layer, rect: Option<ae::Rect>, fallback: (i64, i64)) -> Self {
        Self {
            pixels: read_layer(layer)
                .into_iter()
                .map(prepare_source_pixel)
                .collect(),
            bounds: BufferBounds::new(
                layer_buffer_origin(layer, rect, fallback),
                layer.width(),
                layer.height(),
            ),
        }
    }
}

// Retain just the most recently used non-current frame. Extend often maps
// dozens of adjacent copies to the same endpoint. Memory stays at two source
// frames, irrespective of Copies or resolution; no cache survives a render.
struct TemporalFrames<'a> {
    current: &'a SourceFrame,
    current_time: i32,
    cached_time: Option<i32>,
    cached: Option<SourceFrame>,
}

impl<'a> TemporalFrames<'a> {
    fn new(current: &'a SourceFrame, current_time: i32) -> Self {
        Self {
            current,
            current_time,
            cached_time: None,
            cached: None,
        }
    }

    fn get(
        &mut self,
        time: i32,
        checkout: &mut dyn FnMut(i32) -> Result<Option<SourceFrame>, Error>,
    ) -> Result<Option<&SourceFrame>, Error> {
        if time == self.current_time {
            return Ok(Some(self.current));
        }
        if self.cached_time != Some(time) {
            self.cached = None;
            self.cached = checkout(time)?;
            self.cached_time = Some(time);
        }
        Ok(self.cached.as_ref())
    }
}

fn frame_requests(times: impl Iterator<Item = i32>, current: i32) -> Vec<i32> {
    let mut previous = None;
    times
        .filter(|time| {
            if *time == current || previous == Some(*time) {
                return false;
            }
            previous = Some(*time);
            true
        })
        .collect()
}

fn pattern_index(settings: &RenderSettings, point: Vec2, strip_origin: i32) -> i32 {
    match settings.mode {
        Mode::Strip => settings
            .strip
            .index(point, settings.center)
            .saturating_sub(strip_origin),
        Mode::Tile => settings.tile.index(
            point,
            settings.center,
            settings.time.origin,
            settings.time.axis,
        ),
        Mode::Polar => {
            let sector_ring = |p: Vec2| {
                let p = p - settings.center;
                let radius = p.x.hypot(p.y);
                let width = std::f32::consts::TAU / settings.polar.sectors as f32;
                let angle = p.y.atan2(p.x)
                    + settings.polar.twist * radius / settings.space.max_radius().max(1.0);
                let sector = ((angle + width * 0.5) / width).floor() as i32;
                let ring = if settings.polar.rings > 0 {
                    (radius * settings.polar.rings as f32 / settings.space.max_radius().max(1.0))
                        .floor() as i32
                } else {
                    0
                };
                (sector, ring)
            };
            let (sector, ring) = sector_ring(point);
            let (zero_sector, zero_ring) = sector_ring(settings.time.origin);
            let sectors = settings.polar.sectors as i32;
            (sector - zero_sector)
                .rem_euclid(sectors)
                .saturating_add(ring.saturating_sub(zero_ring).saturating_mul(sectors))
        }
        Mode::Feedback => 0,
    }
}

fn output_point(
    settings: &RenderSettings,
    source: BufferBounds,
    output: BufferBounds,
    x: usize,
    y: usize,
) -> Vec2 {
    let (x, y) = output.layer_coordinate(x, y);
    settings.space.buffer_point_to_pattern(Vec2::new(
        (x - source.origin_x) as f32,
        (y - source.origin_y) as f32,
    ))
}

pub fn required_times(
    in_data: InData,
    settings: &RenderSettings,
    source: BufferBounds,
    output: BufferBounds,
) -> Result<Vec<i32>, Error> {
    if settings.time.delta == 0.0 || settings.mix == 0.0 {
        return Ok(Vec::new());
    }
    let timeline = Timeline::new(in_data);
    let mut times = BTreeSet::new();
    if settings.mode == Mode::Feedback {
        return Ok(frame_requests(
            (0..settings.feedback_iterations).map(|stack| {
                timeline.at(
                    feedback_iteration(
                        stack,
                        settings.feedback_iterations,
                        settings.feedback_composite_order,
                    ) as i32,
                    settings.time,
                )
            }),
            timeline.current,
        ));
    } else if settings.mode == Mode::Strip && settings.strip_clipping != StripClipping::Both {
        let origin = settings.strip.index(settings.time.origin, settings.center);
        return Ok(frame_requests(
            settings
                .strip
                .copy_indices()
                .map(|index| timeline.at(index.saturating_sub(origin), settings.time)),
            timeline.current,
        ));
    } else {
        let origin = settings.strip.index(settings.time.origin, settings.center);
        for y in 0..output.height {
            in_data.interact().abort()?;
            for x in 0..output.width {
                let point = output_point(settings, source, output, x, y);
                times.insert(timeline.at(pattern_index(settings, point, origin), settings.time));
            }
        }
    }
    times.remove(&timeline.current);
    Ok(times.into_iter().collect())
}

#[derive(Clone, Copy)]
struct Location {
    output: usize,
    coordinate: Vec2,
    direction: Option<Vec2>,
}

fn location(
    settings: &RenderSettings,
    point: Vec2,
    output: usize,
    strip_origin: i32,
) -> Option<(Location, i32)> {
    let (coordinate, direction, index) = match settings.mode {
        Mode::Strip => {
            let sample = settings.strip.sample(point, settings.center)?;
            (
                sample.coordinate,
                settings.strip_edge_along.then_some(sample.direction),
                sample.index.saturating_sub(strip_origin),
            )
        }
        Mode::Tile => (
            settings.tile.coordinate(point, settings.center),
            None,
            pattern_index(settings, point, strip_origin),
        ),
        Mode::Polar => (
            polar_coordinate(
                point,
                settings.center,
                settings.polar,
                settings.space.max_radius(),
            ),
            None,
            pattern_index(settings, point, strip_origin),
        ),
        Mode::Feedback => unreachable!(),
    };
    Some((
        Location {
            output,
            coordinate,
            direction,
        },
        index,
    ))
}

fn sample_frame(
    frame: Option<&SourceFrame>,
    reference: BufferBounds,
    settings: &RenderSettings,
    coordinate: Vec2,
    direction: Option<Vec2>,
) -> ae::PixelF32 {
    let Some(frame) = frame else {
        return TRANSPARENT;
    };
    let buffer = settings.space.pattern_point_to_buffer(coordinate)
        + Vec2::new(
            (reference.origin_x - frame.bounds.origin_x) as f32,
            (reference.origin_y - frame.bounds.origin_y) as f32,
        );
    let direction = direction.map(|p| {
        Vec2::new(
            p.x * settings.space.pattern_to_buffer.x,
            p.y * settings.space.pattern_to_buffer.y,
        )
    });
    sample_buffer(
        &frame.pixels,
        frame.bounds.width,
        frame.bounds.height,
        buffer,
        direction,
        settings.sampling,
        settings.edge,
    )
}

pub fn render_temporal(
    in_data: InData,
    current: &SourceFrame,
    output: BufferBounds,
    settings: &RenderSettings,
    checkout: &mut dyn FnMut(i32) -> Result<Option<SourceFrame>, Error>,
) -> Result<Vec<ae::PixelF32>, Error> {
    let timeline = Timeline::new(in_data);
    let mut pixels = vec![TRANSPARENT; output.width * output.height];
    let mut frames = TemporalFrames::new(current, timeline.current);
    if settings.mode == Mode::Feedback {
        // Process taps in stack order while reusing held endpoint frames.
        for stack in 0..settings.feedback_iterations {
            in_data.interact().abort()?;
            let iteration = feedback_iteration(
                stack,
                settings.feedback_iterations,
                settings.feedback_composite_order,
            );
            let time = timeline.at(iteration as i32, settings.time);
            let frame = frames.get(time, checkout)?;
            let weight = feedback_tap_weight(
                iteration,
                settings.feedback_decay,
                settings.feedback_opacity,
            );
            for y in 0..output.height {
                in_data.interact().abort()?;
                for x in 0..output.width {
                    let point = output_point(settings, current.bounds, output, x, y);
                    let coordinate = feedback_coordinate(
                        point,
                        settings.center,
                        iteration,
                        settings.feedback,
                        settings.space.max_radius(),
                    );
                    let sample = sample_frame(frame, current.bounds, settings, coordinate, None);
                    let pixel = &mut pixels[y * output.width + x];
                    *pixel = composite_feedback_sample(*pixel, sample, weight);
                }
            }
        }
    } else if settings.mode == Mode::Strip && settings.strip_clipping != StripClipping::Both {
        let origin = settings.strip.index(settings.time.origin, settings.center);
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for y in [0, output.height.saturating_sub(1)] {
            for x in [0, output.width.saturating_sub(1)] {
                let point = output_point(settings, current.bounds, output, x, y);
                let x = settings.strip.grid_point(point, settings.center).x;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
        let lookup = settings.strip.prepare_copies(min_x, max_x);
        for index in settings.strip.copy_indices() {
            in_data.interact().abort()?;
            let time = timeline.at(index.saturating_sub(origin), settings.time);
            let frame = frames.get(time, checkout)?;
            for y in 0..output.height {
                in_data.interact().abort()?;
                for x in 0..output.width {
                    let point = output_point(settings, current.bounds, output, x, y);
                    if let Some(sample) = settings.strip.sample_copy_prepared(
                        point,
                        settings.center,
                        index,
                        settings.strip_clipping,
                        Some(&lookup),
                    ) {
                        let color = sample_frame(
                            frame,
                            current.bounds,
                            settings,
                            sample.coordinate,
                            settings.strip_edge_along.then_some(sample.direction),
                        );
                        let pixel = &mut pixels[y * output.width + x];
                        *pixel =
                            composite_strip_sample(*pixel, color, settings.strip_overlap_order);
                    }
                }
            }
        }
    } else {
        let mut groups: BTreeMap<i32, Vec<Location>> = BTreeMap::new();
        let origin = settings.strip.index(settings.time.origin, settings.center);
        for y in 0..output.height {
            in_data.interact().abort()?;
            for x in 0..output.width {
                let point = output_point(settings, current.bounds, output, x, y);
                if let Some((location, index)) =
                    location(settings, point, y * output.width + x, origin)
                {
                    groups
                        .entry(timeline.at(index, settings.time))
                        .or_default()
                        .push(location);
                }
            }
        }
        for (time, locations) in groups {
            in_data.interact().abort()?;
            let frame = frames.get(time, checkout)?;
            for location in locations {
                pixels[location.output] = sample_frame(
                    frame,
                    current.bounds,
                    settings,
                    location.coordinate,
                    location.direction,
                );
            }
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_frames_and_prerender_requests_agree_including_transparent_frames() {
        let current = SourceFrame {
            pixels: vec![TRANSPARENT],
            bounds: BufferBounds::new((0, 0), 1, 1),
        };
        let times = [0, 0, 5, 0, 10, 10, 5, 0, 0, -1, -1, 5, -1, 12, -1];
        let expected = frame_requests(times.into_iter(), 5);
        assert_eq!(expected, [0, 10, 0, -1, 12, -1]);
        let mut frames = TemporalFrames::new(&current, 5);
        let mut checked = Vec::new();
        for time in times {
            let frame = frames
                .get(time, &mut |time| {
                    checked.push(time);
                    Ok((time >= 0).then(|| SourceFrame {
                        pixels: vec![ae::PixelF32 {
                            red: time as f32,
                            ..TRANSPARENT
                        }],
                        bounds: current.bounds,
                    }))
                })
                .unwrap();
            assert_eq!(frame.is_some(), time >= 0);
            if let Some(frame) = frame {
                assert_eq!(
                    frame.pixels[0].red,
                    if time == 5 { 0.0 } else { time as f32 }
                );
            }
        }
        assert_eq!(checked, expected);
    }

    #[test]
    fn temporal_offsets_preserve_zero_and_handle_negative_fractional_and_reversed_time() {
        let mut settings = TimeSettings {
            delta: 1.5,
            origin: Vec2::default(),
            axis: IndexAxis::X,
            offset: 0,
            edge: SampleEdge::Clamp,
        };
        let timeline = Timeline {
            current: 120,
            step: 10,
            duration: 300,
        };
        assert_eq!(timeline.at(0, settings), 120);
        assert_eq!(timeline.at(3, settings), 165);
        assert_eq!(timeline.at(-3, settings), 75);
        assert_eq!(timeline.at(100, settings), 290);
        assert_eq!(
            Timeline {
                step: -10,
                ..timeline
            }
            .at(3, settings),
            75
        );
        settings.offset = 2;
        assert_eq!(timeline.at(-2, settings), 120);
        settings.delta = 0.0;
        assert_eq!(timeline.at(1000, settings), 120);
    }
    #[test]
    fn temporal_edges_wrap_in_both_directions_and_handle_still_frames() {
        let mut settings = TimeSettings {
            delta: 1.0,
            origin: Vec2::default(),
            axis: IndexAxis::Distance,
            offset: 0,
            edge: SampleEdge::Tile,
        };
        let t = Timeline {
            current: 0,
            step: 10,
            duration: 100,
        };
        assert_eq!(t.at(-1, settings), 90);
        assert_eq!(t.at(12, settings), 20);
        settings.edge = SampleEdge::Mirror;
        assert_eq!(t.at(-2, settings), 20);
        assert_eq!(t.at(11, settings), 70);
        settings.edge = SampleEdge::Transparent;
        assert_eq!(t.at(-2, settings), -20);
        assert_eq!(Timeline { step: 0, ..t }.at(123, settings), 0);
    }
}
