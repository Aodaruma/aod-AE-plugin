#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

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

#[derive(Default)]
struct Plugin {}

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
        params.add(
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
        )?;

        params.add(
            Params::InputBSource,
            "Input B",
            PopupDef::setup(|d| {
                d.set_options(&["Value", "Layer"]);
                d.set_default(1);
            }),
        )?;

        params.add(Params::LayerB, "Layer B", LayerDef::new())?;

        params.add(
            Params::ValueB,
            "Value B",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-100000.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(-10.0);
                d.set_slider_max(10.0);
                d.set_default(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::InputCSource,
            "Input C",
            PopupDef::setup(|d| {
                d.set_options(&["Value", "Layer"]);
                d.set_default(1);
            }),
        )?;

        params.add(Params::LayerC, "Layer C", LayerDef::new())?;

        params.add(
            Params::ValueC,
            "Value C",
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
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
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
                Self::update_params_ui(&mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn update_params_ui(params: &mut Parameters<Params>) -> Result<(), Error> {
        let op = math_op_from_popup(params.get(Params::Operation)?.as_popup()?.value());
        let source_b =
            input_source_from_popup(params.get(Params::InputBSource)?.as_popup()?.value());
        let source_c =
            input_source_from_popup(params.get(Params::InputCSource)?.as_popup()?.value());

        let uses_b = operation_uses_b(op);
        let uses_c = operation_uses_c(op);
        let uses_eps = operation_uses_epsilon(op);

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

            let clamp_01 = clamp_result || !out_is_f32;

            let mut out_px = PixelF32 {
                red: sanitize_output(
                    apply_math(op, src_a.red, src_b.red, src_c.red, epsilon),
                    clamp_01,
                ),
                green: sanitize_output(
                    apply_math(op, src_a.green, src_b.green, src_c.green, epsilon),
                    clamp_01,
                ),
                blue: sanitize_output(
                    apply_math(op, src_a.blue, src_b.blue, src_c.blue, epsilon),
                    clamp_01,
                ),
                alpha: sanitize_output(
                    apply_math(op, src_a.alpha, src_b.alpha, src_c.alpha, epsilon),
                    clamp_01,
                ),
            };

            if use_original_alpha {
                let mut out_alpha = src_a.alpha;
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
