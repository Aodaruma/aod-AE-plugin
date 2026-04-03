#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, Lab, LinSrgb, Oklab, Oklch, Srgb};
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ScaleMode,
    UseReciprocal,
    ScalePercent,
    ScaleExp,
    ScalePower2,
    Interpolation,
    InterpolationColorSpace,
    MitchellB,
    MitchellC,
    LanczosLobes,
    EqaRadius,
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
const OKLAB_AB_MAX: f32 = 0.5;
const OKLCH_CHROMA_MAX: f32 = 0.4;
const LAB_L_MAX: f32 = 100.0;
const LAB_AB_MAX: f32 = 128.0;
const YIQ_I_MAX: f32 = 0.5957;
const YIQ_Q_MAX: f32 = 0.5226;
const YUV_U_MAX: f32 = 0.436;
const YUV_V_MAX: f32 = 0.615;
const YCBCR_MAX: f32 = 255.0;

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
    Lanczos,
    EqaQuadratic,
}

impl InterpolationMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            1 => Self::Nearest,
            2 => Self::Bilinear,
            3 => Self::Bicubic,
            4 => Self::Mitchell,
            5 => Self::Lanczos,
            6 => Self::EqaQuadratic,
            _ => Self::Bilinear,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum InterpolationColorSpace {
    LinearRgba,
    Srgb,
    Oklab,
    Oklch,
    Lab,
    Yiq,
    Yuv,
    YCbCr,
    Hsl,
    Hsv,
    Cmyk,
}

impl InterpolationColorSpace {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Srgb,
            3 => Self::Oklab,
            4 => Self::Oklch,
            5 => Self::Lab,
            6 => Self::Yiq,
            7 => Self::Yuv,
            8 => Self::YCbCr,
            9 => Self::Hsl,
            10 => Self::Hsv,
            11 => Self::Cmyk,
            _ => Self::LinearRgba,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    scale: f32,
    interpolation: InterpolationMode,
    color_space: InterpolationColorSpace,
    mitchell_b: f32,
    mitchell_c: f32,
    lanczos_lobes: f32,
    eqa_radius: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct WorkingAccumulator {
    sum: [f32; 4],
    alpha_sum: f32,
    weight_sum: f32,
    hue_cos_sum: f32,
    hue_sin_sum: f32,
}

impl WorkingAccumulator {
    fn add_sample(&mut self, px: PixelF32, weight: f32, color_space: InterpolationColorSpace) {
        if weight == 0.0 || !weight.is_finite() {
            return;
        }

        let encoded =
            convert_linear_to_working_pixel(color_space, [px.red, px.green, px.blue, px.alpha]);
        self.sum[0] += encoded[0] * weight;
        self.sum[1] += encoded[1] * weight;
        self.sum[2] += encoded[2] * weight;
        self.sum[3] += encoded[3] * weight;
        self.alpha_sum += px.alpha * weight;
        self.weight_sum += weight;

        if is_hue_space(color_space) {
            let angle = wrap01(encoded[0]) * std::f32::consts::TAU;
            self.hue_cos_sum += angle.cos() * weight;
            self.hue_sin_sum += angle.sin() * weight;
        }
    }

    fn finish(self, color_space: InterpolationColorSpace, normalize: bool) -> PixelF32 {
        if self.weight_sum.abs() <= 1.0e-8 {
            return transparent_pixel();
        }

        let scale = if normalize {
            1.0 / self.weight_sum
        } else {
            1.0
        };
        let mut working = [
            self.sum[0] * scale,
            self.sum[1] * scale,
            self.sum[2] * scale,
            self.sum[3] * scale,
        ];
        let alpha = clamp01(self.alpha_sum * scale);

        if is_hue_space(color_space)
            && (self.hue_cos_sum.abs() > 1.0e-8 || self.hue_sin_sum.abs() > 1.0e-8)
        {
            working[0] = wrap01(self.hue_sin_sum.atan2(self.hue_cos_sum) / std::f32::consts::TAU);
        }

        if !matches!(color_space, InterpolationColorSpace::Cmyk) {
            working[3] = alpha;
        }

        let linear = convert_working_to_linear_pixel(color_space, working, alpha);
        PixelF32 {
            alpha,
            red: clamp01(linear[0]),
            green: clamp01(linear[1]),
            blue: clamp01(linear[2]),
        }
    }
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
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
            Params::UseReciprocal,
            "Use Reciprocal",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

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

