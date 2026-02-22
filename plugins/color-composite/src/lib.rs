#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use seq_macro::seq;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const MAX_COLORS: usize = 16;
const MIN_COLORS: usize = 1;
const DEFAULT_COLORS: usize = 1;
const ALPHA_EPSILON: f32 = 1.0e-6;

const AE_BLEND_MODE_OPTIONS: [&str; 26] = [
    "Normal",
    "Darken",
    "Multiply",
    "Color Burn",
    "Linear Burn",
    "Darker Color",
    "Lighten",
    "Screen",
    "Color Dodge",
    "Linear Dodge (Add)",
    "Lighter Color",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Vivid Light",
    "Linear Light",
    "Pin Light",
    "Hard Mix",
    "Difference",
    "Exclusion",
    "Subtract",
    "Divide",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
];

const DEFAULT_SWATCHES: [Pixel8; 8] = [
    Pixel8 {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 255,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 255,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 0,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 255,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    },
];

seq!(N in 1..=16 {
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ColorCount,
    AddColor,
    RemoveColor,
    ShowAdvancedBlendModes,
    #(
        Color~N,
        Mode~N,
        AdvancedMode~N,
        Opacity~N,
    )*
}
});

seq!(N in 1..=16 {
const COLOR_PARAMS: [Params; 16] = [#(Params::Color~N,)*];
const MODE_PARAMS: [Params; 16] = [#(Params::Mode~N,)*];
const ADVANCED_MODE_PARAMS: [Params; 16] = [#(Params::AdvancedMode~N,)*];
const OPACITY_PARAMS: [Params; 16] = [#(Params::Opacity~N,)*];
});

#[derive(Clone, Copy)]
enum BlendMode {
    Normal,
    Dissolve,
    Behind,
    Replace,
    Erase,
    Merge,
    Split,
    PassThrough,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    Overlay,
    OverlayLegacy,
    SoftLight,
    SoftLightLegacy,
    SoftLightPhotoshop,
    HardLight,
    VividLight,
    VividLightPhotoshop,
    LinearLight,
    InverseLinearLight,
    PinLight,
    PinLightInverse,
    PinLightStrong,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
    DarkerColor,
    LighterColor,
    LumaDarkenOnly,
    LumaLightenOnly,
    Lightness,
    Luminance,
    HsvHue,
    HsvSaturation,
    HsvValue,
    HueDelta,
    ColorLuminance,
    ColorHsvl,
    SaturationLightness,
    SaturationLuminance,
    SaturationHsvl,
    LuminanceHsvl,
    ColorErase,
    Invert,
    InverseColorDodge,
    SoftDodge,
    InverseColorBurn,
    SoftBurn,
    ChannelRed,
    ChannelGreen,
    ChannelBlue,
    ChannelYellow,
    ChannelAqua,
    ChannelMagenta,
    Average,
    GeometricMean,
    HarmonicMean,
    Negation,
    Reflect,
    Glow,
    Cool,
    Warm,
    Phoenix,
    GrainMerge,
    GrainExtract,
    Freeze,
    Heat,
    BitAnd,
    BitOr,
    BitXor,
    BitShift,
    AddUnsigned,
    SubtractUnsigned,
    Binarize,
}

macro_rules! advanced_modes_table {
    ($(($label:expr, $mode:expr)),+ $(,)?) => {
        const ADVANCED_BLEND_MODE_OPTIONS: [&str; <[()]>::len(&[$(advanced_modes_table!(@unit $label)),+])] = [
            $($label),+
        ];
        const ADVANCED_BLEND_MODE_VALUES: [BlendMode; <[()]>::len(&[$(advanced_modes_table!(@unit $label)),+])] = [
            $($mode),+
        ];
    };
    (@unit $label:expr) => { () };
}

const AE_BLEND_MODE_VALUES: [BlendMode; 26] = [
    BlendMode::Normal,
    BlendMode::Darken,
    BlendMode::Multiply,
    BlendMode::ColorBurn,
    BlendMode::LinearBurn,
    BlendMode::DarkerColor,
    BlendMode::Lighten,
    BlendMode::Screen,
    BlendMode::ColorDodge,
    BlendMode::LinearDodge,
    BlendMode::LighterColor,
    BlendMode::Overlay,
    BlendMode::SoftLightPhotoshop,
    BlendMode::HardLight,
    BlendMode::VividLight,
    BlendMode::LinearLight,
    BlendMode::PinLight,
    BlendMode::HardMix,
    BlendMode::Difference,
    BlendMode::Exclusion,
    BlendMode::Subtract,
    BlendMode::Divide,
    BlendMode::Hue,
    BlendMode::Saturation,
    BlendMode::Color,
    BlendMode::Luminosity,
];

advanced_modes_table! {
    ("GIMP Normal (Legacy)", BlendMode::Normal),
    ("GIMP Dissolve", BlendMode::Dissolve),
    ("GIMP Behind (Legacy)", BlendMode::Behind),
    ("GIMP Multiply (Legacy)", BlendMode::Multiply),
    ("GIMP Screen (Legacy)", BlendMode::Screen),
    ("GIMP Overlay (Legacy)", BlendMode::OverlayLegacy),
    ("GIMP Difference (Legacy)", BlendMode::Difference),
    ("GIMP Addition (Legacy)", BlendMode::LinearDodge),
    ("GIMP Subtract (Legacy)", BlendMode::Subtract),
    ("GIMP Darken Only (Legacy)", BlendMode::Darken),
    ("GIMP Lighten Only (Legacy)", BlendMode::Lighten),
    ("GIMP HSV Hue (Legacy)", BlendMode::HsvHue),
    ("GIMP HSV Saturation (Legacy)", BlendMode::HsvSaturation),
    ("GIMP HSL Color (Legacy)", BlendMode::Color),
    ("GIMP HSV Value (Legacy)", BlendMode::HsvValue),
    ("GIMP Divide (Legacy)", BlendMode::Divide),
    ("GIMP Dodge (Legacy)", BlendMode::ColorDodge),
    ("GIMP Burn (Legacy)", BlendMode::ColorBurn),
    ("GIMP Hard Light (Legacy)", BlendMode::HardLight),
    ("GIMP Soft Light (Legacy)", BlendMode::SoftLightLegacy),
    ("GIMP Grain Extract (Legacy)", BlendMode::GrainExtract),
    ("GIMP Grain Merge (Legacy)", BlendMode::GrainMerge),
    ("GIMP Color Erase (Legacy)", BlendMode::ColorErase),
    ("GIMP Overlay", BlendMode::Overlay),
    ("GIMP LCH Hue", BlendMode::Hue),
    ("GIMP LCH Chroma", BlendMode::Saturation),
    ("GIMP LCH Color", BlendMode::Color),
    ("GIMP LCH Lightness", BlendMode::Lightness),
    ("GIMP Normal", BlendMode::Normal),
    ("GIMP Behind", BlendMode::Behind),
    ("GIMP Multiply", BlendMode::Multiply),
    ("GIMP Screen", BlendMode::Screen),
    ("GIMP Difference", BlendMode::Difference),
    ("GIMP Addition", BlendMode::LinearDodge),
    ("GIMP Subtract", BlendMode::Subtract),
    ("GIMP Darken Only", BlendMode::Darken),
    ("GIMP Lighten Only", BlendMode::Lighten),
    ("GIMP HSV Hue", BlendMode::HsvHue),
    ("GIMP HSV Saturation", BlendMode::HsvSaturation),
    ("GIMP HSL Color", BlendMode::Color),
    ("GIMP HSV Value", BlendMode::HsvValue),
    ("GIMP Divide", BlendMode::Divide),
    ("GIMP Dodge", BlendMode::ColorDodge),
    ("GIMP Burn", BlendMode::ColorBurn),
    ("GIMP Hard Light", BlendMode::HardLight),
    ("GIMP Soft Light", BlendMode::SoftLight),
    ("GIMP Grain Extract", BlendMode::GrainExtract),
    ("GIMP Grain Merge", BlendMode::GrainMerge),
    ("GIMP Vivid Light", BlendMode::VividLight),
    ("GIMP Pin Light", BlendMode::PinLight),
    ("GIMP Linear Light", BlendMode::LinearLight),
    ("GIMP Hard Mix", BlendMode::HardMix),
    ("GIMP Exclusion", BlendMode::Exclusion),
    ("GIMP Linear Burn", BlendMode::LinearBurn),
    ("GIMP Luma Darken Only", BlendMode::LumaDarkenOnly),
    ("GIMP Luma Lighten Only", BlendMode::LumaLightenOnly),
    ("GIMP Luminance", BlendMode::Luminance),
    ("GIMP Color Erase", BlendMode::ColorErase),
    ("GIMP Erase", BlendMode::Erase),
    ("GIMP Merge", BlendMode::Merge),
    ("GIMP Split", BlendMode::Split),
    ("GIMP Pass Through", BlendMode::PassThrough),
    ("GIMP Replace", BlendMode::Replace),
    ("Negation", BlendMode::Negation),
    ("Invert", BlendMode::Invert),
    ("Lighter Color", BlendMode::LighterColor),
    ("Darker Color", BlendMode::DarkerColor),
    ("Inverse Linear Light", BlendMode::InverseLinearLight),
    ("Inverse Color Dodge", BlendMode::InverseColorDodge),
    ("Soft Dodge", BlendMode::SoftDodge),
    ("Inverse Color Burn", BlendMode::InverseColorBurn),
    ("Soft Burn", BlendMode::SoftBurn),
    ("Soft Light (Photoshop)", BlendMode::SoftLightPhotoshop),
    ("Vivid Light (Photoshop)", BlendMode::VividLightPhotoshop),
    ("Pin Light (Inverse)", BlendMode::PinLightInverse),
    ("Pin Light (Strong)", BlendMode::PinLightStrong),
    ("Hue (Delta)", BlendMode::HueDelta),
    ("Color (Luminance)", BlendMode::ColorLuminance),
    ("Color (HSVL)", BlendMode::ColorHsvl),
    ("Saturation (Lightness)", BlendMode::SaturationLightness),
    ("Saturation (Luminance)", BlendMode::SaturationLuminance),
    ("Saturation (HSVL)", BlendMode::SaturationHsvl),
    ("Lightness", BlendMode::Lightness),
    ("Luminance (HSVL)", BlendMode::LuminanceHsvl),
    ("Red Channel", BlendMode::ChannelRed),
    ("Green Channel", BlendMode::ChannelGreen),
    ("Blue Channel", BlendMode::ChannelBlue),
    ("Yellow Channels", BlendMode::ChannelYellow),
    ("Aqua Channels", BlendMode::ChannelAqua),
    ("Magenta Channels", BlendMode::ChannelMagenta),
    ("Average", BlendMode::Average),
    ("Geometric Mean", BlendMode::GeometricMean),
    ("Harmonic Mean", BlendMode::HarmonicMean),
    ("Reflect", BlendMode::Reflect),
    ("Glow", BlendMode::Glow),
    ("Freeze", BlendMode::Freeze),
    ("Heat", BlendMode::Heat),
    ("Cool", BlendMode::Cool),
    ("Warm", BlendMode::Warm),
    ("Phoenix", BlendMode::Phoenix),
    ("AND", BlendMode::BitAnd),
    ("OR", BlendMode::BitOr),
    ("XOR", BlendMode::BitXor),
    ("SHIFT", BlendMode::BitShift),
    ("Add (Unsigned)", BlendMode::AddUnsigned),
    ("Subtract (Unsigned)", BlendMode::SubtractUnsigned),
    ("Binarize", BlendMode::Binarize)
}

#[derive(Clone, Copy)]
struct CompositeLayer {
    color: [f32; 3],
    mode: BlendMode,
    opacity: f32,
}

struct RenderSettings {
    layers: Vec<CompositeLayer>,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Composites multiple colors onto a layer using selectable blend modes with dynamic controls.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let supervise_flags = || {
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP
        };

        params.add_with_flags(
            Params::ColorCount,
            "Number of Colors",
            FloatSliderDef::setup(|d| {
                d.set_default(DEFAULT_COLORS as f64);
                d.set_value(DEFAULT_COLORS as f64);
                d.set_valid_min(MIN_COLORS as f32);
                d.set_valid_max(MAX_COLORS as f32);
                d.set_slider_min(MIN_COLORS as f32);
                d.set_slider_max(MAX_COLORS as f32);
                d.set_precision(0);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::AddColor,
            "Add Color",
            ButtonDef::setup(|d| {
                d.set_label("Add");
            }),
        )?;

        params.add(
            Params::RemoveColor,
            "Remove Color",
            ButtonDef::setup(|d| {
                d.set_label("Remove");
            }),
        )?;

        params.add_with_flags(
            Params::ShowAdvancedBlendModes,
            "Show Advanced Blend Modes",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        for idx in 0..MAX_COLORS {
            params.add(
                COLOR_PARAMS[idx],
                &format!("Color{}", idx + 1),
                ColorDef::setup(|d| {
                    d.set_default(default_color(idx));
                }),
            )?;

            params.add(
                MODE_PARAMS[idx],
                &format!("Blend Mode{}", idx + 1),
                PopupDef::setup(|d| {
                    d.set_options(&AE_BLEND_MODE_OPTIONS);
                    d.set_default(1);
                }),
            )?;

            params.add(
                ADVANCED_MODE_PARAMS[idx],
                &format!("Advanced Blend Mode{}", idx + 1),
                PopupDef::setup(|d| {
                    d.set_options(&ADVANCED_BLEND_MODE_OPTIONS);
                    d.set_default(1);
                }),
            )?;

            params.add(
                OPACITY_PARAMS[idx],
                &format!("Opacity{} (%)", idx + 1),
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(100.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(100.0);
                    d.set_default(100.0);
                    d.set_precision(1);
                }),
            )?;
        }

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
                        "AOD_ColorComposite - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ColorComposite")
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
                self.handle_user_changed_param(param_index, params, &mut out_data)?;
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
    fn show_advanced_blend_modes(params: &ae::Parameters<Params>) -> bool {
        params
            .get(Params::ShowAdvancedBlendModes)
            .ok()
            .and_then(|p| p.as_checkbox().ok().map(|c| c.value()))
            .unwrap_or(false)
    }

    fn color_count(params: &ae::Parameters<Params>) -> usize {
        params
            .get(Params::ColorCount)
            .ok()
            .and_then(|p| p.as_float_slider().ok().map(|s| s.value()))
            .map(|v| v.round() as usize)
            .unwrap_or(DEFAULT_COLORS)
            .clamp(MIN_COLORS, MAX_COLORS)
    }

    fn set_color_count(params: &mut ae::Parameters<Params>, count: usize) -> Result<(), Error> {
        let clamped = count.clamp(MIN_COLORS, MAX_COLORS);
        let mut count_param = params.get_mut(Params::ColorCount)?;
        count_param.as_float_slider_mut()?.set_value(clamped as f64);
        count_param.update_param_ui()?;
        Ok(())
    }

    fn handle_user_changed_param(
        &self,
        param_index: usize,
        params: &mut ae::Parameters<Params>,
        out_data: &mut OutData,
    ) -> Result<(), Error> {
        let changed = params.type_at(param_index);
        match changed {
            Params::ColorCount | Params::AddColor | Params::RemoveColor => {
                let current = Self::color_count(params);
                let next = match changed {
                    Params::AddColor => current.saturating_add(1),
                    Params::RemoveColor => current.saturating_sub(1),
                    _ => current,
                }
                .clamp(MIN_COLORS, MAX_COLORS);

                Self::set_color_count(params, next)?;
            }
            Params::ShowAdvancedBlendModes => {}
            _ => return Ok(()),
        }

        out_data.set_out_flag(OutFlags::RefreshUi, true);
        Ok(())
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let count = Self::color_count(params);
        let show_advanced = Self::show_advanced_blend_modes(params);

        for idx in 0..MAX_COLORS {
            let visible = idx < count;
            self.set_param_visible(in_data, params, COLOR_PARAMS[idx], visible)?;
            self.set_param_visible(in_data, params, MODE_PARAMS[idx], visible && !show_advanced)?;
            self.set_param_visible(
                in_data,
                params,
                ADVANCED_MODE_PARAMS[idx],
                visible && show_advanced,
            )?;
            self.set_param_visible(in_data, params, OPACITY_PARAMS[idx], visible)?;

            Self::set_param_enabled(params, COLOR_PARAMS[idx], visible)?;
            Self::set_param_enabled(params, MODE_PARAMS[idx], visible && !show_advanced)?;
            Self::set_param_enabled(params, ADVANCED_MODE_PARAMS[idx], visible && show_advanced)?;
            Self::set_param_enabled(params, OPACITY_PARAMS[idx], visible)?;
        }

        Self::set_param_enabled(params, Params::AddColor, count < MAX_COLORS)?;
        Self::set_param_enabled(params, Params::RemoveColor, count > MIN_COLORS)?;

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

    fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
        let active_colors = Self::color_count(params);
        let show_advanced = Self::show_advanced_blend_modes(params);
        let mut layers = Vec::with_capacity(active_colors);

        for idx in 0..active_colors {
            let color = params
                .get(COLOR_PARAMS[idx])?
                .as_color()?
                .value()
                .to_pixel32();
            let mode = if show_advanced {
                blend_mode_from_advanced_popup(
                    params.get(ADVANCED_MODE_PARAMS[idx])?.as_popup()?.value(),
                )
            } else {
                blend_mode_from_ae_popup(params.get(MODE_PARAMS[idx])?.as_popup()?.value())
            };
            let opacity_percent =
                params.get(OPACITY_PARAMS[idx])?.as_float_slider()?.value() as f32;
            let opacity = (opacity_percent / 100.0).clamp(0.0, 1.0);
            if opacity <= 0.0 {
                continue;
            }

            layers.push(CompositeLayer {
                color: color_to_straight_rgb(color),
                mode,
                opacity,
            });
        }

        Ok(RenderSettings { layers })
    }

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        if out_layer.width() == 0 || out_layer.height() == 0 {
            return Ok(());
        }

        let settings = Self::read_settings(params)?;
        let progress_final = out_layer.height() as i32;

        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x, _y, src, mut dst| {
                let src_px = read_input_pixel(src);
                let mut composed_rgb = pixel_to_straight_rgb(src_px);

                for layer in &settings.layers {
                    let blended = blend_rgb(composed_rgb, layer.color, layer.mode);
                    composed_rgb[0] = lerp(composed_rgb[0], blended[0], layer.opacity);
                    composed_rgb[1] = lerp(composed_rgb[1], blended[1], layer.opacity);
                    composed_rgb[2] = lerp(composed_rgb[2], blended[2], layer.opacity);
                }

                let out_px = straight_rgb_to_pixel(composed_rgb, src_px.alpha);
                write_output_pixel(&mut dst, out_px);
                Ok(())
            },
        )?;

        Ok(())
    }
}

fn default_color(index: usize) -> Pixel8 {
    DEFAULT_SWATCHES[index % DEFAULT_SWATCHES.len()]
}

fn blend_mode_from_ae_popup(value: i32) -> BlendMode {
    AE_BLEND_MODE_VALUES
        .get(value.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(BlendMode::Normal)
}

fn blend_mode_from_advanced_popup(value: i32) -> BlendMode {
    ADVANCED_BLEND_MODE_VALUES
        .get(value.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(BlendMode::Normal)
}

fn blend_rgb(base: [f32; 3], blend: [f32; 3], mode: BlendMode) -> [f32; 3] {
    match mode {
        BlendMode::Behind => base,
        BlendMode::Replace | BlendMode::PassThrough => blend,
        BlendMode::Erase => [0.0, 0.0, 0.0],
        BlendMode::Merge => [
            sanitize_channel(base[0] + blend[0]),
            sanitize_channel(base[1] + blend[1]),
            sanitize_channel(base[2] + blend[2]),
        ],
        BlendMode::Split => [
            sanitize_channel((base[0] - blend[0]).abs()),
            sanitize_channel((base[1] - blend[1]).abs()),
            sanitize_channel((base[2] - blend[2]).abs()),
        ],
        BlendMode::Dissolve => [
            if blend[0] > pseudo_random(base[0], blend[0]) {
                1.0
            } else {
                0.0
            },
            if blend[1] > pseudo_random(base[1], blend[1]) {
                1.0
            } else {
                0.0
            },
            if blend[2] > pseudo_random(base[2], blend[2]) {
                1.0
            } else {
                0.0
            },
        ],
        BlendMode::Hue => {
            let (_, bs, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (sh, _, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(sh, bs, bl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::Saturation => {
            let (bh, _, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_, ss, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(bh, ss, bl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::Color => {
            let (_, _, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (sh, ss, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(sh, ss, bl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::Luminosity => {
            let (bh, bs, _) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_, _, sl) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(bh, bs, sl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::Lightness => {
            let (bh, bs, _) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_, _, sl) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(bh, bs, sl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::HsvHue => {
            let (_, bs, bv) = rgb_to_hsv(base[0], base[1], base[2]);
            let (sh, _, _) = rgb_to_hsv(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsv_to_rgb(sh, bs, bv);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::HsvSaturation => {
            let (bh, _, bv) = rgb_to_hsv(base[0], base[1], base[2]);
            let (_, ss, _) = rgb_to_hsv(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsv_to_rgb(bh, ss, bv);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::ColorHsvl => {
            let (_, _, bv) = rgb_to_hsv(base[0], base[1], base[2]);
            let (sh, ss, _) = rgb_to_hsv(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsv_to_rgb(sh, ss, bv);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::HsvValue | BlendMode::LuminanceHsvl => {
            let (bh, bs, _) = rgb_to_hsv(base[0], base[1], base[2]);
            let (_, _, sv) = rgb_to_hsv(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsv_to_rgb(bh, bs, sv);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::HueDelta => {
            let (bh, bs, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (sh, _, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let delta = wrap_signed_unit(sh - bh);
            let (r, g, b) = hsl_to_rgb(wrap_unit(bh + delta * 0.5), bs, bl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::ColorLuminance => {
            let (_, _, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (sh, ss, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(sh, ss, bl);
            set_rgb_luminance([r, g, b], relative_luma(base[0], base[1], base[2]))
        }
        BlendMode::SaturationLightness => {
            let (bh, _, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_, ss, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(bh, ss, bl);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::SaturationLuminance => {
            let (bh, _, bl) = rgb_to_hsl(base[0], base[1], base[2]);
            let (_, ss, _) = rgb_to_hsl(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsl_to_rgb(bh, ss, bl);
            set_rgb_luminance([r, g, b], relative_luma(base[0], base[1], base[2]))
        }
        BlendMode::SaturationHsvl => {
            let (bh, _, bv) = rgb_to_hsv(base[0], base[1], base[2]);
            let (_, ss, _) = rgb_to_hsv(blend[0], blend[1], blend[2]);
            let (r, g, b) = hsv_to_rgb(bh, ss, bv);
            [
                sanitize_channel(r),
                sanitize_channel(g),
                sanitize_channel(b),
            ]
        }
        BlendMode::Luminance => {
            set_rgb_luminance(base, relative_luma(blend[0], blend[1], blend[2]))
        }
        BlendMode::DarkerColor => {
            let base_sum = base[0] + base[1] + base[2];
            let blend_sum = blend[0] + blend[1] + blend[2];
            if blend_sum < base_sum { blend } else { base }
        }
        BlendMode::LighterColor => {
            let base_luma = relative_luma(base[0], base[1], base[2]);
            let blend_luma = relative_luma(blend[0], blend[1], blend[2]);
            if blend_luma > base_luma { blend } else { base }
        }
        BlendMode::LumaDarkenOnly => {
            let base_luma = relative_luma(base[0], base[1], base[2]);
            let blend_luma = relative_luma(blend[0], blend[1], blend[2]);
            if blend_luma < base_luma { blend } else { base }
        }
        BlendMode::LumaLightenOnly => {
            let base_luma = relative_luma(base[0], base[1], base[2]);
            let blend_luma = relative_luma(blend[0], blend[1], blend[2]);
            if blend_luma > base_luma { blend } else { base }
        }
        BlendMode::ChannelRed => [blend[0], base[1], base[2]],
        BlendMode::ChannelGreen => [base[0], blend[1], base[2]],
        BlendMode::ChannelBlue => [base[0], base[1], blend[2]],
        BlendMode::ChannelYellow => [blend[0], blend[1], base[2]],
        BlendMode::ChannelAqua => [base[0], blend[1], blend[2]],
        BlendMode::ChannelMagenta => [blend[0], base[1], blend[2]],
        BlendMode::Cool => cool_blend(base, blend),
        BlendMode::Warm => warm_blend(base, blend),
        _ => [
            sanitize_channel(blend_channel(base[0], blend[0], mode)),
            sanitize_channel(blend_channel(base[1], blend[1], mode)),
            sanitize_channel(blend_channel(base[2], blend[2], mode)),
        ],
    }
}

fn blend_channel(b: f32, s: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal | BlendMode::PassThrough | BlendMode::Replace => s,
        BlendMode::Behind => b,
        BlendMode::Erase => 0.0,
        BlendMode::Merge => b + s,
        BlendMode::Split => (b - s).abs(),
        BlendMode::Dissolve => {
            if s > pseudo_random(b, s) {
                1.0
            } else {
                0.0
            }
        }
        BlendMode::Darken => b.min(s),
        BlendMode::Multiply => b * s,
        BlendMode::ColorBurn => {
            if s <= 0.0 {
                0.0
            } else {
                1.0 - (1.0 - b) / s.max(1.0e-6)
            }
        }
        BlendMode::LinearBurn => b + s - 1.0,
        BlendMode::Lighten => b.max(s),
        BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - s),
        BlendMode::ColorDodge => {
            if s >= 1.0 {
                1.0
            } else {
                b / (1.0 - s).max(1.0e-6)
            }
        }
        BlendMode::LinearDodge => b + s,
        BlendMode::Overlay => {
            if b <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        BlendMode::OverlayLegacy => overlay_legacy(b, s),
        BlendMode::SoftLight => soft_light_gimp(b, s),
        BlendMode::SoftLightLegacy => overlay_legacy(b, s),
        BlendMode::SoftLightPhotoshop => soft_light_photoshop(b, s),
        BlendMode::HardLight => {
            if s <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        BlendMode::VividLight => vivid_light(b, s),
        BlendMode::VividLightPhotoshop => vivid_light(b, s),
        BlendMode::LinearLight => b + 2.0 * s - 1.0,
        BlendMode::InverseLinearLight => b + 1.0 - 2.0 * s,
        BlendMode::PinLight => {
            if s < 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        BlendMode::PinLightInverse => {
            if b < 0.5 {
                s.min(2.0 * b)
            } else {
                s.max(2.0 * b - 1.0)
            }
        }
        BlendMode::PinLightStrong => {
            if s < 0.5 {
                b.min((4.0 * s - 1.0).clamp(0.0, 1.0))
            } else {
                b.max((4.0 * s - 2.0).clamp(0.0, 1.0))
            }
        }
        BlendMode::HardMix => {
            if vivid_light(b, s) < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::Subtract => b - s,
        BlendMode::Divide => {
            if s.abs() < 1.0e-6 {
                1.0
            } else {
                b / s
            }
        }
        BlendMode::Average => 0.5 * (b + s),
        BlendMode::GeometricMean => (b.max(0.0) * s.max(0.0)).sqrt(),
        BlendMode::HarmonicMean => {
            if (b + s).abs() < 1.0e-6 {
                0.0
            } else {
                2.0 * b * s / (b + s)
            }
        }
        BlendMode::Negation => 1.0 - (1.0 - b - s).abs(),
        BlendMode::Reflect => {
            if s >= 1.0 {
                1.0
            } else {
                b * b / (1.0 - s).max(1.0e-6)
            }
        }
        BlendMode::Glow => {
            if b >= 1.0 {
                1.0
            } else {
                s * s / (1.0 - b).max(1.0e-6)
            }
        }
        BlendMode::Phoenix => b.min(s) - b.max(s) + 1.0,
        BlendMode::GrainMerge => b + s - 0.5,
        BlendMode::GrainExtract => b - s + 0.5,
        BlendMode::Freeze => {
            if s <= 0.0 {
                0.0
            } else {
                1.0 - (1.0 - b).powi(2) / s.max(1.0e-6)
            }
        }
        BlendMode::Heat => {
            if b <= 0.0 {
                0.0
            } else {
                1.0 - (1.0 - s).powi(2) / b.max(1.0e-6)
            }
        }
        BlendMode::ColorErase => {
            if s <= 1.0e-6 {
                b
            } else {
                (b - s) / (1.0 - s).max(1.0e-6)
            }
        }
        BlendMode::Invert => 1.0 - b,
        BlendMode::InverseColorDodge => {
            if b >= 1.0 {
                1.0
            } else {
                s / (1.0 - b).max(1.0e-6)
            }
        }
        BlendMode::SoftDodge => {
            if s < 0.5 {
                b / (1.0 - 2.0 * s).max(1.0e-6)
            } else {
                b + (1.0 - b) * (2.0 * s - 1.0)
            }
        }
        BlendMode::InverseColorBurn => {
            if b <= 0.0 {
                0.0
            } else {
                1.0 - (1.0 - s) / b.max(1.0e-6)
            }
        }
        BlendMode::SoftBurn => {
            if s < 0.5 {
                b - (1.0 - 2.0 * s) * b * 0.5
            } else {
                1.0 - (1.0 - b) / (2.0 * s).max(1.0e-6)
            }
        }
        BlendMode::BitAnd => from_u8(to_u8(b) & to_u8(s)),
        BlendMode::BitOr => from_u8(to_u8(b) | to_u8(s)),
        BlendMode::BitXor => from_u8(to_u8(b) ^ to_u8(s)),
        BlendMode::BitShift => {
            let shift = ((s.clamp(0.0, 1.0) * 7.0).round() as u32).min(7);
            from_u8(to_u8(b).rotate_left(shift))
        }
        BlendMode::AddUnsigned => from_u8(to_u8(b).saturating_add(to_u8(s))),
        BlendMode::SubtractUnsigned => from_u8(to_u8(b).saturating_sub(to_u8(s))),
        BlendMode::Binarize => {
            if b >= s {
                1.0
            } else {
                0.0
            }
        }
        _ => s,
    }
}

fn overlay_legacy(b: f32, s: f32) -> f32 {
    (1.0 - s) * b * b + s * (1.0 - (1.0 - s) * (1.0 - s))
}

fn soft_light_gimp(b: f32, s: f32) -> f32 {
    (1.0 - 2.0 * s) * b * b + 2.0 * s * b
}

fn soft_light_photoshop(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

fn vivid_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        1.0 - (1.0 - b) / (2.0 * s).max(1.0e-6)
    } else {
        b / (1.0 - (2.0 * s - 1.0)).max(1.0e-6)
    }
}

fn set_rgb_luminance(rgb: [f32; 3], target_luma: f32) -> [f32; 3] {
    let src_luma = relative_luma(rgb[0], rgb[1], rgb[2]);
    let mut out = [
        rgb[0] + (target_luma - src_luma),
        rgb[1] + (target_luma - src_luma),
        rgb[2] + (target_luma - src_luma),
    ];
    out = clip_rgb_preserve_luma(out, target_luma);
    [
        sanitize_channel(out[0]),
        sanitize_channel(out[1]),
        sanitize_channel(out[2]),
    ]
}

fn clip_rgb_preserve_luma(mut rgb: [f32; 3], luma: f32) -> [f32; 3] {
    let mut min_c = rgb[0].min(rgb[1]).min(rgb[2]);

    if min_c < 0.0 {
        let denom = (luma - min_c).max(1.0e-6);
        rgb[0] = luma + ((rgb[0] - luma) * luma) / denom;
        rgb[1] = luma + ((rgb[1] - luma) * luma) / denom;
        rgb[2] = luma + ((rgb[2] - luma) * luma) / denom;
    }

    min_c = rgb[0].min(rgb[1]).min(rgb[2]);
    let max_c = rgb[0].max(rgb[1]).max(rgb[2]);
    if max_c > 1.0 {
        let denom = (max_c - luma).max(1.0e-6);
        rgb[0] = luma + ((rgb[0] - luma) * (1.0 - luma)) / denom;
        rgb[1] = luma + ((rgb[1] - luma) * (1.0 - luma)) / denom;
        rgb[2] = luma + ((rgb[2] - luma) * (1.0 - luma)) / denom;
    }

    let _ = min_c;
    rgb
}

fn cool_blend(base: [f32; 3], blend: [f32; 3]) -> [f32; 3] {
    let t = relative_luma(blend[0], blend[1], blend[2]).clamp(0.0, 1.0);
    [
        sanitize_channel(base[0] * (1.0 - 0.6 * t)),
        sanitize_channel(base[1] * (1.0 - 0.2 * t) + 0.1 * t),
        sanitize_channel(base[2] + (1.0 - base[2]) * 0.6 * t),
    ]
}

fn warm_blend(base: [f32; 3], blend: [f32; 3]) -> [f32; 3] {
    let t = relative_luma(blend[0], blend[1], blend[2]).clamp(0.0, 1.0);
    [
        sanitize_channel(base[0] + (1.0 - base[0]) * 0.6 * t),
        sanitize_channel(base[1] * (1.0 - 0.1 * t) + 0.1 * t),
        sanitize_channel(base[2] * (1.0 - 0.6 * t)),
    ]
}

fn pseudo_random(a: f32, b: f32) -> f32 {
    let x = (a * 12.989_8 + b * 78.233).sin() * 43_758.547;
    x - x.floor()
}

fn to_u8(v: f32) -> u8 {
    (sanitize_channel(v).clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn from_u8(v: u8) -> f32 {
    v as f32 / 255.0
}

fn wrap_unit(v: f32) -> f32 {
    let mut x = v - v.floor();
    if x < 0.0 {
        x += 1.0;
    }
    x
}

fn wrap_signed_unit(v: f32) -> f32 {
    let mut x = v;
    if x > 0.5 {
        x -= 1.0;
    }
    if x < -0.5 {
        x += 1.0;
    }
    x
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max <= 1.0e-6 { 0.0 } else { delta / max };
    if delta <= 1.0e-6 {
        return (0.0, s, v);
    }
    let h = if max == r {
        (g - b) / delta + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    (h, s, v)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s <= 1.0e-6 {
        return (v, v, v);
    }
    let hh = (wrap_unit(h) * 6.0).clamp(0.0, 6.0 - 1.0e-6);
    let i = hh.floor() as i32;
    let f = hh - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

fn relative_luma(r: f32, g: f32, b: f32) -> f32 {
    0.212_6 * r + 0.715_2 * g + 0.072_2 * b
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let delta = max - min;
    if delta.abs() < 1.0e-6 {
        return (0.0, 0.0, l);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        (g - b) / delta + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s.abs() < 1.0e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hk = h - h.floor();
    let t = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (t(hk + 1.0 / 3.0), t(hk), t(hk - 1.0 / 3.0))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn sanitize_channel(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn color_to_straight_rgb(color: PixelF32) -> [f32; 3] {
    pixel_to_straight_rgb(color)
}

fn pixel_to_straight_rgb(px: PixelF32) -> [f32; 3] {
    if px.alpha > ALPHA_EPSILON {
        [px.red / px.alpha, px.green / px.alpha, px.blue / px.alpha]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn straight_rgb_to_pixel(rgb: [f32; 3], alpha: f32) -> PixelF32 {
    if alpha > ALPHA_EPSILON {
        PixelF32 {
            alpha,
            red: sanitize_channel(rgb[0]) * alpha,
            green: sanitize_channel(rgb[1]) * alpha,
            blue: sanitize_channel(rgb[2]) * alpha,
        }
    } else {
        PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        }
    }
}

fn read_input_pixel(src: GenericPixel<'_>) -> PixelF32 {
    match src {
        GenericPixel::Pixel8(p) => p.to_pixel32(),
        GenericPixel::Pixel16(p) => p.to_pixel32(),
        GenericPixel::PixelF32(p) => *p,
        GenericPixel::PixelF64(p) => PixelF32 {
            alpha: p.alphaF as f32,
            red: p.redF as f32,
            green: p.greenF as f32,
            blue: p.blueF as f32,
        },
    }
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
