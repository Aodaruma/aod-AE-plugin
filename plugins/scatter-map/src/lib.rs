#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const MAX_GATHER_ATTEMPTS: u32 = 24;
const MAX_SWAP_ATTEMPTS: u32 = 64;
const MAX_DISK_REJECTIONS: u32 = 32;
const SMART_INPUT_ID: u32 = 0;
const SMART_AMOUNT_MAP_ID: u32 = 1;
const SMART_RADIUS_MAP_ID: u32 = 2;
const SMART_GRAIN_SIZE_MAP_ID: u32 = 3;
const SMART_ANISOTROPY_MAP_ID: u32 = 4;
const SMART_KERNEL_TEXTURE_ID: u32 = 5;
const SMART_QUERY_ID_BASE: i32 = 100;

#[cfg(test)]
std::thread_local! {
    static GATHER_PIXEL_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ScatterMode,
    Amount,
    Radius,
    GrainSize,
    GatherSamples,
    Direction,
    Anisotropy,
    KernelGroupStart,
    GrainShape,
    GrainSizeRandomness,
    GrainPositionRandomness,
    GrainDensity,
    KernelRandomness,
    KernelTextureLayer,
    KernelTextureChannel,
    KernelThreshold,
    GrainFillMode,
    GrainFillOpacity,
    KernelGroupEnd,
    MapGroupStart,
    UseAmountMap,
    AmountMapLayer,
    AmountMapChannel,
    UseRadiusMap,
    RadiusMapLayer,
    RadiusMapChannel,
    UseGrainSizeMap,
    GrainSizeMin,
    GrainSizeMapMax,
    GrainSizeMapLayer,
    GrainSizeMapChannel,
    UseAnisotropyMap,
    AnisotropyMapLayer,
    AnisotropyMapMode,
    UseAnisotropyDirection,
    UseAnisotropyStrength,
    AnisotropyDivergenceSource,
    MapGroupEnd,
    OutputGroupStart,
    Seed,
    TemporalMode,
    EdgeMode,
    BlendMode,
    BlendOpacity,
    PreserveAlpha,
    Clamp32,
    OutputGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScatterMode {
    Gather,
    Swap,
}

#[derive(Clone, Copy)]
enum MapChannel {
    Luma,
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrainShape {
    Square,
    Circle,
    Texture,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrainFillMode {
    Texture,
    Average,
    Median,
    Center,
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

#[derive(Clone, Copy)]
enum TemporalMode {
    Static,
    Frame,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeMode {
    Reject,
    Clamp,
    Tile,
    Mirror,
    Transparent,
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
    scatter_mode: ScatterMode,
    amount: f32,
    radius: i32,
    grain_size: usize,
    grain_size_min: usize,
    gather_samples: u32,
    direction: f32,
    anisotropy: f32,
    grain_shape: GrainShape,
    grain_size_randomness: f32,
    grain_position_randomness: f32,
    grain_density: f32,
    kernel_randomness: f32,
    kernel_texture_channel: MapChannel,
    kernel_threshold: f32,
    grain_fill_mode: GrainFillMode,
    grain_fill_opacity: f32,
    use_amount_map: bool,
    amount_map_channel: MapChannel,
    use_radius_map: bool,
    radius_map_channel: MapChannel,
    use_grain_size_map: bool,
    grain_size_map_channel: MapChannel,
    use_anisotropy_map: bool,
    anisotropy_map_mode: AnisotropyMapMode,
    use_anisotropy_direction: bool,
    use_anisotropy_strength: bool,
    anisotropy_divergence_source: DivergenceSource,
    seed: u32,
    temporal_mode: TemporalMode,
    edge_mode: EdgeMode,
    blend_mode: OutputBlendMode,
    blend_opacity: f32,
    preserve_alpha: bool,
    clamp_32: bool,
}

#[derive(Clone, Debug)]
struct LayerBuffer {
    width: usize,
    height: usize,
    pixels: Vec<PixelF32>,
}

#[derive(Clone, Copy)]
struct RenderMaps<'a> {
    amount: Option<&'a LayerBuffer>,
    radius: Option<&'a LayerBuffer>,
    grain_size: Option<&'a LayerBuffer>,
    anisotropy: Option<&'a LayerBuffer>,
    kernel_texture: Option<&'a LayerBuffer>,
}

#[derive(Default)]
struct OwnedRenderMaps {
    amount: Option<LayerBuffer>,
    radius: Option<LayerBuffer>,
    grain_size: Option<LayerBuffer>,
    anisotropy: Option<LayerBuffer>,
    kernel_texture: Option<LayerBuffer>,
}

#[derive(Clone, Copy)]
struct KernelContext<'a> {
    image_width: usize,
    image_height: usize,
    maps: RenderMaps<'a>,
    settings: &'a RenderSettings,
    seed: u32,
    density_layer: u32,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    grid_x: usize,
    grid_y: usize,
}

#[derive(Debug)]
struct GrainPartition {
    offsets: Vec<u32>,
    indices: Vec<u32>,
    columns: usize,
    rows: usize,
}

impl GrainPartition {
    fn group(&self, index: usize) -> &[u32] {
        let start = self.offsets[index] as usize;
        let end = self.offsets[index + 1] as usize;
        &self.indices[start..end]
    }

    #[cfg(test)]
    fn groups(&self) -> impl Iterator<Item = &[u32]> {
        self.offsets.windows(2).map(|range| {
            let start = range[0] as usize;
            let end = range[1] as usize;
            &self.indices[start..end]
        })
    }
}

#[derive(Clone, Copy)]
struct PreparedKernel {
    block: Block,
    center_x: f32,
    center_y: f32,
    radius_x: f32,
    radius_y: f32,
    #[cfg(test)]
    front_priority: u32,
}

#[derive(Clone, Copy)]
struct AnisotropyTransform {
    cosine: f32,
    sine: f32,
    perpendicular_scale: f32,
}

#[derive(Clone, Copy)]
struct GatherGrainState {
    radius: i32,
    transform: AnisotropyTransform,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Applies map-driven gather and swap scatter to layers.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::ScatterMode,
            "Scatter Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Gather", "Swap"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;
        params.add(
            Params::Amount,
            "Amount (%)",
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
            Params::Radius,
            "Radius (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(4096);
                d.set_slider_min(0);
                d.set_slider_max(256);
                d.set_default(8);
            }),
        )?;
        params.add(
            Params::GrainSize,
            "Grain Size (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(1024);
                d.set_slider_min(1);
                d.set_slider_max(128);
                d.set_default(1);
            }),
        )?;
        params.add(
            Params::GatherSamples,
            "Gather Samples",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(32);
                d.set_slider_min(1);
                d.set_slider_max(16);
                d.set_default(1);
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
            "Anisotropy (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(100.0);
                d.set_default(0.0);
                d.set_precision(1);
            }),
        )?;

        params.add_group(
            Params::KernelGroupStart,
            Params::KernelGroupEnd,
            "Grain Kernel",
            false,
            |params| {
                params.add_with_flags(
                    Params::GrainShape,
                    "Shape",
                    PopupDef::setup(|d| {
                        d.set_options(&["Square", "Circle", "Custom Texture"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::GrainSizeRandomness,
                    "Grain Size Randomness (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(0.0);
                        d.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::GrainPositionRandomness,
                    "Position Randomness (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(0.0);
                        d.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::GrainDensity,
                    "Grain Density (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(100.0);
                        d.set_valid_max(800.0);
                        d.set_slider_min(100.0);
                        d.set_slider_max(800.0);
                        d.set_default(100.0);
                        d.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::KernelRandomness,
                    "Shape Randomness (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(0.0);
                        d.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::KernelTextureLayer,
                    "Kernel Texture (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::KernelTextureChannel,
                    "Kernel Texture Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(5);
                    }),
                )?;
                params.add(
                    Params::KernelThreshold,
                    "Kernel Threshold (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(50.0);
                        d.set_precision(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::GrainFillMode,
                    "Fill",
                    PopupDef::setup(|d| {
                        d.set_options(&["Source Texture", "Average", "Median", "Center Pixel"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::GrainFillOpacity,
                    "Fill Opacity (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(100.0);
                        d.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::MapGroupStart,
            Params::MapGroupEnd,
            "Maps",
            false,
            |params| {
                params.add_with_flags(
                    Params::UseAmountMap,
                    "Map Amount",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::AmountMapLayer,
                    "Amount Map Layer (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::AmountMapChannel,
                    "Amount Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::UseRadiusMap,
                    "Map Radius",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::RadiusMapLayer,
                    "Radius Map Layer (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::RadiusMapChannel,
                    "Radius Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::UseGrainSizeMap,
                    "Map Grain Size",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::GrainSizeMin,
                    "Grain Size Min (px)",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(1024);
                        d.set_slider_min(1);
                        d.set_slider_max(128);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::GrainSizeMapMax,
                    "Grain Size Max (px)",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(1024);
                        d.set_slider_min(1);
                        d.set_slider_max(128);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::GrainSizeMapLayer,
                    "Grain Size Map Layer (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::GrainSizeMapChannel,
                    "Grain Size Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;
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
                    "Anisotropy Map Layer (None = Input)",
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
            Params::OutputGroupStart,
            Params::OutputGroupEnd,
            "Output",
            false,
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
                    Params::TemporalMode,
                    "Temporal Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["Static", "Randomize Every Frame"]);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::EdgeMode,
                    "Edge Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Reject and Retry",
                            "Clamp",
                            "Tile",
                            "Mirror",
                            "Transparent",
                        ]);
                        d.set_default(1);
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
                    "Preserve Original Alpha",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
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
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
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
            } => self.do_render(in_data, in_layer, out_data, out_layer, params, None)?,
            ae::Command::SmartPreRender { mut extra } => {
                let req = extra.output_request();
                let settings = read_render_settings(params)?;
                let callbacks = extra.callbacks();
                let in_result = checkout_full_smart_layer(
                    callbacks,
                    0,
                    SMART_QUERY_ID_BASE + SMART_INPUT_ID as i32,
                    SMART_INPUT_ID,
                    &req,
                    in_data,
                )?;
                let full_rect: ae::Rect = in_result.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);

                for (param, checkout_id, enabled) in [
                    (
                        Params::AmountMapLayer,
                        SMART_AMOUNT_MAP_ID,
                        settings.use_amount_map,
                    ),
                    (
                        Params::RadiusMapLayer,
                        SMART_RADIUS_MAP_ID,
                        settings.use_radius_map,
                    ),
                    (
                        Params::GrainSizeMapLayer,
                        SMART_GRAIN_SIZE_MAP_ID,
                        settings.use_grain_size_map,
                    ),
                    (
                        Params::AnisotropyMapLayer,
                        SMART_ANISOTROPY_MAP_ID,
                        settings.use_anisotropy_map,
                    ),
                    (
                        Params::KernelTextureLayer,
                        SMART_KERNEL_TEXTURE_ID,
                        matches!(settings.grain_shape, GrainShape::Texture),
                    ),
                ] {
                    if !enabled {
                        continue;
                    }
                    let param_index = params.index(param).ok_or(Error::BadCallbackParameter)?;
                    checkout_full_smart_layer(
                        callbacks,
                        param_index as i32,
                        SMART_QUERY_ID_BASE + checkout_id as i32,
                        checkout_id,
                        &req,
                        in_data,
                    )?;
                }
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let settings = read_render_settings(params)?;
                let map_layers = OwnedRenderMaps {
                    amount: checkout_smart_layer_buffer(
                        cb,
                        SMART_AMOUNT_MAP_ID,
                        settings.use_amount_map,
                    )?,
                    radius: checkout_smart_layer_buffer(
                        cb,
                        SMART_RADIUS_MAP_ID,
                        settings.use_radius_map,
                    )?,
                    grain_size: checkout_smart_layer_buffer(
                        cb,
                        SMART_GRAIN_SIZE_MAP_ID,
                        settings.use_grain_size_map,
                    )?,
                    anisotropy: checkout_smart_layer_buffer(
                        cb,
                        SMART_ANISOTROPY_MAP_ID,
                        settings.use_anisotropy_map,
                    )?,
                    kernel_texture: checkout_smart_layer_buffer(
                        cb,
                        SMART_KERNEL_TEXTURE_ID,
                        matches!(settings.grain_shape, GrainShape::Texture),
                    )?,
                };
                let in_layer_opt = cb.checkout_layer_pixels(SMART_INPUT_ID)?;
                let out_layer_opt = cb.checkout_output()?;

                if let (Some(in_layer), Some(out_layer)) = (in_layer_opt, out_layer_opt) {
                    self.do_render(
                        in_data,
                        in_layer,
                        out_data,
                        out_layer,
                        params,
                        Some(map_layers),
                    )?;
                }

                cb.checkin_layer_pixels(SMART_INPUT_ID)?;
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
        let mode = scatter_mode_from_popup(params.get(Params::ScatterMode)?.as_popup()?.value());
        let gather = matches!(mode, ScatterMode::Gather);
        self.set_param_visible(in_data, params, Params::GatherSamples, gather)?;
        self.set_param_visible(in_data, params, Params::EdgeMode, gather)?;

        let grain_shape =
            grain_shape_from_popup(params.get(Params::GrainShape)?.as_popup()?.value());
        let custom_texture = matches!(grain_shape, GrainShape::Texture);
        self.set_param_visible(in_data, params, Params::KernelTextureLayer, custom_texture)?;
        self.set_param_visible(
            in_data,
            params,
            Params::KernelTextureChannel,
            custom_texture,
        )?;
        self.set_param_visible(in_data, params, Params::KernelThreshold, custom_texture)?;

        let grain_fill_mode =
            grain_fill_mode_from_popup(params.get(Params::GrainFillMode)?.as_popup()?.value());
        self.set_param_visible(
            in_data,
            params,
            Params::GrainFillOpacity,
            !matches!(grain_fill_mode, GrainFillMode::Texture),
        )?;

        let use_amount_map = params.get(Params::UseAmountMap)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::AmountMapLayer, use_amount_map)?;
        self.set_param_visible(in_data, params, Params::AmountMapChannel, use_amount_map)?;
        let use_radius_map = params.get(Params::UseRadiusMap)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::RadiusMapLayer, use_radius_map)?;
        self.set_param_visible(in_data, params, Params::RadiusMapChannel, use_radius_map)?;
        let use_grain_size_map = params.get(Params::UseGrainSizeMap)?.as_checkbox()?.value();
        Self::set_param_enabled(params, Params::GrainSize, !use_grain_size_map)?;
        Self::set_param_enabled(params, Params::GrainSizeMin, use_grain_size_map)?;
        Self::set_param_enabled(params, Params::GrainSizeMapMax, use_grain_size_map)?;
        for id in [Params::GrainSizeMapLayer, Params::GrainSizeMapChannel] {
            self.set_param_visible(in_data, params, id, use_grain_size_map)?;
        }
        let use_anisotropy_map = params.get(Params::UseAnisotropyMap)?.as_checkbox()?.value();
        let anisotropy_map_mode = anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        );
        for id in [
            Params::AnisotropyMapLayer,
            Params::AnisotropyMapMode,
            Params::UseAnisotropyDirection,
            Params::UseAnisotropyStrength,
        ] {
            self.set_param_visible(in_data, params, id, use_anisotropy_map)?;
        }
        self.set_param_visible(
            in_data,
            params,
            Params::AnisotropyDivergenceSource,
            use_anisotropy_map
                && matches!(
                    anisotropy_map_mode,
                    AnisotropyMapMode::DivergenceDirection | AnisotropyMapMode::DivergenceRotation
                ),
        )?;

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
        in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
        smart_map_layers: Option<OwnedRenderMaps>,
    ) -> Result<(), Error> {
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if out_w == 0 || out_h == 0 || in_layer.width() == 0 || in_layer.height() == 0 {
            return Ok(());
        }

        let settings = downsample_render_settings(
            read_render_settings(params)?,
            f32::from(in_data.downsample_x()),
            f32::from(in_data.downsample_y()),
        );
        let source = read_layer_buffer(&in_layer);
        let working_source = resample_nearest_buffer(source, out_w, out_h);
        let map_layers = if let Some(map_layers) = smart_map_layers {
            map_layers
        } else {
            OwnedRenderMaps {
                amount: checkout_layer_buffer(
                    params,
                    Params::AmountMapLayer,
                    settings.use_amount_map,
                )?,
                radius: checkout_layer_buffer(
                    params,
                    Params::RadiusMapLayer,
                    settings.use_radius_map,
                )?,
                grain_size: checkout_layer_buffer(
                    params,
                    Params::GrainSizeMapLayer,
                    settings.use_grain_size_map,
                )?,
                anisotropy: checkout_layer_buffer(
                    params,
                    Params::AnisotropyMapLayer,
                    settings.use_anisotropy_map,
                )?,
                kernel_texture: checkout_layer_buffer(
                    params,
                    Params::KernelTextureLayer,
                    matches!(settings.grain_shape, GrainShape::Texture),
                )?,
            }
        };
        let maps = RenderMaps {
            amount: map_buffer_ref(
                map_layers.amount.as_ref(),
                &working_source,
                settings.use_amount_map,
            ),
            radius: map_buffer_ref(
                map_layers.radius.as_ref(),
                &working_source,
                settings.use_radius_map,
            ),
            grain_size: map_buffer_ref(
                map_layers.grain_size.as_ref(),
                &working_source,
                settings.use_grain_size_map,
            ),
            anisotropy: map_buffer_ref(
                map_layers.anisotropy.as_ref(),
                &working_source,
                settings.use_anisotropy_map,
            ),
            kernel_texture: map_buffer_ref(
                map_layers.kernel_texture.as_ref(),
                &working_source,
                matches!(settings.grain_shape, GrainShape::Texture),
            ),
        };

        let frame = in_data.current_frame() as i32;
        let render_seed = temporal_seed(settings.seed, settings.temporal_mode, frame);
        let mut rendered = render_scatter(&working_source, maps, &settings, render_seed);
        drop(map_layers);
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        for (index, px) in rendered.iter_mut().enumerate() {
            let original = working_source.pixels[index];
            if settings.preserve_alpha {
                px.alpha = original.alpha;
            }
            *px = blend_with_original(*px, original, settings.blend_mode, settings.blend_opacity);
            *px = sanitize_pixel_for_output(*px, out_is_f32, settings.clamp_32);
        }

        out_layer.iterate(0, out_h as i32, None, |x, y, mut dst| {
            let px = rendered[y as usize * out_w + x as usize];
            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(px),
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn param_affects_ui(param: Params) -> bool {
    matches!(
        param,
        Params::ScatterMode
            | Params::GrainShape
            | Params::GrainFillMode
            | Params::UseAmountMap
            | Params::UseRadiusMap
            | Params::UseGrainSizeMap
            | Params::UseAnisotropyMap
            | Params::AnisotropyMapMode
            | Params::BlendMode
    )
}

fn read_render_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    let use_grain_size_map = params.get(Params::UseGrainSizeMap)?.as_checkbox()?.value();
    let base_grain_size = params
        .get(Params::GrainSize)?
        .as_slider()?
        .value()
        .clamp(1, 1024) as usize;
    let grain_size = if use_grain_size_map {
        params
            .get(Params::GrainSizeMapMax)?
            .as_slider()?
            .value()
            .clamp(1, 1024) as usize
    } else {
        base_grain_size
    };
    let grain_size_min = if use_grain_size_map {
        (params
            .get(Params::GrainSizeMin)?
            .as_slider()?
            .value()
            .clamp(1, 1024) as usize)
            .min(grain_size)
    } else {
        1
    };
    Ok(RenderSettings {
        scatter_mode: scatter_mode_from_popup(params.get(Params::ScatterMode)?.as_popup()?.value()),
        amount: (params.get(Params::Amount)?.as_float_slider()?.value() as f32 / 100.0)
            .clamp(0.0, 1.0),
        radius: params
            .get(Params::Radius)?
            .as_slider()?
            .value()
            .clamp(0, 4096),
        grain_size,
        grain_size_min,
        gather_samples: params
            .get(Params::GatherSamples)?
            .as_slider()?
            .value()
            .clamp(1, 32) as u32,
        direction: (params.get(Params::Direction)?.as_float_slider()?.value() as f32).to_radians(),
        anisotropy: (params.get(Params::Anisotropy)?.as_float_slider()?.value() as f32 / 100.0)
            .clamp(0.0, 1.0),
        grain_shape: grain_shape_from_popup(params.get(Params::GrainShape)?.as_popup()?.value()),
        grain_size_randomness: (params
            .get(Params::GrainSizeRandomness)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        grain_position_randomness: (params
            .get(Params::GrainPositionRandomness)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        grain_density: (params.get(Params::GrainDensity)?.as_float_slider()?.value() as f32
            / 100.0)
            .clamp(1.0, 8.0),
        kernel_randomness: (params
            .get(Params::KernelRandomness)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        kernel_texture_channel: map_channel_from_popup(
            params
                .get(Params::KernelTextureChannel)?
                .as_popup()?
                .value(),
        ),
        kernel_threshold: (params
            .get(Params::KernelThreshold)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        grain_fill_mode: grain_fill_mode_from_popup(
            params.get(Params::GrainFillMode)?.as_popup()?.value(),
        ),
        grain_fill_opacity: (params
            .get(Params::GrainFillOpacity)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        use_amount_map: params.get(Params::UseAmountMap)?.as_checkbox()?.value(),
        amount_map_channel: map_channel_from_popup(
            params.get(Params::AmountMapChannel)?.as_popup()?.value(),
        ),
        use_radius_map: params.get(Params::UseRadiusMap)?.as_checkbox()?.value(),
        radius_map_channel: map_channel_from_popup(
            params.get(Params::RadiusMapChannel)?.as_popup()?.value(),
        ),
        use_grain_size_map,
        grain_size_map_channel: map_channel_from_popup(
            params.get(Params::GrainSizeMapChannel)?.as_popup()?.value(),
        ),
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
        seed: params.get(Params::Seed)?.as_slider()?.value() as u32,
        temporal_mode: temporal_mode_from_popup(
            params.get(Params::TemporalMode)?.as_popup()?.value(),
        ),
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

fn downsample_render_settings(
    mut settings: RenderSettings,
    scale_x: f32,
    scale_y: f32,
) -> RenderSettings {
    // AE normally downsamples both axes equally. The geometric mean remains
    // stable for the uncommon non-square preview ratio without over-favoring
    // either axis while the public controls remain scalar.
    let pixel_scale = (scale_x.max(0.0) * scale_y.max(0.0)).sqrt();
    if settings.radius > 0 {
        settings.radius = ((settings.radius as f32 * pixel_scale).round() as i32).max(1);
    }
    settings.grain_size = ((settings.grain_size as f32 * pixel_scale).round() as usize).max(1);
    settings.grain_size_min = ((settings.grain_size_min as f32 * pixel_scale).round() as usize)
        .max(1)
        .min(settings.grain_size);
    settings
}

fn scatter_mode_from_popup(value: i32) -> ScatterMode {
    match value {
        2 => ScatterMode::Swap,
        _ => ScatterMode::Gather,
    }
}

fn map_channel_from_popup(value: i32) -> MapChannel {
    match value {
        2 => MapChannel::Red,
        3 => MapChannel::Green,
        4 => MapChannel::Blue,
        5 => MapChannel::Alpha,
        _ => MapChannel::Luma,
    }
}

fn grain_shape_from_popup(value: i32) -> GrainShape {
    match value {
        2 => GrainShape::Circle,
        3 => GrainShape::Texture,
        _ => GrainShape::Square,
    }
}

fn grain_fill_mode_from_popup(value: i32) -> GrainFillMode {
    match value {
        2 => GrainFillMode::Average,
        3 => GrainFillMode::Median,
        4 => GrainFillMode::Center,
        _ => GrainFillMode::Texture,
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

fn temporal_mode_from_popup(value: i32) -> TemporalMode {
    match value {
        2 => TemporalMode::Frame,
        _ => TemporalMode::Static,
    }
}

fn edge_mode_from_popup(value: i32) -> EdgeMode {
    match value {
        2 => EdgeMode::Clamp,
        3 => EdgeMode::Tile,
        4 => EdgeMode::Mirror,
        5 => EdgeMode::Transparent,
        _ => EdgeMode::Reject,
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

fn checkout_full_smart_layer(
    callbacks: PreRenderCallbacks,
    param_index: i32,
    query_id: i32,
    checkout_id: u32,
    request: &ae::sys::PF_RenderRequest,
    in_data: InData,
) -> Result<ae::sys::PF_CheckoutResult, Error> {
    let query_result = callbacks.checkout_layer(
        param_index,
        query_id,
        request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;
    let mut full_request = *request;
    full_request.rect = query_result.max_result_rect;
    callbacks.checkout_layer(
        param_index,
        checkout_id as i32,
        &full_request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )
}

fn checkout_smart_layer_buffer(
    callbacks: SmartRenderCallbacks,
    checkout_id: u32,
    enabled: bool,
) -> Result<Option<LayerBuffer>, Error> {
    if !enabled {
        return Ok(None);
    }
    let layer = callbacks.checkout_layer_pixels(checkout_id)?;
    let buffer = layer.as_ref().map(read_layer_buffer);
    callbacks.checkin_layer_pixels(checkout_id)?;
    Ok(buffer)
}

fn checkout_layer_buffer(
    params: &mut Parameters<Params>,
    id: Params,
    enabled: bool,
) -> Result<Option<LayerBuffer>, Error> {
    if !enabled {
        return Ok(None);
    }
    let param = params.get(id)?;
    let layer = param.as_layer()?.value();
    Ok(layer.as_ref().map(read_layer_buffer))
}

fn map_buffer_ref<'a>(
    layer_map: Option<&'a LayerBuffer>,
    input_layer: &'a LayerBuffer,
    enabled: bool,
) -> Option<&'a LayerBuffer> {
    if enabled {
        layer_map.or(Some(input_layer))
    } else {
        None
    }
}

fn read_layer_buffer(layer: &Layer) -> LayerBuffer {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut pixels = vec![transparent_pixel(); width * height];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = match world_type {
                ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
                ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
            };
        }
    }
    LayerBuffer {
        width,
        height,
        pixels,
    }
}

fn resample_nearest_buffer(source: LayerBuffer, width: usize, height: usize) -> LayerBuffer {
    if source.width == width && source.height == height {
        return source;
    }
    let mut pixels = vec![transparent_pixel(); width * height];
    for y in 0..height {
        for x in 0..width {
            let source_x = remap_coord_nearest(x, width, source.width);
            let source_y = remap_coord_nearest(y, height, source.height);
            pixels[y * width + x] = source.pixels[source_y * source.width + source_x];
        }
    }
    LayerBuffer {
        width,
        height,
        pixels,
    }
}

fn remap_coord_nearest(coord: usize, out_len: usize, source_len: usize) -> usize {
    if out_len == 0 || source_len <= 1 {
        return 0;
    }
    let mapped = ((coord as f32 + 0.5) * source_len as f32 / out_len as f32 - 0.5).round();
    (mapped as i64).clamp(0, source_len as i64 - 1) as usize
}

fn density_layers(settings: &RenderSettings) -> Vec<(u32, f32)> {
    let grain_size = settings.grain_size.max(1);
    let useful_layers = grain_size.saturating_mul(grain_size).min(8) as f32;
    let density = settings.grain_density.clamp(1.0, useful_layers);
    let full_layers = density.floor() as u32;
    let mut layers = Vec::with_capacity(density.ceil() as usize);
    for layer in 0..full_layers {
        layers.push((layer, 1.0));
    }
    let remainder = density - full_layers as f32;
    if remainder > 1.0e-6 && full_layers < useful_layers as u32 {
        layers.push((full_layers, remainder));
    }
    layers
}

fn density_layer_seed(seed: u32, layer: u32) -> u32 {
    if layer == 0 {
        seed
    } else {
        hash_u32(seed ^ layer.wrapping_mul(0x9E37_79B9) ^ 0xD1B5_4A35)
    }
}

fn render_scatter(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    match settings.scatter_mode {
        ScatterMode::Gather => render_gather(source, maps, settings, seed),
        ScatterMode::Swap => render_swap(source, maps, settings, seed),
    }
}

fn render_gather(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    let mut output = source.pixels.clone();
    if settings.amount <= 0.0 || settings.radius <= 0 {
        return output;
    }

    let layers = density_layers(settings)
        .into_iter()
        .map(|(density_layer, density_weight)| {
            (
                KernelContext {
                    image_width: source.width,
                    image_height: source.height,
                    maps,
                    settings,
                    seed: density_layer_seed(seed, density_layer),
                    density_layer,
                },
                density_weight,
            )
        })
        .collect::<Vec<_>>();
    render_gather_layers(&mut output, source, &layers);
    output
}

#[cfg(test)]
fn render_gather_layer(
    output: &mut [PixelF32],
    source: &LayerBuffer,
    kernel_context: KernelContext<'_>,
    density_weight: f32,
) {
    render_gather_layers(output, source, &[(kernel_context, density_weight)]);
}

fn render_gather_layers(
    output: &mut [PixelF32],
    source: &LayerBuffer,
    layers: &[(KernelContext<'_>, f32)],
) {
    if source.width == 0 || source.height == 0 || layers.is_empty() {
        return;
    }
    if layers.len() == 1
        && layers[0].0.density_layer == 0
        && layers[0].0.settings.grain_position_randomness <= 0.0
        && layers[0].0.settings.kernel_randomness <= 0.0
    {
        render_non_overlapping_gather_layer(output, source, layers[0].0, layers[0].1);
        return;
    }

    let grain = layers[0].0.settings.grain_size.max(1);
    let columns = source.width.div_ceil(grain);
    let rows = source.height.div_ceil(grain);
    let grains_per_layer = columns * rows;
    let total_grains = grains_per_layer.saturating_mul(layers.len());
    assert!(
        total_grains < u32::MAX as usize,
        "ScatterMap image contains too many grains"
    );

    // A single owner id per pixel is enough because grain_front_priority is a
    // permutation of the stable id for a fixed seed, so priorities cannot tie.
    let priority_seed = layers[0].0.seed ^ 0xCBBB_9D5D;
    let mut winners = vec![u32::MAX; source.width * source.height];
    for (layer_slot, &(context, density_weight)) in layers.iter().enumerate() {
        let id_base = layer_slot * grains_per_layer;
        for block_index in 0..grains_per_layer {
            let block = block_at(
                block_index % columns,
                block_index / columns,
                grain,
                source.width,
                source.height,
            );
            let prepared = prepare_kernel(block, context);
            if gather_grain_radius_at(
                block,
                prepared.center_x,
                prepared.center_y,
                context,
                density_weight,
            )
            .is_none()
            {
                continue;
            }
            let grain_id = (id_base + block_index) as u32;
            let priority = grain_front_priority(priority_seed, grain_id);
            visit_kernel_pixels(prepared, context, |index| {
                let current = winners[index];
                if current == u32::MAX || priority > grain_front_priority(priority_seed, current) {
                    winners[index] = grain_id;
                }
            });
        }
    }

    let mut visible_grains = vec![false; total_grains];
    for &winner in &winners {
        if winner != u32::MAX {
            visible_grains[winner as usize] = true;
        }
    }
    let settings = layers[0].0.settings;
    let fill_changes_grain = !matches!(settings.grain_fill_mode, GrainFillMode::Texture)
        && settings.grain_fill_opacity > 0.0;
    let mut visible_indices = Vec::new();
    let mut median_scratch = Vec::new();
    let mut primary_offsets = Vec::with_capacity(settings.gather_samples as usize);
    for (layer_slot, &(context, density_weight)) in layers.iter().enumerate() {
        let id_base = layer_slot * grains_per_layer;
        for block_index in 0..grains_per_layer {
            let winner_id = (id_base + block_index) as u32;
            if !visible_grains[winner_id as usize] {
                continue;
            }
            let block = block_at(
                block_index % columns,
                block_index / columns,
                grain,
                source.width,
                source.height,
            );
            let prepared = prepare_kernel(block, context);
            let Some(state) = gather_grain_state_at(
                block,
                prepared.center_x,
                prepared.center_y,
                context,
                density_weight,
            ) else {
                continue;
            };
            gather_primary_offsets(block, state, context, &mut primary_offsets);

            if fill_changes_grain {
                visible_indices.clear();
                visit_kernel_bounds(prepared, context, |index| {
                    if winners[index] == winner_id {
                        visible_indices.push(index as u32);
                    }
                });
                if visible_indices.is_empty() {
                    continue;
                }
                for &index in &visible_indices {
                    let index = index as usize;
                    let x = index % source.width;
                    let y = index / source.width;
                    output[index] =
                        gather_pixel(source, x, y, block, state, &primary_offsets, context);
                }
                let representative = grain_representative(
                    output,
                    &visible_indices,
                    block,
                    context,
                    settings.grain_fill_mode,
                    &mut median_scratch,
                );
                for &index in &visible_indices {
                    let index = index as usize;
                    output[index] =
                        lerp_pixel(output[index], representative, settings.grain_fill_opacity);
                }
            } else {
                visit_kernel_bounds(prepared, context, |index| {
                    if winners[index] == winner_id {
                        let x = index % source.width;
                        let y = index / source.width;
                        output[index] =
                            gather_pixel(source, x, y, block, state, &primary_offsets, context);
                    }
                });
            }
        }
    }
}

fn render_non_overlapping_gather_layer(
    output: &mut [PixelF32],
    source: &LayerBuffer,
    context: KernelContext<'_>,
    density_weight: f32,
) {
    let settings = context.settings;
    let grain = settings.grain_size.max(1);
    let columns = source.width.div_ceil(grain);
    let rows = source.height.div_ceil(grain);
    let fill_changes_grain = !matches!(settings.grain_fill_mode, GrainFillMode::Texture)
        && settings.grain_fill_opacity > 0.0;
    let mut visible_indices = Vec::new();
    let mut median_scratch = Vec::new();
    let mut primary_offsets = Vec::with_capacity(settings.gather_samples as usize);

    for block_index in 0..columns * rows {
        let block = block_at(
            block_index % columns,
            block_index / columns,
            grain,
            source.width,
            source.height,
        );
        let prepared = prepare_kernel(block, context);
        let Some(state) = gather_grain_state_at(
            block,
            prepared.center_x,
            prepared.center_y,
            context,
            density_weight,
        ) else {
            continue;
        };
        gather_primary_offsets(block, state, context, &mut primary_offsets);

        if fill_changes_grain {
            visible_indices.clear();
            visit_kernel_pixels(prepared, context, |index| {
                visible_indices.push(index as u32);
            });
            if visible_indices.is_empty() {
                continue;
            }
            for &index in &visible_indices {
                let index = index as usize;
                let x = index % source.width;
                let y = index / source.width;
                output[index] = gather_pixel(source, x, y, block, state, &primary_offsets, context);
            }
            let representative = grain_representative(
                output,
                &visible_indices,
                block,
                context,
                settings.grain_fill_mode,
                &mut median_scratch,
            );
            for &index in &visible_indices {
                let index = index as usize;
                output[index] =
                    lerp_pixel(output[index], representative, settings.grain_fill_opacity);
            }
        } else {
            visit_kernel_pixels(prepared, context, |index| {
                let x = index % source.width;
                let y = index / source.width;
                output[index] = gather_pixel(source, x, y, block, state, &primary_offsets, context);
            });
        }
    }
}

fn gather_grain_state_at(
    block: Block,
    center_x: f32,
    center_y: f32,
    context: KernelContext<'_>,
    density_weight: f32,
) -> Option<GatherGrainState> {
    let radius = gather_grain_radius_at(block, center_x, center_y, context, density_weight)?;
    let (sample_x, sample_y) = sample_point_from_center(center_x, center_y, context);
    let settings = context.settings;
    let (direction, anisotropy) = anisotropy_at(
        context.maps.anisotropy,
        sample_x,
        sample_y,
        context.image_width,
        context.image_height,
        settings,
    );
    Some(GatherGrainState {
        radius,
        transform: anisotropy_transform(direction, anisotropy),
    })
}

fn gather_grain_radius_at(
    block: Block,
    center_x: f32,
    center_y: f32,
    context: KernelContext<'_>,
    density_weight: f32,
) -> Option<i32> {
    let (sample_x, sample_y) = sample_point_from_center(center_x, center_y, context);
    let settings = context.settings;
    let amount = settings.amount
        * density_weight
        * map_value(
            context.maps.amount,
            sample_x,
            sample_y,
            context.image_width,
            context.image_height,
            settings.amount_map_channel,
            1.0,
        );
    let event = rand01(hash_coords(
        context.seed,
        block.grid_x as i32,
        block.grid_y as i32,
        0,
        0xA1,
    ));
    if event >= amount.clamp(0.0, 1.0) {
        return None;
    }
    let radius_factor = map_value(
        context.maps.radius,
        sample_x,
        sample_y,
        context.image_width,
        context.image_height,
        settings.radius_map_channel,
        1.0,
    );
    let radius = ((settings.radius as f32) * radius_factor.clamp(0.0, 1.0)).floor() as i32;
    if radius <= 0 {
        return None;
    }
    Some(radius)
}

#[allow(clippy::too_many_arguments)]
fn gather_pixel(
    source: &LayerBuffer,
    x: usize,
    y: usize,
    block: Block,
    state: GatherGrainState,
    primary_offsets: &[(i32, i32)],
    context: KernelContext<'_>,
) -> PixelF32 {
    #[cfg(test)]
    GATHER_PIXEL_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut sum = transparent_pixel();
    for tap in 0..context.settings.gather_samples {
        let sampled = gather_sample(
            source,
            x,
            y,
            block.grid_x as i32,
            block.grid_y as i32,
            state.radius,
            state.transform,
            tap,
            primary_offsets[tap as usize],
            context.seed,
            context.settings.edge_mode,
        );
        sum.alpha += sampled.alpha;
        sum.red += sampled.red;
        sum.green += sampled.green;
        sum.blue += sampled.blue;
    }
    let inverse = (context.settings.gather_samples as f32).recip();
    PixelF32 {
        alpha: sum.alpha * inverse,
        red: sum.red * inverse,
        green: sum.green * inverse,
        blue: sum.blue * inverse,
    }
}

fn gather_primary_offsets(
    block: Block,
    state: GatherGrainState,
    context: KernelContext<'_>,
    offsets: &mut Vec<(i32, i32)>,
) {
    offsets.clear();
    for tap in 0..context.settings.gather_samples {
        offsets.push(discrete_anisotropic_offset_with_transform(
            block.grid_x as i32,
            block.grid_y as i32,
            tap,
            0,
            state.radius,
            state.transform,
            context.seed,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn gather_sample(
    source: &LayerBuffer,
    x: usize,
    y: usize,
    cell_x: i32,
    cell_y: i32,
    radius: i32,
    transform: AnisotropyTransform,
    tap: u32,
    primary_offset: (i32, i32),
    seed: u32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let first_attempt = if matches!(edge_mode, EdgeMode::Reject) {
        0
    } else {
        MAX_GATHER_ATTEMPTS
    };
    for attempt in first_attempt..MAX_GATHER_ATTEMPTS {
        let (dx, dy) = if attempt == 0 {
            primary_offset
        } else {
            discrete_anisotropic_offset_with_transform(
                cell_x, cell_y, tap, attempt, radius, transform, seed,
            )
        };
        let sample_x = x as i32 + dx;
        let sample_y = y as i32 + dy;
        if matches!(edge_mode, EdgeMode::Reject) {
            if let (Some(sample_x), Some(sample_y)) = (
                in_bounds_coord(sample_x, source.width),
                in_bounds_coord(sample_y, source.height),
            ) {
                return source.pixels[sample_y * source.width + sample_x];
            }
            continue;
        }
        let sample_x = resolve_coord(sample_x, source.width, edge_mode);
        let sample_y = resolve_coord(sample_y, source.height, edge_mode);
        if let (Some(sample_x), Some(sample_y)) = (sample_x, sample_y) {
            return source.pixels[sample_y * source.width + sample_x];
        }
        return transparent_pixel();
    }
    let sample_x = x as i32 + primary_offset.0;
    let sample_y = y as i32 + primary_offset.1;
    let sample_x = resolve_coord(sample_x, source.width, edge_mode);
    let sample_y = resolve_coord(sample_y, source.height, edge_mode);
    if let (Some(sample_x), Some(sample_y)) = (sample_x, sample_y) {
        return source.pixels[sample_y * source.width + sample_x];
    }
    if matches!(edge_mode, EdgeMode::Transparent) {
        return transparent_pixel();
    }
    source.pixels[y * source.width + x]
}

fn build_swap_partition<'a>(
    width: usize,
    height: usize,
    maps: RenderMaps<'a>,
    settings: &'a RenderSettings,
    seed: u32,
) -> (GrainPartition, Vec<(KernelContext<'a>, f32)>, usize, usize) {
    let grain = settings.grain_size.max(1);
    let columns = width.div_ceil(grain);
    let rows = height.div_ceil(grain);
    let groups_per_layer = columns * rows;
    let layers = density_layers(settings)
        .into_iter()
        .map(|(density_layer, density_weight)| {
            (
                KernelContext {
                    image_width: width,
                    image_height: height,
                    maps,
                    settings,
                    seed: density_layer_seed(seed, density_layer),
                    density_layer,
                },
                density_weight,
            )
        })
        .collect::<Vec<_>>();
    let total_grains = groups_per_layer.saturating_mul(layers.len());
    assert!(
        total_grains < u32::MAX as usize && width.saturating_mul(height) <= u32::MAX as usize,
        "ScatterMap image contains too many pixels or grains"
    );

    if layers.len() == 1
        && layers[0].0.density_layer == 0
        && settings.grain_position_randomness <= 0.0
        && settings.kernel_randomness <= 0.0
    {
        let partition = build_non_overlapping_swap_partition(
            width,
            height,
            columns,
            rows,
            layers[0].0,
            layers[0].1,
        );
        return (partition, layers, columns, rows);
    }

    let priority_seed = seed;
    let mut owners = vec![u32::MAX; width * height];
    for (layer_slot, &(context, density_weight)) in layers.iter().enumerate() {
        let id_base = layer_slot * groups_per_layer;
        for block_index in 0..groups_per_layer {
            let block = block_at(
                block_index % columns,
                block_index / columns,
                grain,
                width,
                height,
            );
            let prepared = prepare_kernel(block, context);
            if !swap_grain_is_active(block, prepared, context, density_weight) {
                continue;
            }
            let grain_id = (id_base + block_index) as u32;
            let priority = grain_front_priority(priority_seed, grain_id);
            visit_kernel_pixels(prepared, context, |pixel_index| {
                let current = owners[pixel_index];
                if current == u32::MAX || priority > grain_front_priority(priority_seed, current) {
                    owners[pixel_index] = grain_id;
                }
            });
        }
    }
    let partition = grain_partition_from_global_owners(
        &owners,
        0,
        groups_per_layer * layers.len(),
        columns,
        rows,
    );
    (partition, layers, columns, rows)
}

fn build_non_overlapping_swap_partition(
    width: usize,
    height: usize,
    columns: usize,
    rows: usize,
    context: KernelContext<'_>,
    density_weight: f32,
) -> GrainPartition {
    let grain = context.settings.grain_size.max(1);
    let group_count = columns * rows;
    let mut offsets = Vec::with_capacity(group_count + 1);
    let mut indices = Vec::with_capacity(width * height);
    offsets.push(0_u32);
    for block_index in 0..group_count {
        let block = block_at(
            block_index % columns,
            block_index / columns,
            grain,
            width,
            height,
        );
        let prepared = prepare_kernel(block, context);
        if swap_grain_is_active(block, prepared, context, density_weight) {
            visit_kernel_pixels(prepared, context, |pixel_index| {
                indices.push(pixel_index as u32);
            });
        }
        offsets.push(indices.len() as u32);
    }
    GrainPartition {
        offsets,
        indices,
        columns,
        rows,
    }
}

fn swap_grain_is_active(
    block: Block,
    prepared: PreparedKernel,
    context: KernelContext<'_>,
    density_weight: f32,
) -> bool {
    let (sample_x, sample_y) =
        sample_point_from_center(prepared.center_x, prepared.center_y, context);
    let settings = context.settings;
    let amount = settings.amount
        * density_weight
        * map_value(
            context.maps.amount,
            sample_x,
            sample_y,
            context.image_width,
            context.image_height,
            settings.amount_map_channel,
            1.0,
        );
    if rand01(hash_coords(
        context.seed ^ 0x6C8E_9CF5,
        block.grid_x as i32,
        block.grid_y as i32,
        0,
        0xB1,
    )) >= amount.clamp(0.0, 1.0)
    {
        return false;
    }
    let radius_factor = map_value(
        context.maps.radius,
        sample_x,
        sample_y,
        context.image_width,
        context.image_height,
        settings.radius_map_channel,
        1.0,
    );
    ((settings.radius as f32) * radius_factor.clamp(0.0, 1.0)).floor() as i32 > 0
}

fn grain_partition_from_global_owners(
    owners: &[u32],
    id_base: usize,
    group_count: usize,
    columns: usize,
    rows: usize,
) -> GrainPartition {
    let id_base = id_base as u32;
    let id_end = id_base + group_count as u32;
    let mut offsets = vec![0_u32; group_count + 1];
    for &owner in owners {
        if (id_base..id_end).contains(&owner) {
            offsets[(owner - id_base) as usize + 1] += 1;
        }
    }
    for index in 1..offsets.len() {
        offsets[index] = offsets[index - 1].saturating_add(offsets[index]);
    }
    let assigned = offsets.last().copied().unwrap_or(0);
    let mut indices = vec![0_u32; assigned as usize];
    for (pixel_index, &owner) in owners.iter().enumerate().rev() {
        if !(id_base..id_end).contains(&owner) {
            continue;
        }
        let end = &mut offsets[(owner - id_base) as usize + 1];
        *end -= 1;
        indices[*end as usize] = pixel_index as u32;
    }
    for index in 0..group_count {
        offsets[index] = offsets[index + 1];
    }
    offsets[group_count] = assigned;
    GrainPartition {
        offsets,
        indices,
        columns,
        rows,
    }
}

fn render_swap(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    if source.width == 0 || source.height == 0 || settings.amount <= 0.0 || settings.radius <= 0 {
        return source.pixels.clone();
    }
    let pixel_count = source.width * source.height;
    assert!(
        pixel_count <= u32::MAX as usize,
        "ScatterMap image contains too many pixels"
    );
    let mut permutation: Vec<u32> = (0..pixel_count as u32).collect();
    let (partition, layers, columns, rows) =
        build_swap_partition(source.width, source.height, maps, settings, seed);
    let groups_per_layer = columns * rows;
    let fill_changes_grain = !matches!(settings.grain_fill_mode, GrainFillMode::Texture)
        && settings.grain_fill_opacity > 0.0;
    let mut affected_layers = Vec::new();
    for (layer_slot, &(kernel_context, _)) in layers.iter().enumerate() {
        let affected = build_swap_permutation_for_partition(
            &mut permutation,
            source.width,
            source.height,
            maps,
            settings,
            kernel_context.seed,
            &partition,
            layer_slot * groups_per_layer,
            kernel_context,
        );
        if fill_changes_grain {
            affected_layers.push((layer_slot, affected));
        }
    }
    if !fill_changes_grain {
        drop(partition);
        return permutation
            .into_iter()
            .map(|source_index| source.pixels[source_index as usize])
            .collect();
    }
    let mut output: Vec<PixelF32> = permutation
        .into_iter()
        .map(|source_index| source.pixels[source_index as usize])
        .collect();
    for (layer_slot, affected) in affected_layers {
        let kernel_context = layers[layer_slot].0;
        apply_grain_fill(
            &mut output,
            &partition,
            layer_slot * groups_per_layer,
            &affected,
            kernel_context,
        );
    }
    output
}

#[cfg(test)]
fn build_swap_permutation(
    width: usize,
    height: usize,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<u32> {
    assert!(
        width.saturating_mul(height) <= u32::MAX as usize,
        "ScatterMap image contains too many pixels"
    );
    let mut permutation: Vec<u32> = (0..(width * height) as u32).collect();
    if width == 0 || height == 0 || settings.amount <= 0.0 || settings.radius <= 0 {
        return permutation;
    }
    let (partition, layers, columns, rows) =
        build_swap_partition(width, height, maps, settings, seed);
    let groups_per_layer = columns * rows;
    for (layer_slot, &(kernel_context, _)) in layers.iter().enumerate() {
        build_swap_permutation_for_partition(
            &mut permutation,
            width,
            height,
            maps,
            settings,
            kernel_context.seed,
            &partition,
            layer_slot * groups_per_layer,
            kernel_context,
        );
    }
    permutation
}

#[allow(clippy::too_many_arguments)]
fn build_swap_permutation_for_partition(
    permutation: &mut [u32],
    width: usize,
    height: usize,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
    partition: &GrainPartition,
    group_base: usize,
    kernel_context: KernelContext<'_>,
) -> Vec<bool> {
    let grain = settings.grain_size.max(1);

    let group_count = partition.columns * partition.rows;
    let mut radii = vec![0_i32; group_count];
    let mut order = Vec::with_capacity(group_count.min(partition.indices.len()));
    for (index, radius) in radii.iter_mut().enumerate() {
        if partition.group(group_base + index).is_empty() {
            continue;
        }
        let block = partition_block(index, partition, kernel_context);
        let (center_x, center_y) = grain_sample_point(block, kernel_context);
        let radius_factor = map_value(
            maps.radius,
            center_x,
            center_y,
            width,
            height,
            settings.radius_map_channel,
            1.0,
        );
        *radius = ((settings.radius as f32) * radius_factor.clamp(0.0, 1.0)).floor() as i32;
        if *radius <= 0 {
            continue;
        }
        order.push(index as u32);
    }

    shuffle_values(&mut order, seed ^ 0xD1B5_4A35);
    let mut used = vec![false; group_count];
    for block_index in order {
        let block_index = block_index as usize;
        if used[block_index] {
            continue;
        }
        let block = partition_block(block_index, partition, kernel_context);
        let (block_center_x, block_center_y) = kernel_center(block, kernel_context);
        let (sample_x, sample_y) =
            sample_point_from_center(block_center_x, block_center_y, kernel_context);
        let (direction, anisotropy) =
            anisotropy_at(maps.anisotropy, sample_x, sample_y, width, height, settings);
        let transform = anisotropy_transform(direction, anisotropy);
        let cell_radius = (radii[block_index] as usize).div_ceil(grain).max(1) as i32;
        let mut partner = None;
        for attempt in 0..MAX_SWAP_ATTEMPTS {
            let (offset_x, offset_y) = discrete_anisotropic_offset_with_transform(
                block.grid_x as i32,
                block.grid_y as i32,
                0,
                attempt,
                cell_radius,
                transform,
                seed ^ 0x94D0_49BB,
            );
            let candidate_x = block.grid_x as i32 + offset_x;
            let candidate_y = block.grid_y as i32 + offset_y;
            if candidate_x < 0
                || candidate_y < 0
                || candidate_x >= partition.columns as i32
                || candidate_y >= partition.rows as i32
            {
                continue;
            }
            let candidate_index = candidate_y as usize * partition.columns + candidate_x as usize;
            let candidate = partition_block(candidate_index, partition, kernel_context);
            if candidate_index == block_index
                || used[candidate_index]
                || radii[candidate_index] <= 0
                || partition.group(group_base + candidate_index).len()
                    != partition.group(group_base + block_index).len()
            {
                continue;
            }
            let (candidate_center_x, candidate_center_y) = kernel_center(candidate, kernel_context);
            let dx = candidate_center_x - block_center_x;
            let dy = candidate_center_y - block_center_y;
            let allowed_radius = radii[block_index].min(radii[candidate_index]) as f32;
            if dx * dx + dy * dy > allowed_radius * allowed_radius {
                continue;
            }
            partner = Some(candidate_index);
            break;
        }
        if let Some(partner_index) = partner {
            let allowed_radius = radii[block_index].min(radii[partner_index]);
            if swap_grain_groups(
                permutation,
                partition.group(group_base + block_index),
                partition.group(group_base + partner_index),
                width,
                allowed_radius,
            ) {
                used[block_index] = true;
                used[partner_index] = true;
            }
        }
    }
    used
}

fn swap_grain_groups(
    permutation: &mut [u32],
    a: &[u32],
    b: &[u32],
    width: usize,
    allowed_radius: i32,
) -> bool {
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    let allowed_squared = allowed_radius as i64 * allowed_radius as i64;
    for (&destination_a, &destination_b) in a.iter().zip(b) {
        let destination_a = destination_a as usize;
        let destination_b = destination_b as usize;
        let ax = destination_a % width;
        let ay = destination_a / width;
        let bx = destination_b % width;
        let by = destination_b / width;
        let dx = ax as i64 - bx as i64;
        let dy = ay as i64 - by as i64;
        let source_a = permutation[destination_a] as usize;
        let source_b = permutation[destination_b] as usize;
        let source_ax = source_a % width;
        let source_ay = source_a / width;
        let source_bx = source_b % width;
        let source_by = source_b / width;
        let next_a_dx = ax as i64 - source_bx as i64;
        let next_a_dy = ay as i64 - source_by as i64;
        let next_b_dx = bx as i64 - source_ax as i64;
        let next_b_dy = by as i64 - source_ay as i64;
        if dx * dx + dy * dy > allowed_squared
            || next_a_dx * next_a_dx + next_a_dy * next_a_dy > allowed_squared
            || next_b_dx * next_b_dx + next_b_dy * next_b_dy > allowed_squared
        {
            return false;
        }
    }
    for (&destination_a, &destination_b) in a.iter().zip(b) {
        permutation.swap(destination_a as usize, destination_b as usize);
    }
    true
}

fn block_at(
    grid_x: usize,
    grid_y: usize,
    grain: usize,
    image_width: usize,
    image_height: usize,
) -> Block {
    let x = grid_x * grain;
    let y = grid_y * grain;
    Block {
        x,
        y,
        width: grain.min(image_width - x),
        height: grain.min(image_height - y),
        grid_x,
        grid_y,
    }
}

#[cfg(test)]
fn build_grain_partition(
    width: usize,
    height: usize,
    context: KernelContext<'_>,
) -> GrainPartition {
    let grain = context.settings.grain_size.max(1);
    let columns = width.div_ceil(grain);
    let rows = height.div_ceil(grain);
    let group_count = columns * rows;
    assert!(
        group_count < u32::MAX as usize && width.saturating_mul(height) <= u32::MAX as usize,
        "ScatterMap image contains too many pixels or grains"
    );
    let mut winners = vec![u32::MAX; width * height];
    for block_index in 0..group_count {
        let block = block_at(
            block_index % columns,
            block_index / columns,
            grain,
            width,
            height,
        );
        let prepared = prepare_kernel(block, context);
        visit_kernel_pixels(prepared, context, |pixel_index| {
            let current = winners[pixel_index];
            if current == u32::MAX
                || prepared.front_priority > grain_front_priority(context.seed, current)
            {
                winners[pixel_index] = block_index as u32;
            }
        });
    }
    grain_partition_from_global_owners(&winners, 0, group_count, columns, rows)
}

fn partition_block(index: usize, partition: &GrainPartition, context: KernelContext<'_>) -> Block {
    block_at(
        index % partition.columns,
        index / partition.columns,
        context.settings.grain_size.max(1),
        context.image_width,
        context.image_height,
    )
}

fn kernel_center(block: Block, context: KernelContext<'_>) -> (f32, f32) {
    let randomness = context.settings.grain_position_randomness;
    let base_x = block.x as f32 + block.width as f32 * 0.5;
    let base_y = block.y as f32 + block.height as f32 * 0.5;
    if context.density_layer == 0 && randomness <= 0.0 {
        return (base_x, base_y);
    }
    let random_x = rand01(hash_coords(
        context.seed ^ 0x510E_527F,
        block.grid_x as i32,
        block.grid_y as i32,
        0,
        0xC5,
    ));
    let random_y = rand01(hash_coords(
        context.seed ^ 0x9B05_688C,
        block.grid_x as i32,
        block.grid_y as i32,
        0,
        0xC6,
    ));
    if context.density_layer == 0 {
        let offset_x = (random_x * 2.0 - 1.0) * block.width as f32 * 0.5 * randomness;
        let offset_y = (random_y * 2.0 - 1.0) * block.height as f32 * 0.5 * randomness;
        return (
            (base_x + offset_x).clamp(0.5, context.image_width as f32 - 0.5),
            (base_y + offset_y).clamp(0.5, context.image_height as f32 - 0.5),
        );
    }
    let (phase_x, phase_y) = density_layer_phase(context.density_layer);
    let offset_x = lerp(phase_x, random_x - 0.5, randomness) * block.width as f32;
    let offset_y = lerp(phase_y, random_y - 0.5, randomness) * block.height as f32;
    let center_x = (base_x + offset_x).clamp(0.5, context.image_width as f32 - 0.5);
    let center_y = (base_y + offset_y).clamp(0.5, context.image_height as f32 - 0.5);
    (center_x, center_y)
}

fn density_layer_phase(layer: u32) -> (f32, f32) {
    const PHASES: [(f32, f32); 16] = [
        (0.0, 0.0),
        (-0.5, -0.5),
        (-0.5, 0.0),
        (0.0, -0.5),
        (-0.25, -0.25),
        (0.25, 0.25),
        (-0.25, 0.25),
        (0.25, -0.25),
        (-0.25, 0.0),
        (0.25, 0.0),
        (0.0, -0.25),
        (0.0, 0.25),
        (-0.5, -0.25),
        (-0.5, 0.25),
        (-0.25, -0.5),
        (0.25, -0.5),
    ];
    PHASES[layer.min(PHASES.len() as u32 - 1) as usize]
}

fn grain_front_priority(seed: u32, grain_id: u32) -> u32 {
    hash_u32(seed ^ grain_id.wrapping_mul(0x9E37_79B9) ^ 0xA54F_F53A)
}

fn prepare_kernel(block: Block, context: KernelContext<'_>) -> PreparedKernel {
    let (center_x, center_y) = kernel_center(block, context);
    let scale = kernel_scale_at(block, center_x, center_y, context);
    #[cfg(test)]
    let columns = context
        .image_width
        .div_ceil(context.settings.grain_size.max(1));
    #[cfg(test)]
    let grain_id = block.grid_y * columns + block.grid_x;
    PreparedKernel {
        block,
        center_x,
        center_y,
        radius_x: (block.width as f32 * 0.5 * scale).max(0.5),
        radius_y: (block.height as f32 * 0.5 * scale).max(0.5),
        #[cfg(test)]
        front_priority: grain_front_priority(context.seed, grain_id as u32),
    }
}

fn grain_sample_point(block: Block, context: KernelContext<'_>) -> (usize, usize) {
    let (center_x, center_y) = kernel_center(block, context);
    sample_point_from_center(center_x, center_y, context)
}

fn sample_point_from_center(
    center_x: f32,
    center_y: f32,
    context: KernelContext<'_>,
) -> (usize, usize) {
    let pixel_x = (center_x - 0.5)
        .round()
        .clamp(0.0, context.image_width.saturating_sub(1) as f32) as usize;
    let pixel_y = (center_y - 0.5)
        .round()
        .clamp(0.0, context.image_height.saturating_sub(1) as f32) as usize;
    (pixel_x, pixel_y)
}

#[cfg(test)]
fn kernel_contains(
    block: Block,
    pixel_x: usize,
    pixel_y: usize,
    context: KernelContext<'_>,
) -> bool {
    kernel_contains_prepared(prepare_kernel(block, context), pixel_x, pixel_y, context)
}

fn kernel_contains_prepared(
    prepared: PreparedKernel,
    pixel_x: usize,
    pixel_y: usize,
    context: KernelContext<'_>,
) -> bool {
    let block = prepared.block;
    let nx = (pixel_x as f32 + 0.5 - prepared.center_x) / prepared.radius_x;
    let ny = (pixel_y as f32 + 0.5 - prepared.center_y) / prepared.radius_y;
    let shape_noise = if context.settings.kernel_randomness > 0.0 {
        rand01(hash_coords(
            context.seed ^ 0x3C6E_F372,
            block.grid_x as i32,
            block.grid_y as i32,
            (pixel_y * context.image_width + pixel_x) as i32,
            0xC3,
        )) * 2.0
            - 1.0
    } else {
        0.0
    };

    match context.settings.grain_shape {
        GrainShape::Square | GrainShape::Circle => {
            let distance = if matches!(context.settings.grain_shape, GrainShape::Circle) {
                (nx * nx + ny * ny).sqrt()
            } else {
                nx.abs().max(ny.abs())
            };
            let boundary = 1.0 + shape_noise * context.settings.kernel_randomness * 0.35;
            distance <= boundary
        }
        GrainShape::Texture => {
            if nx.abs() > 1.0 || ny.abs() > 1.0 {
                return false;
            }
            let Some(texture) = context.maps.kernel_texture else {
                return false;
            };
            let u = (nx + 1.0) * 0.5;
            let v = (ny + 1.0) * 0.5;
            let texture_x = u * texture.width as f32 - 0.5;
            let texture_y = v * texture.height as f32 - 0.5;
            let value = scalar_from_pixel(
                sample_map_bilinear(texture, texture_x, texture_y),
                context.settings.kernel_texture_channel,
            )
            .clamp(0.0, 1.0);
            let threshold = (context.settings.kernel_threshold
                + shape_noise * context.settings.kernel_randomness * 0.5)
                .clamp(0.0, 1.0);
            value >= threshold
        }
    }
}

fn kernel_pixel_bounds(
    prepared: PreparedKernel,
    context: KernelContext<'_>,
) -> Option<(usize, usize, usize, usize)> {
    if context.image_width == 0 || context.image_height == 0 {
        return None;
    }
    let support = match context.settings.grain_shape {
        GrainShape::Square | GrainShape::Circle => 1.0 + context.settings.kernel_randomness * 0.35,
        GrainShape::Texture => 1.0,
    };
    let geometric_min_x = (prepared.center_x - prepared.radius_x * support - 0.5).ceil() as i64;
    let geometric_max_x = (prepared.center_x + prepared.radius_x * support - 0.5).floor() as i64;
    let geometric_min_y = (prepared.center_y - prepared.radius_y * support - 0.5).ceil() as i64;
    let geometric_max_y = (prepared.center_y + prepared.radius_y * support - 0.5).floor() as i64;

    let grain = context.settings.grain_size.max(1);
    let columns = context.image_width.div_ceil(grain);
    let rows = context.image_height.div_ceil(grain);
    let neighbor_range =
        usize::from(context.settings.grain_position_randomness > 0.0 || context.density_layer > 0);
    let allowed_min_grid_x = prepared.block.grid_x.saturating_sub(neighbor_range);
    let allowed_max_grid_x = (prepared.block.grid_x + neighbor_range).min(columns - 1);
    let allowed_min_grid_y = prepared.block.grid_y.saturating_sub(neighbor_range);
    let allowed_max_grid_y = (prepared.block.grid_y + neighbor_range).min(rows - 1);
    let allowed_min_x = (allowed_min_grid_x * grain) as i64;
    let allowed_max_x = ((allowed_max_grid_x + 1) * grain).min(context.image_width) as i64 - 1;
    let allowed_min_y = (allowed_min_grid_y * grain) as i64;
    let allowed_max_y = ((allowed_max_grid_y + 1) * grain).min(context.image_height) as i64 - 1;

    let min_x = geometric_min_x.max(allowed_min_x).max(0);
    let max_x = geometric_max_x
        .min(allowed_max_x)
        .min(context.image_width as i64 - 1);
    let min_y = geometric_min_y.max(allowed_min_y).max(0);
    let max_y = geometric_max_y
        .min(allowed_max_y)
        .min(context.image_height as i64 - 1);
    (min_x <= max_x && min_y <= max_y).then_some((
        min_x as usize,
        max_x as usize,
        min_y as usize,
        max_y as usize,
    ))
}

fn visit_kernel_bounds(
    prepared: PreparedKernel,
    context: KernelContext<'_>,
    mut visitor: impl FnMut(usize),
) {
    let Some((min_x, max_x, min_y, max_y)) = kernel_pixel_bounds(prepared, context) else {
        return;
    };
    for y in min_y..=max_y {
        let row = y * context.image_width;
        for x in min_x..=max_x {
            visitor(row + x);
        }
    }
}

fn visit_kernel_pixels(
    prepared: PreparedKernel,
    context: KernelContext<'_>,
    mut visitor: impl FnMut(usize),
) {
    visit_kernel_bounds(prepared, context, |index| {
        let x = index % context.image_width;
        let y = index / context.image_width;
        if kernel_contains_prepared(prepared, x, y, context) {
            visitor(index);
        }
    });
}

#[cfg(test)]
fn kernel_scale(block: Block, context: KernelContext<'_>) -> f32 {
    let (center_x, center_y) = kernel_center(block, context);
    kernel_scale_at(block, center_x, center_y, context)
}

fn kernel_scale_at(_block: Block, center_x: f32, center_y: f32, context: KernelContext<'_>) -> f32 {
    let grain_size = context.settings.grain_size.max(1) as f32;
    let grain_size_min = context
        .settings
        .grain_size_min
        .clamp(1, context.settings.grain_size.max(1)) as f32;
    let (sample_x, sample_y) = sample_point_from_center(center_x, center_y, context);
    let map_factor = map_value(
        context.maps.grain_size,
        sample_x,
        sample_y,
        context.image_width,
        context.image_height,
        context.settings.grain_size_map_channel,
        1.0,
    );
    let map_factor = map_factor.clamp(0.0, 1.0);
    let mapped_size = lerp(grain_size_min, grain_size, map_factor).round();
    let randomized_size = if context.settings.grain_size_randomness > 0.0 {
        let random = rand01(hash_coords(
            context.seed ^ 0xBB67_AE85,
            _block.grid_x as i32,
            _block.grid_y as i32,
            0,
            0xC4,
        ));
        lerp(
            mapped_size,
            grain_size_min,
            context.settings.grain_size_randomness * random,
        )
    } else {
        mapped_size
    };
    (randomized_size / grain_size).clamp(grain_size_min / grain_size, 1.0)
}

fn apply_grain_fill(
    output: &mut [PixelF32],
    partition: &GrainPartition,
    group_base: usize,
    affected: &[bool],
    context: KernelContext<'_>,
) {
    let opacity = context.settings.grain_fill_opacity;
    if matches!(context.settings.grain_fill_mode, GrainFillMode::Texture) || opacity <= 0.0 {
        return;
    }
    let mut median_scratch = Vec::new();
    for block_index in 0..affected.len() {
        let group = partition.group(group_base + block_index);
        if group.is_empty() || !affected.get(block_index).copied().unwrap_or(false) {
            continue;
        }
        let representative = grain_representative(
            output,
            group,
            partition_block(block_index, partition, context),
            context,
            context.settings.grain_fill_mode,
            &mut median_scratch,
        );
        for &index in group {
            let index = index as usize;
            output[index] = lerp_pixel(output[index], representative, opacity);
        }
    }
}

fn grain_representative(
    pixels: &[PixelF32],
    group: &[u32],
    block: Block,
    context: KernelContext<'_>,
    mode: GrainFillMode,
    median_scratch: &mut Vec<f32>,
) -> PixelF32 {
    match mode {
        GrainFillMode::Texture => pixels[group[0] as usize],
        GrainFillMode::Average => {
            let mut sum = transparent_pixel();
            for &index in group {
                let pixel = pixels[index as usize];
                sum.alpha += pixel.alpha;
                sum.red += pixel.red;
                sum.green += pixel.green;
                sum.blue += pixel.blue;
            }
            let inverse = (group.len() as f32).recip();
            PixelF32 {
                alpha: sum.alpha * inverse,
                red: sum.red * inverse,
                green: sum.green * inverse,
                blue: sum.blue * inverse,
            }
        }
        GrainFillMode::Median => {
            median_scratch.clear();
            median_scratch.extend(group.iter().map(|&index| pixels[index as usize].alpha));
            let alpha = median(median_scratch);
            median_scratch.clear();
            median_scratch.extend(group.iter().map(|&index| pixels[index as usize].red));
            let red = median(median_scratch);
            median_scratch.clear();
            median_scratch.extend(group.iter().map(|&index| pixels[index as usize].green));
            let green = median(median_scratch);
            median_scratch.clear();
            median_scratch.extend(group.iter().map(|&index| pixels[index as usize].blue));
            let blue = median(median_scratch);
            PixelF32 {
                alpha,
                red,
                green,
                blue,
            }
        }
        GrainFillMode::Center => {
            let (center_x, center_y) = kernel_center(block, context);
            let center_index = group
                .iter()
                .copied()
                .min_by(|a, b| {
                    let distance = |index: u32| {
                        let index = index as usize;
                        let x = index % context.image_width;
                        let y = index / context.image_width;
                        let dx = x as f32 + 0.5 - center_x;
                        let dy = y as f32 + 0.5 - center_y;
                        dx * dx + dy * dy
                    };
                    distance(*a).total_cmp(&distance(*b))
                })
                .unwrap_or(group[0]);
            pixels[center_index as usize]
        }
    }
}

fn median(values: &mut [f32]) -> f32 {
    debug_assert!(!values.is_empty());
    let middle = values.len() / 2;
    let upper = *values.select_nth_unstable_by(middle, f32::total_cmp).1;
    if values.len().is_multiple_of(2) {
        let lower = values[..middle]
            .iter()
            .copied()
            .max_by(f32::total_cmp)
            .unwrap_or(upper);
        (lower + upper) * 0.5
    } else {
        upper
    }
}

fn anisotropy_at(
    map: Option<&LayerBuffer>,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
    settings: &RenderSettings,
) -> (f32, f32) {
    let mut direction = settings.direction;
    let mut anisotropy = settings.anisotropy;
    if !settings.use_anisotropy_map {
        return (direction, anisotropy);
    }
    let Some(map) = map else {
        return (direction, anisotropy);
    };
    let (dir_x, dir_y, strength) = match settings.anisotropy_map_mode {
        AnisotropyMapMode::HueSaturation => {
            let px = mapped_pixel(map, x, y, out_width, out_height);
            let (hue, saturation) = hue_saturation(px);
            (hue.cos(), hue.sin(), saturation)
        }
        AnisotropyMapMode::Uv => {
            let px = mapped_pixel(map, x, y, out_width, out_height);
            let dir_x = sanitize_non_finite(px.red) * 2.0 - 1.0;
            let dir_y = sanitize_non_finite(px.green) * 2.0 - 1.0;
            let strength = (dir_x * dir_x + dir_y * dir_y).sqrt().clamp(0.0, 1.0);
            (dir_x, dir_y, strength)
        }
        AnisotropyMapMode::Normal => {
            let px = mapped_pixel(map, x, y, out_width, out_height);
            let dir_x = sanitize_non_finite(px.red) * 2.0 - 1.0;
            let dir_y = 1.0 - sanitize_non_finite(px.green) * 2.0;
            let strength = (dir_x * dir_x + dir_y * dir_y).sqrt().clamp(0.0, 1.0);
            (dir_x, dir_y, strength)
        }
        AnisotropyMapMode::DivergenceDirection | AnisotropyMapMode::DivergenceRotation => {
            let Some(flow) = divergence_flow_at(
                map,
                x,
                y,
                out_width,
                out_height,
                settings.anisotropy_divergence_source,
                matches!(
                    settings.anisotropy_map_mode,
                    AnisotropyMapMode::DivergenceRotation
                ),
            ) else {
                return (direction, anisotropy);
            };
            flow
        }
    };
    if settings.use_anisotropy_direction && strength > 1.0e-6 && dir_x.abs() + dir_y.abs() > 1.0e-6
    {
        direction = dir_y.atan2(dir_x);
    }
    if settings.use_anisotropy_strength {
        anisotropy *= strength;
    }
    (direction, anisotropy.clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments)]
fn divergence_flow_at(
    map: &LayerBuffer,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
    source: DivergenceSource,
    rotate: bool,
) -> Option<(f32, f32, f32)> {
    if map.width == 0 || map.height == 0 || out_width == 0 || out_height == 0 {
        return None;
    }
    let left = divergence_value_at(map, x.saturating_sub(1), y, out_width, out_height, source);
    let right = divergence_value_at(
        map,
        x.saturating_add(1).min(out_width - 1),
        y,
        out_width,
        out_height,
        source,
    );
    let up = divergence_value_at(map, x, y.saturating_sub(1), out_width, out_height, source);
    let down = divergence_value_at(
        map,
        x,
        y.saturating_add(1).min(out_height - 1),
        out_width,
        out_height,
        source,
    );
    let gradient_x = right - left;
    let gradient_y = down - up;
    let length_squared = gradient_x * gradient_x + gradient_y * gradient_y;
    if !length_squared.is_finite() || length_squared <= 1.0e-12 {
        return None;
    }
    let length = length_squared.sqrt();
    let (direction_x, direction_y) = if rotate {
        (-gradient_y, gradient_x)
    } else {
        (gradient_x, gradient_y)
    };
    Some((
        direction_x / length,
        direction_y / length,
        (length * 4.0).clamp(0.0, 1.0),
    ))
}

fn divergence_value_at(
    map: &LayerBuffer,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
    source: DivergenceSource,
) -> f32 {
    let px = mapped_pixel(map, x, y, out_width, out_height);
    match source {
        DivergenceSource::Gray => scalar_from_pixel(px, MapChannel::Luma),
        DivergenceSource::Red => sanitize_non_finite(px.red),
        DivergenceSource::Green => sanitize_non_finite(px.green),
        DivergenceSource::Blue => sanitize_non_finite(px.blue),
        DivergenceSource::Alpha => sanitize_non_finite(px.alpha),
        DivergenceSource::HsvValue => {
            let red = sanitize_non_finite(px.red);
            let green = sanitize_non_finite(px.green);
            let blue = sanitize_non_finite(px.blue);
            red.max(green).max(blue)
        }
        DivergenceSource::HslLightness => {
            let red = sanitize_non_finite(px.red);
            let green = sanitize_non_finite(px.green);
            let blue = sanitize_non_finite(px.blue);
            (red.max(green).max(blue) + red.min(green).min(blue)) * 0.5
        }
    }
}

fn hue_saturation(px: PixelF32) -> (f32, f32) {
    let red = sanitize_non_finite(px.red);
    let green = sanitize_non_finite(px.green);
    let blue = sanitize_non_finite(px.blue);
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let saturation = if max.abs() <= 1.0e-6 {
        0.0
    } else {
        (delta / max.abs()).clamp(0.0, 1.0)
    };
    if delta.abs() <= 1.0e-6 {
        return (0.0, saturation);
    }
    let hue_turns = if max == red {
        ((green - blue) / delta).rem_euclid(6.0) / 6.0
    } else if max == green {
        ((blue - red) / delta + 2.0) / 6.0
    } else {
        ((red - green) / delta + 4.0) / 6.0
    };
    (hue_turns * std::f32::consts::TAU, saturation)
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn discrete_anisotropic_offset(
    cell_x: i32,
    cell_y: i32,
    tap: u32,
    attempt: u32,
    radius: i32,
    direction: f32,
    anisotropy: f32,
    seed: u32,
) -> (i32, i32) {
    discrete_anisotropic_offset_with_transform(
        cell_x,
        cell_y,
        tap,
        attempt,
        radius,
        anisotropy_transform(direction, anisotropy),
        seed,
    )
}

fn anisotropy_transform(direction: f32, anisotropy: f32) -> AnisotropyTransform {
    AnisotropyTransform {
        cosine: direction.cos(),
        sine: direction.sin(),
        perpendicular_scale: 1.0 - anisotropy.clamp(0.0, 1.0),
    }
}

#[allow(clippy::too_many_arguments)]
fn discrete_anisotropic_offset_with_transform(
    cell_x: i32,
    cell_y: i32,
    tap: u32,
    attempt: u32,
    radius: i32,
    transform: AnisotropyTransform,
    seed: u32,
) -> (i32, i32) {
    let (dx, dy) = discrete_disk_offset(cell_x, cell_y, tap, attempt, radius, seed);
    let parallel = dx as f32 * transform.cosine + dy as f32 * transform.sine;
    let perpendicular = (-dx as f32 * transform.sine + dy as f32 * transform.cosine)
        * transform.perpendicular_scale;
    let target_x = parallel * transform.cosine - perpendicular * transform.sine;
    let target_y = parallel * transform.sine + perpendicular * transform.cosine;
    let rounded_x = target_x.round() as i32;
    let rounded_y = target_y.round() as i32;
    let radius_squared = radius as i64 * radius as i64;
    if rounded_x as i64 * rounded_x as i64 + rounded_y as i64 * rounded_y as i64 <= radius_squared {
        return (rounded_x, rounded_y);
    }

    let x_candidates = [target_x.floor() as i32, target_x.ceil() as i32];
    let y_candidates = [target_y.floor() as i32, target_y.ceil() as i32];
    let mut best = (0, 0);
    let mut best_error = f32::INFINITY;
    for candidate_x in x_candidates {
        for candidate_y in y_candidates {
            let distance_squared =
                candidate_x as i64 * candidate_x as i64 + candidate_y as i64 * candidate_y as i64;
            if distance_squared > radius_squared {
                continue;
            }
            let error_x = candidate_x as f32 - target_x;
            let error_y = candidate_y as f32 - target_y;
            let error = error_x * error_x + error_y * error_y;
            if error < best_error {
                best = (candidate_x, candidate_y);
                best_error = error;
            }
        }
    }
    best
}

fn discrete_disk_offset(
    cell_x: i32,
    cell_y: i32,
    tap: u32,
    attempt: u32,
    radius: i32,
    seed: u32,
) -> (i32, i32) {
    if radius <= 0 {
        return (0, 0);
    }
    let span = (radius * 2 + 1) as u32;
    for rejection in 0..MAX_DISK_REJECTIONS {
        let channel = attempt
            .wrapping_mul(MAX_DISK_REJECTIONS)
            .wrapping_add(rejection)
            .wrapping_mul(2);
        let x_hash = hash_coords(seed, cell_x, cell_y, tap as i32, channel);
        let y_hash = hash_coords(seed, cell_x, cell_y, tap as i32, channel + 1);
        let dx = (x_hash % span) as i32 - radius;
        let dy = (y_hash % span) as i32 - radius;
        let distance_squared = dx * dx + dy * dy;
        if distance_squared > 0 && distance_squared <= radius * radius {
            return (dx, dy);
        }
    }

    match hash_coords(seed ^ 0x7F4A_7C15, cell_x, cell_y, tap as i32, attempt) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    }
}

fn shuffle_values<T>(values: &mut [T], seed: u32) {
    let mut state = hash_u32(seed ^ values.len() as u32);
    for i in (1..values.len()).rev() {
        state = hash_u32(state.wrapping_add(i as u32).wrapping_add(0x9E37_79B9));
        let j = state as usize % (i + 1);
        values.swap(i, j);
    }
}

fn map_value(
    map: Option<&LayerBuffer>,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
    channel: MapChannel,
    default_value: f32,
) -> f32 {
    let Some(map) = map else {
        return default_value;
    };
    if map.width == 0 || map.height == 0 {
        return default_value;
    }
    scalar_from_pixel(mapped_pixel(map, x, y, out_width, out_height), channel).clamp(0.0, 1.0)
}

fn mapped_pixel(
    map: &LayerBuffer,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
) -> PixelF32 {
    if map.width == 0 || map.height == 0 {
        return transparent_pixel();
    }
    let map_x = remap_coord_float(x, out_width, map.width);
    let map_y = remap_coord_float(y, out_height, map.height);
    sample_map_bilinear(map, map_x, map_y)
}

fn remap_coord_float(coord: usize, out_len: usize, source_len: usize) -> f32 {
    if out_len == 0 || source_len == 0 {
        return 0.0;
    }
    (coord as f32 + 0.5) * source_len as f32 / out_len as f32 - 0.5
}

fn sample_map_bilinear(map: &LayerBuffer, x: f32, y: f32) -> PixelF32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let sample = |sample_x: i32, sample_y: i32| {
        let sample_x = sample_x.clamp(0, map.width as i32 - 1) as usize;
        let sample_y = sample_y.clamp(0, map.height as i32 - 1) as usize;
        map.pixels[sample_y * map.width + sample_x]
    };
    let top = lerp_pixel(sample(x0, y0), sample(x1, y0), tx);
    let bottom = lerp_pixel(sample(x0, y1), sample(x1, y1), tx);
    lerp_pixel(top, bottom, ty)
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: lerp(a.alpha, b.alpha, t),
        red: lerp(a.red, b.red, t),
        green: lerp(a.green, b.green, t),
        blue: lerp(a.blue, b.blue, t),
    }
}

fn scalar_from_pixel(px: PixelF32, channel: MapChannel) -> f32 {
    match channel {
        MapChannel::Luma => {
            0.2126 * sanitize_non_finite(px.red)
                + 0.7152 * sanitize_non_finite(px.green)
                + 0.0722 * sanitize_non_finite(px.blue)
        }
        MapChannel::Red => sanitize_non_finite(px.red),
        MapChannel::Green => sanitize_non_finite(px.green),
        MapChannel::Blue => sanitize_non_finite(px.blue),
        MapChannel::Alpha => sanitize_non_finite(px.alpha),
    }
}

fn in_bounds_coord(coord: i32, len: usize) -> Option<usize> {
    if coord >= 0 && coord < len as i32 {
        Some(coord as usize)
    } else {
        None
    }
}

fn resolve_coord(coord: i32, len: usize, edge_mode: EdgeMode) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let len = len as i32;
    match edge_mode {
        EdgeMode::Reject | EdgeMode::Transparent => in_bounds_coord(coord, len as usize),
        EdgeMode::Clamp => Some(coord.clamp(0, len - 1) as usize),
        EdgeMode::Tile => Some(coord.rem_euclid(len) as usize),
        EdgeMode::Mirror => Some(mirror_index(coord, len) as usize),
    }
}

fn mirror_index(coord: i32, len: i32) -> i32 {
    if len <= 1 {
        return 0;
    }
    let period = 2 * len - 2;
    let value = coord.rem_euclid(period);
    if value < len { value } else { period - value }
}

fn temporal_seed(seed: u32, mode: TemporalMode, frame: i32) -> u32 {
    match mode {
        TemporalMode::Static => seed,
        TemporalMode::Frame => hash_u32(seed ^ (frame as u32).wrapping_mul(0x9E37_79B9)),
    }
}

fn hash_coords(seed: u32, x: i32, y: i32, z: i32, channel: u32) -> u32 {
    let mut hash = seed ^ 0xA511_E9B3;
    hash = hash.wrapping_add((x as u32).wrapping_mul(0x85EB_CA6B));
    hash = hash.wrapping_add((y as u32).wrapping_mul(0xC2B2_AE35));
    hash = hash.wrapping_add((z as u32).wrapping_mul(0x27D4_EB2D));
    hash = hash.wrapping_add(channel.wrapping_mul(0x1656_67B1));
    hash_u32(hash)
}

fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    value
}

fn rand01(value: u32) -> f32 {
    value as f32 / u32::MAX as f32
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
    let red = blend_channel(original.red, scatter.red, mode);
    let green = blend_channel(original.green, scatter.green, mode);
    let blue = blend_channel(original.blue, scatter.blue, mode);
    PixelF32 {
        alpha: lerp(original.alpha, scatter.alpha, opacity),
        red: lerp(original.red, red, opacity),
        green: lerp(original.green, green, opacity),
        blue: lerp(original.blue, blue, opacity),
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

fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn sanitize_pixel_for_output(mut px: PixelF32, out_is_f32: bool, clamp_32: bool) -> PixelF32 {
    px.alpha = sanitize_non_finite(px.alpha);
    px.red = sanitize_non_finite(px.red);
    px.green = sanitize_non_finite(px.green);
    px.blue = sanitize_non_finite(px.blue);
    if !out_is_f32 || clamp_32 {
        px.alpha = px.alpha.clamp(0.0, 1.0);
        px.red = px.red.clamp(0.0, 1.0);
        px.green = px.green.clamp(0.0, 1.0);
        px.blue = px.blue.clamp(0.0, 1.0);
    }
    px
}

fn sanitize_non_finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(value: f32) -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red: value,
            green: value * 0.5,
            blue: 1.0 - value,
        }
    }

    fn pixel_bits(pixels: &[PixelF32]) -> Vec<[u32; 4]> {
        pixels
            .iter()
            .map(|pixel| {
                [
                    pixel.alpha.to_bits(),
                    pixel.red.to_bits(),
                    pixel.green.to_bits(),
                    pixel.blue.to_bits(),
                ]
            })
            .collect()
    }

    fn source(width: usize, height: usize) -> LayerBuffer {
        let count = width * height;
        LayerBuffer {
            width,
            height,
            pixels: (0..count)
                .map(|index| pixel((index + 1) as f32 / (count + 1) as f32))
                .collect(),
        }
    }

    fn settings(mode: ScatterMode) -> RenderSettings {
        RenderSettings {
            scatter_mode: mode,
            amount: 1.0,
            radius: 4,
            grain_size: 1,
            grain_size_min: 1,
            gather_samples: 1,
            direction: 0.0,
            anisotropy: 0.0,
            grain_shape: GrainShape::Square,
            grain_size_randomness: 0.0,
            grain_position_randomness: 0.0,
            grain_density: 1.0,
            kernel_randomness: 0.0,
            kernel_texture_channel: MapChannel::Alpha,
            kernel_threshold: 0.5,
            grain_fill_mode: GrainFillMode::Texture,
            grain_fill_opacity: 1.0,
            use_amount_map: false,
            amount_map_channel: MapChannel::Luma,
            use_radius_map: false,
            radius_map_channel: MapChannel::Luma,
            use_grain_size_map: false,
            grain_size_map_channel: MapChannel::Luma,
            use_anisotropy_map: false,
            anisotropy_map_mode: AnisotropyMapMode::HueSaturation,
            use_anisotropy_direction: true,
            use_anisotropy_strength: true,
            anisotropy_divergence_source: DivergenceSource::Gray,
            seed: 42,
            temporal_mode: TemporalMode::Static,
            edge_mode: EdgeMode::Reject,
            blend_mode: OutputBlendMode::None,
            blend_opacity: 1.0,
            preserve_alpha: false,
            clamp_32: false,
        }
    }

    fn no_maps() -> RenderMaps<'static> {
        RenderMaps {
            amount: None,
            radius: None,
            grain_size: None,
            anisotropy: None,
            kernel_texture: None,
        }
    }

    fn test_kernel_context<'a>(
        width: usize,
        height: usize,
        maps: RenderMaps<'a>,
        settings: &'a RenderSettings,
        seed: u32,
    ) -> KernelContext<'a> {
        KernelContext {
            image_width: width,
            image_height: height,
            maps,
            settings,
            seed,
            density_layer: 0,
        }
    }

    #[test]
    fn gather_single_sample_only_copies_existing_pixels() {
        let source = source(8, 8);
        let rendered = render_gather(&source, no_maps(), &settings(ScatterMode::Gather), 42);
        for output in rendered {
            assert!(source.pixels.iter().any(|input| {
                input.alpha.to_bits() == output.alpha.to_bits()
                    && input.red.to_bits() == output.red.to_bits()
                    && input.green.to_bits() == output.green.to_bits()
                    && input.blue.to_bits() == output.blue.to_bits()
            }));
        }
    }

    #[test]
    fn gather_multiple_samples_can_create_mixed_pixels() {
        let source = source(8, 8);
        let mut settings = settings(ScatterMode::Gather);
        settings.gather_samples = 4;
        let rendered = render_gather(&source, no_maps(), &settings, 42);
        assert!(rendered.iter().any(|output| {
            !source
                .pixels
                .iter()
                .any(|input| input.red.to_bits() == output.red.to_bits())
        }));
    }

    #[test]
    fn precomputed_gather_offsets_match_attempt_by_attempt_sampling() {
        let source = source(8, 8);
        let radius = 5;
        let direction = 0.73;
        let anisotropy = 0.62;
        let transform = anisotropy_transform(direction, anisotropy);
        for edge_mode in [
            EdgeMode::Reject,
            EdgeMode::Clamp,
            EdgeMode::Tile,
            EdgeMode::Mirror,
            EdgeMode::Transparent,
        ] {
            for &(x, y) in &[(0, 0), (7, 0), (3, 4), (0, 7), (7, 7)] {
                for tap in 0..8 {
                    let primary = discrete_anisotropic_offset(
                        2, 3, tap, 0, radius, direction, anisotropy, 91,
                    );
                    let actual = gather_sample(
                        &source, x, y, 2, 3, radius, transform, tap, primary, 91, edge_mode,
                    );
                    let expected = (0..MAX_GATHER_ATTEMPTS)
                        .find_map(|attempt| {
                            let (dx, dy) = discrete_anisotropic_offset(
                                2, 3, tap, attempt, radius, direction, anisotropy, 91,
                            );
                            let sample_x = x as i32 + dx;
                            let sample_y = y as i32 + dy;
                            if matches!(edge_mode, EdgeMode::Reject) {
                                let sample_x = in_bounds_coord(sample_x, source.width)?;
                                let sample_y = in_bounds_coord(sample_y, source.height)?;
                                return Some(source.pixels[sample_y * source.width + sample_x]);
                            }
                            let sample_x = resolve_coord(sample_x, source.width, edge_mode);
                            let sample_y = resolve_coord(sample_y, source.height, edge_mode);
                            Some(match (sample_x, sample_y) {
                                (Some(sample_x), Some(sample_y)) => {
                                    source.pixels[sample_y * source.width + sample_x]
                                }
                                _ => transparent_pixel(),
                            })
                        })
                        .unwrap_or(source.pixels[y * source.width + x]);
                    assert_eq!(pixel_bits(&[actual]), pixel_bits(&[expected]));
                }
            }
        }
    }

    #[test]
    fn zero_amount_is_identity() {
        let source = source(8, 8);
        let mut settings = settings(ScatterMode::Gather);
        settings.amount = 0.0;
        let rendered = render_gather(&source, no_maps(), &settings, 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn black_amount_map_is_identity() {
        let source = source(8, 8);
        let black_map = LayerBuffer {
            width: 8,
            height: 8,
            pixels: vec![transparent_pixel(); 64],
        };
        let maps = RenderMaps {
            amount: Some(&black_map),
            ..no_maps()
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 2;
        settings.grain_density = 4.0;
        let rendered = render_gather(&source, maps, &settings, 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn swap_is_a_strict_permutation() {
        let source = source(16, 16);
        let settings = settings(ScatterMode::Swap);
        let rendered = render_swap(&source, no_maps(), &settings, 42);
        let mut input_values: Vec<u32> = source.pixels.iter().map(|px| px.red.to_bits()).collect();
        let mut output_values: Vec<u32> = rendered.iter().map(|px| px.red.to_bits()).collect();
        input_values.sort_unstable();
        output_values.sort_unstable();
        assert_eq!(input_values, output_values);
        assert!(
            source
                .pixels
                .iter()
                .zip(rendered.iter())
                .any(|(input, output)| input.red.to_bits() != output.red.to_bits())
        );
        let permutation =
            build_swap_permutation(source.width, source.height, no_maps(), &settings, 42);
        for (destination, source_index) in permutation.into_iter().enumerate() {
            let source_index = source_index as usize;
            let destination_x = destination % source.width;
            let destination_y = destination / source.width;
            let source_x = source_index % source.width;
            let source_y = source_index / source.width;
            let dx = destination_x as i32 - source_x as i32;
            let dy = destination_y as i32 - source_y as i32;
            assert!(dx * dx + dy * dy <= settings.radius.pow(2));
        }
    }

    #[test]
    fn swap_respects_radius() {
        let source = source(16, 16);
        let settings = settings(ScatterMode::Swap);
        let permutation =
            build_swap_permutation(source.width, source.height, no_maps(), &settings, 42);
        for (destination, source_index) in permutation.into_iter().enumerate() {
            let source_index = source_index as usize;
            let destination_x = destination % source.width;
            let destination_y = destination / source.width;
            let source_x = source_index % source.width;
            let source_y = source_index / source.width;
            let dx = destination_x as i32 - source_x as i32;
            let dy = destination_y as i32 - source_y as i32;
            assert!(dx * dx + dy * dy <= settings.radius * settings.radius);
        }
    }

    #[test]
    fn swap_with_black_amount_map_is_identity() {
        let source = source(8, 8);
        let black_map = LayerBuffer {
            width: 8,
            height: 8,
            pixels: vec![transparent_pixel(); 64],
        };
        let maps = RenderMaps {
            amount: Some(&black_map),
            ..no_maps()
        };
        let mut settings = settings(ScatterMode::Swap);
        settings.grain_size = 2;
        settings.grain_density = 4.0;
        let rendered = render_swap(&source, maps, &settings, 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn circle_kernel_keeps_center_and_masks_corners() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 5;
        settings.grain_shape = GrainShape::Circle;
        let block = block_at(0, 0, 5, 5, 5);
        let context = test_kernel_context(5, 5, no_maps(), &settings, 42);
        assert!(kernel_contains(block, 2, 2, context));
        assert!(!kernel_contains(block, 0, 0, context));
    }

    #[test]
    fn texture_kernel_uses_selected_channel_and_threshold() {
        let texture = LayerBuffer {
            width: 2,
            height: 1,
            pixels: vec![
                PixelF32 {
                    alpha: 1.0,
                    ..transparent_pixel()
                },
                transparent_pixel(),
            ],
        };
        let maps = RenderMaps {
            kernel_texture: Some(&texture),
            ..no_maps()
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 2;
        settings.grain_shape = GrainShape::Texture;
        settings.kernel_texture_channel = MapChannel::Alpha;
        settings.kernel_threshold = 0.5;
        let block = block_at(0, 0, 2, 2, 1);
        let context = test_kernel_context(2, 1, maps, &settings, 42);
        assert!(kernel_contains(block, 0, 0, context));
        assert!(!kernel_contains(block, 1, 0, context));
    }

    #[test]
    fn black_grain_size_map_reduces_kernel_coverage() {
        let black = LayerBuffer {
            width: 8,
            height: 8,
            pixels: vec![transparent_pixel(); 64],
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        settings.use_grain_size_map = true;
        let block = block_at(0, 0, 8, 8, 8);
        let full_count = (0..8)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                kernel_contains(
                    block,
                    x,
                    y,
                    test_kernel_context(8, 8, no_maps(), &settings, 42),
                )
            })
            .count();
        let mapped = RenderMaps {
            grain_size: Some(&black),
            ..no_maps()
        };
        let mapped_count = (0..8)
            .flat_map(|y| (0..8).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                kernel_contains(
                    block,
                    x,
                    y,
                    test_kernel_context(8, 8, mapped, &settings, 42),
                )
            })
            .count();
        assert_eq!(full_count, 64);
        assert!(mapped_count < full_count);
        assert!(mapped_count > 0);
    }

    #[test]
    fn grain_size_randomness_is_deterministic_and_shrinks() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 16;
        settings.grain_size_min = 6;
        settings.grain_size_randomness = 1.0;
        let block = block_at(0, 0, 16, 16, 16);
        let context = test_kernel_context(16, 16, no_maps(), &settings, 91);
        let first = kernel_scale(block, context);
        let second = kernel_scale(block, context);
        assert_eq!(first.to_bits(), second.to_bits());
        assert!((6.0 / 16.0..=1.0).contains(&first));
        assert!(first < 1.0);
    }

    #[test]
    fn grain_size_range_uses_max_when_randomness_is_zero() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 16;
        settings.grain_size_min = 6;
        settings.grain_size_randomness = 0.0;
        let block = block_at(0, 0, 16, 16, 16);
        let context = test_kernel_context(16, 16, no_maps(), &settings, 91);
        assert_eq!(kernel_scale(block, context), 1.0);
    }

    #[test]
    fn grain_size_map_interpolates_between_min_and_max() {
        let map_with_value = |value| LayerBuffer {
            width: 1,
            height: 1,
            pixels: vec![PixelF32 {
                alpha: 1.0,
                red: value,
                green: value,
                blue: value,
            }],
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 16;
        settings.grain_size_min = 4;
        settings.use_grain_size_map = true;
        let block = block_at(0, 0, 16, 16, 16);

        let black = map_with_value(0.0);
        let black_maps = RenderMaps {
            grain_size: Some(&black),
            ..no_maps()
        };
        let black_scale = kernel_scale(
            block,
            test_kernel_context(16, 16, black_maps, &settings, 91),
        );
        assert_eq!(black_scale, 4.0 / 16.0);

        let gray = map_with_value(0.5);
        let gray_maps = RenderMaps {
            grain_size: Some(&gray),
            ..no_maps()
        };
        let gray_scale = kernel_scale(block, test_kernel_context(16, 16, gray_maps, &settings, 91));
        assert_eq!(gray_scale, 10.0 / 16.0);

        let white = map_with_value(1.0);
        let white_maps = RenderMaps {
            grain_size: Some(&white),
            ..no_maps()
        };
        assert_eq!(
            kernel_scale(
                block,
                test_kernel_context(16, 16, white_maps, &settings, 91),
            ),
            1.0
        );
    }

    #[test]
    fn grain_size_range_clamps_invalid_min_and_combined_randomness() {
        let block = block_at(0, 0, 16, 16, 16);
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 16;
        settings.grain_size_min = 24;
        settings.grain_size_randomness = 1.0;
        assert_eq!(
            kernel_scale(block, test_kernel_context(16, 16, no_maps(), &settings, 91),),
            1.0
        );

        let gray = LayerBuffer {
            width: 1,
            height: 1,
            pixels: vec![PixelF32 {
                alpha: 1.0,
                red: 0.5,
                green: 0.5,
                blue: 0.5,
            }],
        };
        let maps = RenderMaps {
            grain_size: Some(&gray),
            ..no_maps()
        };
        settings.grain_size_min = 4;
        for seed in 0..32 {
            let scale = kernel_scale(block, test_kernel_context(16, 16, maps, &settings, seed));
            assert!((4.0 / 16.0..=10.0 / 16.0).contains(&scale));
        }
    }

    #[test]
    fn zero_position_randomness_keeps_the_grid_center() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        let block = block_at(1, 1, 8, 24, 24);
        let context = test_kernel_context(24, 24, no_maps(), &settings, 91);
        assert_eq!(kernel_center(block, context), (12.0, 12.0));
    }

    #[test]
    fn grain_density_controls_layer_count_and_fraction() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        assert_eq!(density_layers(&settings), vec![(0, 1.0)]);

        settings.grain_density = 4.0;
        assert_eq!(
            density_layers(&settings),
            vec![(0, 1.0), (1, 1.0), (2, 1.0), (3, 1.0)]
        );

        settings.grain_density = 4.5;
        assert_eq!(density_layers(&settings).last(), Some(&(4, 0.5)));

        settings.grain_density = 8.0;
        settings.grain_size = 1;
        assert_eq!(density_layers(&settings), vec![(0, 1.0)]);
        settings.grain_size = 2;
        assert_eq!(density_layers(&settings).len(), 4);
    }

    #[test]
    fn density_100_matches_the_original_single_layer_paths() {
        let source = source(16, 16);
        let mut gather_settings = settings(ScatterMode::Gather);
        gather_settings.grain_size = 4;
        gather_settings.grain_shape = GrainShape::Circle;
        gather_settings.grain_position_randomness = 0.7;
        gather_settings.grain_fill_mode = GrainFillMode::Average;
        gather_settings.grain_fill_opacity = 0.6;
        let actual = render_gather(&source, no_maps(), &gather_settings, 91);
        let context = test_kernel_context(16, 16, no_maps(), &gather_settings, 91);
        let mut expected = source.pixels.clone();
        render_gather_layer(&mut expected, &source, context, 1.0);
        assert_eq!(pixel_bits(&actual), pixel_bits(&expected));

        let mut swap_settings = settings(ScatterMode::Swap);
        swap_settings.grain_size = 4;
        swap_settings.grain_position_randomness = 0.7;
        swap_settings.radius = 12;
        let actual = build_swap_permutation(16, 16, no_maps(), &swap_settings, 91);
        let context = test_kernel_context(16, 16, no_maps(), &swap_settings, 91);
        let partition = build_grain_partition(16, 16, context);
        let mut expected = (0..16 * 16).collect::<Vec<_>>();
        build_swap_permutation_for_partition(
            &mut expected,
            16,
            16,
            no_maps(),
            &swap_settings,
            91,
            &partition,
            0,
            context,
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn higher_grain_density_adds_centers_and_reduces_gaps() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        settings.grain_shape = GrainShape::Circle;
        settings.grain_position_randomness = 1.0;

        let coverage = |layer_count: u32| {
            let mut covered = vec![false; 64 * 64];
            for density_layer in 0..layer_count {
                let context = KernelContext {
                    image_width: 64,
                    image_height: 64,
                    maps: no_maps(),
                    settings: &settings,
                    seed: density_layer_seed(42, density_layer),
                    density_layer,
                };
                let partition = build_grain_partition(64, 64, context);
                for index in partition.groups().flatten() {
                    covered[*index as usize] = true;
                }
            }
            covered.into_iter().filter(|covered| *covered).count()
        };
        let sparse_coverage = coverage(1);
        let dense_coverage = coverage(8);

        assert!(dense_coverage > sparse_coverage);
        assert!(dense_coverage as f32 / 4096.0 > 0.99);
    }

    #[test]
    fn four_regular_density_layers_cover_circle_grid_gaps() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        settings.grain_shape = GrainShape::Circle;
        let mut covered = vec![false; 64 * 64];
        for density_layer in 0..4 {
            let context = KernelContext {
                image_width: 64,
                image_height: 64,
                maps: no_maps(),
                settings: &settings,
                seed: density_layer_seed(42, density_layer),
                density_layer,
            };
            let partition = build_grain_partition(64, 64, context);
            for index in partition.groups().flatten() {
                covered[*index as usize] = true;
            }
        }
        let coverage = covered.into_iter().filter(|covered| *covered).count();
        assert!(coverage as f32 / 4096.0 > 0.99);
    }

    #[test]
    fn density_layers_keep_the_original_kernel_footprint() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        let block = block_at(1, 1, 8, 32, 32);
        let coverage = |density_layer| {
            let context = KernelContext {
                image_width: 32,
                image_height: 32,
                maps: no_maps(),
                settings: &settings,
                seed: density_layer_seed(42, density_layer),
                density_layer,
            };
            (0..32)
                .flat_map(|y| (0..32).map(move |x| (x, y)))
                .filter(|&(x, y)| kernel_contains(block, x, y, context))
                .count()
        };
        assert_eq!(coverage(0), 64);
        assert_eq!(coverage(1), 64);
    }

    #[test]
    fn position_randomness_is_seeded_and_changes_ownership() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 4;
        settings.grain_position_randomness = 1.0;
        let ownership = |seed| {
            let context = test_kernel_context(16, 16, no_maps(), &settings, seed);
            let partition = build_grain_partition(16, 16, context);
            let mut owners = vec![None; 16 * 16];
            for (owner, group) in partition.groups().enumerate() {
                for &index in group {
                    owners[index as usize] = Some(owner);
                }
            }
            owners
        };
        assert_eq!(ownership(42), ownership(42));
        assert_ne!(ownership(42), ownership(43));
    }

    #[test]
    fn overlapping_kernels_keep_one_random_front_grain() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 4;
        settings.grain_position_randomness = 1.0;

        let mut verified = false;
        for seed in 0..512 {
            let context = test_kernel_context(8, 4, no_maps(), &settings, seed);
            let left = prepare_kernel(block_at(0, 0, 4, 8, 4), context);
            let right = prepare_kernel(block_at(1, 0, 4, 8, 4), context);
            let overlap = (0..32)
                .filter(|&index| {
                    let x = index % 8;
                    let y = index / 8;
                    kernel_contains_prepared(left, x, y, context)
                        && kernel_contains_prepared(right, x, y, context)
                })
                .collect::<Vec<_>>();
            if overlap.len() < 2 {
                continue;
            }

            let partition = build_grain_partition(8, 4, context);
            let expected_owner = usize::from(right.front_priority > left.front_priority);
            let other_owner = 1 - expected_owner;
            for index in overlap {
                assert!(partition.group(expected_owner).contains(&(index as u32)));
                assert!(!partition.group(other_owner).contains(&(index as u32)));
            }
            verified = true;
            break;
        }
        assert!(verified, "test setup did not produce a multi-pixel overlap");
    }

    #[test]
    fn prepared_partition_preserves_the_kernel_union() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 7;
        settings.grain_shape = GrainShape::Circle;
        settings.grain_position_randomness = 1.0;
        settings.grain_size_randomness = 0.6;
        settings.kernel_randomness = 1.0;
        let context = KernelContext {
            image_width: 31,
            image_height: 19,
            maps: no_maps(),
            settings: &settings,
            seed: density_layer_seed(73, 3),
            density_layer: 3,
        };
        let grain = settings.grain_size;
        let columns = context.image_width.div_ceil(grain);
        let rows = context.image_height.div_ceil(grain);
        let partition = build_grain_partition(context.image_width, context.image_height, context);
        let mut actual = vec![false; context.image_width * context.image_height];
        for index in partition.groups().flatten() {
            actual[*index as usize] = true;
        }

        let mut expected = vec![false; actual.len()];
        for y in 0..context.image_height {
            for x in 0..context.image_width {
                let grid_x = (x / grain).min(columns - 1);
                let grid_y = (y / grain).min(rows - 1);
                let min_x = grid_x.saturating_sub(1);
                let max_x = (grid_x + 1).min(columns - 1);
                let min_y = grid_y.saturating_sub(1);
                let max_y = (grid_y + 1).min(rows - 1);
                expected[y * context.image_width + x] = (min_y..=max_y).any(|candidate_y| {
                    (min_x..=max_x).any(|candidate_x| {
                        kernel_contains(
                            block_at(
                                candidate_x,
                                candidate_y,
                                grain,
                                context.image_width,
                                context.image_height,
                            ),
                            x,
                            y,
                            context,
                        )
                    })
                });
            }
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn dense_gather_samples_each_visible_pixel_at_most_once() {
        let source = source(64, 64);
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 8;
        settings.grain_density = 8.0;
        settings.grain_shape = GrainShape::Circle;
        settings.grain_position_randomness = 1.0;
        settings.gather_samples = 4;
        settings.radius = 16;

        GATHER_PIXEL_CALLS.with(|calls| calls.set(0));
        let first = render_gather(&source, no_maps(), &settings, 73);
        let calls = GATHER_PIXEL_CALLS.with(std::cell::Cell::get);
        let second = render_gather(&source, no_maps(), &settings, 73);

        assert!(calls > 0);
        assert!(calls <= source.width * source.height);
        assert_eq!(pixel_bits(&first), pixel_bits(&second));
    }

    #[test]
    fn randomized_grain_swap_remains_a_strict_permutation() {
        let source = source(24, 24);
        let mut settings = settings(ScatterMode::Swap);
        settings.grain_size = 4;
        settings.grain_position_randomness = 1.0;
        settings.radius = 16;
        let rendered = render_swap(&source, no_maps(), &settings, 42);
        let mut input_values: Vec<u32> = source.pixels.iter().map(|px| px.red.to_bits()).collect();
        let mut output_values: Vec<u32> = rendered.iter().map(|px| px.red.to_bits()).collect();
        input_values.sort_unstable();
        output_values.sort_unstable();
        assert_eq!(input_values, output_values);
        assert!(
            source
                .pixels
                .iter()
                .zip(&rendered)
                .any(|(input, output)| input.red.to_bits() != output.red.to_bits())
        );
    }

    #[test]
    fn dense_grain_swap_remains_a_radius_bounded_permutation() {
        let source = source(24, 24);
        let mut settings = settings(ScatterMode::Swap);
        settings.grain_size = 6;
        settings.grain_density = 4.0;
        settings.grain_position_randomness = 1.0;
        settings.radius = 16;
        let permutation =
            build_swap_permutation(source.width, source.height, no_maps(), &settings, 73);
        let mut sorted = permutation.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            (0..(source.width * source.height) as u32).collect::<Vec<_>>()
        );
        assert!(
            permutation
                .iter()
                .enumerate()
                .any(|(destination, &source_index)| destination != source_index as usize)
        );
        for (destination, source_index) in permutation.into_iter().enumerate() {
            let source_index = source_index as usize;
            let destination_x = destination % source.width;
            let destination_y = destination / source.width;
            let source_x = source_index % source.width;
            let source_y = source_index / source.width;
            let dx = destination_x as i32 - source_x as i32;
            let dy = destination_y as i32 - source_y as i32;
            assert!(dx * dx + dy * dy <= settings.radius.pow(2));
        }
    }

    #[test]
    fn grain_group_swap_is_all_or_none() {
        let mut permutation = (0..8_u32).collect::<Vec<_>>();
        let before = permutation.clone();

        assert!(!swap_grain_groups(&mut permutation, &[0, 1], &[2, 7], 4, 2,));
        assert_eq!(permutation, before);

        assert!(!swap_grain_groups(&mut permutation, &[0, 1], &[2], 4, 2,));
        assert_eq!(permutation, before);
    }

    #[test]
    fn radius_map_is_evaluated_once_at_the_grain_center() {
        let source = source(4, 4);
        let mut radius_pixels = vec![pixel(1.0); 16];
        radius_pixels[2 * 4 + 2] = transparent_pixel();
        let radius_map = LayerBuffer {
            width: 4,
            height: 4,
            pixels: radius_pixels,
        };
        let maps = RenderMaps {
            radius: Some(&radius_map),
            ..no_maps()
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 4;
        settings.radius = 4;
        let rendered = render_gather(&source, maps, &settings, 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn average_fill_flattens_each_affected_grain() {
        let mut output = source(4, 4).pixels;
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 4;
        settings.grain_fill_mode = GrainFillMode::Average;
        settings.grain_fill_opacity = 1.0;
        let context = test_kernel_context(4, 4, no_maps(), &settings, 42);
        let partition = build_grain_partition(4, 4, context);
        let expected = output.iter().map(|px| px.red).sum::<f32>() / output.len() as f32;
        apply_grain_fill(&mut output, &partition, 0, &[true], context);
        assert!(
            output
                .iter()
                .all(|pixel| (pixel.red - expected).abs() < 1.0e-6)
        );
    }

    #[test]
    fn median_and_center_fill_choose_the_requested_representative() {
        let block = block_at(0, 0, 2, 2, 2);
        let group = [0, 1, 2, 3];
        let pixels = [pixel(0.0), pixel(0.0), pixel(0.0), pixel(1.0)];
        let settings = settings(ScatterMode::Gather);
        let context = test_kernel_context(2, 2, no_maps(), &settings, 42);
        let mut scratch = Vec::new();
        let median_pixel = grain_representative(
            &pixels,
            &group,
            block,
            context,
            GrainFillMode::Median,
            &mut scratch,
        );
        assert_eq!(median_pixel.red, 0.0);

        let center_block = block_at(0, 0, 3, 3, 3);
        let center_pixels = source(3, 3).pixels;
        let center_group = (0..9).collect::<Vec<_>>();
        let center_context = test_kernel_context(3, 3, no_maps(), &settings, 42);
        let center_pixel = grain_representative(
            &center_pixels,
            &center_group,
            center_block,
            center_context,
            GrainFillMode::Center,
            &mut scratch,
        );
        assert_eq!(center_pixel.red.to_bits(), center_pixels[4].red.to_bits());
    }

    #[test]
    fn fill_opacity_blends_with_the_grain_texture() {
        let original = source(2, 2).pixels;
        let mut output = original.clone();
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 2;
        settings.grain_fill_mode = GrainFillMode::Average;
        settings.grain_fill_opacity = 0.5;
        let context = test_kernel_context(2, 2, no_maps(), &settings, 42);
        let partition = build_grain_partition(2, 2, context);
        let average = original.iter().map(|px| px.red).sum::<f32>() / 4.0;
        apply_grain_fill(&mut output, &partition, 0, &[true], context);
        for (before, after) in original.iter().zip(output) {
            assert!((after.red - lerp(before.red, average, 0.5)).abs() < 1.0e-6);
        }
    }

    #[test]
    fn shape_randomness_is_seeded_and_changes_the_boundary() {
        let mut settings = settings(ScatterMode::Gather);
        settings.grain_size = 16;
        settings.grain_shape = GrainShape::Circle;
        settings.kernel_randomness = 1.0;
        let block = block_at(0, 0, 16, 16, 16);
        let mask = |seed| {
            let context = test_kernel_context(16, 16, no_maps(), &settings, seed);
            (0..16)
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .map(|(x, y)| kernel_contains(block, x, y, context))
                .collect::<Vec<_>>()
        };
        assert_eq!(mask(42), mask(42));
        assert_ne!(mask(42), mask(43));
    }

    #[test]
    fn anisotropy_map_decodes_hue_and_saturation() {
        let map = LayerBuffer {
            width: 1,
            height: 1,
            pixels: vec![PixelF32 {
                alpha: 1.0,
                red: 1.0,
                green: 0.0,
                blue: 0.0,
            }],
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.anisotropy = 1.0;
        settings.use_anisotropy_map = true;
        let (direction, strength) = anisotropy_at(Some(&map), 0, 0, 1, 1, &settings);
        assert!(direction.abs() < 1.0e-6);
        assert!((strength - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn divergence_direction_follows_the_scalar_gradient() {
        let map = LayerBuffer {
            width: 3,
            height: 3,
            pixels: (0..9)
                .map(|index| {
                    let value = (index % 3) as f32 * 0.05;
                    PixelF32 {
                        alpha: 1.0,
                        red: value,
                        green: 0.0,
                        blue: 0.0,
                    }
                })
                .collect(),
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.direction = 0.7;
        settings.anisotropy = 0.8;
        settings.use_anisotropy_map = true;
        settings.anisotropy_map_mode = AnisotropyMapMode::DivergenceDirection;
        settings.anisotropy_divergence_source = DivergenceSource::Red;
        let (direction, anisotropy) = anisotropy_at(Some(&map), 1, 1, 3, 3, &settings);
        assert!(direction.abs() < 1.0e-6);
        assert!((anisotropy - 0.32).abs() < 1.0e-6);
    }

    #[test]
    fn divergence_rotation_turns_the_gradient_ninety_degrees() {
        let map = LayerBuffer {
            width: 3,
            height: 3,
            pixels: (0..9)
                .map(|index| PixelF32 {
                    alpha: 1.0,
                    red: (index % 3) as f32 * 0.5,
                    green: 0.0,
                    blue: 0.0,
                })
                .collect(),
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.anisotropy = 1.0;
        settings.use_anisotropy_map = true;
        settings.anisotropy_map_mode = AnisotropyMapMode::DivergenceRotation;
        settings.anisotropy_divergence_source = DivergenceSource::Red;
        let (direction, anisotropy) = anisotropy_at(Some(&map), 1, 1, 3, 3, &settings);
        assert!((direction - std::f32::consts::FRAC_PI_2).abs() < 1.0e-6);
        assert!((anisotropy - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn zero_divergence_preserves_base_anisotropy() {
        let map = LayerBuffer {
            width: 3,
            height: 3,
            pixels: vec![
                PixelF32 {
                    alpha: 1.0,
                    red: 0.5,
                    green: 0.5,
                    blue: 0.5,
                };
                9
            ],
        };
        let mut settings = settings(ScatterMode::Gather);
        settings.direction = 0.7;
        settings.anisotropy = 0.8;
        settings.use_anisotropy_map = true;
        settings.anisotropy_map_mode = AnisotropyMapMode::DivergenceDirection;
        let (direction, anisotropy) = anisotropy_at(Some(&map), 1, 1, 3, 3, &settings);
        assert_eq!(direction.to_bits(), settings.direction.to_bits());
        assert_eq!(anisotropy.to_bits(), settings.anisotropy.to_bits());
    }

    #[test]
    fn full_anisotropy_collapses_offsets_to_direction_axis() {
        for attempt in 0..16 {
            let (_, dy) = discrete_anisotropic_offset(0, 0, 0, attempt, 8, 0.0, 1.0, 42);
            assert_eq!(dy, 0);
        }
    }

    #[test]
    fn anisotropy_rounding_stays_inside_the_radius() {
        for attempt in 0..64 {
            let (dx, dy) = discrete_anisotropic_offset(
                0,
                0,
                0,
                attempt,
                1,
                std::f32::consts::FRAC_PI_4,
                1.0,
                42,
            );
            assert!(dx * dx + dy * dy <= 1);
        }
    }

    #[test]
    fn circle_kernel_swap_remains_a_strict_permutation() {
        let source = source(16, 16);
        let mut settings = settings(ScatterMode::Swap);
        settings.grain_size = 4;
        settings.grain_shape = GrainShape::Circle;
        settings.radius = 12;
        let rendered = render_swap(&source, no_maps(), &settings, 42);
        let mut input_values: Vec<u32> = source.pixels.iter().map(|px| px.red.to_bits()).collect();
        let mut output_values: Vec<u32> = rendered.iter().map(|px| px.red.to_bits()).collect();
        input_values.sort_unstable();
        output_values.sort_unstable();
        assert_eq!(input_values, output_values);
    }

    #[test]
    fn temporal_frame_mode_changes_seed() {
        let seed_a = temporal_seed(10, TemporalMode::Frame, 1);
        let seed_b = temporal_seed(10, TemporalMode::Frame, 2);
        assert_ne!(seed_a, seed_b);
        assert_eq!(
            temporal_seed(10, TemporalMode::Static, 1),
            temporal_seed(10, TemporalMode::Static, 2)
        );
    }

    #[test]
    fn downsample_scales_pixel_distance_controls() {
        let mut base = settings(ScatterMode::Gather);
        base.radius = 12;
        base.grain_size = 8;
        base.grain_size_min = 4;

        let half = downsample_render_settings(base, 0.5, 0.5);
        assert_eq!(half.radius, 6);
        assert_eq!(half.grain_size, 4);
        assert_eq!(half.grain_size_min, 2);

        let mut minimums = settings(ScatterMode::Gather);
        minimums.radius = 1;
        minimums.grain_size = 1;
        minimums.grain_size_min = 1;
        let quarter = downsample_render_settings(minimums, 0.25, 0.25);
        assert_eq!(quarter.radius, 1);
        assert_eq!(quarter.grain_size, 1);
        assert_eq!(quarter.grain_size_min, 1);
    }
}
