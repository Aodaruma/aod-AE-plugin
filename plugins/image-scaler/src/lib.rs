#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ScalePercent,
    UseReciprocal,
    Interpolation,
    ScaleMode,
    ScaleExp,
    ScalePower2,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Scales layers with selectable interpolation modes and optional reciprocal scaling.";
const MIN_SCALE_FACTOR: f32 = 0.001;
const MAX_SCALE_FACTOR: f32 = 1_000_000.0;

#[derive(Clone, Copy, Debug)]
enum ScaleMode {
    Linear,
    Exp,
    Power2,
}

impl ScaleMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Exp,
            3 => Self::Power2,
            _ => Self::Linear,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum InterpolationMode {
    Nearest,
    Bilinear,
    Bicubic,
    Mitchell,
    Lanczos3,
}

impl InterpolationMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            1 => Self::Nearest,
            2 => Self::Bilinear,
            3 => Self::Bicubic,
            4 => Self::Mitchell,
            5 => Self::Lanczos3,
            _ => Self::Bilinear,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    scale: f32,
    interpolation: InterpolationMode,
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::ScalePercent,
            "Scale (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.1);
                d.set_valid_max(20000.0);
                d.set_slider_min(1.0);
                d.set_slider_max(20000.0);
                d.set_default(100.0);
                d.set_precision(2);
            }),
        )?;

        params.add_with_flags(
            Params::UseReciprocal,
            "Use Reciprocal",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::Interpolation,
            "Interpolation",
            PopupDef::setup(|d| {
                d.set_options(&["Nearest", "Bilinear", "Bicubic", "Mitchell", "Lanczos3"]);
                d.set_default(2);
            }),
        )?;

        params.add_with_flags(
            Params::ScaleMode,
            "Scale Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Linear", "Exp", "Power2"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::ScaleExp,
            "Scale (Exp)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.1);
                d.set_valid_max(20000.0);
                d.set_slider_min(1.0);
                d.set_slider_max(20000.0);
                d.set_default(100.0);
                d.set_precision(2);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::ScalePower2,
            "Scale (Power2)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.1);
                d.set_valid_max(20000.0);
                d.set_slider_min(1.0);
                d.set_slider_max(20000.0);
                d.set_default(100.0);
                d.set_precision(2);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    format!(
                        "AOD_ImageScaler - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImageScaler")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let req = extra.output_request();

                if let Ok(in_result) = extra.callbacks().checkout_layer(
                    0,
                    0,
                    &req,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                ) {
                    let _ = extra.union_result_rect(in_result.result_rect.into());
                    let _ = extra.union_max_result_rect(in_result.max_result_rect.into());
                } else {
                    return Err(Error::InterruptCancel);
                }
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let in_layer_opt = cb.checkout_layer_pixels(0)?;
                let out_layer_opt = cb.checkout_output()?;

                if let (Some(in_layer), Some(out_layer)) = (in_layer_opt, out_layer_opt) {
                    self.do_render(in_layer, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let t = params.type_at(param_index);
                if t == Params::ScaleMode || t == Params::UseReciprocal {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params_copy = params.cloned();
                self.update_params_ui(in_data, &mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let scale_mode =
            ScaleMode::from_popup_value(params.get(Params::ScaleMode)?.as_popup()?.value());
        let use_reciprocal = params.get(Params::UseReciprocal)?.as_checkbox()?.value();

        self.set_param_visible(
            in_data,
            params,
            Params::ScalePercent,
            matches!(scale_mode, ScaleMode::Linear),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ScaleExp,
            matches!(scale_mode, ScaleMode::Exp),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ScalePower2,
            matches!(scale_mode, ScaleMode::Power2),
        )?;

        let exp_name = if use_reciprocal {
            "Scale (Log e)"
        } else {
            "Scale (Exp)"
        };
        Self::set_param_name(params, Params::ScaleExp, exp_name)?;

        let power2_name = if use_reciprocal {
            "Scale (Log 2)"
        } else {
            "Scale (Power2)"
        };
        Self::set_param_name(params, Params::ScalePower2, power2_name)?;

        Ok(())
    }

    fn set_param_name(
        params: &mut ae::Parameters<Params>,
        id: Params,
        name: &str,
    ) -> Result<(), Error> {
        let mut p = params.get_mut(id)?;
        p.set_name(name)?;
        p.update_param_ui()?;
        Ok(())
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if in_data.is_premiere() {
            return Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible);
        }

        if let Some(plugin_id) = self.aegp_id {
            let effect = in_data.effect();
            if let Some(index) = params.index(id)
                && let Ok(effect_ref) = effect.aegp_effect(plugin_id)
                && let Ok(stream) = effect_ref.new_stream_by_index(plugin_id, index as i32)
            {
                return stream.set_dynamic_stream_flag(
                    ae::aegp::DynamicStreamFlags::Hidden,
                    false,
                    !visible,
                );
            }
        }

        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_ui_flag(
        params: &mut ae::Parameters<Params>,
        id: Params,
        flag: ae::pf::ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let flag_bits = flag.bits();
        let current_status = (params.get(id)?.ui_flags().bits() & flag_bits) != 0;
        if current_status == status {
            return Ok(());
        }

        let mut p = params.get_mut(id)?;
        p.set_ui_flag(flag, status);
        p.update_param_ui()?;
        Ok(())
    }

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let settings = read_settings(params)?;
        let source = capture_source(&in_layer);
        let center_x = (width as f32 - 1.0) * 0.5;
        let center_y = (height as f32 - 1.0) * 0.5;
        let progress_final = out_layer.height() as i32;

        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let src_x = (x as f32 - center_x) / settings.scale + center_x;
            let src_y = (y as f32 - center_y) / settings.scale + center_y;
            let sampled =
                sample_pixel(&source, width, height, src_x, src_y, settings.interpolation);
            write_output_pixel(&mut dst, sampled);
            Ok(())
        })?;

        Ok(())
    }
}

