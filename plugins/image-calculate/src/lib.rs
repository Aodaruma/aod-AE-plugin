#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use color_art::{Color as ArtColor, ColorSpace as ArtColorSpace};
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, Lab, LinSrgb, Oklab, Oklch, Srgb};
use std::env;
use std::str::FromStr;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Operation,
    InputBSource,
    LayerB,
    ValueB,
    InputCSource,
    LayerC,
    ValueC,
    Epsilon,
    ClampResult,
    UseOriginalAlpha,
    Channel,
    CalculationColorSpace,
}

#[derive(Clone, Copy)]
enum InputSource {
    Value,
    Layer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Logarithm,
    SquareRoot,
    InverseSquareRoot,
    Absolute,
    Exponent,
    Minimum,
    Maximum,
    LessThan,
    GreaterThan,
    Sign,
    Compare,
    SmoothMinimum,
    SmoothMaximum,
    Round,
    Floor,
    Ceil,
    Truncate,
    Fraction,
    Modulo,
    Wrap,
    Snap,
    PingPong,
    Sine,
    Cosine,
    Tangent,
    Arcsine,
    Arccosine,
    Arctangent,
    Arctan2,
    HyperbolicSine,
    HyperbolicCosine,
    HyperbolicTangent,
    ToRadians,
    ToDegrees,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChannelMode {
    Rgba,
    Rgb,
    R,
    G,
    B,
    A,
    Luminance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CalcColorSpace {
    Rgb,
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

#[derive(Clone, Copy, Debug)]
struct EncodedColor {
    r: f32,
    g: f32,
    b: f32,
    a_override: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct CalcPixel {
    c0: f32,
    c1: f32,
    c2: f32,
    aux: f32,
    alpha: f32,
}

struct OperationUiInfo {
    expression: &'static str,
    b_label: &'static str,
    c_label: &'static str,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies Blender-style math operations to one or two input layers.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::Operation,
            "Operation",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Add",
                    "Subtract",
                    "Multiply",
                    "Divide",
                    "Power",
                    "Logarithm",
                    "Square Root",
                    "Inverse Square Root",
                    "Absolute",
                    "Exponent",
                    "Minimum",
                    "Maximum",
                    "Less Than",
                    "Greater Than",
                    "Sign",
                    "Compare",
                    "Smooth Minimum",
                    "Smooth Maximum",
                    "Round",
                    "Floor",
                    "Ceil",
                    "Truncate",
                    "Fraction",
                    "Modulo",
                    "Wrap",
                    "Snap",
                    "Ping-pong",
                    "Sine",
                    "Cosine",
                    "Tangent",
                    "Arcsine",
                    "Arccosine",
                    "Arctangent",
                    "Arctan2",
                    "Hyperbolic Sine",
                    "Hyperbolic Cosine",
                    "Hyperbolic Tangent",
                    "To Radians",
                    "To Degrees",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::InputBSource,
            "Input B (Operand)",
            PopupDef::setup(|d| {
                d.set_options(&["Value", "Layer"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(Params::LayerB, "Layer B (Operand)", LayerDef::new())?;

        params.add(
            Params::ValueB,
            "Value B (Operand)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-100000.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(-10.0);
                d.set_slider_max(10.0);
                d.set_default(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add_with_flags(
            Params::InputCSource,
            "Input C (Parameter)",
            PopupDef::setup(|d| {
                d.set_options(&["Value", "Layer"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(Params::LayerC, "Layer C (Parameter)", LayerDef::new())?;

        params.add(
            Params::ValueC,
            "Value C (Parameter)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-100000.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(-10.0);
                d.set_slider_max(10.0);
                d.set_default(0.1);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::Epsilon,
            "Epsilon",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.000000001);
                d.set_valid_max(1.0);
                d.set_slider_min(0.000001);
                d.set_slider_max(0.1);
                d.set_default(0.00001);
                d.set_precision(8);
            }),
        )?;

        params.add(
            Params::ClampResult,
            "Clamp Result 0..1",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::UseOriginalAlpha,
            "Use Original Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::Channel,
            "Channel",
            PopupDef::setup(|d| {
                d.set_options(&["RGBA", "RGB", "R", "G", "B", "A", "Luminance"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::CalculationColorSpace,
            "Calculation Color Space",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "RGB", "OKLAB", "OKLCH", "LAB", "YIQ", "YUV", "YCbCr", "HSL", "HSV", "CMYK",
                ]);
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
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    format!(
                        "AOD_ImageCalculate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImageCalculate")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_data, in_layer, out_data, out_layer, params)?;
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
                    self.do_render(in_data, in_layer, out_data, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let t = params.type_at(param_index);
                if t == Params::Operation || t == Params::InputBSource || t == Params::InputCSource
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
        let op = math_op_from_popup(params.get(Params::Operation)?.as_popup()?.value());
        let source_b =
            input_source_from_popup(params.get(Params::InputBSource)?.as_popup()?.value());
        let source_c =
            input_source_from_popup(params.get(Params::InputCSource)?.as_popup()?.value());
        let ui = operation_ui_info(op);

        let uses_b = operation_uses_b(op);
        let uses_c = operation_uses_c(op);
        let uses_eps = operation_uses_epsilon(op);

        Self::set_param_name(
            params,
            Params::Operation,
            &format!("Operation (f={})", ui.expression),
        )?;
        Self::set_param_name(
            params,
            Params::InputBSource,
            &format!("Input B ({})", ui.b_label),
        )?;
        Self::set_param_name(params, Params::LayerB, &format!("Layer B ({})", ui.b_label))?;
        Self::set_param_name(params, Params::ValueB, &format!("Value B ({})", ui.b_label))?;
        Self::set_param_name(
            params,
            Params::InputCSource,
            &format!("Input C ({})", ui.c_label),
        )?;
        Self::set_param_name(params, Params::LayerC, &format!("Layer C ({})", ui.c_label))?;
        Self::set_param_name(params, Params::ValueC, &format!("Value C ({})", ui.c_label))?;

        self.set_param_visible(in_data, params, Params::InputBSource, uses_b)?;
        self.set_param_visible(in_data, params, Params::LayerB, uses_b)?;
        self.set_param_visible(in_data, params, Params::ValueB, uses_b)?;
        Self::set_param_enabled(params, Params::InputBSource, uses_b)?;
        Self::set_param_enabled(
            params,
            Params::LayerB,
            uses_b && matches!(source_b, InputSource::Layer),
        )?;
        Self::set_param_enabled(
            params,
            Params::ValueB,
            uses_b && matches!(source_b, InputSource::Value),
        )?;

        self.set_param_visible(in_data, params, Params::InputCSource, uses_c)?;
        self.set_param_visible(in_data, params, Params::LayerC, uses_c)?;
        self.set_param_visible(in_data, params, Params::ValueC, uses_c)?;
        Self::set_param_enabled(params, Params::InputCSource, uses_c)?;
        Self::set_param_enabled(
            params,
            Params::LayerC,
            uses_c && matches!(source_c, InputSource::Layer),
        )?;
        Self::set_param_enabled(
            params,
            Params::ValueC,
            uses_c && matches!(source_c, InputSource::Value),
        )?;
        Self::set_param_enabled(params, Params::Epsilon, uses_eps)?;

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

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::DISABLED, !enabled)
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
        _in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let w = in_layer.width();
        let h = in_layer.height();
        if w == 0 || h == 0 {
            return Ok(());
        }

        let op = math_op_from_popup(params.get(Params::Operation)?.as_popup()?.value());
        let uses_b = operation_uses_b(op);
        let uses_c = operation_uses_c(op);
        let input_b_source =
            input_source_from_popup(params.get(Params::InputBSource)?.as_popup()?.value());
        let input_c_source =
            input_source_from_popup(params.get(Params::InputCSource)?.as_popup()?.value());
        let value_b = params.get(Params::ValueB)?.as_float_slider()?.value() as f32;
        let value_c = params.get(Params::ValueC)?.as_float_slider()?.value() as f32;
        let epsilon = params.get(Params::Epsilon)?.as_float_slider()?.value() as f32;
        let epsilon = epsilon.max(1.0e-12);
        let clamp_result = params.get(Params::ClampResult)?.as_checkbox()?.value();
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();
        let channel_mode =
            channel_mode_from_popup(params.get(Params::Channel)?.as_popup()?.value());
        let calc_color_space = calc_color_space_from_popup(
            params
                .get(Params::CalculationColorSpace)?
                .as_popup()?
                .value(),
        );

        let layer_b_checkout = params.checkout_at(Params::LayerB, None, None, None)?;
        let layer_b = layer_b_checkout.as_layer()?.value();
        let layer_b_world_type = layer_b.as_ref().map(|layer| layer.world_type());
        let use_layer_b =
            uses_b && matches!(input_b_source, InputSource::Layer) && layer_b.is_some();

        let layer_c_checkout = params.checkout_at(Params::LayerC, None, None, None)?;
        let layer_c = layer_c_checkout.as_layer()?.value();
        let layer_c_world_type = layer_c.as_ref().map(|layer| layer.world_type());
        let use_layer_c =
            uses_c && matches!(input_c_source, InputSource::Layer) && layer_c.is_some();

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        let progress_final = h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;

            let src_a = read_pixel_f32(&in_layer, in_world_type, x, y);

            let src_b = sample_input(
                x,
                y,
                use_layer_b,
                layer_b.as_ref(),
                layer_b_world_type,
                value_b,
            );
            let src_c = sample_input(
                x,
                y,
                use_layer_c,
                layer_c.as_ref(),
                layer_c_world_type,
                value_c,
            );

            let calc_a = encode_calc_pixel(calc_color_space, src_a);
            let calc_b = if use_layer_b {
                encode_calc_pixel(calc_color_space, src_b)
            } else {
                calc_pixel_from_scalar(value_b)
            };
            let calc_c = if use_layer_c {
                encode_calc_pixel(calc_color_space, src_c)
            } else {
                calc_pixel_from_scalar(value_c)
            };

            let mut out_calc = calc_a;
            let mut color_changed = false;
            let mut grayscale_linear = None;

            match channel_mode {
                ChannelMode::Rgba => {
                    out_calc.c0 = apply_math(op, calc_a.c0, calc_b.c0, calc_c.c0, epsilon);
                    out_calc.c1 = apply_math(op, calc_a.c1, calc_b.c1, calc_c.c1, epsilon);
                    out_calc.c2 = apply_math(op, calc_a.c2, calc_b.c2, calc_c.c2, epsilon);
                    out_calc.alpha =
                        apply_math(op, calc_a.alpha, calc_b.alpha, calc_c.alpha, epsilon);
                    color_changed = true;
                }
                ChannelMode::Rgb => {
                    out_calc.c0 = apply_math(op, calc_a.c0, calc_b.c0, calc_c.c0, epsilon);
                    out_calc.c1 = apply_math(op, calc_a.c1, calc_b.c1, calc_c.c1, epsilon);
                    out_calc.c2 = apply_math(op, calc_a.c2, calc_b.c2, calc_c.c2, epsilon);
                    color_changed = true;
                }
                ChannelMode::R => {
                    out_calc.c0 = apply_math(op, calc_a.c0, calc_b.c0, calc_c.c0, epsilon);
                    color_changed = true;
                }
                ChannelMode::G => {
                    out_calc.c1 = apply_math(op, calc_a.c1, calc_b.c1, calc_c.c1, epsilon);
                    color_changed = true;
                }
                ChannelMode::B => {
                    out_calc.c2 = apply_math(op, calc_a.c2, calc_b.c2, calc_c.c2, epsilon);
                    color_changed = true;
                }
                ChannelMode::A => {
                    out_calc.alpha =
                        apply_math(op, calc_a.alpha, calc_b.alpha, calc_c.alpha, epsilon);
                }
                ChannelMode::Luminance => {
                    let lum_a = luminance_from_pixel(src_a);
                    let lum_b = if use_layer_b {
                        luminance_from_pixel(src_b)
                    } else {
                        value_b
                    };
                    let lum_c = if use_layer_c {
                        luminance_from_pixel(src_c)
                    } else {
                        value_c
                    };
                    grayscale_linear = Some(apply_math(op, lum_a, lum_b, lum_c, epsilon));
                    color_changed = true;
                }
            }

            let out_rgb = if let Some(luma) = grayscale_linear {
                let luma = sanitize_non_finite(luma);
                let encoded =
                    encode_from_linear(CalcColorSpace::Rgb, LinSrgb::new(luma, luma, luma));
                [encoded.r, encoded.g, encoded.b]
            } else if color_changed {
                out_calc.c0 = sanitize_non_finite(out_calc.c0);
                out_calc.c1 = sanitize_non_finite(out_calc.c1);
                out_calc.c2 = sanitize_non_finite(out_calc.c2);
                out_calc.aux = sanitize_non_finite(out_calc.aux);
                let lin = decode_to_linear(
                    calc_color_space,
                    out_calc.c0,
                    out_calc.c1,
                    out_calc.c2,
                    out_calc.aux,
                );
                let encoded = encode_from_linear(CalcColorSpace::Rgb, lin);
                [encoded.r, encoded.g, encoded.b]
            } else {
                [src_a.red, src_a.green, src_a.blue]
            };

            let mut out_alpha = sanitize_non_finite(out_calc.alpha);
            let clamp_01 = clamp_result || !out_is_f32;

            let mut out_px = PixelF32 {
                red: sanitize_output(out_rgb[0], clamp_01),
                green: sanitize_output(out_rgb[1], clamp_01),
                blue: sanitize_output(out_rgb[2], clamp_01),
                alpha: sanitize_output(out_alpha, clamp_01),
            };

            if use_original_alpha {
                out_alpha = src_a.alpha;
                if !out_alpha.is_finite() {
                    out_alpha = 0.0;
                }
                out_alpha = out_alpha.clamp(0.0, 1.0);
                out_px.red *= out_alpha;
                out_px.green *= out_alpha;
                out_px.blue *= out_alpha;
                out_px.alpha = out_alpha;
            }

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                    dst.set_from_f32(out_px);
                }
            }

            Ok(())
        })?;

        Ok(())
    }
}

fn input_source_from_popup(value: i32) -> InputSource {
    match value {
        2 => InputSource::Layer,
        _ => InputSource::Value,
    }
}

fn channel_mode_from_popup(value: i32) -> ChannelMode {
    match value {
        2 => ChannelMode::Rgb,
        3 => ChannelMode::R,
        4 => ChannelMode::G,
        5 => ChannelMode::B,
        6 => ChannelMode::A,
        7 => ChannelMode::Luminance,
        _ => ChannelMode::Rgba,
    }
}

fn calc_color_space_from_popup(value: i32) -> CalcColorSpace {
    match value {
        2 => CalcColorSpace::Oklab,
        3 => CalcColorSpace::Oklch,
        4 => CalcColorSpace::Lab,
        5 => CalcColorSpace::Yiq,
        6 => CalcColorSpace::Yuv,
        7 => CalcColorSpace::YCbCr,
        8 => CalcColorSpace::Hsl,
        9 => CalcColorSpace::Hsv,
        10 => CalcColorSpace::Cmyk,
        _ => CalcColorSpace::Rgb,
    }
}

fn math_op_from_popup(value: i32) -> MathOp {
    match value {
        2 => MathOp::Subtract,
        3 => MathOp::Multiply,
        4 => MathOp::Divide,
        5 => MathOp::Power,
        6 => MathOp::Logarithm,
        7 => MathOp::SquareRoot,
        8 => MathOp::InverseSquareRoot,
        9 => MathOp::Absolute,
        10 => MathOp::Exponent,
        11 => MathOp::Minimum,
        12 => MathOp::Maximum,
        13 => MathOp::LessThan,
        14 => MathOp::GreaterThan,
        15 => MathOp::Sign,
        16 => MathOp::Compare,
        17 => MathOp::SmoothMinimum,
        18 => MathOp::SmoothMaximum,
        19 => MathOp::Round,
        20 => MathOp::Floor,
        21 => MathOp::Ceil,
        22 => MathOp::Truncate,
        23 => MathOp::Fraction,
        24 => MathOp::Modulo,
        25 => MathOp::Wrap,
        26 => MathOp::Snap,
        27 => MathOp::PingPong,
        28 => MathOp::Sine,
        29 => MathOp::Cosine,
        30 => MathOp::Tangent,
        31 => MathOp::Arcsine,
        32 => MathOp::Arccosine,
        33 => MathOp::Arctangent,
        34 => MathOp::Arctan2,
        35 => MathOp::HyperbolicSine,
        36 => MathOp::HyperbolicCosine,
        37 => MathOp::HyperbolicTangent,
        38 => MathOp::ToRadians,
        39 => MathOp::ToDegrees,
        _ => MathOp::Add,
    }
}

fn operation_ui_info(op: MathOp) -> OperationUiInfo {
    match op {
        MathOp::Add => OperationUiInfo {
            expression: "A+B",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Subtract => OperationUiInfo {
            expression: "A-B",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Multiply => OperationUiInfo {
            expression: "A*B",
            b_label: "Factor",
            c_label: "Parameter",
        },
        MathOp::Divide => OperationUiInfo {
            expression: "A/B",
            b_label: "Divisor",
            c_label: "Parameter",
        },
        MathOp::Power => OperationUiInfo {
            expression: "pow(A,B)",
            b_label: "Exponent",
            c_label: "Parameter",
        },
        MathOp::Logarithm => OperationUiInfo {
            expression: "log_B(A)",
            b_label: "Base",
            c_label: "Parameter",
        },
        MathOp::SquareRoot => OperationUiInfo {
            expression: "sqrt(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::InverseSquareRoot => OperationUiInfo {
            expression: "1/sqrt(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Absolute => OperationUiInfo {
            expression: "abs(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Exponent => OperationUiInfo {
            expression: "exp(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Minimum => OperationUiInfo {
            expression: "min(A,B)",
            b_label: "Other Value",
            c_label: "Parameter",
        },
        MathOp::Maximum => OperationUiInfo {
            expression: "max(A,B)",
            b_label: "Other Value",
            c_label: "Parameter",
        },
        MathOp::LessThan => OperationUiInfo {
            expression: "A<B ? 1:0",
            b_label: "Threshold",
            c_label: "Parameter",
        },
        MathOp::GreaterThan => OperationUiInfo {
            expression: "A>B ? 1:0",
            b_label: "Threshold",
            c_label: "Parameter",
        },
        MathOp::Sign => OperationUiInfo {
            expression: "sign(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Compare => OperationUiInfo {
            expression: "|A-B|<=C ?1:0",
            b_label: "Compare To",
            c_label: "Tolerance",
        },
        MathOp::SmoothMinimum => OperationUiInfo {
            expression: "smin(A,B,C)",
            b_label: "Other Value",
            c_label: "Smoothness",
        },
        MathOp::SmoothMaximum => OperationUiInfo {
            expression: "smax(A,B,C)",
            b_label: "Other Value",
            c_label: "Smoothness",
        },
        MathOp::Round => OperationUiInfo {
            expression: "round(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Floor => OperationUiInfo {
            expression: "floor(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Ceil => OperationUiInfo {
            expression: "ceil(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Truncate => OperationUiInfo {
            expression: "trunc(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Fraction => OperationUiInfo {
            expression: "fract(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Modulo => OperationUiInfo {
            expression: "mod(A,B)",
            b_label: "Divisor",
            c_label: "Parameter",
        },
        MathOp::Wrap => OperationUiInfo {
            expression: "wrap(A,B,C)",
            b_label: "Range Min",
            c_label: "Range Max",
        },
        MathOp::Snap => OperationUiInfo {
            expression: "floor(A/B)*B",
            b_label: "Step",
            c_label: "Parameter",
        },
        MathOp::PingPong => OperationUiInfo {
            expression: "pingpong(A,B)",
            b_label: "Scale",
            c_label: "Parameter",
        },
        MathOp::Sine => OperationUiInfo {
            expression: "sin(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Cosine => OperationUiInfo {
            expression: "cos(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Tangent => OperationUiInfo {
            expression: "tan(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Arcsine => OperationUiInfo {
            expression: "asin(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Arccosine => OperationUiInfo {
            expression: "acos(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Arctangent => OperationUiInfo {
            expression: "atan(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::Arctan2 => OperationUiInfo {
            expression: "atan2(A,B)",
            b_label: "X",
            c_label: "Parameter",
        },
        MathOp::HyperbolicSine => OperationUiInfo {
            expression: "sinh(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::HyperbolicCosine => OperationUiInfo {
            expression: "cosh(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::HyperbolicTangent => OperationUiInfo {
            expression: "tanh(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::ToRadians => OperationUiInfo {
            expression: "radians(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
        MathOp::ToDegrees => OperationUiInfo {
            expression: "degrees(A)",
            b_label: "Operand",
            c_label: "Parameter",
        },
    }
}

fn operation_uses_b(op: MathOp) -> bool {
    !matches!(
        op,
        MathOp::SquareRoot
            | MathOp::InverseSquareRoot
            | MathOp::Absolute
            | MathOp::Exponent
            | MathOp::Sign
            | MathOp::Round
            | MathOp::Floor
            | MathOp::Ceil
            | MathOp::Truncate
            | MathOp::Fraction
            | MathOp::Sine
            | MathOp::Cosine
            | MathOp::Tangent
            | MathOp::Arcsine
            | MathOp::Arccosine
            | MathOp::Arctangent
            | MathOp::HyperbolicSine
            | MathOp::HyperbolicCosine
            | MathOp::HyperbolicTangent
            | MathOp::ToRadians
            | MathOp::ToDegrees
    )
}

fn operation_uses_c(op: MathOp) -> bool {
    matches!(
        op,
        MathOp::Compare | MathOp::SmoothMinimum | MathOp::SmoothMaximum | MathOp::Wrap
    )
}

fn operation_uses_epsilon(op: MathOp) -> bool {
    matches!(
        op,
        MathOp::Divide
            | MathOp::Power
            | MathOp::Logarithm
            | MathOp::InverseSquareRoot
            | MathOp::Compare
            | MathOp::Modulo
            | MathOp::Wrap
            | MathOp::Snap
            | MathOp::PingPong
    )
}

fn apply_math(op: MathOp, a: f32, b: f32, c: f32, eps: f32) -> f32 {
    match op {
        MathOp::Add => a + b,
        MathOp::Subtract => a - b,
        MathOp::Multiply => a * b,
        MathOp::Divide => {
            if b.abs() <= eps {
                0.0
            } else {
                a / b
            }
        }
        MathOp::Power => safe_pow(a, b, eps),
        MathOp::Logarithm => safe_log(a, b, eps),
        MathOp::SquareRoot => a.max(0.0).sqrt(),
        MathOp::InverseSquareRoot => {
            if a <= eps {
                0.0
            } else {
                a.sqrt().recip()
            }
        }
        MathOp::Absolute => a.abs(),
        MathOp::Exponent => a.exp(),
        MathOp::Minimum => a.min(b),
        MathOp::Maximum => a.max(b),
        MathOp::LessThan => {
            if a < b {
                1.0
            } else {
                0.0
            }
        }
        MathOp::GreaterThan => {
            if a > b {
                1.0
            } else {
                0.0
            }
        }
        MathOp::Sign => {
            if a > eps {
                1.0
            } else if a < -eps {
                -1.0
            } else {
                0.0
            }
        }
        MathOp::Compare => {
            if (a - b).abs() <= c.abs().max(eps) {
                1.0
            } else {
                0.0
            }
        }
        MathOp::SmoothMinimum => smooth_min(a, b, c.abs().max(eps)),
        MathOp::SmoothMaximum => smooth_max(a, b, c.abs().max(eps)),
        MathOp::Round => a.round(),
        MathOp::Floor => a.floor(),
        MathOp::Ceil => a.ceil(),
        MathOp::Truncate => a.trunc(),
        MathOp::Fraction => a.fract(),
        MathOp::Modulo => modulo_floor(a, b, eps),
        MathOp::Wrap => wrap_range(a, b, c, eps),
        MathOp::Snap => snap_value(a, b, eps),
        MathOp::PingPong => ping_pong(a, b, eps),
        MathOp::Sine => a.sin(),
        MathOp::Cosine => a.cos(),
        MathOp::Tangent => a.tan(),
        MathOp::Arcsine => a.clamp(-1.0, 1.0).asin(),
        MathOp::Arccosine => a.clamp(-1.0, 1.0).acos(),
        MathOp::Arctangent => a.atan(),
        MathOp::Arctan2 => a.atan2(b),
        MathOp::HyperbolicSine => a.sinh(),
        MathOp::HyperbolicCosine => a.cosh(),
        MathOp::HyperbolicTangent => a.tanh(),
        MathOp::ToRadians => a.to_radians(),
        MathOp::ToDegrees => a.to_degrees(),
    }
}

fn safe_pow(a: f32, b: f32, eps: f32) -> f32 {
    if a < 0.0 {
        let nearest = b.round();
        if (b - nearest).abs() > eps {
            return 0.0;
        }
    }
    a.powf(b)
}

fn safe_log(a: f32, b: f32, eps: f32) -> f32 {
    if a <= 0.0 || b <= 0.0 || (b - 1.0).abs() <= eps {
        return 0.0;
    }
    a.ln() / b.ln()
}

fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    (b + (a - b) * h) - k * h * (1.0 - h)
}

fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    (b + (a - b) * h) + k * h * (1.0 - h)
}

fn modulo_floor(a: f32, b: f32, eps: f32) -> f32 {
    if b.abs() <= eps {
        return 0.0;
    }
    a - (a / b).floor() * b
}

fn wrap_range(v: f32, b: f32, c: f32, eps: f32) -> f32 {
    let min_v = b.min(c);
    let max_v = b.max(c);
    let range = max_v - min_v;
    if range.abs() <= eps {
        return min_v;
    }
    (v - min_v).rem_euclid(range) + min_v
}

fn snap_value(v: f32, step: f32, eps: f32) -> f32 {
    if step.abs() <= eps {
        return 0.0;
    }
    (v / step).floor() * step
}

fn ping_pong(v: f32, scale: f32, eps: f32) -> f32 {
    let scale = scale.abs();
    if scale <= eps {
        return 0.0;
    }
    let t = (v / scale).rem_euclid(2.0);
    if t <= 1.0 {
        t * scale
    } else {
        (2.0 - t) * scale
    }
}

fn fill_pixel(v: f32) -> PixelF32 {
    PixelF32 {
        red: v,
        green: v,
        blue: v,
        alpha: v,
    }
}

fn sanitize_output(mut v: f32, clamp_01: bool) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    if clamp_01 {
        v = v.clamp(0.0, 1.0);
    }
    v
}

#[inline]
fn sanitize_non_finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[inline]
fn wrap01(x: f32) -> f32 {
    let mut v = x % 1.0;
    if v < 0.0 {
        v += 1.0;
    }
    v
}

#[inline]
fn encode_signed(value: f32, max_abs: f32) -> f32 {
    (value / (2.0 * max_abs)) + 0.5
}

#[inline]
fn decode_signed(channel: f32, max_abs: f32) -> f32 {
    (channel - 0.5) * (2.0 * max_abs)
}

#[inline]
fn encode_pos(value: f32, max: f32) -> f32 {
    value / max
}

#[inline]
fn decode_pos(channel: f32, max: f32) -> f32 {
    channel * max
}

fn calc_pixel_from_scalar(value: f32) -> CalcPixel {
    let value = sanitize_non_finite(value);
    CalcPixel {
        c0: value,
        c1: value,
        c2: value,
        aux: value,
        alpha: value,
    }
}

fn encode_calc_pixel(space: CalcColorSpace, px: PixelF32) -> CalcPixel {
    let lin = decode_to_linear(CalcColorSpace::Rgb, px.red, px.green, px.blue, px.alpha);
    let encoded = encode_from_linear(space, lin);
    CalcPixel {
        c0: encoded.r,
        c1: encoded.g,
        c2: encoded.b,
        aux: encoded.a_override.unwrap_or(0.0),
        alpha: px.alpha,
    }
}

fn luminance_from_pixel(px: PixelF32) -> f32 {
    let lin = decode_to_linear(CalcColorSpace::Rgb, px.red, px.green, px.blue, px.alpha);
    sanitize_non_finite(0.2126 * lin.red + 0.7152 * lin.green + 0.0722 * lin.blue)
}

fn decode_to_linear(space: CalcColorSpace, r: f32, g: f32, b: f32, a: f32) -> LinSrgb<f32> {
    const OKLAB_AB_MAX: f32 = 0.5;
    const OKLCH_CHROMA_MAX: f32 = 0.4;
    const LAB_L_MAX: f32 = 100.0;
    const LAB_AB_MAX: f32 = 128.0;
    const YIQ_I_MAX: f32 = 0.5957;
    const YIQ_Q_MAX: f32 = 0.5226;
    const YUV_U_MAX: f32 = 0.436;
    const YUV_V_MAX: f32 = 0.615;
    const YCBCR_MAX: f32 = 255.0;

    match space {
        CalcColorSpace::Rgb => Srgb::new(r, g, b).into_linear(),
        CalcColorSpace::Oklab => {
            let l = b;
            let a = decode_signed(r, OKLAB_AB_MAX);
            let bb = decode_signed(g, OKLAB_AB_MAX);
            LinSrgb::from_color(Oklab::new(l, a, bb))
        }
        CalcColorSpace::Oklch => {
            let l = b;
            let chroma = decode_pos(g, OKLCH_CHROMA_MAX);
            let hue = wrap01(r) * 360.0;
            LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)))
        }
        CalcColorSpace::Lab => {
            let l = b * LAB_L_MAX;
            let a = decode_signed(r, LAB_AB_MAX);
            let bb = decode_signed(g, LAB_AB_MAX);
            LinSrgb::from_color(Lab::new(l, a, bb))
        }
        CalcColorSpace::Yiq => {
            let y = b;
            let i = decode_signed(r, YIQ_I_MAX);
            let q = decode_signed(g, YIQ_Q_MAX);
            let spec = format!("yiq({:.6},{:.6},{:.6})", y, i, q);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::Yuv => {
            let y = b;
            let u = decode_signed(r, YUV_U_MAX);
            let v = decode_signed(g, YUV_V_MAX);
            let spec = format!("yuv({:.6},{:.6},{:.6})", y, u, v);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::YCbCr => {
            let y = decode_pos(b, YCBCR_MAX);
            let cb = decode_pos(r, YCBCR_MAX);
            let cr = decode_pos(g, YCBCR_MAX);
            let spec = format!("ycbcr({:.3},{:.3},{:.3})", y, cb, cr);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::Hsl => {
            let hue = wrap01(r) * 360.0;
            let saturation = g;
            let lightness = b;
            LinSrgb::from_color(Hsl::new(RgbHue::from_degrees(hue), saturation, lightness))
        }
        CalcColorSpace::Hsv => {
            let hue = wrap01(r) * 360.0;
            let saturation = g;
            let value = b;
            LinSrgb::from_color(Hsv::new(RgbHue::from_degrees(hue), saturation, value))
        }
        CalcColorSpace::Cmyk => {
            let c = r as f64;
            let m = g as f64;
            let y = b as f64;
            let k = a as f64;
            match ArtColor::from_cmyk(c, m, y, k) {
                Ok(color) => {
                    let rgb = color.vec_of(ArtColorSpace::RGB);
                    Srgb::new(
                        (rgb[0] / 255.0) as f32,
                        (rgb[1] / 255.0) as f32,
                        (rgb[2] / 255.0) as f32,
                    )
                    .into_linear()
                }
                Err(_) => Srgb::new(r, g, b).into_linear(),
            }
        }
    }
}

fn encode_from_linear(space: CalcColorSpace, lin: LinSrgb<f32>) -> EncodedColor {
    const OKLAB_AB_MAX: f32 = 0.5;
    const OKLCH_CHROMA_MAX: f32 = 0.4;
    const LAB_L_MAX: f32 = 100.0;
    const LAB_AB_MAX: f32 = 128.0;
    const YIQ_I_MAX: f32 = 0.5957;
    const YIQ_Q_MAX: f32 = 0.5226;
    const YUV_U_MAX: f32 = 0.436;
    const YUV_V_MAX: f32 = 0.615;
    const YCBCR_MAX: f32 = 255.0;

    match space {
        CalcColorSpace::Rgb => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            EncodedColor {
                r: srgb.red,
                g: srgb.green,
                b: srgb.blue,
                a_override: None,
            }
        }
        CalcColorSpace::Oklab => {
            let c: Oklab<f32> = Oklab::from_color(lin);
            EncodedColor {
                r: encode_signed(c.a, OKLAB_AB_MAX),
                g: encode_signed(c.b, OKLAB_AB_MAX),
                b: c.l,
                a_override: None,
            }
        }
        CalcColorSpace::Oklch => {
            let c: Oklch<f32> = Oklch::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: encode_pos(c.chroma, OKLCH_CHROMA_MAX),
                b: c.l,
                a_override: None,
            }
        }
        CalcColorSpace::Lab => {
            let c = Lab::from_color(lin);
            EncodedColor {
                r: encode_signed(c.a, LAB_AB_MAX),
                g: encode_signed(c.b, LAB_AB_MAX),
                b: c.l / LAB_L_MAX,
                a_override: None,
            }
        }
        CalcColorSpace::Yiq => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let yiq = art.vec_of(ArtColorSpace::YIQ);
            EncodedColor {
                r: encode_signed(yiq[1] as f32, YIQ_I_MAX),
                g: encode_signed(yiq[2] as f32, YIQ_Q_MAX),
                b: yiq[0] as f32,
                a_override: None,
            }
        }
        CalcColorSpace::Yuv => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let yuv = art.vec_of(ArtColorSpace::YUV);
            EncodedColor {
                r: encode_signed(yuv[1] as f32, YUV_U_MAX),
                g: encode_signed(yuv[2] as f32, YUV_V_MAX),
                b: yuv[0] as f32,
                a_override: None,
            }
        }
        CalcColorSpace::YCbCr => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let ycbcr = art.vec_of(ArtColorSpace::YCbCr);
            EncodedColor {
                r: encode_pos(ycbcr[1] as f32, YCBCR_MAX),
                g: encode_pos(ycbcr[2] as f32, YCBCR_MAX),
                b: encode_pos(ycbcr[0] as f32, YCBCR_MAX),
                a_override: None,
            }
        }
        CalcColorSpace::Hsl => {
            let c = Hsl::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: c.saturation,
                b: c.lightness,
                a_override: None,
            }
        }
        CalcColorSpace::Hsv => {
            let c = Hsv::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: c.saturation,
                b: c.value,
                a_override: None,
            }
        }
        CalcColorSpace::Cmyk => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let cmyk = art.vec_of(ArtColorSpace::CMYK);
            EncodedColor {
                r: cmyk[0] as f32,
                g: cmyk[1] as f32,
                b: cmyk[2] as f32,
                a_override: Some(cmyk[3] as f32),
            }
        }
    }
}

fn sample_input(
    x: usize,
    y: usize,
    use_layer: bool,
    layer: Option<&Layer>,
    world_type: Option<ae::aegp::WorldType>,
    value: f32,
) -> PixelF32 {
    if use_layer && let (Some(layer), Some(world_type)) = (layer, world_type) {
        let bx = x.min(layer.width().saturating_sub(1));
        let by = y.min(layer.height().saturating_sub(1));
        return read_pixel_f32(layer, world_type, bx, by);
    }
    fill_pixel(value)
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}
