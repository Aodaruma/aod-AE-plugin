#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, LinSrgb, Oklab, Oklch, Srgb};
use std::env;
use std::f32::consts::TAU;

use ae::pf::*;
use utils::ToPixel;

const OKLCH_CHROMA_MAX: f32 = 0.4;
const GOLDEN_ANGLE: f32 = 2.399_963_1;
const BLUE_NOISE_8X8: [u8; 64] = [
    0, 48, 12, 60, 3, 51, 15, 63, 32, 16, 44, 28, 35, 19, 47, 31, 8, 56, 4, 52, 11, 59, 7, 55, 40,
    24, 36, 20, 43, 27, 39, 23, 2, 50, 14, 62, 1, 49, 13, 61, 34, 18, 46, 30, 33, 17, 45, 29, 10,
    58, 6, 54, 9, 57, 5, 53, 42, 26, 38, 22, 41, 25, 37, 21,
];

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ColorSpace,
    ScatterRadius,
    ScatterSamples,
    ScatterAlgorithm,
    SamplingDistribution,
    DistributionShape,
    GrainSize,
    Direction,
    Anisotropy,
    MapGroupStart,
    UseScatterMap,
    ScatterMapLayer,
    ScatterMapMode,
    ScatterMapChannel,
    UseGrainMap,
    GrainMapLayer,
    GrainMapMode,
    GrainMapChannel,
    MapGroupEnd,
    AnisotropyGroupStart,
    UseAnisotropyMap,
    AnisotropyMapLayer,
    AnisotropyMapMode,
    UseAnisotropyDirection,
    UseAnisotropyStrength,
    AnisotropyDivergenceSource,
    AnisotropyGroupEnd,
    TextureGroupStart,
    TextureMode,
    TextureLayer,
    TextureMapMode,
    TextureMapChannel,
    TextureInfluence,
    TextureGroupEnd,
    OutputGroupStart,
    Seed,
    EdgeMode,
    BlendMode,
    BlendOpacity,
    PreserveAlpha,
    Clamp32,
    OutputGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScatterColorSpace {
    LinearRgba,
    LinearRgb,
    Alpha,
    Srgb,
    Oklab,
    Oklch,
    Hsl,
    Hsv,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScatterAlgorithm {
    FbmVector,
    CellBlock,
    CellSmooth,
    NoiseVector,
    DomainWarpFbm,
    CurlFbm,
    LegacySquareCell,
    LegacySmoothGrid,
    LegacyVoronoiCell,
    LegacyBlueNoise,
}

impl ScatterAlgorithm {
    fn is_legacy_sampling(self) -> bool {
        matches!(
            self,
            ScatterAlgorithm::LegacySquareCell
                | ScatterAlgorithm::LegacySmoothGrid
                | ScatterAlgorithm::LegacyVoronoiCell
                | ScatterAlgorithm::LegacyBlueNoise
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SamplingDistribution {
    Uniform,
    Gaussian,
    Exponential,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarMapMode {
    Gray,
    HsvValue,
    HslLightness,
    RgbaChannel,
}

#[derive(Clone, Copy)]
enum RgbaChannel {
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnisotropyMapMode {
    HueSaturation,
    Uv,
    Normal,
    DivergenceDirection,
    DivergenceRotation,
}

#[derive(Clone, Copy)]
enum DivergenceSource {
    Gray,
    Red,
    Green,
    Blue,
    Alpha,
    HsvValue,
    HslLightness,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextureMode {
    Off,
    Amount,
    Direction,
    AmountAndDirection,
}

#[derive(Clone, Copy)]
enum EdgeMode {
    None,
    Repeat,
    Tile,
    Mirror,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputBlendMode {
    None,
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
}

#[derive(Clone, Copy)]
struct RenderSettings {
    color_space: ScatterColorSpace,
    scatter_radius: f32,
    samples: u32,
    scatter_algorithm: ScatterAlgorithm,
    sampling_distribution: SamplingDistribution,
    distribution_shape: f32,
    use_scatter_map: bool,
    scatter_map_mode: ScalarMapMode,
    scatter_map_channel: RgbaChannel,
    grain_size: f32,
    use_grain_map: bool,
    grain_map_mode: ScalarMapMode,
    grain_map_channel: RgbaChannel,
    direction: f32,
    anisotropy: f32,
    use_anisotropy_map: bool,
    anisotropy_map_mode: AnisotropyMapMode,
    use_anisotropy_direction: bool,
    use_anisotropy_strength: bool,
    anisotropy_divergence_source: DivergenceSource,
    texture_mode: TextureMode,
    texture_map_mode: ScalarMapMode,
    texture_map_channel: RgbaChannel,
    texture_influence: f32,
    seed: u32,
    edge_mode: EdgeMode,
    blend_mode: OutputBlendMode,
    blend_opacity: f32,
    preserve_alpha: bool,
    clamp_32: bool,
}

#[derive(Clone)]
struct LayerBuffer {
    width: usize,
    height: usize,
    pixels: Vec<PixelF32>,
}

#[derive(Clone, Copy)]
struct FlowVector {
    dir_x: f32,
    dir_y: f32,
    strength: f32,
    valid: bool,
}

#[derive(Clone, Copy)]
struct OutputCoord {
    x: usize,
    y: usize,
    out_w: usize,
    out_h: usize,
}

#[derive(Clone, Copy)]
struct ScatterSampleParams {
    center_x: f32,
    center_y: f32,
    radius: f32,
    grain: f32,
    angle: f32,
    anisotropy: f32,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Applies map-driven stochastic scatter to layers.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::ColorSpace,
            "Color Space",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Linear RGBA",
                    "Linear RGB",
                    "Alpha",
                    "sRGB",
                    "OKLab",
                    "OKLCH",
                    "HSL",
                    "HSV",
                ]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::ScatterRadius,
            "Scatter Amount (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(4096.0);
                d.set_slider_min(0.0);
                d.set_slider_max(256.0);
                d.set_default(24.0);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::ScatterSamples,
            "Complexity",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(128);
                d.set_slider_min(1);
                d.set_slider_max(32);
                d.set_default(4);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::ScatterAlgorithm,
            "Scatter Algorithm",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "fBM Vector",
                    "Cell Block",
                    "Cell Smooth",
                    "Noise Vector",
                    "Domain Warp fBM",
                    "Curl fBM",
                    "Legacy Square Cell",
                    "Legacy Smooth Grid",
                    "Legacy Voronoi Cell",
                    "Legacy Blue Noise",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::SamplingDistribution,
            "Sampling Distribution",
            PopupDef::setup(|d| {
                d.set_options(&["Uniform", "Gaussian", "Exponential"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::DistributionShape,
            "Distribution Shape",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(32.0);
                d.set_slider_min(0.0);
                d.set_slider_max(8.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::GrainSize,
            "Grain Size (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(4096.0);
                d.set_slider_min(1.0);
                d.set_slider_max(256.0);
                d.set_default(2.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Direction,
            "Direction (deg)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-3600.0);
                d.set_valid_max(3600.0);
                d.set_slider_min(-180.0);
                d.set_slider_max(180.0);
                d.set_default(0.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Anisotropy,
            "Anisotropy",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add_group(
            Params::MapGroupStart,
            Params::MapGroupEnd,
            "Maps",
            true,
            |params| {
                params.add_with_flags(
                    Params::UseScatterMap,
                    "Map Scatter Amount",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::ScatterMapLayer,
                    "Scatter Map Layer",
                    LayerDef::new(),
                )?;

                params.add_with_flags(
                    Params::ScatterMapMode,
                    "Scatter Map Source",
                    PopupDef::setup(|d| {
                        d.set_options(&["Gray", "HSV Value", "HSL Lightness", "RGBA Channel"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::ScatterMapChannel,
                    "Scatter RGBA Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;

                params.add_with_flags(
                    Params::UseGrainMap,
                    "Map Grain Size",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(Params::GrainMapLayer, "Grain Map Layer", LayerDef::new())?;

                params.add_with_flags(
                    Params::GrainMapMode,
                    "Grain Map Source",
                    PopupDef::setup(|d| {
                        d.set_options(&["Gray", "HSV Value", "HSL Lightness", "RGBA Channel"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::GrainMapChannel,
                    "Grain RGBA Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::AnisotropyGroupStart,
            Params::AnisotropyGroupEnd,
            "Anisotropy Map",
            true,
            |params| {
                params.add_with_flags(
                    Params::UseAnisotropyMap,
                    "Map Anisotropy",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::AnisotropyMapLayer,
                    "Anisotropy Map Layer",
                    LayerDef::new(),
                )?;

                params.add_with_flags(
                    Params::AnisotropyMapMode,
                    "Anisotropy Map Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Hue / Saturation",
                            "UV (XY -> RG)",
                            "Normal (RG)",
                            "Divergence Direction",
                            "Divergence Rotation",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::UseAnisotropyDirection,
                    "Use Map Direction",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                params.add(
                    Params::UseAnisotropyStrength,
                    "Use Map Strength",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                params.add(
                    Params::AnisotropyDivergenceSource,
                    "Divergence Source",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Gray",
                            "Red",
                            "Green",
                            "Blue",
                            "Alpha",
                            "HSV Value",
                            "HSL Lightness",
                        ]);
                        d.set_default(1);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::TextureGroupStart,
            Params::TextureGroupEnd,
            "Texture",
            true,
            |params| {
                params.add_with_flags(
                    Params::TextureMode,
                    "Texture Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Off",
                            "Amount Modulation",
                            "Direction Jitter",
                            "Amount + Direction",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(Params::TextureLayer, "Texture Layer", LayerDef::new())?;

                params.add_with_flags(
                    Params::TextureMapMode,
                    "Texture Source",
                    PopupDef::setup(|d| {
                        d.set_options(&["Gray", "HSV Value", "HSL Lightness", "RGBA Channel"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::TextureMapChannel,
                    "Texture RGBA Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::TextureInfluence,
                    "Texture Influence",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.5);
                        d.set_precision(3);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::OutputGroupStart,
            Params::OutputGroupEnd,
            "Output",
            true,
            |params| {
                params.add(
                    Params::Seed,
                    "Seed",
                    SliderDef::setup(|d| {
                        d.set_valid_min(0);
                        d.set_valid_max(100000);
                        d.set_slider_min(0);
                        d.set_slider_max(10000);
                        d.set_default(0);
                    }),
                )?;

                params.add(
                    Params::EdgeMode,
                    "Edge Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["None (Zero)", "Repeat", "Tile", "Mirror"]);
                        d.set_default(2);
                    }),
                )?;

                params.add_with_flags(
                    Params::BlendMode,
                    "Blend With Original",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "None",
                            "Normal",
                            "Add",
                            "Multiply",
                            "Screen",
                            "Overlay",
                            "Difference",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::BlendOpacity,
                    "Blend Opacity (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(100.0);
                        d.set_precision(1);
                    }),
                )?;

                params.add(
                    Params::PreserveAlpha,
                    "Preserve Alpha",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                params.add(
                    Params::Clamp32,
                    "Clamp (32bpc)",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
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
                        "AOD_ScatterMap - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ScatterMap")
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
            ae::Command::UserChangedParam { param_index }
                if param_affects_ui(params.type_at(param_index)) =>
            {
                out_data.set_out_flag(OutFlags::RefreshUi, true);
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
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let scatter_algorithm =
            scatter_algorithm_from_popup(params.get(Params::ScatterAlgorithm)?.as_popup()?.value());
        let use_legacy_sampling = scatter_algorithm.is_legacy_sampling();
        Self::set_param_name(
            params,
            Params::ScatterSamples,
            if use_legacy_sampling {
                "Samples"
            } else {
                "Complexity"
            },
        )?;

        let use_scatter_map = params.get(Params::UseScatterMap)?.as_checkbox()?.value();
        let scatter_map_mode =
            scalar_map_mode_from_popup(params.get(Params::ScatterMapMode)?.as_popup()?.value());
        self.set_param_visible(in_data, params, Params::ScatterMapLayer, use_scatter_map)?;
        self.set_param_visible(in_data, params, Params::ScatterMapMode, use_scatter_map)?;
        self.set_param_visible(
            in_data,
            params,
            Params::ScatterMapChannel,
            use_scatter_map && matches!(scatter_map_mode, ScalarMapMode::RgbaChannel),
        )?;

        let use_grain_map = params.get(Params::UseGrainMap)?.as_checkbox()?.value();
        let grain_map_mode =
            scalar_map_mode_from_popup(params.get(Params::GrainMapMode)?.as_popup()?.value());
        self.set_param_visible(in_data, params, Params::GrainMapLayer, use_grain_map)?;
        self.set_param_visible(in_data, params, Params::GrainMapMode, use_grain_map)?;
        self.set_param_visible(
            in_data,
            params,
            Params::GrainMapChannel,
            use_grain_map && matches!(grain_map_mode, ScalarMapMode::RgbaChannel),
        )?;

        let use_anisotropy_map = params.get(Params::UseAnisotropyMap)?.as_checkbox()?.value();
        let anisotropy_mode = anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        );
        let show_divergence_source = use_anisotropy_map
            && matches!(
                anisotropy_mode,
                AnisotropyMapMode::DivergenceDirection | AnisotropyMapMode::DivergenceRotation
            );
        self.set_param_visible(
            in_data,
            params,
            Params::AnisotropyMapLayer,
            use_anisotropy_map,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::AnisotropyMapMode,
            use_anisotropy_map,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::UseAnisotropyDirection,
            use_anisotropy_map,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::UseAnisotropyStrength,
            use_anisotropy_map,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::AnisotropyDivergenceSource,
            show_divergence_source,
        )?;

        let texture_mode =
            texture_mode_from_popup(params.get(Params::TextureMode)?.as_popup()?.value());
        let use_texture = !matches!(texture_mode, TextureMode::Off);
        let texture_map_mode =
            scalar_map_mode_from_popup(params.get(Params::TextureMapMode)?.as_popup()?.value());
        self.set_param_visible(in_data, params, Params::TextureLayer, use_texture)?;
        self.set_param_visible(in_data, params, Params::TextureMapMode, use_texture)?;
        self.set_param_visible(
            in_data,
            params,
            Params::TextureMapChannel,
            use_texture && matches!(texture_map_mode, ScalarMapMode::RgbaChannel),
        )?;
        self.set_param_visible(in_data, params, Params::TextureInfluence, use_texture)?;

        let blend_mode =
            output_blend_mode_from_popup(params.get(Params::BlendMode)?.as_popup()?.value());
        self.set_param_visible(
            in_data,
            params,
            Params::BlendOpacity,
            !matches!(blend_mode, OutputBlendMode::None),
        )?;

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
        _in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        let src_w = in_layer.width();
        let src_h = in_layer.height();
        if out_w == 0 || out_h == 0 || src_w == 0 || src_h == 0 {
            return Ok(());
        }

        let settings = read_render_settings(params)?;
        let source = read_layer_buffer(&in_layer);

        let scatter_map =
            checkout_layer_buffer(params, Params::ScatterMapLayer, settings.use_scatter_map)?;
        let grain_map =
            checkout_layer_buffer(params, Params::GrainMapLayer, settings.use_grain_map)?;
        let anisotropy_map = checkout_layer_buffer(
            params,
            Params::AnisotropyMapLayer,
            settings.use_anisotropy_map,
        )?;
        let texture = checkout_layer_buffer(
            params,
            Params::TextureLayer,
            !matches!(settings.texture_mode, TextureMode::Off),
        )?;

        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        out_layer.iterate(0, out_h as i32, None, |x, y, mut dst| {
            let out_x = x as usize;
            let out_y = y as usize;
            let coord = OutputCoord {
                x: out_x,
                y: out_y,
                out_w,
                out_h,
            };
            let center_x = remap_coord_to_layer_float(out_x, out_w, source.width);
            let center_y = remap_coord_to_layer_float(out_y, out_h, source.height);
            let center = sample_bilinear(&source, center_x, center_y, EdgeMode::Repeat);

            let scatter_factor = scalar_map_value(
                scatter_map.as_ref(),
                coord,
                settings.scatter_map_mode,
                settings.scatter_map_channel,
                1.0,
            );
            let grain_factor = scalar_map_value(
                grain_map.as_ref(),
                coord,
                settings.grain_map_mode,
                settings.grain_map_channel,
                1.0,
            );
            let texture_value = scalar_map_value(
                texture.as_ref(),
                coord,
                settings.texture_map_mode,
                settings.texture_map_channel,
                0.5,
            );

            let mut radius = settings.scatter_radius * scatter_factor.max(0.0);
            let mut angle = settings.direction;
            if matches!(
                settings.texture_mode,
                TextureMode::Amount | TextureMode::AmountAndDirection
            ) {
                let amount_mod = 1.0 + (texture_value - 0.5) * 2.0 * settings.texture_influence;
                radius *= amount_mod.max(0.0);
            }
            if matches!(
                settings.texture_mode,
                TextureMode::Direction | TextureMode::AmountAndDirection
            ) {
                angle += (texture_value - 0.5) * TAU * settings.texture_influence;
            }

            let mut anisotropy = settings.anisotropy;
            if settings.use_anisotropy_map {
                let flow = anisotropy_flow_at(
                    anisotropy_map.as_ref(),
                    coord,
                    settings.anisotropy_map_mode,
                    settings.anisotropy_divergence_source,
                );
                if flow.valid {
                    if settings.use_anisotropy_direction {
                        angle = flow.dir_y.atan2(flow.dir_x);
                    }
                    if settings.use_anisotropy_strength {
                        anisotropy *= flow.strength.clamp(0.0, 1.0);
                    }
                }
            }

            let mut out_px = if radius <= 1.0e-6 {
                center
            } else {
                let grain = (settings.grain_size * grain_factor.max(0.0)).max(1.0);
                let sample_params = ScatterSampleParams {
                    center_x,
                    center_y,
                    radius,
                    grain,
                    angle,
                    anisotropy: anisotropy.clamp(0.0, 1.0),
                };
                if settings.scatter_algorithm.is_legacy_sampling() {
                    scatter_pixel(&source, center, sample_params, &settings)
                } else {
                    displace_pixel(&source, sample_params, &settings)
                }
            };

            if matches!(settings.color_space, ScatterColorSpace::Alpha) {
                out_px.red = center.red;
                out_px.green = center.green;
                out_px.blue = center.blue;
            }
            if settings.preserve_alpha && !matches!(settings.color_space, ScatterColorSpace::Alpha)
            {
                out_px.alpha = center.alpha;
            }
            out_px =
                blend_with_original(out_px, center, settings.blend_mode, settings.blend_opacity);
            out_px = sanitize_pixel_for_output(out_px, out_is_f32, settings.clamp_32);

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

fn param_affects_ui(param: Params) -> bool {
    matches!(
        param,
        Params::ScatterAlgorithm
            | Params::UseScatterMap
            | Params::ScatterMapMode
            | Params::UseGrainMap
            | Params::GrainMapMode
            | Params::UseAnisotropyMap
            | Params::AnisotropyMapMode
            | Params::TextureMode
            | Params::TextureMapMode
            | Params::BlendMode
    )
}

fn read_render_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    Ok(RenderSettings {
        color_space: color_space_from_popup(params.get(Params::ColorSpace)?.as_popup()?.value()),
        scatter_radius: params
            .get(Params::ScatterRadius)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        samples: params
            .get(Params::ScatterSamples)?
            .as_slider()?
            .value()
            .clamp(1, 128) as u32,
        scatter_algorithm: scatter_algorithm_from_popup(
            params.get(Params::ScatterAlgorithm)?.as_popup()?.value(),
        ),
        sampling_distribution: sampling_distribution_from_popup(
            params
                .get(Params::SamplingDistribution)?
                .as_popup()?
                .value(),
        ),
        distribution_shape: (params
            .get(Params::DistributionShape)?
            .as_float_slider()?
            .value() as f32)
            .clamp(0.0, 32.0),
        use_scatter_map: params.get(Params::UseScatterMap)?.as_checkbox()?.value(),
        scatter_map_mode: scalar_map_mode_from_popup(
            params.get(Params::ScatterMapMode)?.as_popup()?.value(),
        ),
        scatter_map_channel: rgba_channel_from_popup(
            params.get(Params::ScatterMapChannel)?.as_popup()?.value(),
        ),
        grain_size: params
            .get(Params::GrainSize)?
            .as_float_slider()?
            .value()
            .max(1.0) as f32,
        use_grain_map: params.get(Params::UseGrainMap)?.as_checkbox()?.value(),
        grain_map_mode: scalar_map_mode_from_popup(
            params.get(Params::GrainMapMode)?.as_popup()?.value(),
        ),
        grain_map_channel: rgba_channel_from_popup(
            params.get(Params::GrainMapChannel)?.as_popup()?.value(),
        ),
        direction: (params.get(Params::Direction)?.as_float_slider()?.value() as f32).to_radians(),
        anisotropy: (params.get(Params::Anisotropy)?.as_float_slider()?.value() as f32)
            .clamp(0.0, 1.0),
        use_anisotropy_map: params.get(Params::UseAnisotropyMap)?.as_checkbox()?.value(),
        anisotropy_map_mode: anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        ),
        use_anisotropy_direction: params
            .get(Params::UseAnisotropyDirection)?
            .as_checkbox()?
            .value(),
        use_anisotropy_strength: params
            .get(Params::UseAnisotropyStrength)?
            .as_checkbox()?
            .value(),
        anisotropy_divergence_source: divergence_source_from_popup(
            params
                .get(Params::AnisotropyDivergenceSource)?
                .as_popup()?
                .value(),
        ),
        texture_mode: texture_mode_from_popup(params.get(Params::TextureMode)?.as_popup()?.value()),
        texture_map_mode: scalar_map_mode_from_popup(
            params.get(Params::TextureMapMode)?.as_popup()?.value(),
        ),
        texture_map_channel: rgba_channel_from_popup(
            params.get(Params::TextureMapChannel)?.as_popup()?.value(),
        ),
        texture_influence: (params
            .get(Params::TextureInfluence)?
            .as_float_slider()?
            .value() as f32)
            .clamp(0.0, 1.0),
        seed: params.get(Params::Seed)?.as_slider()?.value() as u32,
        edge_mode: edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value()),
        blend_mode: output_blend_mode_from_popup(
            params.get(Params::BlendMode)?.as_popup()?.value(),
        ),
        blend_opacity: (params.get(Params::BlendOpacity)?.as_float_slider()?.value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        preserve_alpha: params.get(Params::PreserveAlpha)?.as_checkbox()?.value(),
        clamp_32: params.get(Params::Clamp32)?.as_checkbox()?.value(),
    })
}

fn color_space_from_popup(value: i32) -> ScatterColorSpace {
    match value {
        2 => ScatterColorSpace::LinearRgb,
        3 => ScatterColorSpace::Alpha,
        4 => ScatterColorSpace::Srgb,
        5 => ScatterColorSpace::Oklab,
        6 => ScatterColorSpace::Oklch,
        7 => ScatterColorSpace::Hsl,
        8 => ScatterColorSpace::Hsv,
        _ => ScatterColorSpace::LinearRgba,
    }
}

fn scatter_algorithm_from_popup(value: i32) -> ScatterAlgorithm {
    match value {
        2 => ScatterAlgorithm::CellBlock,
        3 => ScatterAlgorithm::CellSmooth,
        4 => ScatterAlgorithm::NoiseVector,
        5 => ScatterAlgorithm::DomainWarpFbm,
        6 => ScatterAlgorithm::CurlFbm,
        7 => ScatterAlgorithm::LegacySquareCell,
        8 => ScatterAlgorithm::LegacySmoothGrid,
        9 => ScatterAlgorithm::LegacyVoronoiCell,
        10 => ScatterAlgorithm::LegacyBlueNoise,
        _ => ScatterAlgorithm::FbmVector,
    }
}

fn sampling_distribution_from_popup(value: i32) -> SamplingDistribution {
    match value {
        2 => SamplingDistribution::Gaussian,
        3 => SamplingDistribution::Exponential,
        _ => SamplingDistribution::Uniform,
    }
}

fn scalar_map_mode_from_popup(value: i32) -> ScalarMapMode {
    match value {
        2 => ScalarMapMode::HsvValue,
        3 => ScalarMapMode::HslLightness,
        4 => ScalarMapMode::RgbaChannel,
        _ => ScalarMapMode::Gray,
    }
}

fn rgba_channel_from_popup(value: i32) -> RgbaChannel {
    match value {
        2 => RgbaChannel::Green,
        3 => RgbaChannel::Blue,
        4 => RgbaChannel::Alpha,
        _ => RgbaChannel::Red,
    }
}

fn anisotropy_map_mode_from_popup(value: i32) -> AnisotropyMapMode {
    match value {
        2 => AnisotropyMapMode::Uv,
        3 => AnisotropyMapMode::Normal,
        4 => AnisotropyMapMode::DivergenceDirection,
        5 => AnisotropyMapMode::DivergenceRotation,
        _ => AnisotropyMapMode::HueSaturation,
    }
}

fn divergence_source_from_popup(value: i32) -> DivergenceSource {
    match value {
        2 => DivergenceSource::Red,
        3 => DivergenceSource::Green,
        4 => DivergenceSource::Blue,
        5 => DivergenceSource::Alpha,
        6 => DivergenceSource::HsvValue,
        7 => DivergenceSource::HslLightness,
        _ => DivergenceSource::Gray,
    }
}

fn texture_mode_from_popup(value: i32) -> TextureMode {
    match value {
        2 => TextureMode::Amount,
        3 => TextureMode::Direction,
        4 => TextureMode::AmountAndDirection,
        _ => TextureMode::Off,
    }
}

fn edge_mode_from_popup(value: i32) -> EdgeMode {
    match value {
        1 => EdgeMode::None,
        3 => EdgeMode::Tile,
        4 => EdgeMode::Mirror,
        _ => EdgeMode::Repeat,
    }
}

fn output_blend_mode_from_popup(value: i32) -> OutputBlendMode {
    match value {
        2 => OutputBlendMode::Normal,
        3 => OutputBlendMode::Add,
        4 => OutputBlendMode::Multiply,
        5 => OutputBlendMode::Screen,
        6 => OutputBlendMode::Overlay,
        7 => OutputBlendMode::Difference,
        _ => OutputBlendMode::None,
    }
}

fn checkout_layer_buffer(
    params: &mut Parameters<Params>,
    id: Params,
    enabled: bool,
) -> Result<Option<LayerBuffer>, Error> {
    if !enabled {
        return Ok(None);
    }

    let checkout = params.checkout_at(id, None, None, None)?;
    let layer = checkout.as_layer()?.value();
    Ok(layer.as_ref().map(read_layer_buffer))
}

fn read_layer_buffer(layer: &Layer) -> LayerBuffer {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut pixels = vec![
        PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        width * height
    ];

    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = read_pixel_f32(layer, world_type, x, y);
        }
    }

    LayerBuffer {
        width,
        height,
        pixels,
    }
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn displace_pixel(
    src: &LayerBuffer,
    sample_params: ScatterSampleParams,
    settings: &RenderSettings,
) -> PixelF32 {
    let (vx, vy) = displacement_vector(sample_params, settings);
    let (dx, dy) = anisotropic_displacement(vx, vy, sample_params);
    sample_bilinear(
        src,
        sample_params.center_x + dx,
        sample_params.center_y + dy,
        settings.edge_mode,
    )
}

fn displacement_vector(
    sample_params: ScatterSampleParams,
    settings: &RenderSettings,
) -> (f32, f32) {
    let p = (
        sample_params.center_x / sample_params.grain,
        sample_params.center_y / sample_params.grain,
    );
    let octaves = settings.samples.clamp(1, 8);
    let v = match settings.scatter_algorithm {
        ScatterAlgorithm::CellBlock => cell_block_displacement(p, settings.seed),
        ScatterAlgorithm::CellSmooth => cell_smooth_displacement(p, settings.seed),
        ScatterAlgorithm::NoiseVector => noise_vector(p, settings.seed),
        ScatterAlgorithm::DomainWarpFbm => domain_warp_fbm_displacement(p, octaves, settings.seed),
        ScatterAlgorithm::CurlFbm => curl_fbm_displacement(p, octaves, settings.seed),
        ScatterAlgorithm::FbmVector => fbm_vector_displacement(p, octaves, settings.seed),
        _ => (0.0, 0.0),
    };
    shape_displacement_vector(v, settings)
}

fn shape_displacement_vector(v: (f32, f32), settings: &RenderSettings) -> (f32, f32) {
    let v = limit_vector(v);
    let len2 = (v.0 * v.0 + v.1 * v.1).clamp(0.0, 1.0);
    if len2 <= 1.0e-8 {
        return v;
    }

    let len = len2.sqrt();
    let shaped_len = sample_radius(
        len2,
        settings.sampling_distribution,
        settings.distribution_shape,
    );
    let scale = shaped_len / len;
    (v.0 * scale, v.1 * scale)
}

fn anisotropic_displacement(vx: f32, vy: f32, sample_params: ScatterSampleParams) -> (f32, f32) {
    let dir_x = sample_params.angle.cos();
    let dir_y = sample_params.angle.sin();
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let minor = (1.0 - sample_params.anisotropy).clamp(0.0, 1.0);
    let u = vx * sample_params.radius;
    let v = vy * sample_params.radius * minor;
    (dir_x * u + perp_x * v, dir_y * u + perp_y * v)
}

fn cell_block_displacement(p: (f32, f32), seed: u32) -> (f32, f32) {
    hash_vector_2d(p.0.floor() as i32, p.1.floor() as i32, seed)
}

fn cell_smooth_displacement(p: (f32, f32), seed: u32) -> (f32, f32) {
    let cell_x = p.0.floor() as i32;
    let cell_y = p.1.floor() as i32;
    let fx = smoothstep(p.0 - cell_x as f32);
    let fy = smoothstep(p.1 - cell_y as f32);

    let h00 = hash_vector_2d(cell_x, cell_y, seed);
    let h10 = hash_vector_2d(cell_x + 1, cell_y, seed);
    let h01 = hash_vector_2d(cell_x, cell_y + 1, seed);
    let h11 = hash_vector_2d(cell_x + 1, cell_y + 1, seed);

    let top = (lerp(h00.0, h10.0, fx), lerp(h00.1, h10.1, fx));
    let bottom = (lerp(h01.0, h11.0, fx), lerp(h01.1, h11.1, fx));
    (lerp(top.0, bottom.0, fy), lerp(top.1, bottom.1, fy))
}

fn noise_vector(p: (f32, f32), seed: u32) -> (f32, f32) {
    (
        value_noise_signed(p.0, p.1, seed, 0x21),
        value_noise_signed(p.0 + 19.19, p.1 - 7.31, seed, 0x4D),
    )
}

fn fbm_vector_displacement(p: (f32, f32), octaves: u32, seed: u32) -> (f32, f32) {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut amp = 1.0;
    let mut total_amp = 0.0;
    let mut freq = 1.0;
    for octave in 0..octaves {
        let octave_seed = seed ^ octave.wrapping_mul(0x9E37_79B9);
        let (nx, ny) = noise_vector((p.0 * freq, p.1 * freq), octave_seed);
        sum_x += nx * amp;
        sum_y += ny * amp;
        total_amp += amp;
        amp *= 0.5;
        freq *= 2.0;
    }

    if total_amp <= 1.0e-6 {
        (0.0, 0.0)
    } else {
        (sum_x / total_amp, sum_y / total_amp)
    }
}

fn domain_warp_fbm_displacement(p: (f32, f32), octaves: u32, seed: u32) -> (f32, f32) {
    let q = fbm_vector_displacement(p, octaves, seed ^ 0xA53A_9D13);
    let r = fbm_vector_displacement(
        (p.0 + q.0 * 2.0 + 1.7, p.1 + q.1 * 2.0 + 9.2),
        octaves,
        seed ^ 0xC2B2_AE35,
    );
    let s = fbm_vector_displacement(
        (p.0 + r.0 * 2.0 + 8.3, p.1 + r.1 * 2.0 + 2.8),
        octaves,
        seed ^ 0x27D4_EB2F,
    );
    (s.0, s.1)
}

fn curl_fbm_displacement(p: (f32, f32), octaves: u32, seed: u32) -> (f32, f32) {
    let eps = 0.5;
    let px1 = scalar_fbm((p.0 + eps, p.1), octaves, seed);
    let px0 = scalar_fbm((p.0 - eps, p.1), octaves, seed);
    let py1 = scalar_fbm((p.0, p.1 + eps), octaves, seed);
    let py0 = scalar_fbm((p.0, p.1 - eps), octaves, seed);
    let dx = (px1 - px0) / (2.0 * eps);
    let dy = (py1 - py0) / (2.0 * eps);
    (dy * 2.0, -dx * 2.0)
}

fn scalar_fbm(p: (f32, f32), octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut total_amp = 0.0;
    let mut freq = 1.0;
    for octave in 0..octaves {
        let octave_seed = seed ^ octave.wrapping_mul(0x85EB_CA6B);
        sum += value_noise_signed(p.0 * freq, p.1 * freq, octave_seed, 0x77) * amp;
        total_amp += amp;
        amp *= 0.5;
        freq *= 2.0;
    }

    if total_amp <= 1.0e-6 {
        0.0
    } else {
        sum / total_amp
    }
}

fn value_noise_signed(x: f32, y: f32, seed: u32, channel: u32) -> f32 {
    let cell_x = x.floor() as i32;
    let cell_y = y.floor() as i32;
    let fx = smoothstep(x - cell_x as f32);
    let fy = smoothstep(y - cell_y as f32);

    let h00 = hash_signed_2d(cell_x, cell_y, seed, channel);
    let h10 = hash_signed_2d(cell_x + 1, cell_y, seed, channel);
    let h01 = hash_signed_2d(cell_x, cell_y + 1, seed, channel);
    let h11 = hash_signed_2d(cell_x + 1, cell_y + 1, seed, channel);

    let top = lerp(h00, h10, fx);
    let bottom = lerp(h01, h11, fx);
    lerp(top, bottom, fy).clamp(-1.0, 1.0)
}

fn hash_vector_2d(cell_x: i32, cell_y: i32, seed: u32) -> (f32, f32) {
    (
        hash_signed_2d(cell_x, cell_y, seed, 0),
        hash_signed_2d(cell_x, cell_y, seed, 1),
    )
}

fn hash_signed_2d(cell_x: i32, cell_y: i32, seed: u32, channel: u32) -> f32 {
    rand01(hash_3d(cell_x, cell_y, 0, channel, seed)) * 2.0 - 1.0
}

fn limit_vector(v: (f32, f32)) -> (f32, f32) {
    let len2 = v.0 * v.0 + v.1 * v.1;
    if len2 > 1.0 {
        let inv_len = len2.sqrt().recip();
        (v.0 * inv_len, v.1 * inv_len)
    } else {
        v
    }
}

fn scatter_pixel(
    src: &LayerBuffer,
    center: PixelF32,
    sample_params: ScatterSampleParams,
    settings: &RenderSettings,
) -> PixelF32 {
    let dir_x = sample_params.angle.cos();
    let dir_y = sample_params.angle.sin();
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let minor = (1.0 - sample_params.anisotropy).clamp(0.0, 1.0);

    let mut acc = ColorAccumulator::new(settings.color_space);
    for tap in 0..settings.samples {
        let (disk_x, disk_y) = scatter_unit_offset(sample_params, tap, settings);
        let u = disk_x * sample_params.radius;
        let v = disk_y * sample_params.radius * minor;
        let dx = dir_x * u + perp_x * v;
        let dy = dir_y * u + perp_y * v;
        let sample = sample_bilinear(
            src,
            sample_params.center_x + dx,
            sample_params.center_y + dy,
            settings.edge_mode,
        );
        acc.add(sample);
    }

    acc.finish(center, settings.preserve_alpha)
}

fn scatter_unit_offset(
    sample_params: ScatterSampleParams,
    tap: u32,
    settings: &RenderSettings,
) -> (f32, f32) {
    match settings.scatter_algorithm {
        ScatterAlgorithm::LegacySquareCell => square_cell_offset(sample_params, tap, settings),
        ScatterAlgorithm::LegacySmoothGrid => smooth_grid_offset(sample_params, tap, settings),
        ScatterAlgorithm::LegacyVoronoiCell => voronoi_cell_offset(sample_params, tap, settings),
        ScatterAlgorithm::LegacyBlueNoise => blue_noise_offset(sample_params, tap, settings),
        _ => (0.0, 0.0),
    }
}

fn square_cell_offset(
    sample_params: ScatterSampleParams,
    tap: u32,
    settings: &RenderSettings,
) -> (f32, f32) {
    let (cell_x, cell_y) = grain_cell(sample_params);
    random_disk_offset(cell_x, cell_y, tap, settings)
}

fn smooth_grid_offset(
    sample_params: ScatterSampleParams,
    tap: u32,
    settings: &RenderSettings,
) -> (f32, f32) {
    let grid_x = sample_params.center_x / sample_params.grain;
    let grid_y = sample_params.center_y / sample_params.grain;
    let cell_x = grid_x.floor() as i32;
    let cell_y = grid_y.floor() as i32;
    let fx = smoothstep(grid_x - cell_x as f32);
    let fy = smoothstep(grid_y - cell_y as f32);

    let p00 = random_disk_offset(cell_x, cell_y, tap, settings);
    let p10 = random_disk_offset(cell_x + 1, cell_y, tap, settings);
    let p01 = random_disk_offset(cell_x, cell_y + 1, tap, settings);
    let p11 = random_disk_offset(cell_x + 1, cell_y + 1, tap, settings);

    let top = (lerp(p00.0, p10.0, fx), lerp(p00.1, p10.1, fx));
    let bottom = (lerp(p01.0, p11.0, fx), lerp(p01.1, p11.1, fx));
    (lerp(top.0, bottom.0, fy), lerp(top.1, bottom.1, fy))
}

fn voronoi_cell_offset(
    sample_params: ScatterSampleParams,
    tap: u32,
    settings: &RenderSettings,
) -> (f32, f32) {
    let grid_x = sample_params.center_x / sample_params.grain;
    let grid_y = sample_params.center_y / sample_params.grain;
    let base_x = grid_x.floor() as i32;
    let base_y = grid_y.floor() as i32;
    let seed = settings.seed;

    let mut best_x = base_x;
    let mut best_y = base_y;
    let mut best_d2 = f32::INFINITY;
    for y in -1..=1 {
        for x in -1..=1 {
            let cell_x = base_x + x;
            let cell_y = base_y + y;
            let feature_x =
                cell_x as f32 + rand01(hash_3d(cell_x, cell_y, -1, 9, seed ^ 0x7A37_9B1D));
            let feature_y =
                cell_y as f32 + rand01(hash_3d(cell_x, cell_y, -1, 10, seed ^ 0x7A37_9B1D));
            let dx = grid_x - feature_x;
            let dy = grid_y - feature_y;
            let d2 = dx * dx + dy * dy;
            if d2 < best_d2 {
                best_d2 = d2;
                best_x = cell_x;
                best_y = cell_y;
            }
        }
    }

    let shifted_settings = RenderSettings {
        seed: seed ^ 0x517C_C1B7,
        ..*settings
    };
    random_disk_offset(best_x, best_y, tap, &shifted_settings)
}

fn blue_noise_offset(
    sample_params: ScatterSampleParams,
    tap: u32,
    settings: &RenderSettings,
) -> (f32, f32) {
    let phase_scale = (sample_params.grain / 8.0).max(1.0);
    let cell_x = (sample_params.center_x / phase_scale).floor() as i32;
    let cell_y = (sample_params.center_y / phase_scale).floor() as i32;
    let mask_x = cell_x.rem_euclid(8) as usize;
    let mask_y = cell_y.rem_euclid(8) as usize;
    let mask = (BLUE_NOISE_8X8[mask_y * 8 + mask_x] as f32 + 0.5) / 64.0;
    let phase = rand01(hash_3d(cell_x, cell_y, 0, 11, settings.seed));
    let jitter = rand01(hash_3d(cell_x, cell_y, tap as i32, 12, settings.seed));
    let sample_count = settings.samples.max(1) as f32;
    let radial_index = (tap as f32 + 0.5 + (jitter - 0.5) * 0.5).clamp(0.0, sample_count);
    let radius = sample_radius(
        radial_index / sample_count,
        settings.sampling_distribution,
        settings.distribution_shape,
    );
    let angle = tap as f32 * GOLDEN_ANGLE + (mask + phase) * TAU;

    (angle.cos() * radius, angle.sin() * radius)
}

fn random_disk_offset(cell_x: i32, cell_y: i32, tap: u32, settings: &RenderSettings) -> (f32, f32) {
    let angle = rand01(hash_3d(cell_x, cell_y, tap as i32, 0, settings.seed)) * TAU;
    let radius_u = rand01(hash_3d(cell_x, cell_y, tap as i32, 1, settings.seed));
    let radius = sample_radius(
        radius_u,
        settings.sampling_distribution,
        settings.distribution_shape,
    );
    (angle.cos() * radius, angle.sin() * radius)
}

fn sample_radius(u: f32, distribution: SamplingDistribution, shape: f32) -> f32 {
    let u = u.clamp(0.0, 1.0 - f32::EPSILON);
    let shape = shape.max(0.0);
    match distribution {
        SamplingDistribution::Uniform => u.powf(1.0 / (shape + 1.0)).clamp(0.0, 1.0),
        SamplingDistribution::Gaussian => {
            let sigma = 1.0 / (shape + 1.0);
            let max_cdf = 1.0 - (-0.5 / (sigma * sigma)).exp();
            (sigma * (-2.0 * (1.0 - u * max_cdf).ln()).sqrt()).clamp(0.0, 1.0)
        }
        SamplingDistribution::Exponential => {
            let lambda = shape + 1.0;
            let max_cdf = 1.0 - (-lambda).exp();
            (-(1.0 - u * max_cdf).ln() / lambda).clamp(0.0, 1.0)
        }
    }
}

fn grain_cell(sample_params: ScatterSampleParams) -> (i32, i32) {
    (
        (sample_params.center_x / sample_params.grain).floor() as i32,
        (sample_params.center_y / sample_params.grain).floor() as i32,
    )
}

struct ColorAccumulator {
    color_space: ScatterColorSpace,
    sum: [f32; 4],
    hue_x: f32,
    hue_y: f32,
    count: f32,
}

impl ColorAccumulator {
    fn new(color_space: ScatterColorSpace) -> Self {
        Self {
            color_space,
            sum: [0.0; 4],
            hue_x: 0.0,
            hue_y: 0.0,
            count: 0.0,
        }
    }

    fn add(&mut self, px: PixelF32) {
        self.count += 1.0;
        match self.color_space {
            ScatterColorSpace::LinearRgba
            | ScatterColorSpace::LinearRgb
            | ScatterColorSpace::Alpha => {
                self.sum[0] += px.red;
                self.sum[1] += px.green;
                self.sum[2] += px.blue;
                self.sum[3] += px.alpha;
            }
            ScatterColorSpace::Srgb => {
                let lin = LinSrgb::new(px.red, px.green, px.blue);
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                self.sum[0] += srgb.red;
                self.sum[1] += srgb.green;
                self.sum[2] += srgb.blue;
                self.sum[3] += px.alpha;
            }
            ScatterColorSpace::Oklab => {
                let c: Oklab<f32> = Oklab::from_color(LinSrgb::new(px.red, px.green, px.blue));
                self.sum[0] += c.l;
                self.sum[1] += c.a;
                self.sum[2] += c.b;
                self.sum[3] += px.alpha;
            }
            ScatterColorSpace::Oklch => {
                let c: Oklch<f32> = Oklch::from_color(LinSrgb::new(px.red, px.green, px.blue));
                let hue = c.hue.into_degrees().to_radians();
                let weight = c.chroma.max(1.0e-6);
                self.hue_x += hue.cos() * weight;
                self.hue_y += hue.sin() * weight;
                self.sum[0] += c.l;
                self.sum[1] += c.chroma.min(OKLCH_CHROMA_MAX * 4.0);
                self.sum[3] += px.alpha;
            }
            ScatterColorSpace::Hsl => {
                let c = Hsl::from_color(LinSrgb::new(px.red, px.green, px.blue));
                let hue = c.hue.into_degrees().to_radians();
                let weight = c.saturation.max(1.0e-6);
                self.hue_x += hue.cos() * weight;
                self.hue_y += hue.sin() * weight;
                self.sum[0] += c.saturation;
                self.sum[1] += c.lightness;
                self.sum[3] += px.alpha;
            }
            ScatterColorSpace::Hsv => {
                let c = Hsv::from_color(LinSrgb::new(px.red, px.green, px.blue));
                let hue = c.hue.into_degrees().to_radians();
                let weight = c.saturation.max(1.0e-6);
                self.hue_x += hue.cos() * weight;
                self.hue_y += hue.sin() * weight;
                self.sum[0] += c.saturation;
                self.sum[1] += c.value;
                self.sum[3] += px.alpha;
            }
        }
    }

    fn finish(&self, center: PixelF32, preserve_alpha: bool) -> PixelF32 {
        let inv = self.count.max(1.0).recip();
        let avg_alpha = self.sum[3] * inv;
        let alpha = if preserve_alpha {
            center.alpha
        } else {
            avg_alpha
        };

        match self.color_space {
            ScatterColorSpace::LinearRgba => PixelF32 {
                red: self.sum[0] * inv,
                green: self.sum[1] * inv,
                blue: self.sum[2] * inv,
                alpha,
            },
            ScatterColorSpace::LinearRgb => PixelF32 {
                red: self.sum[0] * inv,
                green: self.sum[1] * inv,
                blue: self.sum[2] * inv,
                alpha,
            },
            ScatterColorSpace::Alpha => PixelF32 {
                red: center.red,
                green: center.green,
                blue: center.blue,
                alpha: avg_alpha,
            },
            ScatterColorSpace::Srgb => {
                let lin = Srgb::new(self.sum[0] * inv, self.sum[1] * inv, self.sum[2] * inv)
                    .into_linear();
                PixelF32 {
                    red: lin.red,
                    green: lin.green,
                    blue: lin.blue,
                    alpha,
                }
            }
            ScatterColorSpace::Oklab => {
                let lin = LinSrgb::from_color(Oklab::new(
                    self.sum[0] * inv,
                    self.sum[1] * inv,
                    self.sum[2] * inv,
                ));
                PixelF32 {
                    red: lin.red,
                    green: lin.green,
                    blue: lin.blue,
                    alpha,
                }
            }
            ScatterColorSpace::Oklch => {
                let hue = hue_from_vector(self.hue_x, self.hue_y);
                let lin = LinSrgb::from_color(Oklch::new(
                    self.sum[0] * inv,
                    self.sum[1] * inv,
                    OklabHue::from_degrees(hue),
                ));
                PixelF32 {
                    red: lin.red,
                    green: lin.green,
                    blue: lin.blue,
                    alpha,
                }
            }
            ScatterColorSpace::Hsl => {
                let hue = hue_from_vector(self.hue_x, self.hue_y);
                let lin = LinSrgb::from_color(Hsl::new(
                    RgbHue::from_degrees(hue),
                    (self.sum[0] * inv).clamp(0.0, 1.0),
                    (self.sum[1] * inv).clamp(0.0, 1.0),
                ));
                PixelF32 {
                    red: lin.red,
                    green: lin.green,
                    blue: lin.blue,
                    alpha,
                }
            }
            ScatterColorSpace::Hsv => {
                let hue = hue_from_vector(self.hue_x, self.hue_y);
                let lin = LinSrgb::from_color(Hsv::new(
                    RgbHue::from_degrees(hue),
                    (self.sum[0] * inv).clamp(0.0, 1.0),
                    (self.sum[1] * inv).clamp(0.0, 1.0),
                ));
                PixelF32 {
                    red: lin.red,
                    green: lin.green,
                    blue: lin.blue,
                    alpha,
                }
            }
        }
    }
}

fn hue_from_vector(x: f32, y: f32) -> f32 {
    if x.abs() + y.abs() <= 1.0e-12 {
        0.0
    } else {
        y.atan2(x).to_degrees().rem_euclid(360.0)
    }
}

fn anisotropy_flow_at(
    map: Option<&LayerBuffer>,
    coord: OutputCoord,
    mode: AnisotropyMapMode,
    divergence_source: DivergenceSource,
) -> FlowVector {
    let Some(map) = map else {
        return FlowVector {
            dir_x: 1.0,
            dir_y: 0.0,
            strength: 0.0,
            valid: false,
        };
    };

    match mode {
        AnisotropyMapMode::HueSaturation => {
            let px = sample_map_pixel(map, coord);
            let (h, s, _) = rgb_to_hsv(px.red, px.green, px.blue);
            let angle = h * TAU;
            FlowVector {
                dir_x: angle.cos(),
                dir_y: angle.sin(),
                strength: s.clamp(0.0, 1.0),
                valid: true,
            }
        }
        AnisotropyMapMode::Uv | AnisotropyMapMode::Normal => {
            let px = sample_map_pixel(map, coord);
            let dir_x = sanitize_non_finite(px.red) * 2.0 - 1.0;
            let mut dir_y = sanitize_non_finite(px.green) * 2.0 - 1.0;
            if matches!(mode, AnisotropyMapMode::Normal) {
                dir_y = -dir_y;
            }
            normalized_flow(dir_x, dir_y, (dir_x * dir_x + dir_y * dir_y).sqrt())
        }
        AnisotropyMapMode::DivergenceDirection | AnisotropyMapMode::DivergenceRotation => {
            let left = scalar_divergence_value(
                map,
                OutputCoord {
                    x: coord.x.saturating_sub(1),
                    ..coord
                },
                divergence_source,
            );
            let right = scalar_divergence_value(
                map,
                OutputCoord {
                    x: (coord.x + 1).min(coord.out_w.saturating_sub(1)),
                    ..coord
                },
                divergence_source,
            );
            let up = scalar_divergence_value(
                map,
                OutputCoord {
                    y: coord.y.saturating_sub(1),
                    ..coord
                },
                divergence_source,
            );
            let down = scalar_divergence_value(
                map,
                OutputCoord {
                    y: (coord.y + 1).min(coord.out_h.saturating_sub(1)),
                    ..coord
                },
                divergence_source,
            );
            let gx = right - left;
            let gy = down - up;
            let (dir_x, dir_y) = if matches!(mode, AnisotropyMapMode::DivergenceRotation) {
                (-gy, gx)
            } else {
                (gx, gy)
            };
            normalized_flow(dir_x, dir_y, (gx * gx + gy * gy).sqrt() * 4.0)
        }
    }
}

fn normalized_flow(dir_x: f32, dir_y: f32, strength: f32) -> FlowVector {
    let len2 = dir_x * dir_x + dir_y * dir_y;
    if !len2.is_finite() || len2 <= 1.0e-12 {
        return FlowVector {
            dir_x: 1.0,
            dir_y: 0.0,
            strength: 0.0,
            valid: false,
        };
    }
    let inv = len2.sqrt().recip();
    FlowVector {
        dir_x: dir_x * inv,
        dir_y: dir_y * inv,
        strength: sanitize_non_finite(strength).clamp(0.0, 1.0),
        valid: true,
    }
}

fn scalar_map_value(
    map: Option<&LayerBuffer>,
    coord: OutputCoord,
    mode: ScalarMapMode,
    channel: RgbaChannel,
    default_value: f32,
) -> f32 {
    if let Some(map) = map {
        sanitize_non_finite(scalar_from_pixel(
            sample_map_pixel(map, coord),
            mode,
            channel,
        ))
    } else {
        default_value
    }
}

fn scalar_divergence_value(map: &LayerBuffer, coord: OutputCoord, source: DivergenceSource) -> f32 {
    let px = sample_map_pixel(map, coord);
    match source {
        DivergenceSource::Gray => luma(px),
        DivergenceSource::Red => sanitize_non_finite(px.red),
        DivergenceSource::Green => sanitize_non_finite(px.green),
        DivergenceSource::Blue => sanitize_non_finite(px.blue),
        DivergenceSource::Alpha => sanitize_non_finite(px.alpha),
        DivergenceSource::HsvValue => {
            let (_, _, v) = rgb_to_hsv(px.red, px.green, px.blue);
            v
        }
        DivergenceSource::HslLightness => {
            let (_, _, l) = rgb_to_hsl(px.red, px.green, px.blue);
            l
        }
    }
}

fn scalar_from_pixel(px: PixelF32, mode: ScalarMapMode, channel: RgbaChannel) -> f32 {
    match mode {
        ScalarMapMode::Gray => luma(px),
        ScalarMapMode::HsvValue => {
            let (_, _, v) = rgb_to_hsv(px.red, px.green, px.blue);
            v
        }
        ScalarMapMode::HslLightness => {
            let (_, _, l) = rgb_to_hsl(px.red, px.green, px.blue);
            l
        }
        ScalarMapMode::RgbaChannel => match channel {
            RgbaChannel::Red => sanitize_non_finite(px.red),
            RgbaChannel::Green => sanitize_non_finite(px.green),
            RgbaChannel::Blue => sanitize_non_finite(px.blue),
            RgbaChannel::Alpha => sanitize_non_finite(px.alpha),
        },
    }
}

fn luma(px: PixelF32) -> f32 {
    0.2126 * sanitize_non_finite(px.red)
        + 0.7152 * sanitize_non_finite(px.green)
        + 0.0722 * sanitize_non_finite(px.blue)
}

fn sample_map_pixel(map: &LayerBuffer, coord: OutputCoord) -> PixelF32 {
    let map_x = remap_coord_to_layer_float(coord.x, coord.out_w, map.width);
    let map_y = remap_coord_to_layer_float(coord.y, coord.out_h, map.height);
    sample_bilinear(map, map_x, map_y, EdgeMode::Repeat)
}

fn sample_bilinear(src: &LayerBuffer, x: f32, y: f32, edge_mode: EdgeMode) -> PixelF32 {
    if src.width == 0 || src.height == 0 || !x.is_finite() || !y.is_finite() {
        return transparent_pixel();
    }

    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = sample_pixel(src, x0, y0, edge_mode);
    let p10 = sample_pixel(src, x1, y0, edge_mode);
    let p01 = sample_pixel(src, x0, y1, edge_mode);
    let p11 = sample_pixel(src, x1, y1, edge_mode);

    let top = lerp_pixel(p00, p10, tx);
    let bottom = lerp_pixel(p01, p11, tx);
    lerp_pixel(top, bottom, ty)
}

fn sample_pixel(src: &LayerBuffer, x: i32, y: i32, edge_mode: EdgeMode) -> PixelF32 {
    let xi = resolve_coord(x, src.width, edge_mode);
    let yi = resolve_coord(y, src.height, edge_mode);
    if let (Some(xi), Some(yi)) = (xi, yi) {
        src.pixels[yi * src.width + xi]
    } else {
        transparent_pixel()
    }
}

fn resolve_coord(coord: i32, len: usize, edge_mode: EdgeMode) -> Option<usize> {
    if len == 0 {
        return None;
    }

    let len_i = len as i32;
    match edge_mode {
        EdgeMode::None => {
            if coord < 0 || coord >= len_i {
                None
            } else {
                Some(coord as usize)
            }
        }
        EdgeMode::Repeat => Some(coord.clamp(0, len_i - 1) as usize),
        EdgeMode::Tile => Some(coord.rem_euclid(len_i) as usize),
        EdgeMode::Mirror => Some(mirror_index(coord, len_i) as usize),
    }
}

fn mirror_index(coord: i32, len: i32) -> i32 {
    if len <= 1 {
        return 0;
    }
    let period = 2 * len - 2;
    let t = coord.rem_euclid(period);
    if t < len { t } else { period - t }
}

fn remap_coord_to_layer_float(coord: usize, out_len: usize, layer_len: usize) -> f32 {
    if out_len == 0 || layer_len == 0 {
        return 0.0;
    }
    ((coord as f32 + 0.5) * layer_len as f32 / out_len as f32) - 0.5
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
        alpha: a.alpha + (b.alpha - a.alpha) * t,
    }
}

fn blend_with_original(
    scatter: PixelF32,
    original: PixelF32,
    mode: OutputBlendMode,
    opacity: f32,
) -> PixelF32 {
    if matches!(mode, OutputBlendMode::None) {
        return scatter;
    }

    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return original;
    }

    let red = blend_channel(original.red, scatter.red, mode);
    let green = blend_channel(original.green, scatter.green, mode);
    let blue = blend_channel(original.blue, scatter.blue, mode);

    PixelF32 {
        red: lerp(original.red, red, opacity),
        green: lerp(original.green, green, opacity),
        blue: lerp(original.blue, blue, opacity),
        alpha: lerp(original.alpha, scatter.alpha, opacity),
    }
}

fn blend_channel(base: f32, source: f32, mode: OutputBlendMode) -> f32 {
    match mode {
        OutputBlendMode::None | OutputBlendMode::Normal => source,
        OutputBlendMode::Add => base + source,
        OutputBlendMode::Multiply => base * source,
        OutputBlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - source),
        OutputBlendMode::Overlay => {
            if base <= 0.5 {
                2.0 * base * source
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - source)
            }
        }
        OutputBlendMode::Difference => (base - source).abs(),
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let r = sanitize_non_finite(r);
    let g = sanitize_non_finite(g);
    let b = sanitize_non_finite(b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let hue = if delta <= 1.0e-6 {
        0.0
    } else if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() <= f32::EPSILON {
        (((b - r) / delta) + 2.0) / 6.0
    } else {
        (((r - g) / delta) + 4.0) / 6.0
    }
    .rem_euclid(1.0);
    let sat = if max <= 1.0e-6 { 0.0 } else { delta / max };

    (hue, sat, max)
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let r = sanitize_non_finite(r);
    let g = sanitize_non_finite(g);
    let b = sanitize_non_finite(b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = (max + min) * 0.5;
    if delta <= 1.0e-6 {
        return (0.0, 0.0, lightness);
    }

    let sat = delta / (1.0 - (2.0 * lightness - 1.0).abs()).max(1.0e-6);
    let hue = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() <= f32::EPSILON {
        (((b - r) / delta) + 2.0) / 6.0
    } else {
        (((r - g) / delta) + 4.0) / 6.0
    }
    .rem_euclid(1.0);
    (hue, sat, lightness)
}

fn hash_3d(cell_x: i32, cell_y: i32, tap: i32, channel: u32, seed: u32) -> u32 {
    let mut h = seed ^ 0xA511_E9B3;
    h = h.wrapping_add((cell_x as u32).wrapping_mul(0x85EB_CA6B));
    h = h.wrapping_add((cell_y as u32).wrapping_mul(0xC2B2_AE35));
    h = h.wrapping_add((tap as u32).wrapping_mul(0x27D4_EB2D));
    h = h.wrapping_add(channel.wrapping_mul(0x1656_67B1));
    hash_u32(h)
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

fn rand01(v: u32) -> f32 {
    v as f32 / u32::MAX as f32
}

fn sanitize_pixel_for_output(mut px: PixelF32, out_is_f32: bool, clamp_32: bool) -> PixelF32 {
    px.red = sanitize_non_finite(px.red);
    px.green = sanitize_non_finite(px.green);
    px.blue = sanitize_non_finite(px.blue);
    px.alpha = sanitize_non_finite(px.alpha);

    if !out_is_f32 || clamp_32 {
        px.red = px.red.clamp(0.0, 1.0);
        px.green = px.green.clamp(0.0, 1.0);
        px.blue = px.blue.clamp(0.0, 1.0);
        px.alpha = px.alpha.clamp(0.0, 1.0);
    }

    px
}

fn sanitize_non_finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}