        params.add_with_flags(
            Params::Interpolation,
            "Interpolation",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Nearest",
                    "Bilinear",
                    "Bicubic",
                    "Mitchell",
                    "Lanczos",
                    "EQA Quadratic",
                ]);
                d.set_default(2);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::InterpolationColorSpace,
            "Interpolation Color Space",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Linear RGBA",
                    "sRGB",
                    "OKLAB",
                    "OKLCH",
                    "LAB",
                    "YIQ",
                    "YUV",
                    "YCbCr",
                    "HSL",
                    "HSV",
                    "CMYK",
                ]);
                d.set_default(1);
            }),
        )?;

        params.add_with_flags(
            Params::MitchellB,
            "Mitchell B",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(1.0 / 3.0);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::MitchellC,
            "Mitchell C",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(1.0 / 3.0);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::LanczosLobes,
            "Lanczos Lobes",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(8.0);
                d.set_slider_min(1.0);
                d.set_slider_max(6.0);
                d.set_default(3.0);
                d.set_precision(2);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::EqaRadius,
            "EQA Radius",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.5);
                d.set_valid_max(8.0);
                d.set_slider_min(0.5);
                d.set_slider_max(4.0);
                d.set_default(2.0);
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
                if t == Params::ScaleMode
                    || t == Params::UseReciprocal
                    || t == Params::Interpolation
                {
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
        let interpolation = InterpolationMode::from_popup_value(
            params.get(Params::Interpolation)?.as_popup()?.value(),
        );

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
        self.set_param_visible(
            in_data,
            params,
            Params::MitchellB,
            matches!(interpolation, InterpolationMode::Mitchell),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::MitchellC,
            matches!(interpolation, InterpolationMode::Mitchell),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::LanczosLobes,
            matches!(interpolation, InterpolationMode::Lanczos),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::EqaRadius,
            matches!(interpolation, InterpolationMode::EqaQuadratic),
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
            let sampled = sample_pixel(&source, width, height, src_x, src_y, &settings);
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

    let interpolation =
        InterpolationMode::from_popup_value(params.get(Params::Interpolation)?.as_popup()?.value());
    let color_space = InterpolationColorSpace::from_popup_value(
        params
            .get(Params::InterpolationColorSpace)?
            .as_popup()?
            .value(),
    );
    let mitchell_b =
        (params.get(Params::MitchellB)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let mitchell_c =
        (params.get(Params::MitchellC)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let lanczos_lobes =
        (params.get(Params::LanczosLobes)?.as_float_slider()?.value() as f32).clamp(1.0, 8.0);
    let eqa_radius =
        (params.get(Params::EqaRadius)?.as_float_slider()?.value() as f32).clamp(0.5, 8.0);

    Ok(Settings {
        scale,
        interpolation,
        color_space,
        mitchell_b,
        mitchell_c,
        lanczos_lobes,
        eqa_radius,
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
    settings: &Settings,
) -> PixelF32 {
    match settings.interpolation {
        InterpolationMode::Nearest => sample_nearest(src, width, height, x, y),
        InterpolationMode::Bilinear => {
            sample_bilinear(src, width, height, x, y, settings.color_space)
        }
        InterpolationMode::Bicubic => {
            sample_bicubic(src, width, height, x, y, settings.color_space)
        }
        InterpolationMode::Mitchell => sample_mitchell(
            src,
            width,
            height,
            x,
            y,
            settings.color_space,
            settings.mitchell_b,
            settings.mitchell_c,
        ),
        InterpolationMode::Lanczos => sample_lanczos(
            src,
            width,
            height,
            x,
            y,
            settings.color_space,
            settings.lanczos_lobes,
        ),
        InterpolationMode::EqaQuadratic => sample_eqa_quadratic(
            src,
            width,
            height,
            x,
            y,
            settings.color_space,
            settings.eqa_radius,
        ),
    }
}

fn sample_nearest(src: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let nx = x.round() as isize;
    let ny = y.round() as isize;
    fetch_or_transparent(src, width, height, nx, ny)
}

fn sample_bilinear(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
) -> PixelF32 {
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

    let mut acc = WorkingAccumulator::default();
    acc.add_sample(p00, (1.0 - tx) * (1.0 - ty), color_space);
    acc.add_sample(p10, tx * (1.0 - ty), color_space);
    acc.add_sample(p01, (1.0 - tx) * ty, color_space);
    acc.add_sample(p11, tx * ty, color_space);
    acc.finish(color_space, false)
}

fn sample_bicubic(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
) -> PixelF32 {
    sample_separable(src, width, height, x, y, color_space, 2.0, false, |d| {
        cubic_weight(d, -0.5)
    })
}

fn sample_mitchell(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
    b: f32,
    c: f32,
) -> PixelF32 {
    sample_separable(src, width, height, x, y, color_space, 2.0, false, |d| {
        mitchell_weight(d, b, c)
    })
}

fn sample_lanczos(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
    lobes: f32,
) -> PixelF32 {
    let lobes = lobes.max(1.0);
    sample_separable(src, width, height, x, y, color_space, lobes, false, |d| {
        lanczos_weight(d, lobes)
    })
}

fn sample_eqa_quadratic(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
    radius: f32,
) -> PixelF32 {
    let radius = radius.max(0.5);
    let radius_sq = radius * radius;
    let min_y = (y - radius).floor() as isize;
    let max_y = (y + radius).ceil() as isize;
    let min_x = (x - radius).floor() as isize;
    let max_x = (x + radius).ceil() as isize;

    let mut acc = WorkingAccumulator::default();
    for sy in min_y..=max_y {
        let dy = y - sy as f32;
        for sx in min_x..=max_x {
            let dx = x - sx as f32;
            let t = (dx * dx + dy * dy) / radius_sq;
            if t >= 1.0 {
                continue;
            }

            let w = (1.0 - t) * (1.0 - t);
            let p = fetch_or_transparent(src, width, height, sx, sy);
            acc.add_sample(p, w, color_space);
        }
    }

    acc.finish(color_space, true)
}

fn sample_separable<F>(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    color_space: InterpolationColorSpace,
    radius: f32,
    normalize: bool,
    mut weight_fn: F,
) -> PixelF32
where
    F: FnMut(f32) -> f32,
{
    let radius = radius.max(0.5);
    let min_y = (y - radius).floor() as isize;
    let max_y = (y + radius).ceil() as isize;
    let min_x = (x - radius).floor() as isize;
    let max_x = (x + radius).ceil() as isize;

    let mut acc = WorkingAccumulator::default();
    for sy in min_y..=max_y {
        let wy = weight_fn(y - sy as f32);
        if wy == 0.0 {
            continue;
        }

        for sx in min_x..=max_x {
            let wx = weight_fn(x - sx as f32);
            if wx == 0.0 {
                continue;
            }

            let p = fetch_or_transparent(src, width, height, sx, sy);
            acc.add_sample(p, wx * wy, color_space);
        }
    }

    acc.finish(color_space, normalize)
}

fn cubic_weight(d: f32, a: f32) -> f32 {
    let x = d.abs();
    if x <= 1.0 {
        (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn mitchell_weight(d: f32, b: f32, c: f32) -> f32 {
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
    if x.abs() < 1.0e-6 {
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

fn is_hue_space(space: InterpolationColorSpace) -> bool {
    matches!(
        space,
        InterpolationColorSpace::Oklch
            | InterpolationColorSpace::Hsl
            | InterpolationColorSpace::Hsv
    )
}

fn convert_linear_to_working_pixel(
    space: InterpolationColorSpace,
    linear_rgba: [f32; 4],
) -> [f32; 4] {
    let lin = LinSrgb::new(linear_rgba[0], linear_rgba[1], linear_rgba[2]);
    match space {
        InterpolationColorSpace::LinearRgba => linear_rgba,
        InterpolationColorSpace::Srgb => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            [srgb.red, srgb.green, srgb.blue, linear_rgba[3]]
        }
        InterpolationColorSpace::Oklab => {
            let c: Oklab<f32> = Oklab::from_color(lin);
            [
                encode_signed(c.a, OKLAB_AB_MAX),
                encode_signed(c.b, OKLAB_AB_MAX),
                c.l,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Oklch => {
            let c: Oklch<f32> = Oklch::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                encode_pos(c.chroma, OKLCH_CHROMA_MAX),
                c.l,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Lab => {
            let c = Lab::from_color(lin);
            [
                encode_signed(c.a, LAB_AB_MAX),
                encode_signed(c.b, LAB_AB_MAX),
                c.l / LAB_L_MAX,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Yiq => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let r = srgb.red;
            let g = srgb.green;
            let b = srgb.blue;
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let i = 0.595716 * r - 0.274453 * g - 0.321263 * b;
            let q = 0.211456 * r - 0.522591 * g + 0.311135 * b;
            [
                encode_signed(i, YIQ_I_MAX),
                encode_signed(q, YIQ_Q_MAX),
                y,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Yuv => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let r = srgb.red;
            let g = srgb.green;
            let b = srgb.blue;
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let u = -0.14713 * r - 0.28886 * g + 0.436 * b;
            let v = 0.615 * r - 0.51499 * g - 0.10001 * b;
            [
                encode_signed(u, YUV_U_MAX),
                encode_signed(v, YUV_V_MAX),
                y,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::YCbCr => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let r = srgb.red * 255.0;
            let g = srgb.green * 255.0;
            let b = srgb.blue * 255.0;
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b;
            let cr = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
            [
                encode_pos(cb, YCBCR_MAX),
                encode_pos(cr, YCBCR_MAX),
                encode_pos(y, YCBCR_MAX),
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Hsl => {
            let c = Hsl::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                c.saturation,
                c.lightness,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Hsv => {
            let c = Hsv::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                c.saturation,
                c.value,
                linear_rgba[3],
            ]
        }
        InterpolationColorSpace::Cmyk => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let r = srgb.red;
            let g = srgb.green;
            let b = srgb.blue;
            let k = 1.0 - r.max(g).max(b);
            if k >= 1.0 - 1.0e-6 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                let inv = 1.0 / (1.0 - k);
                let c = (1.0 - r - k) * inv;
                let m = (1.0 - g - k) * inv;
                let y = (1.0 - b - k) * inv;
                [c, m, y, k]
            }
        }
    }
}

fn convert_working_to_linear_pixel(
    space: InterpolationColorSpace,
    working_rgba: [f32; 4],
    source_alpha: f32,
) -> [f32; 4] {
    match space {
        InterpolationColorSpace::LinearRgba => [
            working_rgba[0],
            working_rgba[1],
            working_rgba[2],
            source_alpha,
        ],
        InterpolationColorSpace::Srgb => {
            let lin = Srgb::new(
                sanitize_unit(working_rgba[0]),
                sanitize_unit(working_rgba[1]),
                sanitize_unit(working_rgba[2]),
            )
            .into_linear();
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Oklab => {
            let l = sanitize_unit(working_rgba[2]);
            let a = decode_signed(sanitize_unit(working_rgba[0]), OKLAB_AB_MAX);
            let b = decode_signed(sanitize_unit(working_rgba[1]), OKLAB_AB_MAX);
            let lin = LinSrgb::from_color(Oklab::new(l, a, b));
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Oklch => {
            let l = sanitize_unit(working_rgba[2]);
            let chroma = decode_pos(sanitize_unit(working_rgba[1]), OKLCH_CHROMA_MAX);
            let hue = wrap01(sanitize_unit(working_rgba[0])) * 360.0;
            let lin = LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)));
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Lab => {
            let l = sanitize_unit(working_rgba[2]) * LAB_L_MAX;
            let a = decode_signed(sanitize_unit(working_rgba[0]), LAB_AB_MAX);
            let b = decode_signed(sanitize_unit(working_rgba[1]), LAB_AB_MAX);
            let lin = LinSrgb::from_color(Lab::new(l, a, b));
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Yiq => {
            let y = sanitize_unit(working_rgba[2]);
            let i = decode_signed(sanitize_unit(working_rgba[0]), YIQ_I_MAX);
            let q = decode_signed(sanitize_unit(working_rgba[1]), YIQ_Q_MAX);
            let sr = y + 0.9563 * i + 0.6210 * q;
            let sg = y - 0.2721 * i - 0.6474 * q;
            let sb = y - 1.1070 * i + 1.7046 * q;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Yuv => {
            let y = sanitize_unit(working_rgba[2]);
            let u = decode_signed(sanitize_unit(working_rgba[0]), YUV_U_MAX);
            let v = decode_signed(sanitize_unit(working_rgba[1]), YUV_V_MAX);
            let sr = y + 1.13983 * v;
            let sg = y - 0.39465 * u - 0.58060 * v;
            let sb = y + 2.03211 * u;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::YCbCr => {
            let y = decode_pos(sanitize_unit(working_rgba[2]), YCBCR_MAX);
            let cb = decode_pos(sanitize_unit(working_rgba[0]), YCBCR_MAX);
            let cr = decode_pos(sanitize_unit(working_rgba[1]), YCBCR_MAX);
            let sr = (y + 1.402 * (cr - 128.0)) / 255.0;
            let sg = (y - 0.344136 * (cb - 128.0) - 0.714136 * (cr - 128.0)) / 255.0;
            let sb = (y + 1.772 * (cb - 128.0)) / 255.0;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Hsl => {
            let hue = wrap01(sanitize_unit(working_rgba[0])) * 360.0;
            let saturation = sanitize_unit(working_rgba[1]);
            let lightness = sanitize_unit(working_rgba[2]);
            let lin =
                LinSrgb::from_color(Hsl::new(RgbHue::from_degrees(hue), saturation, lightness));
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Hsv => {
            let hue = wrap01(sanitize_unit(working_rgba[0])) * 360.0;
            let saturation = sanitize_unit(working_rgba[1]);
            let value = sanitize_unit(working_rgba[2]);
            let lin = LinSrgb::from_color(Hsv::new(RgbHue::from_degrees(hue), saturation, value));
            [lin.red, lin.green, lin.blue, source_alpha]
        }
        InterpolationColorSpace::Cmyk => {
            let c = sanitize_unit(working_rgba[0]);
            let m = sanitize_unit(working_rgba[1]);
            let y = sanitize_unit(working_rgba[2]);
            let k = sanitize_unit(working_rgba[3]);
            let sr = (1.0 - c) * (1.0 - k);
            let sg = (1.0 - m) * (1.0 - k);
            let sb = (1.0 - y) * (1.0 - k);
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, source_alpha]
        }
    }
}

fn wrap01(x: f32) -> f32 {
    let mut v = x % 1.0;
    if v < 0.0 {
        v += 1.0;
    }
    v
}

fn encode_signed(value: f32, max_abs: f32) -> f32 {
    (value / (2.0 * max_abs)) + 0.5
}

fn decode_signed(channel: f32, max_abs: f32) -> f32 {
    (channel - 0.5) * (2.0 * max_abs)
}

fn encode_pos(value: f32, max: f32) -> f32 {
    value / max
}

fn decode_pos(channel: f32, max: f32) -> f32 {
    channel * max
}

fn sanitize_unit(mut v: f32) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    v.clamp(0.0, 1.0)
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
