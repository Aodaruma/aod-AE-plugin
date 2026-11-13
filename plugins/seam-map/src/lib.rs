#![allow(clippy::drop_non_drop, clippy::question_mark)] // Upstream define_effect! macro.
// Template material: MPL-2.0 with the additional permission in TEMPLATE-LICENSE.txt.
use ae::pf::*;
use after_effects as ae;
use utils::seam::{self, mapped};

#[cfg(all(test, target_os = "windows"))]
mod pipl_tests;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MapLayer,
    MapChannel,
    SeparateXY,
    BlackScale,
    WhiteScale,
    BlackX,
    WhiteX,
    BlackY,
    WhiteY,
    Anchor,
    Tolerance,
    Channel,
    Restore,
}

#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Controls exact seam-carved scaling with a grayscale layer and an anchor value.";

const SCALE_PARAMS: [(Params, &str, bool); 6] = [
    (Params::BlackScale, "Black Scale (%)", false),
    (Params::WhiteScale, "White Scale (%)", false),
    (Params::BlackX, "Black Scale X (%)", true),
    (Params::WhiteX, "White Scale X (%)", true),
    (Params::BlackY, "Black Scale Y (%)", true),
    (Params::WhiteY, "White Scale Y (%)", true),
];

#[derive(Clone, Copy)]
struct Settings {
    mapped: mapped::Settings,
    map_channel: i32,
}

