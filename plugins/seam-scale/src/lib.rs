#![allow(clippy::drop_non_drop, clippy::question_mark)] // Upstream define_effect! macro.
// Template material: MPL-2.0 with the additional permission in TEMPLATE-LICENSE.txt.
use ae::pf::*;
use after_effects as ae;

mod seam;

#[cfg(all(test, target_os = "windows"))]
#[path = "pipl_tests.rs"]
mod pipl_tests;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    SeparateXY,
    Scale,
    ScaleX,
    ScaleY,
    Restore,
    Channel,
    Output,
}

#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Resizes images with exact dynamic-programming seam carving and optional size restoration.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::SeparateXY,
            "Separate X/Y Scale",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        for (id, name, enabled) in [
            (Params::Scale, "Scale (%)", true),
            (Params::ScaleX, "Scale X (%)", false),
            (Params::ScaleY, "Scale Y (%)", false),
        ] {
            params.add_with_flags(
                id,
                name,
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-400.0);
                    d.set_valid_max(400.0);
                    d.set_slider_min(-400.0);
                    d.set_slider_max(400.0);
                    d.set_default(100.0);
                    d.set_precision(2);
                }),
                ParamFlag::empty(),
                if enabled {
                    ParamUIFlags::empty()
                } else {
                    ParamUIFlags::DISABLED
                },
            )?;
        }
        params.add(
            Params::Restore,
            "Restore Original Size",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;
        // Append to preserve the disk IDs of existing projects and keyframes.
        params.add(
            Params::Channel,
            "Seam Channel",
            PopupDef::setup(|d| {
                d.set_options(&["RGB", "RGBA", "R", "G", "B", "A", "Lightness"]);
                d.set_default(2);
            }),
        )?;
        params.add(
            Params::Output,
            "Output",
            PopupDef::setup(|d| {
                d.set_options(&["Image", "Seam Map (Displacement)"]);
                d.set_default(1);
            }),
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        match cmd {
            Command::About => out_data.set_return_msg(&format!(
                "AOD_SeamScale {}\r\r{PLUGIN_DESCRIPTION}",
                env!("CARGO_PKG_VERSION")
            )),
            Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag(OutFlags::PixIndependent, false);
                out_data.set_out_flag(OutFlags::UseOutputExtent, false);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
            }
            Command::UserChangedParam { .. } => out_data.set_out_flag(OutFlags::RefreshUi, true),
            Command::UpdateParamsUi => {
                let separate = params.get(Params::SeparateXY)?.as_checkbox()?.value();
                let mut copy = params.cloned();
                for (id, enabled) in [
                    (Params::Scale, !separate),
                    (Params::ScaleX, separate),
                    (Params::ScaleY, separate),
                ] {
                    let disabled = copy.get(id)?.ui_flags().contains(ParamUIFlags::DISABLED);
                    if disabled == enabled {
                        let mut param = copy.get_mut(id)?;
                        param.set_ui_flag(ParamUIFlags::DISABLED, !enabled);
                        param.update_param_ui()?;
                    }
                }
            }
            Command::Render {
                in_layer,
                out_layer,
            } => {
                render(
                    in_data,
                    Some(in_layer),
                    out_layer,
                    params,
                    false,
                    &mut out_data,
                )?;
            }
            Command::SmartPreRender { mut extra } => {
                let canvas = canvas_size(in_data);
                let bounds = ae::Rect {
                    left: 0,
                    top: 0,
                    right: canvas[0] as i32,
                    bottom: canvas[1] as i32,
                };
                let request = extra.output_request();
                let mut full = request;
                // A seam can cross every pixel: a tile or alpha-only checkout is insufficient.
                full.rect = bounds.into();
                full.channel_mask = 15;
                full.preserve_rgb_of_zero_alpha = 1;
                extra.callbacks().checkout_layer(
                    0,
                    0,
                    &full,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let rect: ae::Rect = request.rect.into();
                let left = rect.left.max(0).min(bounds.right);
                let top = rect.top.max(0).min(bounds.bottom);
                extra.set_result_rect(ae::Rect {
                    left,
                    top,
                    right: rect.right.min(bounds.right).max(left),
                    bottom: rect.bottom.min(bounds.bottom).max(top),
                });
                extra.set_max_result_rect(bounds);
            }
            Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let source = cb.checkout_layer_pixels(0)?;
                let result: Result<(), Error> = (|| {
                    if let Some(output) = cb.checkout_output()? {
                        render(in_data, source, output, params, true, &mut out_data)?;
                    }
                    Ok(())
                })();
                // Always release a successful checkout, including cancellation/error paths.
                let checkin = cb.checkin_layer_pixels(0);
                result?;
                checkin?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn canvas_size(in_data: InData) -> [usize; 2] {
    [
        (in_data.width().max(1) as f64 * f64::from(f32::from(in_data.downsample_x())))
            .ceil()
            .max(1.0) as usize,
        (in_data.height().max(1) as f64 * f64::from(f32::from(in_data.downsample_y())))
            .ceil()
            .max(1.0) as usize,
    ]
}

fn scale_target(size: usize, percent: f64) -> usize {
    let percent = if percent.is_finite() {
        percent.abs().min(400.0)
    } else {
        100.0
    };
    (size as f64 * percent / 100.0).round().max(1.0) as usize
}

fn host_error(error: seam::CarveError, output: &mut OutData) -> Error {
    match error {
        seam::CarveError::Cancelled => Error::InterruptCancel,
        seam::CarveError::OutOfMemory => {
            output.set_return_msg("AOD_SeamScale: insufficient memory for exact seam carving.");
            Error::OutOfMemory
        }
        seam::CarveError::InvalidSize => {
            output.set_return_msg("AOD_SeamScale: input, intermediate and target images must be at most 32768 px per side and 33554432 pixels.");
            Error::BadCallbackParameter
        }
    }
}

fn render(
    in_data: InData,
    source: Option<Layer>,
    mut output: Layer,
    params: &mut Parameters<Params>,
    smart: bool,
    out_data: &mut OutData,
) -> Result<(), Error> {
    let canvas = canvas_size(in_data);
    let separate = params.get(Params::SeparateXY)?.as_checkbox()?.value();
    let shared = params.get(Params::Scale)?.as_float_slider()?.value();
    let sx = if separate {
        params.get(Params::ScaleX)?.as_float_slider()?.value()
    } else {
        shared
    };
    let sy = if separate {
        params.get(Params::ScaleY)?.as_float_slider()?.value()
    } else {
        shared
    };
    let target = [scale_target(canvas[0], sx), scale_target(canvas[1], sy)];
    let restore = params.get(Params::Restore)?.as_checkbox()?.value();
    let channel = seam::Channel::from_popup(params.get(Params::Channel)?.as_popup()?.value());
    let map_output = params.get(Params::Output)?.as_popup()?.value() == 2;
    let count = seam::area(canvas[0], canvas[1]).map_err(|e| host_error(e, out_data))?;
    seam::area(target[0], canvas[1])
        .and_then(|_| seam::area(target[0], target[1]))
        .map_err(|e| host_error(e, out_data))?;
    let mut pixels = seam::buffer(count, [0.0; 4]).map_err(|e| host_error(e, out_data))?;
    let pre = in_data.pre_effect_source_origin();
    let mut input_origin = [-pre.h, -pre.v];
    if let Some(layer) = source {
        if smart {
            input_origin = [layer.origin().h, layer.origin().v];
        }
        for y in 0..canvas[1] {
            if y % 32 == 0 {
                in_data.interact().abort()?;
            }
            let iy = y as i64 - i64::from(input_origin[1]);
            if iy < 0 || iy >= layer.height() as i64 {
                continue;
            }
            for x in 0..canvas[0] {
                let ix = x as i64 - i64::from(input_origin[0]);
                if ix < 0 || ix >= layer.width() as i64 {
                    continue;
                }
                let p = utils::image::read_pixel(&layer, ix as usize, iy as usize);
                pixels[y * canvas[0] + x] = [p.red, p.green, p.blue, p.alpha];
            }
        }
    }
    let mut image =
        seam::Image::new(canvas[0], canvas[1], pixels).map_err(|e| host_error(e, out_data))?;
    in_data.interact().abort()?;
    let mut last_abort_check = std::time::Instant::now();
    let mut cancel = || {
        // Avoid tens of thousands of host callbacks per frame while keeping
        // cancellation responsive. This does not affect any image computation.
        if last_abort_check.elapsed() < std::time::Duration::from_millis(25) {
            return false;
        }
        last_abort_check = std::time::Instant::now();
        in_data.interact().abort().is_err()
    };
    if map_output {
        image
            .track_coordinates(&mut cancel)
            .map_err(|e| host_error(e, out_data))?;
    }
    let mut image = seam::resize(image, target[0], target[1], channel, &mut cancel)
        .map_err(|e| host_error(e, out_data))?;
    image
        .flip(
            [sx.is_finite() && sx < 0.0, sy.is_finite() && sy < 0.0],
            &mut cancel,
        )
        .map_err(|e| host_error(e, out_data))?;
    if map_output {
        image = image
            .into_coordinate_image(&mut cancel)
            .map_err(|e| host_error(e, out_data))?;
    }
    let origin = if smart {
        [output.origin().h, output.origin().v]
    } else {
        let shift = in_data.output_origin();
        [input_origin[0] - shift.h, input_origin[1] - shift.v]
    };
    output.iterate(0, output.height() as i32, None, |x, y, mut dst| {
        let sample = if map_output {
            seam::sample_displacement_output
        } else {
            seam::sample_output
        };
        let p = sample(
            &image,
            canvas,
            [
                i64::from(x) + i64::from(origin[0]),
                i64::from(y) + i64::from(origin[1]),
            ],
            restore,
        );
        // Round integer output instead of truncating: retained pixels stay bit-exact.
        match &mut dst {
            GenericPixelMut::Pixel8(d) => {
                d.red = quantize(p[0], 255) as u8;
                d.green = quantize(p[1], 255) as u8;
                d.blue = quantize(p[2], 255) as u8;
                d.alpha = quantize(p[3], 255) as u8;
            }
            GenericPixelMut::Pixel16(d) => {
                d.red = quantize(p[0], 32768);
                d.green = quantize(p[1], 32768);
                d.blue = quantize(p[2], 32768);
                d.alpha = quantize(p[3], 32768);
            }
            GenericPixelMut::PixelF32(d) => {
                **d = PixelF32 {
                    red: p[0],
                    green: p[1],
                    blue: p[2],
                    alpha: p[3],
                }
            }
            GenericPixelMut::PixelF64(_) => return Err(Error::BadCallbackParameter),
        }
        Ok(())
    })
}

fn quantize(value: f32, maximum: u16) -> u16 {
    (value.clamp(0.0, 1.0) * f32::from(maximum)).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_roundtrip_is_exact() {
        for maximum in [255, 32768] {
            for value in 0..=maximum {
                assert_eq!(
                    quantize(f32::from(value) / f32::from(maximum), maximum),
                    value
                );
            }
        }
    }
    #[test]
    fn scale_rounding_and_limits() {
        assert_eq!(scale_target(17, 50.0), 9);
        assert_eq!(scale_target(1, 1.0), 1);
        assert_eq!(scale_target(20, f64::NAN), 20);
        assert_eq!(scale_target(20, 500.0), 80);
        assert_eq!(scale_target(20, -500.0), 80);
        assert_eq!(scale_target(17, -50.0), 9);
        assert_eq!(scale_target(20, 0.0), 1);
        assert_eq!(scale_target(1920, 0.01), 1);
        assert_eq!(scale_target(20, f64::NEG_INFINITY), 20);
    }
}
