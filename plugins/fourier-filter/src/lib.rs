#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

#[cfg(feature = "gpu_wgpu")]
use std::sync::{Arc, OnceLock};

use ae::pf::*;
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, Lab, LinSrgb, Oklab, Oklch, Srgb};
use utils::ToPixel;

#[cfg(feature = "gpu_wgpu")]
use utils::spectral_wgpu::SpectralWgpuContext;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    FilterGroupStart,
    FilterGroupEnd,
    MaskGroupStart,
    MaskGroupEnd,
    OutputGroupStart,
    OutputGroupEnd,
    FilterMode,
    ColorSpace,
    ProcessWidth,
    ProcessHeight,
    TargetChannels,
    SplitXY,
    ParamA,
    ParamAX,
    ParamAY,
    ParamB,
    ParamBX,
    ParamBY,
    ParamC,
    ParamCX,
    ParamCY,
    ParamD,
    ParamDX,
    ParamDY,
    VowelType,
    MaskLayer,
    MaskChannel,
    MaskStrength,
    InvertMask,
    SwapMaskQuadrants,
    Mix,
    OutputGain,
    UseGpu,
    ClampOutput,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterMode {
    LowPass,
    HighPass,
    BandPass,
    Dj,
    Comb,
    Morph,
    Resampling,
    Vowel,
    Notch,
    NotchLowPass,
    LayerMask,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetChannels {
    Rgba,
    Rgb,
    Alpha,
    R,
    G,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskChannel {
    Luminance,
    Rgba,
    Rgb,
    Alpha,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VowelType {
    A,
    E,
    I,
    O,
    U,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterColorSpace {
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

struct ModeUiProfile {
    param_a_name: &'static str,
    param_b_name: &'static str,
    param_c_name: &'static str,
    param_d_name: &'static str,
    param_a_range: Option<FloatRange>,
    param_b_range: Option<FloatRange>,
    param_c_range: Option<FloatRange>,
    param_d_range: Option<FloatRange>,
    show_param_a: bool,
    show_param_b: bool,
    show_param_c: bool,
    show_param_d: bool,
    show_vowel_type: bool,
    show_mask_controls: bool,
}

#[derive(Clone, Copy)]
struct FloatRange {
    valid_min: f32,
    valid_max: f32,
    slider_min: f32,
    slider_max: f32,
}

#[derive(Clone, Copy)]
struct FilterControls {
    mode: FilterMode,
    target: TargetChannels,
    a: DirectionalParam,
    b: DirectionalParam,
    c: DirectionalParam,
    d: DirectionalParam,
    vowel: VowelType,
    mix: f32,
    output_gain: f32,
}

#[derive(Clone, Copy)]
struct DirectionalParam {
    x: f32,
    y: f32,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies FFT-domain filters and mask-driven spectral shaping with dynamic controls.";

#[cfg(feature = "gpu_wgpu")]
static WGPU_CONTEXT: OnceLock<Result<Arc<SpectralWgpuContext>, ()>> = OnceLock::new();

#[cfg(feature = "gpu_wgpu")]
fn wgpu_context() -> Option<Arc<SpectralWgpuContext>> {
    match WGPU_CONTEXT.get_or_init(|| SpectralWgpuContext::new().map(Arc::new).map_err(|_| ())) {
        Ok(ctx) => Some(ctx.clone()),
        Err(_) => None,
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
            Params::FilterMode,
            "Filter Mode",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Low-pass",
                    "High-pass",
                    "Band-pass",
                    "Notch",
                    "DJ",
                    "Comb",
                    "Morph",
                    "Resampling",
                    "Vowel",
                    "Notch + LP",
                    "Layer Mask",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::ColorSpace,
            "Color Space",
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

        params.add(
            Params::ProcessWidth,
            "Process Width (0=Source)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(8192);
                d.set_slider_min(0);
                d.set_slider_max(4096);
                d.set_default(0);
            }),
        )?;

        params.add(
            Params::ProcessHeight,
            "Process Height (0=Source)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(8192);
                d.set_slider_min(0);
                d.set_slider_max(4096);
                d.set_default(0);
            }),
        )?;

        params.add(
            Params::TargetChannels,
            "Target Channels",
            PopupDef::setup(|d| {
                d.set_options(&["RGBA", "RGB", "Alpha", "R", "G", "B"]);
                d.set_default(1);
            }),
        )?;

        params.add_group(
            Params::FilterGroupStart,
            Params::FilterGroupEnd,
            "Filter Parameters",
            false,
            |params| {
                params.add_with_flags(
                    Params::SplitXY,
                    "Use X/Y Split",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::ParamA,
                    "Param A",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.25);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamB,
                    "Param B",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamC,
                    "Param C",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(4.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(4.0);
                        d.set_default(0.50);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::ParamD,
                    "Param D",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamAX,
                    "Param A X",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.25);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamBX,
                    "Param B X",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::ParamCX,
                    "Param C X",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(4.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(4.0);
                        d.set_default(0.50);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamDX,
                    "Param D X",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamAY,
                    "Param A Y",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.25);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::ParamBY,
                    "Param B Y",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamCY,
                    "Param C Y",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(4.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(4.0);
                        d.set_default(0.50);
                        d.set_precision(4);
                    }),
                )?;
                params.add(
                    Params::ParamDY,
                    "Param D Y",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(0.5);
                        d.set_slider_min(0.0);
                        d.set_slider_max(0.5);
                        d.set_default(0.10);
                        d.set_precision(4);
                    }),
                )?;

                params.add_with_flags(
                    Params::VowelType,
                    "Vowel Type",
                    PopupDef::setup(|d| {
                        d.set_options(&["A", "E", "I", "O", "U"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::MaskGroupStart,
            Params::MaskGroupEnd,
            "Mask (Layer Mask Mode)",
            false,
            |params| {
                params.add(Params::MaskLayer, "Mask Layer", LayerDef::new())?;

                params.add(
                    Params::MaskChannel,
                    "Mask Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luminance", "RGBA", "RGB", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::MaskStrength,
                    "Mask Strength",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::InvertMask,
                    "Invert Mask",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                )?;

                params.add(
                    Params::SwapMaskQuadrants,
                    "Swap Mask Quadrants",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::OutputGroupStart,
            Params::OutputGroupEnd,
            "Output",
            false,
            |params| {
                params.add(
                    Params::Mix,
                    "Mix",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::OutputGain,
                    "Output Gain",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(8.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(4.0);
                        d.set_default(1.0);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::UseGpu,
                    "Use GPU if available",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                params.add(
                    Params::ClampOutput,
                    "Clamp Output 0..1",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                Ok(())
            },
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
                        "AOD_FourierFilter - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_FourierFilter")
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
                let changed = params.type_at(param_index);
                if changed == Params::FilterMode || changed == Params::SplitXY {
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
        let mode = filter_mode_from_popup(params.get(Params::FilterMode)?.as_popup()?.value());
        let ui = mode_ui_profile(mode);
        let split_xy = params.get(Params::SplitXY)?.as_checkbox()?.value();
        let show_split_toggle =
            ui.show_param_a || ui.show_param_b || ui.show_param_c || ui.show_param_d;

        self.set_param_visible(in_data, params, Params::SplitXY, show_split_toggle)?;
        Self::set_param_enabled(params, Params::SplitXY, show_split_toggle)?;

        Self::set_param_name(params, Params::ParamA, ui.param_a_name)?;
        Self::set_param_name(params, Params::ParamB, ui.param_b_name)?;
        Self::set_param_name(params, Params::ParamC, ui.param_c_name)?;
        Self::set_param_name(params, Params::ParamD, ui.param_d_name)?;
        Self::set_float_param_range(params, Params::ParamA, ui.param_a_range)?;
        Self::set_float_param_range(params, Params::ParamB, ui.param_b_range)?;
        Self::set_float_param_range(params, Params::ParamC, ui.param_c_range)?;
        Self::set_float_param_range(params, Params::ParamD, ui.param_d_range)?;

        let param_ax_name = format!("{} X", ui.param_a_name);
        let param_ay_name = format!("{} Y", ui.param_a_name);
        let param_bx_name = format!("{} X", ui.param_b_name);
        let param_by_name = format!("{} Y", ui.param_b_name);
        let param_cx_name = format!("{} X", ui.param_c_name);
        let param_cy_name = format!("{} Y", ui.param_c_name);
        let param_dx_name = format!("{} X", ui.param_d_name);
        let param_dy_name = format!("{} Y", ui.param_d_name);
        Self::set_param_name(params, Params::ParamAX, &param_ax_name)?;
        Self::set_param_name(params, Params::ParamAY, &param_ay_name)?;
        Self::set_param_name(params, Params::ParamBX, &param_bx_name)?;
        Self::set_param_name(params, Params::ParamBY, &param_by_name)?;
        Self::set_param_name(params, Params::ParamCX, &param_cx_name)?;
        Self::set_param_name(params, Params::ParamCY, &param_cy_name)?;
        Self::set_param_name(params, Params::ParamDX, &param_dx_name)?;
        Self::set_param_name(params, Params::ParamDY, &param_dy_name)?;
        Self::set_float_param_range(params, Params::ParamAX, ui.param_a_range)?;
        Self::set_float_param_range(params, Params::ParamAY, ui.param_a_range)?;
        Self::set_float_param_range(params, Params::ParamBX, ui.param_b_range)?;
        Self::set_float_param_range(params, Params::ParamBY, ui.param_b_range)?;
        Self::set_float_param_range(params, Params::ParamCX, ui.param_c_range)?;
        Self::set_float_param_range(params, Params::ParamCY, ui.param_c_range)?;
        Self::set_float_param_range(params, Params::ParamDX, ui.param_d_range)?;
        Self::set_float_param_range(params, Params::ParamDY, ui.param_d_range)?;

        self.set_param_visible(
            in_data,
            params,
            Params::ParamA,
            ui.show_param_a && !split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamB,
            ui.show_param_b && !split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamC,
            ui.show_param_c && !split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamD,
            ui.show_param_d && !split_xy,
        )?;
        Self::set_param_enabled(params, Params::ParamA, ui.show_param_a && !split_xy)?;
        Self::set_param_enabled(params, Params::ParamB, ui.show_param_b && !split_xy)?;
        Self::set_param_enabled(params, Params::ParamC, ui.show_param_c && !split_xy)?;
        Self::set_param_enabled(params, Params::ParamD, ui.show_param_d && !split_xy)?;

        self.set_param_visible(
            in_data,
            params,
            Params::ParamAX,
            ui.show_param_a && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamAY,
            ui.show_param_a && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamBX,
            ui.show_param_b && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamBY,
            ui.show_param_b && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamCX,
            ui.show_param_c && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamCY,
            ui.show_param_c && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamDX,
            ui.show_param_d && split_xy,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ParamDY,
            ui.show_param_d && split_xy,
        )?;
        Self::set_param_enabled(params, Params::ParamAX, ui.show_param_a && split_xy)?;
        Self::set_param_enabled(params, Params::ParamAY, ui.show_param_a && split_xy)?;
        Self::set_param_enabled(params, Params::ParamBX, ui.show_param_b && split_xy)?;
        Self::set_param_enabled(params, Params::ParamBY, ui.show_param_b && split_xy)?;
        Self::set_param_enabled(params, Params::ParamCX, ui.show_param_c && split_xy)?;
        Self::set_param_enabled(params, Params::ParamCY, ui.show_param_c && split_xy)?;
        Self::set_param_enabled(params, Params::ParamDX, ui.show_param_d && split_xy)?;
        Self::set_param_enabled(params, Params::ParamDY, ui.show_param_d && split_xy)?;

        self.set_param_visible(in_data, params, Params::VowelType, ui.show_vowel_type)?;
        Self::set_param_enabled(params, Params::VowelType, ui.show_vowel_type)?;

        self.set_param_visible(in_data, params, Params::MaskLayer, ui.show_mask_controls)?;
        self.set_param_visible(in_data, params, Params::MaskChannel, ui.show_mask_controls)?;
        self.set_param_visible(in_data, params, Params::MaskStrength, ui.show_mask_controls)?;
        self.set_param_visible(in_data, params, Params::InvertMask, ui.show_mask_controls)?;
        self.set_param_visible(
            in_data,
            params,
            Params::SwapMaskQuadrants,
            ui.show_mask_controls,
        )?;
        Self::set_param_enabled(params, Params::MaskLayer, ui.show_mask_controls)?;
        Self::set_param_enabled(params, Params::MaskChannel, ui.show_mask_controls)?;
        Self::set_param_enabled(params, Params::MaskStrength, ui.show_mask_controls)?;
        Self::set_param_enabled(params, Params::InvertMask, ui.show_mask_controls)?;
        Self::set_param_enabled(params, Params::SwapMaskQuadrants, ui.show_mask_controls)?;

        if cfg!(feature = "gpu_wgpu") {
            Self::set_param_name(params, Params::UseGpu, "Use GPU if available")?;
            Self::set_param_enabled(params, Params::UseGpu, true)?;
        } else {
            Self::set_param_name(params, Params::UseGpu, "Use GPU (not built)")?;
            Self::set_param_enabled(params, Params::UseGpu, false)?;
        }

        Ok(())
    }

    fn set_float_param_range(
        params: &mut Parameters<Params>,
        id: Params,
        range: Option<FloatRange>,
    ) -> Result<(), Error> {
        let Some(range) = range else {
            return Ok(());
        };

        let mut p = params.get_mut(id)?;
        let mut changed = false;
        match p.as_param_mut()? {
            ae::pf::Param::FloatSlider(mut slider) => {
                if !is_close(slider.valid_min(), range.valid_min) {
                    slider.set_valid_min(range.valid_min);
                    changed = true;
                }
                if !is_close(slider.valid_max(), range.valid_max) {
                    slider.set_valid_max(range.valid_max);
                    changed = true;
                }
                if !is_close(slider.slider_min(), range.slider_min) {
                    slider.set_slider_min(range.slider_min);
                    changed = true;
                }
                if !is_close(slider.slider_max(), range.slider_max) {
                    slider.set_slider_max(range.slider_max);
                    changed = true;
                }
            }
            _ => return Err(Error::InvalidParms),
        }

        if changed {
            p.update_param_ui()?;
        }
        Ok(())
    }

    fn set_param_name(
        params: &mut Parameters<Params>,
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
        params: &mut Parameters<Params>,
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
        params: &mut Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::DISABLED, !enabled)
    }

    fn set_param_ui_flag(
        params: &mut Parameters<Params>,
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
        let in_w = in_layer.width();
        let in_h = in_layer.height();
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if in_w == 0 || in_h == 0 || out_w == 0 || out_h == 0 {
            return Ok(());
        }

        let mode = filter_mode_from_popup(params.get(Params::FilterMode)?.as_popup()?.value());
        let color_space =
            filter_color_space_from_popup(params.get(Params::ColorSpace)?.as_popup()?.value());
        let target =
            target_channels_from_popup(params.get(Params::TargetChannels)?.as_popup()?.value());
        let process_w =
            resolve_process_dim(params.get(Params::ProcessWidth)?.as_slider()?.value(), in_w);
        let process_h = resolve_process_dim(
            params.get(Params::ProcessHeight)?.as_slider()?.value(),
            in_h,
        );
        if process_w == 0 || process_h == 0 {
            return Ok(());
        }

        let split_xy = params.get(Params::SplitXY)?.as_checkbox()?.value();
        let controls = FilterControls {
            mode,
            target,
            a: read_directional_param(
                params,
                split_xy,
                Params::ParamA,
                Params::ParamAX,
                Params::ParamAY,
            )?,
            b: read_directional_param(
                params,
                split_xy,
                Params::ParamB,
                Params::ParamBX,
                Params::ParamBY,
            )?,
            c: read_directional_param(
                params,
                split_xy,
                Params::ParamC,
                Params::ParamCX,
                Params::ParamCY,
            )?,
            d: read_directional_param(
                params,
                split_xy,
                Params::ParamD,
                Params::ParamDX,
                Params::ParamDY,
            )?,
            vowel: vowel_type_from_popup(params.get(Params::VowelType)?.as_popup()?.value()),
            mix: (params.get(Params::Mix)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0),
            output_gain: (params.get(Params::OutputGain)?.as_float_slider()?.value() as f32)
                .clamp(0.0, 8.0),
        };
        let clamp_output = params.get(Params::ClampOutput)?.as_checkbox()?.value();
        let use_gpu = params.get(Params::UseGpu)?.as_checkbox()?.value();

        let in_world_type = in_layer.world_type();
        let source_linear_rgba = sample_layer_rgba(&in_layer, in_world_type, process_w, process_h);
        let source_working = convert_linear_to_working_buffer(color_space, &source_linear_rgba);
        let mut centered = source_working.clone();
        for v in &mut centered {
            *v -= 0.5;
        }

        let (mut real, mut imag) = forward_spectrum(process_w, process_h, &centered, use_gpu)?;

        let mask_gains = if mode == FilterMode::LayerMask {
            let mask_checkout = params.checkout_at(Params::MaskLayer, None, None, None)?;
            let mask_layer_opt = mask_checkout.as_layer()?.value();
            let mask_channel =
                mask_channel_from_popup(params.get(Params::MaskChannel)?.as_popup()?.value());
            let mask_strength = (params.get(Params::MaskStrength)?.as_float_slider()?.value()
                as f32)
                .clamp(0.0, 1.0);
            let invert_mask = params.get(Params::InvertMask)?.as_checkbox()?.value();
            let swap_quadrants = params
                .get(Params::SwapMaskQuadrants)?
                .as_checkbox()?
                .value();

            Some(build_mask_gains(
                mask_layer_opt.as_ref(),
                process_w,
                process_h,
                mask_channel,
                mask_strength,
                invert_mask,
                swap_quadrants,
            ))
        } else {
            None
        };

        apply_filter_in_place(
            &mut real,
            &mut imag,
            process_w,
            process_h,
            controls,
            mask_gains.as_deref(),
        )?;

        let reconstructed_centered = inverse_spectrum(process_w, process_h, &real, &imag, use_gpu)?;
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let clamp_effective = clamp_output || !out_is_f32;

        let progress_final = out_h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let sx = resize_sample_coord(x, out_w, process_w);
            let sy = resize_sample_coord(y, out_h, process_h);
            let i = (sy * process_w + sx) * 4;

            let mut mixed_working = [
                source_working[i],
                source_working[i + 1],
                source_working[i + 2],
                source_working[i + 3],
            ];
            let filtered_working = [
                reconstructed_centered[i] + 0.5,
                reconstructed_centered[i + 1] + 0.5,
                reconstructed_centered[i + 2] + 0.5,
                reconstructed_centered[i + 3] + 0.5,
            ];

            for channel in 0..4 {
                if controls.target.includes(channel) {
                    let src = mixed_working[channel];
                    let mixed = src + (filtered_working[channel] - src) * controls.mix;
                    mixed_working[channel] = sanitize_output(mixed, false);
                }
            }

            let source_alpha = source_linear_rgba[i + 3];
            let decoded = convert_working_to_linear_pixel(color_space, mixed_working, source_alpha);
            let out_px = PixelF32 {
                red: sanitize_output(decoded[0], clamp_effective),
                green: sanitize_output(decoded[1], clamp_effective),
                blue: sanitize_output(decoded[2], clamp_effective),
                alpha: sanitize_output(decoded[3], clamp_effective),
            };

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(out_px),
            }

            Ok(())
        })?;

        Ok(())
    }
}

fn mode_ui_profile(mode: FilterMode) -> ModeUiProfile {
    match mode {
        FilterMode::LowPass => ModeUiProfile {
            param_a_name: "Cutoff (0..1)",
            param_b_name: "Softness (0..0.5)",
            param_c_name: "Resonance (0..4)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 4.0,
                slider_min: 0.0,
                slider_max: 4.0,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::HighPass => ModeUiProfile {
            param_a_name: "Cutoff (0..1)",
            param_b_name: "Softness (0..0.5)",
            param_c_name: "Resonance (0..4)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 4.0,
                slider_min: 0.0,
                slider_max: 4.0,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::BandPass => ModeUiProfile {
            param_a_name: "Center (0..1)",
            param_b_name: "Width (0..1)",
            param_c_name: "Softness (0..0.5)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::Dj => ModeUiProfile {
            param_a_name: "Tilt (-1..1)",
            param_b_name: "Pivot (0..1)",
            param_c_name: "Resonance (0..4)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: -1.0,
                valid_max: 1.0,
                slider_min: -1.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 4.0,
                slider_min: 0.0,
                slider_max: 4.0,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::Comb => ModeUiProfile {
            param_a_name: "Spacing (0.02..1)",
            param_b_name: "Sharpness (0..1)",
            param_c_name: "Amount (-1..1)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: 0.02,
                valid_max: 1.0,
                slider_min: 0.02,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: -1.0,
                valid_max: 1.0,
                slider_min: -1.0,
                slider_max: 1.0,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::Morph => ModeUiProfile {
            param_a_name: "Cutoff (0..1)",
            param_b_name: "Softness (0..0.5)",
            param_c_name: "Morph (LP->HP 0..1)",
            param_d_name: "Resonance (0..4)",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_d_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 4.0,
                slider_min: 0.0,
                slider_max: 4.0,
            }),
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: true,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::Resampling => ModeUiProfile {
            param_a_name: "Nyquist (0.05..1)",
            param_b_name: "Alias Mix (0..1)",
            param_c_name: "Softness (0..0.5)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: 0.05,
                valid_max: 1.0,
                slider_min: 0.05,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::Vowel => ModeUiProfile {
            param_a_name: "Shift (-1..1)",
            param_b_name: "Q (0.1..8)",
            param_c_name: "Amount (0..2)",
            param_d_name: "Unused",
            param_a_range: Some(FloatRange {
                valid_min: -1.0,
                valid_max: 1.0,
                slider_min: -1.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.1,
                valid_max: 8.0,
                slider_min: 0.1,
                slider_max: 8.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 2.0,
                slider_min: 0.0,
                slider_max: 2.0,
            }),
            param_d_range: None,
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: false,
            show_vowel_type: true,
            show_mask_controls: false,
        },
        FilterMode::Notch => ModeUiProfile {
            param_a_name: "Notch Center (0..1)",
            param_b_name: "Notch Width (0..1)",
            param_c_name: "Softness (0..0.5)",
            param_d_name: "Depth (0..1)",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            param_d_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: true,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::NotchLowPass => ModeUiProfile {
            param_a_name: "Notch Center (0..1)",
            param_b_name: "Notch Width (0..1)",
            param_c_name: "LP Cutoff (0..1)",
            param_d_name: "Softness (0..0.5)",
            param_a_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_b_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_c_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 1.0,
                slider_min: 0.0,
                slider_max: 1.0,
            }),
            param_d_range: Some(FloatRange {
                valid_min: 0.0,
                valid_max: 0.5,
                slider_min: 0.0,
                slider_max: 0.5,
            }),
            show_param_a: true,
            show_param_b: true,
            show_param_c: true,
            show_param_d: true,
            show_vowel_type: false,
            show_mask_controls: false,
        },
        FilterMode::LayerMask => ModeUiProfile {
            param_a_name: "Unused",
            param_b_name: "Unused",
            param_c_name: "Unused",
            param_d_name: "Unused",
            param_a_range: None,
            param_b_range: None,
            param_c_range: None,
            param_d_range: None,
            show_param_a: false,
            show_param_b: false,
            show_param_c: false,
            show_param_d: false,
            show_vowel_type: false,
            show_mask_controls: true,
        },
    }
}

fn filter_mode_from_popup(value: i32) -> FilterMode {
    match value {
        2 => FilterMode::HighPass,
        3 => FilterMode::BandPass,
        4 => FilterMode::Notch,
        5 => FilterMode::Dj,
        6 => FilterMode::Comb,
        7 => FilterMode::Morph,
        8 => FilterMode::Resampling,
        9 => FilterMode::Vowel,
        10 => FilterMode::NotchLowPass,
        11 => FilterMode::LayerMask,
        _ => FilterMode::LowPass,
    }
}

fn filter_color_space_from_popup(value: i32) -> FilterColorSpace {
    match value {
        2 => FilterColorSpace::Srgb,
        3 => FilterColorSpace::Oklab,
        4 => FilterColorSpace::Oklch,
        5 => FilterColorSpace::Lab,
        6 => FilterColorSpace::Yiq,
        7 => FilterColorSpace::Yuv,
        8 => FilterColorSpace::YCbCr,
        9 => FilterColorSpace::Hsl,
        10 => FilterColorSpace::Hsv,
        11 => FilterColorSpace::Cmyk,
        _ => FilterColorSpace::LinearRgba,
    }
}

fn target_channels_from_popup(value: i32) -> TargetChannels {
    match value {
        2 => TargetChannels::Rgb,
        3 => TargetChannels::Alpha,
        4 => TargetChannels::R,
        5 => TargetChannels::G,
        6 => TargetChannels::B,
        _ => TargetChannels::Rgba,
    }
}

fn mask_channel_from_popup(value: i32) -> MaskChannel {
    match value {
        2 => MaskChannel::Rgba,
        3 => MaskChannel::Rgb,
        4 => MaskChannel::Alpha,
        _ => MaskChannel::Luminance,
    }
}

fn vowel_type_from_popup(value: i32) -> VowelType {
    match value {
        2 => VowelType::E,
        3 => VowelType::I,
        4 => VowelType::O,
        5 => VowelType::U,
        _ => VowelType::A,
    }
}

impl TargetChannels {
    fn includes(self, channel: usize) -> bool {
        match self {
            TargetChannels::Rgba => true,
            TargetChannels::Rgb => channel < 3,
            TargetChannels::Alpha => channel == 3,
            TargetChannels::R => channel == 0,
            TargetChannels::G => channel == 1,
            TargetChannels::B => channel == 2,
        }
    }
}

impl DirectionalParam {
    fn from_scalar(v: f32) -> Self {
        Self { x: v, y: v }
    }

    fn value_for_direction(self, nx: f32, ny: f32) -> f32 {
        let wx = nx.abs();
        let wy = ny.abs();
        let sum = wx + wy;
        if sum <= 1.0e-6 {
            0.5 * (self.x + self.y)
        } else {
            (self.x * wx + self.y * wy) / sum
        }
    }
}

fn read_directional_param(
    params: &Parameters<Params>,
    split_xy: bool,
    scalar: Params,
    x_param: Params,
    y_param: Params,
) -> Result<DirectionalParam, Error> {
    if split_xy {
        Ok(DirectionalParam {
            x: params.get(x_param)?.as_float_slider()?.value() as f32,
            y: params.get(y_param)?.as_float_slider()?.value() as f32,
        })
    } else {
        let v = params.get(scalar)?.as_float_slider()?.value() as f32;
        Ok(DirectionalParam::from_scalar(v))
    }
}

fn forward_spectrum(
    width: usize,
    height: usize,
    input_centered_rgba: &[f32],
    prefer_gpu: bool,
) -> Result<(Vec<f32>, Vec<f32>), Error> {
    #[cfg(feature = "gpu_wgpu")]
    {
        if prefer_gpu
            && let Some(ctx) = wgpu_context()
            && let Ok(out) = ctx.forward_rgba(width as u32, height as u32, input_centered_rgba)
        {
            return Ok((out.real, out.imag));
        }
    }

    #[cfg(not(feature = "gpu_wgpu"))]
    let _ = prefer_gpu;

    let spectrum = utils::spectral::fft2_rgba(input_centered_rgba, width, height)
        .map_err(|_| Error::BadCallbackParameter)?;
    Ok((spectrum.real, spectrum.imag))
}

fn inverse_spectrum(
    width: usize,
    height: usize,
    input_real_rgba: &[f32],
    input_imag_rgba: &[f32],
    prefer_gpu: bool,
) -> Result<Vec<f32>, Error> {
    #[cfg(feature = "gpu_wgpu")]
    {
        if prefer_gpu
            && let Some(ctx) = wgpu_context()
            && let Ok(out) = ctx.inverse_rgba(
                width as u32,
                height as u32,
                input_real_rgba,
                input_imag_rgba,
            )
        {
            return Ok(out);
        }
    }

    #[cfg(not(feature = "gpu_wgpu"))]
    let _ = prefer_gpu;

    utils::spectral::ifft2_rgba(input_real_rgba, input_imag_rgba, width, height)
        .map_err(|_| Error::BadCallbackParameter)
}

fn apply_filter_in_place(
    real: &mut [f32],
    imag: &mut [f32],
    width: usize,
    height: usize,
    controls: FilterControls,
    layer_mask: Option<&[f32]>,
) -> Result<(), Error> {
    let expected = width
        .checked_mul(height)
        .and_then(|p| p.checked_mul(4))
        .ok_or(Error::BadCallbackParameter)?;
    if real.len() != expected || imag.len() != expected {
        return Err(Error::BadCallbackParameter);
    }
    if let Some(mask) = layer_mask
        && mask.len() != expected
    {
        return Err(Error::BadCallbackParameter);
    }

    for y in 0..height {
        let fy = signed_frequency(y, height);
        let ny = normalize_frequency(fy, height);
        for x in 0..width {
            let fx = signed_frequency(x, width);
            let nx = normalize_frequency(fx, width);
            let radial =
                ((nx * nx + ny * ny).sqrt() * std::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0);
            let angle = ny.atan2(nx);

            let mut base_gain = filter_gain(controls, radial, angle, nx, ny);
            if !base_gain.is_finite() {
                base_gain = 0.0;
            }
            base_gain = base_gain.max(0.0);

            let i = (y * width + x) * 4;
            for channel in 0..4 {
                if !controls.target.includes(channel) {
                    continue;
                }

                let mut gain = base_gain * controls.output_gain;
                if controls.mode == FilterMode::LayerMask
                    && let Some(mask) = layer_mask
                {
                    gain *= mask[i + channel];
                }
                if !gain.is_finite() {
                    gain = 0.0;
                }
                gain = gain.max(0.0);

                real[i + channel] *= gain;
                imag[i + channel] *= gain;
            }
        }
    }

    Ok(())
}

fn filter_gain(controls: FilterControls, radial: f32, angle: f32, nx: f32, ny: f32) -> f32 {
    match controls.mode {
        FilterMode::LowPass => {
            let cutoff = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let soft = controls.b.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let resonance = controls.c.value_for_direction(nx, ny).clamp(0.0, 4.0);
            let pass = 1.0 - smoothstep(cutoff - soft, cutoff + soft, radial);
            let bump = gaussian(radial, cutoff, 0.03 + soft * 0.5) * resonance * 0.5;
            (pass * (1.0 + bump)).max(0.0)
        }
        FilterMode::HighPass => {
            let cutoff = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let soft = controls.b.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let resonance = controls.c.value_for_direction(nx, ny).clamp(0.0, 4.0);
            let pass = smoothstep(cutoff - soft, cutoff + soft, radial);
            let bump = gaussian(radial, cutoff, 0.03 + soft * 0.5) * resonance * 0.5;
            (pass * (1.0 + bump)).max(0.0)
        }
        FilterMode::BandPass => {
            let center = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let width = controls
                .b
                .value_for_direction(nx, ny)
                .abs()
                .clamp(0.001, 1.0);
            let soft = controls.c.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let half = width * 0.5;
            let dist = (radial - center).abs();
            1.0 - smoothstep(half - soft, half + soft, dist)
        }
        FilterMode::Dj => {
            let tilt = controls.a.value_for_direction(nx, ny).clamp(-1.0, 1.0);
            let pivot = controls.b.value_for_direction(nx, ny).clamp(0.02, 0.98);
            let resonance = controls.c.value_for_direction(nx, ny).clamp(0.0, 4.0);
            let low_weight = 1.0 - smoothstep(pivot - 0.05, pivot + 0.05, radial);
            let high_weight = 1.0 - low_weight;
            let base = if tilt >= 0.0 {
                (1.0 - low_weight * tilt * 0.9) + high_weight * tilt
            } else {
                (1.0 - high_weight * (-tilt) * 0.9) + low_weight * (-tilt)
            };
            let bump = gaussian(radial, pivot, 0.03) * resonance * 0.6;
            (base + bump).max(0.0)
        }
        FilterMode::Comb => {
            let spacing = controls
                .a
                .value_for_direction(nx, ny)
                .abs()
                .clamp(0.02, 1.0);
            let sharpness = lerp(
                1.0,
                24.0,
                controls.b.value_for_direction(nx, ny).clamp(0.0, 1.0),
            );
            let amount = controls.c.value_for_direction(nx, ny).clamp(-1.0, 1.0);
            let wave = (std::f32::consts::TAU * radial / spacing).cos();
            let shaped = wave.signum() * wave.abs().powf(sharpness);
            (1.0 + amount * shaped).max(0.0)
        }
        FilterMode::Morph => {
            let cutoff = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let soft = controls.b.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let morph = controls.c.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let resonance = controls.d.value_for_direction(nx, ny).clamp(0.0, 4.0);
            let lp = 1.0 - smoothstep(cutoff - soft, cutoff + soft, radial);
            let hp = smoothstep(cutoff - soft, cutoff + soft, radial);
            let base = lerp(lp, hp, morph);
            let bump = gaussian(radial, cutoff, 0.03 + soft * 0.5) * resonance * 0.5;
            (base * (1.0 + bump)).max(0.0)
        }
        FilterMode::Resampling => {
            let nyquist = controls.a.value_for_direction(nx, ny).clamp(0.05, 1.0);
            let alias_mix = controls.b.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let soft = controls.c.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let low = 1.0 - smoothstep(nyquist - soft, nyquist + soft, radial);
            let mirrored = 1.0 - smoothstep(0.0, nyquist.max(0.05), (radial - nyquist).abs());
            ((1.0 - alias_mix) * low + alias_mix * mirrored).clamp(0.0, 2.0)
        }
        FilterMode::Vowel => {
            let shift = controls.a.value_for_direction(nx, ny).clamp(-1.0, 1.0);
            let q = controls.b.value_for_direction(nx, ny).clamp(0.1, 8.0);
            let amount = controls.c.value_for_direction(nx, ny).clamp(0.0, 2.0);
            let shift_ratio = 2.0f32.powf(shift);
            let (centers, widths) = vowel_formants(controls.vowel);
            let mut gain = 0.05;
            for idx in 0..3 {
                let center = (centers[idx] * shift_ratio).clamp(0.0, 1.0);
                let width = (widths[idx] / q).max(0.005);
                gain += gaussian(radial, center, width) * (0.8 + amount);
            }
            let directional = 0.85 + 0.15 * angle.cos().abs();
            (gain * directional).max(0.0)
        }
        FilterMode::Notch => {
            let notch_center = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let notch_width = controls
                .b
                .value_for_direction(nx, ny)
                .abs()
                .clamp(0.001, 1.0);
            let soft = controls.c.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let depth = controls.d.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let notch_half = notch_width * 0.5;
            let dist = (radial - notch_center).abs();
            let notch_band = 1.0 - smoothstep(notch_half - soft, notch_half + soft, dist);
            (1.0 - depth * notch_band).max(0.0)
        }
        FilterMode::NotchLowPass => {
            let notch_center = controls.a.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let notch_width = controls
                .b
                .value_for_direction(nx, ny)
                .abs()
                .clamp(0.001, 1.0);
            let lp_cutoff = controls.c.value_for_direction(nx, ny).clamp(0.0, 1.0);
            let soft = controls.d.value_for_direction(nx, ny).abs().clamp(0.0, 0.5);
            let notch_half = notch_width * 0.5;
            let dist = (radial - notch_center).abs();
            let notch_keep = smoothstep(notch_half - soft, notch_half + soft, dist);
            let lp = 1.0 - smoothstep(lp_cutoff - soft, lp_cutoff + soft, radial);
            (notch_keep * lp).max(0.0)
        }
        FilterMode::LayerMask => 1.0,
    }
}

fn vowel_formants(vowel: VowelType) -> ([f32; 3], [f32; 3]) {
    match vowel {
        VowelType::A => ([0.09, 0.21, 0.34], [0.04, 0.05, 0.06]),
        VowelType::E => ([0.07, 0.27, 0.39], [0.03, 0.05, 0.05]),
        VowelType::I => ([0.05, 0.32, 0.45], [0.03, 0.04, 0.05]),
        VowelType::O => ([0.08, 0.18, 0.29], [0.04, 0.05, 0.06]),
        VowelType::U => ([0.06, 0.14, 0.24], [0.04, 0.05, 0.06]),
    }
}

fn build_mask_gains(
    layer: Option<&Layer>,
    out_w: usize,
    out_h: usize,
    channel: MaskChannel,
    strength: f32,
    invert: bool,
    swap_quadrants: bool,
) -> Vec<f32> {
    let mut mask = vec![1.0f32; out_w * out_h * 4];
    let Some(layer) = layer else {
        return mask;
    };

    let sampled = sample_layer_rgba(layer, layer.world_type(), out_w, out_h);
    for y in 0..out_h {
        for x in 0..out_w {
            let (sx, sy) = if swap_quadrants {
                ((x + out_w / 2) % out_w, (y + out_h / 2) % out_h)
            } else {
                (x, y)
            };
            let si = (sy * out_w + sx) * 4;
            let oi = (y * out_w + x) * 4;

            let r = sanitize_unit(sampled[si]);
            let g = sanitize_unit(sampled[si + 1]);
            let b = sanitize_unit(sampled[si + 2]);
            let a = sanitize_unit(sampled[si + 3]);
            let lum = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);

            let mut rgba = match channel {
                MaskChannel::Luminance => [lum, lum, lum, lum],
                MaskChannel::Rgba => [r, g, b, a],
                MaskChannel::Rgb => [r, g, b, 1.0],
                MaskChannel::Alpha => [a, a, a, a],
            };

            for c in 0..4 {
                let mut v = rgba[c];
                if invert {
                    v = 1.0 - v;
                }
                rgba[c] = lerp(1.0, v, strength).clamp(0.0, 1.0);
                mask[oi + c] = rgba[c];
            }
        }
    }

    mask
}

fn convert_linear_to_working_buffer(
    space: FilterColorSpace,
    source_linear_rgba: &[f32],
) -> Vec<f32> {
    if source_linear_rgba.is_empty() {
        return vec![];
    }

    let mut out = vec![0.0f32; source_linear_rgba.len()];
    for i in (0..source_linear_rgba.len()).step_by(4) {
        let converted = convert_linear_to_working_pixel(
            space,
            [
                source_linear_rgba[i],
                source_linear_rgba[i + 1],
                source_linear_rgba[i + 2],
                source_linear_rgba[i + 3],
            ],
        );
        out[i] = converted[0];
        out[i + 1] = converted[1];
        out[i + 2] = converted[2];
        out[i + 3] = converted[3];
    }
    out
}

fn convert_linear_to_working_pixel(space: FilterColorSpace, linear_rgba: [f32; 4]) -> [f32; 4] {
    const OKLAB_AB_MAX: f32 = 0.5;
    const OKLCH_CHROMA_MAX: f32 = 0.4;
    const LAB_L_MAX: f32 = 100.0;
    const LAB_AB_MAX: f32 = 128.0;
    const YIQ_I_MAX: f32 = 0.5957;
    const YIQ_Q_MAX: f32 = 0.5226;
    const YUV_U_MAX: f32 = 0.436;
    const YUV_V_MAX: f32 = 0.615;
    const YCBCR_MAX: f32 = 255.0;

    let lin = LinSrgb::new(linear_rgba[0], linear_rgba[1], linear_rgba[2]);
    match space {
        FilterColorSpace::LinearRgba => linear_rgba,
        FilterColorSpace::Srgb => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            [srgb.red, srgb.green, srgb.blue, linear_rgba[3]]
        }
        FilterColorSpace::Oklab => {
            let c: Oklab<f32> = Oklab::from_color(lin);
            [
                encode_signed(c.a, OKLAB_AB_MAX),
                encode_signed(c.b, OKLAB_AB_MAX),
                c.l,
                linear_rgba[3],
            ]
        }
        FilterColorSpace::Oklch => {
            let c: Oklch<f32> = Oklch::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                encode_pos(c.chroma, OKLCH_CHROMA_MAX),
                c.l,
                linear_rgba[3],
            ]
        }
        FilterColorSpace::Lab => {
            let c = Lab::from_color(lin);
            [
                encode_signed(c.a, LAB_AB_MAX),
                encode_signed(c.b, LAB_AB_MAX),
                c.l / LAB_L_MAX,
                linear_rgba[3],
            ]
        }
        FilterColorSpace::Yiq => {
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
        FilterColorSpace::Yuv => {
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
        FilterColorSpace::YCbCr => {
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
        FilterColorSpace::Hsl => {
            let c = Hsl::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                c.saturation,
                c.lightness,
                linear_rgba[3],
            ]
        }
        FilterColorSpace::Hsv => {
            let c = Hsv::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                c.saturation,
                c.value,
                linear_rgba[3],
            ]
        }
        FilterColorSpace::Cmyk => {
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
    space: FilterColorSpace,
    working_rgba: [f32; 4],
    source_alpha: f32,
) -> [f32; 4] {
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
        FilterColorSpace::LinearRgba => working_rgba,
        FilterColorSpace::Srgb => {
            let lin = Srgb::new(working_rgba[0], working_rgba[1], working_rgba[2]).into_linear();
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Oklab => {
            let l = working_rgba[2];
            let a = decode_signed(working_rgba[0], OKLAB_AB_MAX);
            let b = decode_signed(working_rgba[1], OKLAB_AB_MAX);
            let lin = LinSrgb::from_color(Oklab::new(l, a, b));
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Oklch => {
            let l = working_rgba[2];
            let chroma = decode_pos(working_rgba[1], OKLCH_CHROMA_MAX);
            let hue = wrap01(working_rgba[0]) * 360.0;
            let lin = LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)));
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Lab => {
            let l = working_rgba[2] * LAB_L_MAX;
            let a = decode_signed(working_rgba[0], LAB_AB_MAX);
            let b = decode_signed(working_rgba[1], LAB_AB_MAX);
            let lin = LinSrgb::from_color(Lab::new(l, a, b));
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Yiq => {
            let y = working_rgba[2];
            let i = decode_signed(working_rgba[0], YIQ_I_MAX);
            let q = decode_signed(working_rgba[1], YIQ_Q_MAX);
            let sr = y + 0.9563 * i + 0.6210 * q;
            let sg = y - 0.2721 * i - 0.6474 * q;
            let sb = y - 1.1070 * i + 1.7046 * q;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Yuv => {
            let y = working_rgba[2];
            let u = decode_signed(working_rgba[0], YUV_U_MAX);
            let v = decode_signed(working_rgba[1], YUV_V_MAX);
            let sr = y + 1.13983 * v;
            let sg = y - 0.39465 * u - 0.58060 * v;
            let sb = y + 2.03211 * u;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::YCbCr => {
            let y = decode_pos(working_rgba[2], YCBCR_MAX);
            let cb = decode_pos(working_rgba[0], YCBCR_MAX);
            let cr = decode_pos(working_rgba[1], YCBCR_MAX);
            let sr = (y + 1.402 * (cr - 128.0)) / 255.0;
            let sg = (y - 0.344136 * (cb - 128.0) - 0.714136 * (cr - 128.0)) / 255.0;
            let sb = (y + 1.772 * (cb - 128.0)) / 255.0;
            let lin = Srgb::new(sr, sg, sb).into_linear();
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Hsl => {
            let hue = wrap01(working_rgba[0]) * 360.0;
            let saturation = working_rgba[1];
            let lightness = working_rgba[2];
            let lin =
                LinSrgb::from_color(Hsl::new(RgbHue::from_degrees(hue), saturation, lightness));
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Hsv => {
            let hue = wrap01(working_rgba[0]) * 360.0;
            let saturation = working_rgba[1];
            let value = working_rgba[2];
            let lin = LinSrgb::from_color(Hsv::new(RgbHue::from_degrees(hue), saturation, value));
            [lin.red, lin.green, lin.blue, working_rgba[3]]
        }
        FilterColorSpace::Cmyk => {
            let c = working_rgba[0];
            let m = working_rgba[1];
            let y = working_rgba[2];
            let k = working_rgba[3];
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

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= 1.0e-6 {
        if x < edge0 { 0.0 } else { 1.0 }
    } else {
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

fn gaussian(x: f32, center: f32, sigma: f32) -> f32 {
    let sigma = sigma.max(1.0e-6);
    let t = (x - center) / sigma;
    (-0.5 * t * t).exp()
}

fn sanitize_unit(mut v: f32) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    v.clamp(0.0, 1.0)
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

fn resolve_process_dim(param: i32, fallback: usize) -> usize {
    if param <= 0 { fallback } else { param as usize }
}

fn signed_frequency(index: usize, len: usize) -> f32 {
    let half = len / 2;
    if index <= half {
        index as f32
    } else {
        index as f32 - len as f32
    }
}

fn normalize_frequency(freq: f32, len: usize) -> f32 {
    let nyquist = (len as f32 * 0.5).max(1.0);
    freq / nyquist
}

fn resize_sample_coord(coord: usize, out_len: usize, src_len: usize) -> usize {
    if src_len <= 1 || out_len <= 1 {
        0
    } else {
        ((coord * src_len) / out_len).min(src_len - 1)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn is_close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0e-6
}

fn sample_layer_rgba(
    layer: &Layer,
    world_type: ae::aegp::WorldType,
    out_w: usize,
    out_h: usize,
) -> Vec<f32> {
    let src_w = layer.width();
    let src_h = layer.height();
    let mut out = vec![0.0f32; out_w * out_h * 4];

    for y in 0..out_h {
        let sy = resize_sample_coord(y, out_h, src_h);
        for x in 0..out_w {
            let sx = resize_sample_coord(x, out_w, src_w);
            let p = read_pixel_f32(layer, world_type, sx, sy);
            let i = (y * out_w + x) * 4;
            out[i] = p.red;
            out[i + 1] = p.green;
            out[i + 2] = p.blue;
            out[i + 3] = p.alpha;
        }
    }

    out
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}
