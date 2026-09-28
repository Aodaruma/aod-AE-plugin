use super::*;
use ae::aegp::{Layer, LayerFlags, TimeMode};

pub(super) fn effect_layer(in_data: InData) -> Result<Layer, Error> {
    Ok(ae::aegp::suites::PFInterface::new()?
        .effect_layer(in_data.effect())?
        .into())
}

pub(super) fn selected(
    params: &Parameters<Params>,
    in_data: InData,
    plugin_id: Option<ae::aegp::PluginId>,
    id: Params,
) -> Result<Option<Layer>, Error> {
    let Some(plugin_id) = plugin_id.filter(|_| !in_data.is_premiere()) else {
        return Ok(None);
    };
    let effect = in_data.effect().aegp_effect(plugin_id)?;
    let stream = effect.new_stream_by_index(
        plugin_id,
        params.index(id).ok_or(Error::InvalidIndex)? as i32,
    )?;
    let value = stream.new_value(
        plugin_id,
        TimeMode::LayerTime,
        ae::Time {
            value: in_data.current_time(),
            scale: in_data.time_scale(),
        },
        false,
    )?;
    let ae::aegp::StreamValue::LayerId(layer_id) = value else {
        return Ok(None);
    };
    if layer_id == ae::sys::AEGP_LayerIDVal_NONE as i32 {
        return Ok(None);
    }
    let comp = effect_layer(in_data)?.parent_comp()?;
    let handle = ae::aegp::CompHandle::from_raw(comp.as_ptr());
    Ok(Some(
        ae::aegp::suites::Layer::new()?
            .layer_from_layer_id(&handle, layer_id as u32)?
            .into(),
    ))
}

fn seconds(t: ae::Time) -> f64 {
    t.value as f64 / t.scale.max(1) as f64
}

#[derive(Clone, Copy)]
pub(super) struct TimeSpan {
    pub first: f64,
    pub end: f64,
    pub frame: f64,
    // PF parameter checkout uses effect-layer time, not parent-comp time.
    comp_zero: f64,
    comp_per_effect: f64,
    scale: u32,
}

impl TimeSpan {
    pub fn checkout_time(self, sample: usize, count: usize) -> i32 {
        let frames = ((self.end - self.first) / self.frame - 1e-6)
            .ceil()
            .max(1.0) as usize;
        let index = if count <= 1 {
            0
        } else {
            sample.min(count - 1) * (frames - 1) / (count - 1)
        };
        let comp_time = (self.first + index as f64 * self.frame).min(self.end - 1e-7);
        ((comp_time - self.comp_zero) / self.comp_per_effect * self.scale as f64).round() as i32
    }
}

pub(super) fn texture_span(
    params: &Parameters<Params>,
    in_data: InData,
    plugin_id: Option<ae::aegp::PluginId>,
) -> Result<Option<TimeSpan>, Error> {
    let Some(layer) = selected(params, in_data, plugin_id, Params::TextureLayer)? else {
        return Ok(None);
    };
    let comp = layer.parent_comp()?;
    let mut first = seconds(layer.in_point(TimeMode::CompTime)?);
    let mut end = first + seconds(layer.duration(TimeMode::CompTime)?);
    if first > end {
        std::mem::swap(&mut first, &mut end);
    }
    let source = layer.source_item()?;
    if !source.as_ptr().is_null() && !layer.flags()?.contains(LayerFlags::TIME_REMAPPING) {
        let duration = source.duration()?;
        if duration.value > 0 {
            let a = seconds(layer.convert_layer_to_comp_time(ae::Time {
                value: 0,
                scale: duration.scale,
            })?);
            let b = seconds(layer.convert_layer_to_comp_time(duration)?);
            first = first.max(a.min(b));
            end = end.min(a.max(b));
            if b < a && end > first {
                // Reversed footage's source end is exclusive at the left
                // boundary. Move inside without skipping a subframe trim.
                let inset = (1.0 / comp.framerate()?.max(1.0)).min((end - b) * 0.5);
                first = first.max(b + inset);
            }
        }
    }
    if end <= first {
        return Ok(None);
    }
    let interface = ae::aegp::suites::PFInterface::new()?;
    let comp_zero = seconds(interface.convert_effect_to_comp_time(
        in_data.effect(),
        0,
        in_data.time_scale(),
    )?);
    let comp_one = seconds(interface.convert_effect_to_comp_time(
        in_data.effect(),
        in_data.time_scale() as i32,
        in_data.time_scale(),
    )?);
    let comp_per_effect = comp_one - comp_zero;
    if comp_per_effect.abs() < 1e-9 {
        return Err(Error::InvalidParms);
    }
    Ok(Some(TimeSpan {
        first,
        end,
        frame: 1.0 / comp.framerate()?.max(1.0),
        comp_zero,
        comp_per_effect,
        scale: in_data.time_scale(),
    }))
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Affine(pub [f32; 6]);
impl Affine {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    pub fn layer(layer: &Layer, time: ae::Time) -> Result<Self, Error> {
        let matrix: ae::sys::A_Matrix4 = layer.to_world_xform(time)?.into();
        let m = matrix.mat;
        Ok(Self([
            m[0][0] as f32,
            m[0][1] as f32,
            m[1][0] as f32,
            m[1][1] as f32,
            m[3][0] as f32,
            m[3][1] as f32,
        ]))
    }
    pub fn point(self, x: f32, y: f32) -> [f32; 2] {
        let [a, b, c, d, tx, ty] = self.0;
        [a * x + c * y + tx, b * x + d * y + ty]
    }
    pub fn inverse(self) -> Option<Self> {
        let [a, b, c, d, x, y] = self.0;
        let det = a * d - b * c;
        (det.abs() > 1e-8).then(|| {
            Self([
                d / det,
                -b / det,
                -c / det,
                a / det,
                (c * y - d * x) / det,
                (b * x - a * y) / det,
            ])
        })
    }
}

pub(super) fn rasterized_in_comp(layer: &Layer) -> Result<bool, Error> {
    Ok(layer.object_type()? == ae::aegp::ObjectType::Vector
        || layer
            .flags()?
            .intersects(LayerFlags::COLLAPSE | LayerFlags::ADJUSTMENT_LAYER))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_random_frames_never_reach_exclusive_end() {
        let span = TimeSpan {
            first: 2.0,
            end: 2.125,
            frame: 1.0 / 24.0,
            comp_zero: 1.0,
            comp_per_effect: 2.0,
            scale: 48000,
        };
        for i in 0..16 {
            let t = span.checkout_time(i, 16) as f64 / 48000.0;
            let comp = 1.0 + 2.0 * t;
            assert!((2.0..2.125).contains(&comp));
        }
        assert_eq!(span.checkout_time(0, 1), 24000);
    }
    #[test]
    fn map_transform_round_trip() {
        let a = Affine([1.2, 0.4, -0.2, 0.7, 100.0, -50.0]);
        let p = a.point(20.0, 30.0);
        let q = a.inverse().unwrap().point(p[0], p[1]);
        assert!((q[0] - 20.0).abs() < 1e-4 && (q[1] - 30.0).abs() < 1e-4);
    }
}
