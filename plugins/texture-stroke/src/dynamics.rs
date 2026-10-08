use super::*;
use std::collections::HashMap;

pub(super) const SIZE: usize = 0;
pub(super) const DENSITY: usize = 1;
pub(super) const ROTATION: usize = 2;
pub(super) const OPACITY: usize = 3;

pub(super) struct Channel {
    pub name: &'static str,
    pub start: Params,
    pub end: Params,
    pub source: Params,
    pub map: Params,
    pub curve: Params,
}
pub(super) const CHANNELS: [Channel; 4] = [
    Channel {
        name: "Size",
        start: Params::SizeDynamicsStart,
        end: Params::SizeDynamicsEnd,
        source: Params::SizeDynamicsSource,
        map: Params::SizeMapLayer,
        curve: Params::SizeCurve,
    },
    Channel {
        name: "Density",
        start: Params::DensityDynamicsStart,
        end: Params::DensityDynamicsEnd,
        source: Params::DensityDynamicsSource,
        map: Params::DensityMapLayer,
        curve: Params::DensityCurve,
    },
    Channel {
        name: "Rotation",
        start: Params::RotationDynamicsStart,
        end: Params::RotationDynamicsEnd,
        source: Params::RotationDynamicsSource,
        map: Params::RotationMapLayer,
        curve: Params::RotationCurve,
    },
    Channel {
        name: "Opacity",
        start: Params::OpacityDynamicsStart,
        end: Params::OpacityDynamicsEnd,
        source: Params::OpacityDynamicsSource,
        map: Params::OpacityMapLayer,
        curve: Params::OpacityCurve,
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Source {
    #[default]
    None,
    Curvature,
    Crowding,
    Length,
    MapLuma,
    MapAlpha,
}
impl Source {
    pub fn popup(v: i32) -> Self {
        match v {
            2 => Self::Curvature,
            3 => Self::Crowding,
            4 => Self::Length,
            5 => Self::MapLuma,
            6 => Self::MapAlpha,
            _ => Self::None,
        }
    }
    pub fn is_map(self) -> bool {
        matches!(self, Self::MapLuma | Self::MapAlpha)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Dynamics {
    pub sources: [Source; 4],
    pub curves: [curves::Curve; 4],
    pub curvature_radius: f32,
    pub crowding_radius: f32,
    pub length_reference: f32,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self {
            sources: [Source::None; 4],
            curves: [curves::Curve::default(); 4],
            curvature_radius: 100.0,
            crowding_radius: 100.0,
            length_reference: 1000.0,
        }
    }
}
impl Dynamics {
    pub fn read(params: &Parameters<Params>) -> Result<Self, Error> {
        let mut result = Self::default();
        for (i, c) in CHANNELS.iter().enumerate() {
            result.sources[i] = Source::popup(params.get(c.source)?.as_popup()?.value());
            result.curves[i] = curves::Curve::read(params, c.curve)?;
        }
        result.curvature_radius = params
            .get(Params::CurvatureRadius)?
            .as_float_slider()?
            .value() as f32;
        result.crowding_radius = params
            .get(Params::CrowdingRadius)?
            .as_float_slider()?
            .value() as f32;
        result.length_reference = params
            .get(Params::LengthReference)?
            .as_float_slider()?
            .value() as f32;
        Ok(result)
    }
}

pub(super) fn setup(params: &mut Parameters<Params>) -> Result<(), Error> {
    params.add_group(
        Params::DynamicsStart,
        Params::DynamicsEnd,
        "Path / Map Dynamics",
        true,
        |params| {
            for c in &CHANNELS {
                params.add_group(c.start, c.end, c.name, true, |params| {
                    params.add_with_flags(
                        c.source,
                        &format!("{} Input", c.name),
                        PopupDef::setup(|d| {
                            d.set_options(&[
                                "None",
                                "Curvature",
                                "Path Crowding",
                                "Path Length",
                                "Map Luminance",
                                "Map Alpha",
                            ]);
                            d.set_default(1);
                        }),
                        ae::ParamFlag::SUPERVISE
                            | ae::ParamFlag::CANNOT_TIME_VARY
                            | ae::ParamFlag::CANNOT_INTERP,
                        ae::ParamUIFlags::empty(),
                    )?;
                    params.add_with_flags(
                        c.map,
                        &format!("{} Map Layer", c.name),
                        LayerDef::new(),
                        ae::ParamFlag::SUPERVISE
                            | ae::ParamFlag::CANNOT_TIME_VARY
                            | ae::ParamFlag::CANNOT_INTERP,
                        ae::ParamUIFlags::empty(),
                    )?;
                    curves::add(params, c.curve, &format!("{} Response", c.name))
                })?;
            }
            for (id, name, default) in [
                (Params::CurvatureRadius, "Curvature Radius (px)", 100.0),
                (Params::CrowdingRadius, "Crowding Radius (px)", 100.0),
                (Params::LengthReference, "Length Reference (px)", 1000.0),
            ] {
                params.add(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(1.0);
                        d.set_valid_max(100000.0);
                        d.set_slider_min(1.0);
                        d.set_slider_max(2000.0);
                        d.set_default(default);
                        d.set_precision(1);
                    }),
                )?;
            }
            Ok(())
        },
    )
}

pub(super) struct Map {
    pixels: Vec<PixelF32>,
    width: usize,
    height: usize,
    from_comp: layers::Affine,
    scale: [f32; 2],
}
impl Map {
    fn sample(&self, comp: [f32; 2], alpha: bool) -> f32 {
        let p = self.from_comp.point(comp[0], comp[1]);
        let x = p[0] * self.scale[0] - 0.5;
        let y = p[1] * self.scale[1] - 0.5;
        let pixel = |x: i32, y: i32| {
            if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
                return 0.0;
            }
            let p = self.pixels[y as usize * self.width + x as usize];
            if alpha {
                p.alpha
            } else {
                (p.red * 0.2126 + p.green * 0.7152 + p.blue * 0.0722).clamp(0.0, 1.0)
            }
        };
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x.floor(), y - y.floor());
        lerp(
            lerp(pixel(ix, iy), pixel(ix + 1, iy), fx),
            lerp(pixel(ix, iy + 1), pixel(ix + 1, iy + 1), fx),
            fy,
        )
        .clamp(0.0, 1.0)
    }
}

