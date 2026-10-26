//! Arc-length coordinates in full-resolution square pixels.
use super::*;

pub(super) const PARAMS: [Params; 10] = [
    Params::CurveStart,
    Params::CurvePath,
    Params::CurveLoop,
    Params::CurveFit,
    Params::CurveWidth,
    Params::CurveOffset,
    Params::CurveNormalOffset,
    Params::CurveReverse,
    Params::CurveFlipNormal,
    Params::CurveQuality,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Fit {
    Fill,
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CurveSettings {
    pub loop_path: bool,
    pub fit: Fit,
    pub width: f32,
    pub offset: f32,
    pub normal_offset: f32,
    pub reverse: bool,
    pub flip_normal: bool,
    pub sample_step: f32,
}

impl Default for CurveSettings {
    fn default() -> Self {
        Self {
            loop_path: true,
            fit: Fit::Fill,
            width: 200.0,
            offset: 0.0,
            normal_offset: 0.0,
            reverse: false,
            flip_normal: false,
            sample_step: 0.5,
        }
    }
}

pub(super) fn params_setup(params: &mut Parameters<Params>) -> Result<(), Error> {
    params.add_group(
        Params::CurveStart,
        Params::CurveEnd,
        "CurveAlonged",
        false,
        |params| {
            params.add(
                Params::CurvePath,
                "Mask Path",
                PathDef::setup(|d| {
                    d.set_default(1);
                }),
            )?;
            params.add(
                Params::CurveLoop,
                "Loop Closed Path",
                CheckBoxDef::setup(|d| {
                    d.set_default(true);
                }),
            )?;
            params.add(
                Params::CurveFit,
                "Fit",
                PopupDef::setup(|d| {
                    d.set_options(&["Fill", "Contain", "Cover"]);
                    d.set_default(1);
                }),
            )?;
            for (id, name, min, max, slider_min, slider_max, default) in [
                (
                    Params::CurveWidth,
                    "Band Width",
                    0.01,
                    100000.0,
                    1.0,
                    2000.0,
                    200.0,
                ),
                (
                    Params::CurveOffset,
                    "Along Offset (%)",
                    -10000.0,
                    10000.0,
                    -100.0,
                    100.0,
                    0.0,
                ),
                (
                    Params::CurveNormalOffset,
                    "Normal Offset",
                    -100000.0,
                    100000.0,
                    -1000.0,
                    1000.0,
                    0.0,
                ),
            ] {
                params.add(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(min);
                        d.set_valid_max(max);
                        d.set_slider_min(slider_min);
                        d.set_slider_max(slider_max);
                        d.set_default(default);
                        d.set_precision(2);
                    }),
                )?;
            }
            params.add(
                Params::CurveReverse,
                "Reverse Along",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
            )?;
            params.add(
                Params::CurveFlipNormal,
                "Flip Normal",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
            )?;
            params.add(
                Params::CurveQuality,
                "Path Quality",
                PopupDef::setup(|d| {
                    d.set_options(&["Draft", "Normal", "High"]);
                    d.set_default(2);
                }),
            )?;
            Ok(())
        },
    )?;
    Ok(())
}

pub(super) fn read_settings(params: &Parameters<Params>) -> Result<CurveSettings, Error> {
    let slider = |id, fallback| -> Result<f32, Error> {
        Ok(finite_or(
            params.get(id)?.as_float_slider()?.value() as f32,
            fallback,
        ))
    };
    Ok(CurveSettings {
        loop_path: params.get(Params::CurveLoop)?.as_checkbox()?.value(),
        fit: match params.get(Params::CurveFit)?.as_popup()?.value() {
            2 => Fit::Contain,
            3 => Fit::Cover,
            _ => Fit::Fill,
        },
        width: slider(Params::CurveWidth, 200.0)?.clamp(0.01, 100000.0),
        offset: slider(Params::CurveOffset, 0.0)?.clamp(-10000.0, 10000.0) * 0.01,
        normal_offset: slider(Params::CurveNormalOffset, 0.0)?.clamp(-100000.0, 100000.0),
        reverse: params.get(Params::CurveReverse)?.as_checkbox()?.value(),
        flip_normal: params.get(Params::CurveFlipNormal)?.as_checkbox()?.value(),
        sample_step: match params.get(Params::CurveQuality)?.as_popup()?.value() {
            1 => 2.0,
            3 => 0.25,
            _ => 0.5,
        },
    })
}

