// These warnings originate in the upstream effect entry-point macro.
#![allow(clippy::drop_non_drop, clippy::question_mark)]
use ae::pf::*;
use after_effects as ae;
use utils::ToPixel;
use utils::image::{TRANSPARENT, read_pixel};
mod projection;
use projection::{Geometry, Projection, Rotation, sample};

const PLUGIN_DESCRIPTION: &str =
    "Converts panoramic projections while resizing only the effect output bounds.";
const FORMATS: &[&str] = &[
    "Equirectangular",
    "Cubemap 3x2",
    "Cubemap 6x1",
    "Cubemap Cross 4x3",
    "Fisheye 180",
];
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Input,
    Output,
    Width,
    Pan,
    Tilt,
    Roll,
    Interpolation,
    Samples,
    // IDs 8..=10 belonged to removed axis controls. Use explicit IDs >= 11
    // for future parameters so old projects cannot bind them to new controls.
}
#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);
#[derive(Clone, Copy)]
struct RenderData {
    geometry: Geometry,
    input: Projection,
    output: Projection,
    rotation: Rotation,
    bilinear: bool,
    samples: u32,
}
impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        for (id, name) in [(Params::Input, "Input"), (Params::Output, "Output")] {
            params.add(
                id,
                name,
                PopupDef::setup(|d| {
                    d.set_options(FORMATS);
                    d.set_default(1);
                }),
            )?;
        }
        params.add(
            Params::Width,
            "Output Width (0 = Input)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(30000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(8192.0);
                d.set_default(0.0);
                d.set_precision(0);
            }),
        )?;
        for (id, name) in [
            (Params::Pan, "Pan (Y)"),
            (Params::Tilt, "Tilt (X)"),
            (Params::Roll, "Roll (Z)"),
        ] {
            params.add(
                id,
                name,
                AngleDef::setup(|d| {
                    d.set_default(0.0);
                }),
            )?;
        }
        params.add(
            Params::Interpolation,
            "Interpolation",
            PopupDef::setup(|d| {
                d.set_options(&["Nearest", "Bilinear"]);
                d.set_default(2);
            }),
        )?;
        params.add(
            Params::Samples,
            "Antialiasing",
            PopupDef::setup(|d| {
                d.set_options(&["1 sample", "4 samples", "16 samples"]);
                d.set_default(1);
            }),
        )?;
        Ok(())
    }
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => out_data.set_return_msg(&format!(
                "AOD_PanoramaConvert {}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{} Aodaruma",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_YEAR")
            )),
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::IExpandBuffer, true);
                out_data.set_out_flag(OutFlags::PixIndependent, false);
                out_data.set_out_flag(OutFlags::UseOutputExtent, false);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
            }
            ae::Command::FrameSetup { .. } => {
                let data = settings(params, in_data, &mut out_data)?;
                let b = data.geometry.bounds;
                out_data.set_width((b[2] - b[0]) as u32);
                out_data.set_height((b[3] - b[1]) as u32);
                let source = in_data.pre_effect_source_origin();
                out_data.set_origin(ae::Point {
                    h: -source.h - b[0],
                    v: -source.v - b[1],
                });
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let data = settings(params, in_data, &mut out_data)?;
                render(in_data, Some(in_layer), out_layer, data, false)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let data = settings(params, in_data, &mut out_data)?;
                let mut request = extra.output_request();
                let requested: ae::Rect = request.rect.into();
                // Projection and spherical rotation may read any input pixel.
                request.rect =
                    rect([0, 0, data.geometry.source[0], data.geometry.source[1]]).into();
                request.preserve_rgb_of_zero_alpha = 1;
                extra.callbacks().checkout_layer(
                    0,
                    0,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let b = data.geometry.bounds;
                let intersection = [
                    b[0].max(requested.left),
                    b[1].max(requested.top),
                    b[2].min(requested.right),
                    b[3].min(requested.bottom),
                ];
                extra.set_max_result_rect(rect(b));
                extra.set_result_rect(rect(
                    if intersection[0] >= intersection[2] || intersection[1] >= intersection[3] {
                        [0; 4]
                    } else {
                        intersection
                    },
                ));
                extra.set_pre_render_data(data);
            }
            ae::Command::SmartRender { extra } => {
                let data = *extra
                    .pre_render_data::<RenderData>()
                    .ok_or(Error::BadCallbackParameter)?;
                let cb = extra.callbacks();
                let input = cb.checkout_layer_pixels(0)?;
                let result = (|| {
                    if let Some(output) = cb.checkout_output()? {
                        render(in_data, input, output, data, true)?;
                    }
                    Ok::<(), Error>(())
                })();
                let checkin = cb.checkin_layer_pixels(0);
                result?;
                checkin?;
            }
            _ => {}
        }
        Ok(())
    }
}
fn rect(b: [i32; 4]) -> ae::Rect {
    ae::Rect {
        left: b[0],
        top: b[1],
        right: b[2],
        bottom: b[3],
    }
}
fn ratio(v: ae::RationalScale) -> f64 {
    if v.num > 0 && v.den > 0 {
        f64::from(v.num) / f64::from(v.den)
    } else {
        1.0
    }
}
fn settings(
    params: &Parameters<Params>,
    input: InData,
    out: &mut OutData,
) -> Result<RenderData, Error> {
    let src = Projection::from_index(params.get(Params::Input)?.as_popup()?.value());
    let dst = Projection::from_index(params.get(Params::Output)?.as_popup()?.value());
    let size = [input.width(), input.height()];
    if (ratio(input.pixel_aspect_ratio()) - 1.0).abs() > 1e-6 || !src.valid_size(size) {
        out.set_error_msg("AOD_PanoramaConvert: use square pixels. Cubemap input needs square, equally sized faces; Fisheye input must be square.");
        return Err(Error::BadCallbackParameter);
    }
    let geometry = Geometry::new(size, [ratio(input.downsample_x()), ratio(input.downsample_y())], dst, params.get(Params::Width)?.as_float_slider()?.value()).ok_or_else(|| {
        out.set_error_msg("AOD_PanoramaConvert: output dimensions must be between 1 and 30000 pixels. Reduce Output Width.");
        Error::BadCallbackParameter
    })?;
    let mut angles = [0.0; 3];
    for (i, id) in [Params::Tilt, Params::Pan, Params::Roll]
        .into_iter()
        .enumerate()
    {
        angles[i] = params.get(id)?.as_angle()?.float_value()?;
        if !angles[i].is_finite() {
            return Err(Error::BadCallbackParameter);
        }
    }
    Ok(RenderData {
        geometry,
        input: src,
        output: dst,
        rotation: Rotation::new(angles),
        bilinear: params.get(Params::Interpolation)?.as_popup()?.value() != 1,
        samples: match params.get(Params::Samples)?.as_popup()?.value() {
            2 => 2,
            3 => 4,
            _ => 1,
        },
    })
}
fn render(
    input_data: InData,
    input: Option<Layer>,
    mut output: Layer,
    data: RenderData,
    smart: bool,
) -> Result<(), Error> {
    let pre = input_data.pre_effect_source_origin();
    let source_origin = input
        .as_ref()
        .map(|layer| {
            if smart {
                layer.origin()
            } else {
                ae::Point {
                    h: -pre.h,
                    v: -pre.v,
                }
            }
        })
        .unwrap_or(ae::Point { h: 0, v: 0 });
    let output_origin = if smart {
        output.origin()
    } else {
        let shift = input_data.output_origin();
        ae::Point {
            h: source_origin.h - shift.h,
            v: source_origin.v - shift.v,
        }
    };
    let read = |x: i32, y: i32| {
        let x = x - source_origin.h;
        let y = y - source_origin.v;
        match input.as_ref() {
            Some(layer)
                if x >= 0 && y >= 0 && x < layer.width() as i32 && y < layer.height() as i32 =>
            {
                read_pixel(layer, x as usize, y as usize)
            }
            _ => TRANSPARENT,
        }
    };
    let g = data.geometry;
    let world_type = output.world_type();
    input_data.interact().abort()?;
    output.iterate(0, output.height() as i32, None, |x, y, mut dest| {
        let mut p = TRANSPARENT;
        for sy in 0..data.samples {
            for sx in 0..data.samples {
                let point = [
                    f64::from(x + output_origin.h)
                        + (f64::from(sx) + 0.5) / f64::from(data.samples),
                    f64::from(y + output_origin.v)
                        + (f64::from(sy) + 0.5) / f64::from(data.samples),
                ];
                if let Some(d) = data.output.direction(g.uv(point)) {
                    let c = sample(
                        data.input,
                        data.rotation.apply(d),
                        g.source_extent,
                        data.bilinear,
                        &read,
                    );
                    p.alpha += c.alpha;
                    p.red += c.red;
                    p.green += c.green;
                    p.blue += c.blue;
                }
            }
        }
        let weight = 1.0 / (data.samples * data.samples) as f32;
        p.alpha *= weight;
        p.red *= weight;
        p.green *= weight;
        p.blue *= weight;
        match world_type {
            ae::aegp::WorldType::U8 => dest.set_from_u8(p.to_pixel8()),
            ae::aegp::WorldType::U15 => dest.set_from_u16(p.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dest.set_from_f32(p),
        }
        Ok(())
    })
}