pub(super) struct Maps {
    maps: [Option<Map>; 4],
    to_comp: layers::Affine,
}
impl Default for Maps {
    fn default() -> Self {
        Self {
            maps: std::array::from_fn(|_| None),
            to_comp: layers::Affine::IDENTITY,
        }
    }
}
impl Maps {
    pub fn checkout(
        params: &Parameters<Params>,
        in_data: InData,
        plugin_id: Option<ae::aegp::PluginId>,
        d: Dynamics,
    ) -> Result<Self, Error> {
        let mut result = Self::default();
        if !d.sources.iter().any(|s| s.is_map()) || in_data.is_premiere() {
            return Ok(result);
        }
        let time = ae::aegp::suites::PFInterface::new()?.convert_effect_to_comp_time(
            in_data.effect(),
            in_data.current_time(),
            in_data.time_scale(),
        )?;
        let owner = layers::effect_layer(in_data)?;
        if !shapes::is_2d_hierarchy(&owner)? {
            return Ok(result);
        }
        if !layers::rasterized_in_comp(&owner)? {
            result.to_comp = layers::Affine::layer(&owner, time)?;
        }
        for (i, c) in CHANNELS.iter().enumerate() {
            if !d.sources[i].is_map() {
                continue;
            }
            let Some(layer) = layers::selected(params, in_data, plugin_id, c.map)? else {
                continue;
            };
            if !shapes::is_2d_hierarchy(&layer)? {
                continue;
            }
            let from_comp = if layers::rasterized_in_comp(&layer)? {
                layers::Affine::IDENTITY
            } else {
                let Some(inverse) = layers::Affine::layer(&layer, time)?.inverse() else {
                    continue;
                };
                inverse
            };
            let checkout = params.checkout_at(c.map, Some(in_data.current_time()), None, None)?;
            if let Some(buffer) = checkout.as_layer()?.value() {
                result.maps[i] = Some(Map {
                    pixels: read_layer_rgba(&buffer),
                    width: buffer.width(),
                    height: buffer.height(),
                    from_comp,
                    scale: render_scale(in_data),
                });
            }
        }
        Ok(result)
    }
}