fn settings(params: &Parameters<Params>) -> Result<Settings, Error> {
    let get = |p| -> Result<f64, Error> { Ok(params.get(p)?.as_float_slider()?.value()) };
    let scales = if params.get(Params::SeparateXY)?.as_checkbox()?.value() {
        [
            [get(Params::BlackX)?, get(Params::WhiteX)?],
            [get(Params::BlackY)?, get(Params::WhiteY)?],
        ]
    } else {
        [[get(Params::BlackScale)?, get(Params::WhiteScale)?]; 2]
    };
    Ok(Settings {
        mapped: mapped::Settings {
            scales,
            anchor: get(Params::Anchor)? / 100.0,
            tolerance: get(Params::Tolerance)? / 100.0,
            restore: params.get(Params::Restore)?.as_checkbox()?.value(),
            channel: seam::Channel::from_popup(params.get(Params::Channel)?.as_popup()?.value()),
        },
        map_channel: params.get(Params::MapChannel)?.as_popup()?.value(),
    })
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(Params::MapLayer, "Scale Map Layer", LayerDef::new())?;
        params.add(
            Params::MapChannel,
            "Map Channel",
            PopupDef::setup(|d| {
                d.set_options(&["Lightness", "R", "G", "B", "A"]);
                d.set_default(1);
            }),
        )?;
        params.add_with_flags(
            Params::SeparateXY,
            "Separate X/Y Scale",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        for (id, name, separate) in SCALE_PARAMS {
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
                if separate {
                    ParamUIFlags::DISABLED
                } else {
                    ParamUIFlags::empty()
                },
            )?;
        }
        for (id, name, default, maximum) in [
            (Params::Anchor, "Anchor Map Value (%)", 0.0, 100.0),
            (Params::Tolerance, "Anchor Tolerance (+/- %)", 0.2, 50.0),
        ] {
            params.add(
                id,
                name,
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(maximum);
                    d.set_slider_min(0.0);
                    d.set_slider_max(maximum);
                    d.set_default(default);
                    d.set_precision(2);
                }),
            )?;
        }
        params.add(
            Params::Channel,
            "Seam Channel",
            PopupDef::setup(|d| {
                d.set_options(&["RGB", "RGBA", "R", "G", "B", "A", "Lightness"]);
                d.set_default(2);
            }),
        )?;
        params.add(
            Params::Restore,
            "Restore Original Size",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
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
                "AOD_SeamMap {}\r\r{PLUGIN_DESCRIPTION}",
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
                for (id, _, individual) in SCALE_PARAMS {
                    let enabled = individual == separate;
                    if copy.get(id)?.ui_flags().contains(ParamUIFlags::DISABLED) == enabled {
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
                let settings = settings(params)?;
                let checkout = params.checkout_at(Params::MapLayer, None, None, None)?;
                let map = checkout.as_layer()?.value();
                render(
                    in_data,
                    Some(in_layer),
                    map.as_ref(),
                    out_layer,
                    settings,
                    false,
                    &mut out_data,
                )?;
            }
            Command::SmartPreRender { mut extra } => {
                let [w, h] = canvas_size(in_data);
                let bounds = ae::Rect {
                    left: 0,
                    top: 0,
                    right: w as i32,
                    bottom: h as i32,
                };
                let request = extra.output_request();
                let mut full = request;
                full.rect = bounds.into();
                full.channel_mask = 15;
                full.preserve_rgb_of_zero_alpha = 1;
                let map_index = params
                    .index(Params::MapLayer)
                    .ok_or(Error::BadCallbackParameter)?;
                for (param, id) in [(0, 0), (map_index as i32, 1)] {
                    extra.callbacks().checkout_layer(
                        param,
                        id,
                        &full,
                        in_data.current_time(),
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                }
                let rect: ae::Rect = request.rect.into();
                let left = rect.left.clamp(0, bounds.right);
                let top = rect.top.clamp(0, bounds.bottom);
                extra.set_result_rect(ae::Rect {
                    left,
                    top,
                    right: rect.right.min(bounds.right).max(left),
                    bottom: rect.bottom.min(bounds.bottom).max(top),
                });
                extra.set_max_result_rect(bounds);
            }
            Command::SmartRender { extra } => {
                let settings = settings(params)?;
                let cb = extra.callbacks();
                let source = cb.checkout_layer_pixels(0)?;
                let mut map_checked_out = false;
                let result: Result<(), Error> = (|| {
                    let map = cb.checkout_layer_pixels(1)?;
                    map_checked_out = true;
                    if let Some(output) = cb.checkout_output()? {
                        render(
                            in_data,
                            source,
                            map.as_ref(),
                            output,
                            settings,
                            true,
                            &mut out_data,
                        )?;
                    }
                    Ok(())
                })();
                let map_checkin = if map_checked_out {
                    cb.checkin_layer_pixels(1)
                } else {
                    Ok(())
                };
                let source_checkin = cb.checkin_layer_pixels(0);
                result?;
                map_checkin?;
                source_checkin?;
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

fn host_error(error: seam::CarveError, output: &mut OutData) -> Error {
    match error {
        seam::CarveError::Cancelled => Error::InterruptCancel,
        seam::CarveError::OutOfMemory => {
            output.set_return_msg("AOD_SeamMap: insufficient memory for exact seam carving.");
            Error::OutOfMemory
        }
        seam::CarveError::InvalidSize => {
            output.set_return_msg(
                "AOD_SeamMap: images must be at most 32768 px per side and 33554432 pixels.",
            );
            Error::BadCallbackParameter
        }
    }
}

fn map_value(pixel: ae::PixelF32, channel: i32) -> f32 {
    let value = match channel {
        2 => pixel.red,
        3 => pixel.green,
        4 => pixel.blue,
        5 => pixel.alpha,
        _ => 0.2126 * pixel.red + 0.7152 * pixel.green + 0.0722 * pixel.blue,
    };
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn read_at(layer: &Layer, x: usize, y: usize, origin: [i32; 2]) -> Option<ae::PixelF32> {
    let ix = x as i64 - i64::from(origin[0]);
    let iy = y as i64 - i64::from(origin[1]);
    if ix < 0 || iy < 0 || ix >= layer.width() as i64 || iy >= layer.height() as i64 {
        None
    } else {
        Some(utils::image::read_pixel(layer, ix as usize, iy as usize))
    }
}

#[allow(clippy::too_many_arguments)]
fn render(
    in_data: InData,
    source: Option<Layer>,
    map: Option<&Layer>,
    mut output: Layer,
    settings: Settings,
    smart: bool,
    out_data: &mut OutData,
) -> Result<(), Error> {
    let canvas = canvas_size(in_data);
    let count = seam::area(canvas[0], canvas[1]).map_err(|e| host_error(e, out_data))?;
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
            for x in 0..canvas[0] {
                if let Some(p) = read_at(&layer, x, y, input_origin) {
                    pixels[y * canvas[0] + x] = [p.red, p.green, p.blue, p.alpha];
                }
            }
        }
    }
    let mut image =
        seam::Image::new(canvas[0], canvas[1], pixels).map_err(|e| host_error(e, out_data))?;
    if let Some(map) = map {
        let mut values = seam::buffer(count, 0.0).map_err(|e| host_error(e, out_data))?;
        let origin = [map.origin().h, map.origin().v];
        for y in 0..canvas[1] {
            if y % 32 == 0 {
                in_data.interact().abort()?;
            }
            for x in 0..canvas[0] {
                if let Some(p) = read_at(map, x, y, origin) {
                    values[y * canvas[0] + x] = map_value(p, settings.map_channel);
                }
            }
        }
        let mut last_abort_check = std::time::Instant::now();
        let mut cancel = || {
            if last_abort_check.elapsed() < std::time::Duration::from_millis(25) {
                return false;
            }
            last_abort_check = std::time::Instant::now();
            in_data.interact().abort().is_err()
        };
        image = mapped::apply(image, &values, settings.mapped, &mut cancel)
            .map_err(|e| host_error(e, out_data))?;
    }
    let origin = if smart {
        [output.origin().h, output.origin().v]
    } else {
        let shift = in_data.output_origin();
        [input_origin[0] - shift.h, input_origin[1] - shift.v]
    };
    output.iterate(0, output.height() as i32, None, |x, y, mut dst| {
        let x = i64::from(x) + i64::from(origin[0]);
        let y = i64::from(y) + i64::from(origin[1]);
        let p = if x < 0 || y < 0 || x >= canvas[0] as i64 || y >= canvas[1] as i64 {
            [0.0; 4]
        } else {
            image.pixels[y as usize * canvas[0] + x as usize]
        };
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
                **d = ae::PixelF32 {
                    red: p[0],
                    green: p[1],
                    blue: p[2],
                    alpha: p[3],
                };
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
    fn map_channels_clamp_hdr_and_nonfinite_inputs() {
        let p = ae::PixelF32 {
            red: 0.2,
            green: 0.4,
            blue: 0.6,
            alpha: 0.8,
        };
        assert!((map_value(p, 1) - 0.37192).abs() < 1e-6);
        for (channel, expected) in [(2, 0.2), (3, 0.4), (4, 0.6), (5, 0.8)] {
            assert_eq!(map_value(p, channel), expected);
        }
        assert_eq!(map_value(ae::PixelF32 { red: f32::NAN, ..p }, 2), 0.0);
        assert_eq!(map_value(ae::PixelF32 { red: 2.0, ..p }, 2), 1.0);
    }
}