type Point = (f32, f32);
fn sub(a: Point, b: Point) -> Point {
    (a.0 - b.0, a.1 - b.1)
}
fn dot(a: Point, b: Point) -> f32 {
    a.0 * b.0 + a.1 * b.1
}
fn lerp(a: Point, b: Point, t: f32) -> Point {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}
fn normalized(v: Point) -> Point {
    let length = v.0.hypot(v.1);
    if length > 1.0e-6 {
        (v.0 / length, v.1 / length)
    } else {
        (0.0, 0.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    a: Point,
    b: Point,
    ta: Point,
    tb: Point,
    start: f32,
    length: f32,
}
impl Segment {
    fn frame(self, t: f32) -> (Point, Point) {
        let tangent = normalized(lerp(self.ta, self.tb, t));
        let tangent = if tangent == (0.0, 0.0) {
            normalized(sub(self.b, self.a))
        } else {
            tangent
        };
        (lerp(self.a, self.b, t), (-tangent.1, tangent.0))
    }
    fn closest(self, p: Point) -> (f32, f32) {
        let delta = sub(self.b, self.a);
        let t = (dot(sub(p, self.a), delta) / dot(delta, delta)).clamp(0.0, 1.0);
        (
            dot(
                sub(p, lerp(self.a, self.b, t)),
                sub(p, lerp(self.a, self.b, t)),
            ),
            t,
        )
    }
}

#[derive(Debug)]
struct Node {
    min: Point,
    max: Point,
    range: std::ops::Range<usize>,
    children: Option<(usize, usize)>,
}
impl Node {
    fn distance(&self, p: Point) -> f32 {
        let dx = (self.min.0 - p.0).max(0.0).max(p.0 - self.max.0);
        let dy = (self.min.1 - p.1).max(0.0).max(p.1 - self.max.1);
        dx * dx + dy * dy
    }
}

pub(super) struct CurvePath {
    segments: Vec<Segment>,
    indices: Vec<usize>,
    nodes: Vec<Node>,
    length: f32,
    closed: bool,
}

impl CurvePath {
    fn new(samples: &[(Point, Point)], closed: bool) -> Option<Self> {
        let mut segments = Vec::new();
        let mut length = 0.0;
        for pair in samples.windows(2) {
            let ((a, ta), (b, tb)) = (pair[0], pair[1]);
            if ![a.0, a.1, b.0, b.1, ta.0, ta.1, tb.0, tb.1]
                .iter()
                .all(|v| v.is_finite())
            {
                return None;
            }
            let delta = sub(b, a);
            let len = delta.0.hypot(delta.1);
            if len <= 1.0e-6 {
                continue;
            }
            let fallback = normalized(delta);
            segments.push(Segment {
                a,
                b,
                ta: if ta == (0.0, 0.0) {
                    fallback
                } else {
                    normalized(ta)
                },
                tb: if tb == (0.0, 0.0) {
                    fallback
                } else {
                    normalized(tb)
                },
                start: length,
                length: len,
            });
            length += len;
        }
        if segments.is_empty() || !length.is_finite() {
            return None;
        }
        let indices = (0..segments.len()).collect();
        let mut path = Self {
            segments,
            indices,
            nodes: Vec::new(),
            length,
            closed,
        };
        path.build_node(0..path.segments.len());
        Some(path)
    }

    fn build_node(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut min = (f32::INFINITY, f32::INFINITY);
        let mut max = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &i in &self.indices[range.clone()] {
            for p in [self.segments[i].a, self.segments[i].b] {
                min = (min.0.min(p.0), min.1.min(p.1));
                max = (max.0.max(p.0), max.1.max(p.1));
            }
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let horizontal = max.0 - min.0 >= max.1 - min.1;
            let mid = range.len() / 2;
            self.indices[range.clone()].select_nth_unstable_by(mid, |&a, &b| {
                let a = self.segments[a];
                let b = self.segments[b];
                let ac = if horizontal {
                    a.a.0 + a.b.0
                } else {
                    a.a.1 + a.b.1
                };
                let bc = if horizontal {
                    b.a.0 + b.b.0
                } else {
                    b.a.1 + b.b.1
                };
                ac.total_cmp(&bc)
            });
            let left = self.build_node(range.start..range.start + mid);
            let right = self.build_node(range.start + mid..range.end);
            self.nodes[index].children = Some((left, right));
        }
        index
    }

    fn nearest(&self, node: usize, p: Point, best: &mut (f32, usize, f32)) {
        let node = &self.nodes[node];
        if node.distance(p) > best.0 {
            return;
        }
        if let Some((a, b)) = node.children {
            let (a, b) = if self.nodes[a].distance(p) <= self.nodes[b].distance(p) {
                (a, b)
            } else {
                (b, a)
            };
            self.nearest(a, p, best);
            self.nearest(b, p, best);
        } else {
            for &i in &self.indices[node.range.clone()] {
                let (distance, t) = self.segments[i].closest(p);
                if distance < best.0 || (distance == best.0 && i < best.1) {
                    *best = (distance, i, t);
                }
            }
        }
    }

    fn frame(&self, along: f32) -> (Point, Point) {
        let i = self
            .segments
            .partition_point(|s| s.start + s.length < along)
            .min(self.segments.len() - 1);
        let s = self.segments[i];
        s.frame(((along - s.start) / s.length).clamp(0.0, 1.0))
    }

    fn project(&self, p: Point) -> Option<Point> {
        let mut best = (f32::INFINITY, usize::MAX, 0.0);
        self.nearest(0, p, &mut best);
        let segment = self.segments[best.1];
        let (center, normal) = segment.frame(best.2);
        let delta = sub(p, center);
        // Open paths have butt caps; do not smear the endpoint across the canvas.
        if !self.closed
            && ((best.1 == 0 && best.2 <= 0.0 && dot(delta, segment.ta) < -1.0e-4)
                || (best.1 == self.segments.len() - 1
                    && best.2 >= 1.0
                    && dot(delta, segment.tb) > 1.0e-4))
        {
            return None;
        }
        Some((segment.start + best.2 * segment.length, dot(delta, normal)))
    }
}

pub(super) fn checkout_path(
    in_data: InData,
    params: &Parameters<Params>,
    settings: Settings,
) -> Result<Option<CurvePath>, Error> {
    let id = params.get(Params::CurvePath)?.as_path()?.path_id();
    if id == 0 || in_data.is_premiere() {
        return Ok(None);
    }
    let Some(path) = in_data.effect().checkout_path(
        id,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?
    else {
        return Ok(None);
    };
    let count = path.num_segments()?.max(0) as usize;
    if count == 0 {
        return Ok(None);
    }
    // Bound preparation memory/work even for unusually dense masks.
    if count > 32768 {
        return Ok(None);
    }
    let mut prepared = Vec::with_capacity(count);
    let mut total = 0.0;
    for i in 0..count {
        let mut prep = path.prepare_seg_length(i as i32, 100)?;
        let length = prep.length()?;
        if !length.is_finite() || length < 0.0 {
            return Ok(None);
        }
        total += length;
        prepared.push((prep, length));
    }
    let scale = settings.pixel_scale;
    let step = (settings.curve.sample_step / scale.0.max(scale.1))
        .max((total / 32768.0) as f32)
        .max(1.0e-6);
    let mut samples = Vec::new();
    for (mut prep, length) in prepared {
        let steps = (length / step as f64).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let (x, y, dx, dy) = prep.eval_deriv1(length * i as f64 / steps as f64)?;
            // PF path coordinates already include host downsampling.
            samples.push((
                (x as f32 * scale.0, y as f32 * scale.1),
                (dx as f32 * scale.0, dy as f32 * scale.1),
            ));
        }
    }
    Ok(CurvePath::new(&samples, !path.is_open()?))
}

struct Mapping<'a> {
    path: &'a CurvePath,
    settings: CurveSettings,
    canvas: Point,
    scale: Point,
}
impl<'a> Mapping<'a> {
    fn new(path: &'a CurvePath, settings: CurveSettings, canvas: Point) -> Self {
        let sx = path.length / canvas.0.max(1.0e-6);
        let sy = settings.width / canvas.1.max(1.0e-6);
        let scale = match settings.fit {
            Fit::Fill => (sx, sy),
            Fit::Contain => (sx.max(sy), sx.max(sy)),
            Fit::Cover => (sx.min(sy), sx.min(sy)),
        };
        Self {
            path,
            settings,
            canvas,
            scale,
        }
    }
    fn wraps(&self) -> bool {
        self.path.closed && self.settings.loop_path
    }
    fn to_curve(&self, p: Point) -> Option<Point> {
        let s = (p.0 - self.canvas.0 * 0.5) * self.scale.0 + self.path.length * 0.5;
        let d = (p.1 - self.canvas.1 * 0.5) * self.scale.1;
        if s < -1.0e-4
            || s > self.path.length + 1.0e-4
            || d.abs() > self.settings.width * 0.5 + 1.0e-4
        {
            return None;
        }
        let along = if self.settings.reverse {
            self.path.length - s
        } else {
            s
        } + self.settings.offset * self.path.length;
        let along = if self.wraps() {
            along.rem_euclid(self.path.length)
        } else if along < 0.0 || along > self.path.length {
            return None;
        } else {
            along
        };
        let d = self.settings.normal_offset + if self.settings.flip_normal { -d } else { d };
        let (p, n) = self.path.frame(along);
        Some((p.0 + n.0 * d, p.1 + n.1 * d))
    }
    fn to_rect(&self, p: Point) -> Option<Point> {
        let (along, normal) = self.path.project(p)?;
        let s = along - self.settings.offset * self.path.length;
        let s = if self.wraps() {
            s.rem_euclid(self.path.length)
        } else {
            s
        };
        let s = if self.settings.reverse {
            self.path.length - s
        } else {
            s
        };
        let d = normal - self.settings.normal_offset;
        let d = if self.settings.flip_normal { -d } else { d };
        if s < -1.0e-4
            || s > self.path.length + 1.0e-4
            || d.abs() > self.settings.width * 0.5 + 1.0e-4
        {
            return None;
        }
        let p = (
            (s - self.path.length * 0.5) / self.scale.0 + self.canvas.0 * 0.5,
            d / self.scale.1 + self.canvas.1 * 0.5,
        );
        if p.0 < -1.0e-4
            || p.0 > self.canvas.0 + 1.0e-4
            || p.1 < -1.0e-4
            || p.1 > self.canvas.1 + 1.0e-4
        {
            None
        } else {
            Some(p)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    source: &[PixelF32],
    src_size: (usize, usize),
    out_size: (usize, usize),
    src_origin: ae::Point,
    out_origin: ae::Point,
    layer_size: Point,
    path: Option<&CurvePath>,
    settings: Settings,
) -> Vec<PixelF32> {
    let mut output = vec![TRANSPARENT; out_size.0 * out_size.1];
    let Some(path) = path else {
        return output;
    };
    let identity = settings.post_transform.is_identity();
    let inverse = settings.post_transform.matrix().inverse();
    if !identity && inverse.is_none() {
        return output;
    }
    let map = Mapping::new(path, settings.curve, layer_size);
    for y in 0..out_size.1 {
        for x in 0..out_size.0 {
            let p = if identity {
                (x as f32, y as f32)
            } else {
                inverse_post_transform_point(
                    (x as f32, y as f32),
                    settings.post_transform,
                    settings.pixel_scale,
                    inverse.unwrap(),
                )
            };
            let p = (
                (p.0 + out_origin.h as f32) * settings.pixel_scale.0,
                (p.1 + out_origin.v as f32) * settings.pixel_scale.1,
            );
            let coordinate = match settings.direction {
                Direction::Forward => map.to_curve(p),
                Direction::Inverse => map.to_rect(p),
            };
            output[y * out_size.0 + x] = coordinate
                .map(|p| {
                    finish_pixel(
                        sample(
                            source,
                            src_size.0,
                            src_size.1,
                            p.0 / settings.pixel_scale.0 - src_origin.h as f32,
                            p.1 / settings.pixel_scale.1 - src_origin.v as f32,
                            settings.interpolation,
                            settings.edge,
                        ),
                        settings,
                    )
                })
                .unwrap_or(TRANSPARENT);
        }
    }
    output
}

#[cfg(test)]
mod tests;