struct Segment {
    path: usize,
    x: f32,
    y: f32,
    length: f32,
}
pub(super) struct Inputs<'a> {
    d: Dynamics,
    maps: &'a Maps,
    grid: HashMap<(i32, i32), Vec<Segment>>,
}
impl<'a> Inputs<'a> {
    pub fn new(points: &[PathPoint], d: Dynamics, maps: &'a Maps) -> Self {
        let mut result = Self {
            d,
            maps,
            grid: HashMap::new(),
        };
        if d.sources.contains(&Source::Crowding) {
            let r = d.crowding_radius.max(1.0);
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if a.path_index != b.path_index {
                    continue;
                }
                let x = (a.x + b.x) * 0.5;
                let y = (a.y + b.y) * 0.5;
                result
                    .grid
                    .entry(((x / r).floor() as i32, (y / r).floor() as i32))
                    .or_default()
                    .push(Segment {
                        path: a.path_index,
                        x,
                        y,
                        length: (b.x - a.x).hypot(b.y - a.y),
                    });
            }
        }
        result
    }
    fn crowding(&self, p: PathPoint) -> f32 {
        let r = self.d.crowding_radius.max(1.0);
        let cell = ((p.x / r).floor() as i32, (p.y / r).floor() as i32);
        let mut weight = 0.0;
        for y in cell.1 - 1..=cell.1 + 1 {
            for x in cell.0 - 1..=cell.0 + 1 {
                if let Some(segments) = self.grid.get(&(x, y)) {
                    for s in segments {
                        if s.path != p.path_index {
                            weight +=
                                s.length * (1.0 - (s.x - p.x).hypot(s.y - p.y) / r).max(0.0) / r;
                        }
                    }
                }
            }
        }
        weight / (1.0 + weight)
    }
    pub fn values(&self, p: PathPoint, curvature: f32, length: f32) -> [f32; 4] {
        let mut out = [1.0, 1.0, 0.0, 1.0];
        let comp = self.maps.to_comp.point(p.x, p.y);
        let crowding = if self.d.sources.contains(&Source::Crowding) {
            self.crowding(p)
        } else {
            0.0
        };
        for (i, source) in self.d.sources.iter().enumerate() {
            let x = match source {
                Source::None => continue,
                Source::Curvature => {
                    let k = curvature * self.d.curvature_radius;
                    k / (1.0 + k)
                }
                Source::Crowding => crowding,
                Source::Length => (length / self.d.length_reference.max(1.0)).clamp(0.0, 1.0),
                Source::MapLuma | Source::MapAlpha => {
                    let Some(map) = &self.maps.maps[i] else {
                        continue;
                    };
                    map.sample(comp, *source == Source::MapAlpha)
                }
            };
            let value = self.d.curves[i].evaluate(x);
            out[i] = if i == ROTATION {
                (value - 0.5) * std::f32::consts::TAU
            } else {
                value * 2.0
            };
        }
        out
    }
}

pub(super) fn curvature(path: &[PathPoint], index: usize) -> f32 {
    let a = path[index.saturating_sub(1)];
    let b = path[(index + 1).min(path.len() - 1)];
    let arc = (b.along - a.along).max(1e-5);
    let cross = a.tangent_x * b.tangent_y - a.tangent_y * b.tangent_x;
    let dot = a.tangent_x * b.tangent_x + a.tangent_y * b.tangent_y;
    cross.atan2(dot).abs() / arc
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(id: usize, y: f32, step: usize) -> Vec<PathPoint> {
        (0..=200)
            .step_by(step)
            .map(|x| PathPoint {
                path_index: id,
                x: x as f32,
                y,
                tangent_x: 1.0,
                tangent_y: 0.0,
                along: x as f32,
                stroke_width: 12.0,
            })
            .collect()
    }
    #[test]
    fn crowding_uses_neighbor_length_not_vertex_count() {
        let d = Dynamics {
            sources: [Source::Crowding; 4],
            crowding_radius: 40.0,
            ..Default::default()
        };
        let maps = Maps::default();
        let p = line(0, 0.0, 1)[100];
        let isolated = line(0, 0.0, 1);
        assert_eq!(Inputs::new(&isolated, d, &maps).crowding(p), 0.0);
        let a = [isolated.clone(), line(1, 10.0, 1)].concat();
        let b = [isolated, line(1, 10.0, 4)].concat();
        let va = Inputs::new(&a, d, &maps).crowding(p);
        let vb = Inputs::new(&b, d, &maps).crowding(p);
        assert!(va > 0.3 && (va - vb).abs() < 0.005);
    }
    #[test]
    fn map_reads_absolute_transformed_pixels_and_alpha() {
        let map = Map {
            pixels: vec![premultiply([1.0; 3], 0.25), premultiply([1.0; 3], 1.0)],
            width: 2,
            height: 1,
            from_comp: layers::Affine([1.0, 0.0, 0.0, 1.0, -20.0, -30.0]),
            scale: [1.0; 2],
        };
        assert_eq!(map.sample([20.5, 30.5], true), 0.25);
        assert_eq!(map.sample([21.5, 30.5], false), 1.0);
        assert_eq!(map.sample([0.5, 0.5], true), 0.0);
    }

    #[test]
    fn curvature_measures_turning_per_pixel() {
        let radius = 50.0;
        let points: Vec<_> = (0..101)
            .map(|i| {
                let angle = i as f32 * 0.01;
                PathPoint {
                    path_index: 0,
                    x: radius * angle.cos(),
                    y: radius * angle.sin(),
                    tangent_x: -angle.sin(),
                    tangent_y: angle.cos(),
                    along: radius * angle,
                    stroke_width: 12.0,
                }
            })
            .collect();
        assert!((curvature(&points, 50) - 1.0 / radius).abs() < 1e-5);
        assert_eq!(curvature(&line(0, 0.0, 2), 20), 0.0);
    }
}