fn read_settings(params: &mut Parameters<Params>) -> Result<Settings, Error> {
    let use_reciprocal = params.get(Params::UseReciprocal)?.as_checkbox()?.value();
    let scale_mode =
        ScaleMode::from_popup_value(params.get(Params::ScaleMode)?.as_popup()?.value());

    let raw_scale = match scale_mode {
        ScaleMode::Linear => {
            let scale_percent = params.get(Params::ScalePercent)?.as_float_slider()?.value() as f32;
            let base_scale = (scale_percent / 100.0).max(MIN_SCALE_FACTOR);
            if use_reciprocal {
                1.0 / base_scale
            } else {
                base_scale
            }
        }
        ScaleMode::Exp => {
            let scale_percent = params.get(Params::ScaleExp)?.as_float_slider()?.value() as f32;
            let base_scale = (scale_percent / 100.0).max(MIN_SCALE_FACTOR);
            if use_reciprocal {
                base_scale.ln()
            } else {
                base_scale.exp()
            }
        }
        ScaleMode::Power2 => {
            let scale_percent = params.get(Params::ScalePower2)?.as_float_slider()?.value() as f32;
            let base_scale = (scale_percent / 100.0).max(MIN_SCALE_FACTOR);
            if use_reciprocal {
                base_scale.log2()
            } else {
                2.0_f32.powf(base_scale)
            }
        }
    };
    let scale = if raw_scale.is_finite() {
        raw_scale
    } else if raw_scale.is_sign_positive() {
        MAX_SCALE_FACTOR
    } else {
        MIN_SCALE_FACTOR
    }
    .clamp(MIN_SCALE_FACTOR, MAX_SCALE_FACTOR);

    let interpolation_value = params.get(Params::Interpolation)?.as_popup()?.value();
    let interpolation = InterpolationMode::from_popup_value(interpolation_value);

    Ok(Settings {
        scale,
        interpolation,
    })
}

fn capture_source(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut src = vec![transparent_pixel(); width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            src[idx] = match world_type {
                ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
                ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
            };
        }
    }

    src
}

fn sample_pixel(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    mode: InterpolationMode,
) -> PixelF32 {
    match mode {
        InterpolationMode::Nearest => sample_nearest(src, width, height, x, y),
        InterpolationMode::Bilinear => sample_bilinear(src, width, height, x, y),
        InterpolationMode::Bicubic => sample_bicubic(src, width, height, x, y),
        InterpolationMode::Mitchell => sample_mitchell(src, width, height, x, y),
        InterpolationMode::Lanczos3 => sample_lanczos3(src, width, height, x, y),
    }
}

fn sample_nearest(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let nx = x.round() as isize;
    let ny = y.round() as isize;
    fetch_or_transparent(src, width, height, nx, ny)
}

fn sample_bilinear(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = fetch_or_transparent(src, width, height, x0, y0);
    let p10 = fetch_or_transparent(src, width, height, x1, y0);
    let p01 = fetch_or_transparent(src, width, height, x0, y1);
    let p11 = fetch_or_transparent(src, width, height, x1, y1);

    let row0 = lerp_pixel(p00, p10, tx);
    let row1 = lerp_pixel(p01, p11, tx);
    lerp_pixel(row0, row1, ty)
}

fn sample_bicubic(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let x_base = x.floor() as isize;
    let y_base = y.floor() as isize;

    let mut acc = PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    };

    for my in -1..=2 {
        let sy = y_base + my;
        let wy = cubic_weight(y - sy as f32);
        if wy == 0.0 {
            continue;
        }

        for mx in -1..=2 {
            let sx = x_base + mx;
            let wx = cubic_weight(x - sx as f32);
            if wx == 0.0 {
                continue;
            }

            let w = wx * wy;
            let p = fetch_or_transparent(src, width, height, sx, sy);

            acc.alpha += p.alpha * w;
            acc.red += p.red * w;
            acc.green += p.green * w;
            acc.blue += p.blue * w;
        }
    }

    PixelF32 {
        alpha: clamp01(acc.alpha),
        red: clamp01(acc.red),
        green: clamp01(acc.green),
        blue: clamp01(acc.blue),
    }
}

fn sample_mitchell(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let x_base = x.floor() as isize;
    let y_base = y.floor() as isize;

    let mut acc = PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    };

    for my in -1..=2 {
        let sy = y_base + my;
        let wy = mitchell_weight(y - sy as f32);
        if wy == 0.0 {
            continue;
        }

        for mx in -1..=2 {
            let sx = x_base + mx;
            let wx = mitchell_weight(x - sx as f32);
            if wx == 0.0 {
                continue;
            }

            let w = wx * wy;
            let p = fetch_or_transparent(src, width, height, sx, sy);

            acc.alpha += p.alpha * w;
            acc.red += p.red * w;
            acc.green += p.green * w;
            acc.blue += p.blue * w;
        }
    }

    PixelF32 {
        alpha: clamp01(acc.alpha),
        red: clamp01(acc.red),
        green: clamp01(acc.green),
        blue: clamp01(acc.blue),
    }
}

fn sample_lanczos3(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let x_base = x.floor() as isize;
    let y_base = y.floor() as isize;

    let mut acc = PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    };

    for my in -2..=3 {
        let sy = y_base + my;
        let wy = lanczos_weight(y - sy as f32, 3.0);
        if wy == 0.0 {
            continue;
        }

        for mx in -2..=3 {
            let sx = x_base + mx;
            let wx = lanczos_weight(x - sx as f32, 3.0);
            if wx == 0.0 {
                continue;
            }

            let w = wx * wy;
            let p = fetch_or_transparent(src, width, height, sx, sy);

            acc.alpha += p.alpha * w;
            acc.red += p.red * w;
            acc.green += p.green * w;
            acc.blue += p.blue * w;
        }
    }

    PixelF32 {
        alpha: clamp01(acc.alpha),
        red: clamp01(acc.red),
        green: clamp01(acc.green),
        blue: clamp01(acc.blue),
    }
}

fn cubic_weight(d: f32) -> f32 {
    let a = -0.5;
    let x = d.abs();
    if x <= 1.0 {
        (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn mitchell_weight(d: f32) -> f32 {
    let b = 1.0 / 3.0;
    let c = 1.0 / 3.0;
    let x = d.abs();

    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
            + (-18.0 + 12.0 * b + 6.0 * c) * x * x
            + (6.0 - 2.0 * b))
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x * x * x
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}

fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        let pix = std::f32::consts::PI * x;
        pix.sin() / pix
    }
}

fn lanczos_weight(d: f32, a: f32) -> f32 {
    let x = d.abs();
    if x >= a { 0.0 } else { sinc(x) * sinc(x / a) }
}

fn fetch_or_transparent(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
) -> PixelF32 {
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        return transparent_pixel();
    }
    src[y as usize * width + x as usize]
}

fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: lerp(a.alpha, b.alpha, t),
        red: lerp(a.red, b.red, t),
        green: lerp(a.green, b.green, t),
        blue: lerp(a.blue, b.blue, t),
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn write_output_pixel(dst: &mut GenericPixelMut<'_>, px: PixelF32) {
    match dst {
        GenericPixelMut::Pixel8(p) => {
            **p = px.to_pixel8();
        }
        GenericPixelMut::Pixel16(p) => {
            **p = px.to_pixel16();
        }
        GenericPixelMut::PixelF32(p) => {
            **p = px;
        }
        GenericPixelMut::PixelF64(p) => {
            p.alphaF = px.alpha as _;
            p.redF = px.red as _;
            p.greenF = px.green as _;
            p.blueF = px.blue as _;
        }
    }
}
