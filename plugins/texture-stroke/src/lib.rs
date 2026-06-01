#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::Write;

use ae::pf::*;
use utils::ToPixel;

const PLUGIN_DESCRIPTION: &str = "Generates textured strokes from layer mask contours.";
const MAX_TIME_SAMPLES: usize = 16;
const MAX_BRUSH_STAMPS: usize = 100_000;
const ALPHA_EPSILON: f32 = 1.0e-6;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TextureLayer,
    PathGroupStart,
    PathSource,
    MaskPath,
    PathGroupEnd,
    OutputMode,
    StrokeSide,
    StrokeWidth,
    TextureOpacity,
    StrokeBlendMode,
    MaskGroupStart,
    FeatherInfluence,
    StrokeWidthSource,
    FeatherWidthScale,
    FeatherDebugLogging,
    MaskGroupEnd,
    TimeGroupStart,
    TextureTimeMode,
    FixedFrame,
    TimeRangeFrames,
    TimeSamples,
    TimeSeed,
    TimeGroupEnd,
    BrushGroupStart,
    BrushFallbackGroupStart,
    FallbackBrushShape,
    FallbackBrushSoftness,
    BrushFallbackGroupEnd,
    BrushSizeGroupStart,
    BrushSizeMin,
    BrushSizeMax,
    BrushSizeRandomness,
    BrushSizeNoiseScale,
    BrushSizeSeed,
    BrushSizeGroupEnd,
    BrushSpacingGroupStart,
    StampDensity,
    SpacingMin,
    SpacingMax,
    SpacingRandomness,
    SpacingNoiseScale,
    SpacingSeed,
    BrushSpacingGroupEnd,
    BrushOpacityGroupStart,
    BrushOpacityMin,
    BrushOpacityMax,
    BrushOpacityRandomness,
    BrushOpacityNoiseScale,
    BrushOpacitySeed,
    BrushOpacityGroupEnd,
    BrushRotationGroupStart,
    RotateWithStroke,
    DirectionOffset,
    RotationMin,
    RotationMax,
    RotationRandomness,
    RotationNoiseScale,
    RotationSeed,
    ReverseDirection,
    BrushRotationGroupEnd,
    BrushGroupEnd,
    SideColorGroupStart,
    SideColorMode,
    SideColorTarget,
    SideColorA,
    SideColorB,
    SideColorOpacity,
    SideColorBlendMode,
    SideColorGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Composite,
    StrokeOnly,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StrokeSide {
    Both,
    LeftOutside,
    RightInside,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PathSource {
    AllPaths,
    SelectedPath,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StrokeWidthSource {
    StrokeWidth,
    MaskFeather,
    StrokeWidthPlusMaskFeather,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FallbackBrushShape {
    Circle,
    Square,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextureTimeMode {
    Current,
    FixedFrame,
    AlongStroke,
    RandomPerStamp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SideColorMode {
    None,
    Solid,
    Gradient,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Add,
    Overlay,
    Difference,
}

#[derive(Clone, Copy)]
struct Settings {
    path_source: PathSource,
    output_mode: OutputMode,
    stroke_side: StrokeSide,
    stroke_width: f32,
    feather_influence: f32,
    stroke_width_source: StrokeWidthSource,
    feather_width_scale: f32,
    feather_debug_logging: bool,
    texture_time_mode: TextureTimeMode,
    fixed_frame: i32,
    time_range_frames: i32,
    time_samples: usize,
    time_seed: u32,
    fallback_brush_shape: FallbackBrushShape,
    fallback_brush_softness: f32,
    brush_size_min: f32,
    brush_size_max: f32,
    brush_size_randomness: f32,
    brush_size_noise_scale: f32,
    brush_size_seed: u32,
    stamp_density: f32,
    spacing_min: f32,
    spacing_max: f32,
    spacing_randomness: f32,
    spacing_noise_scale: f32,
    spacing_seed: u32,
    brush_opacity_min: f32,
    brush_opacity_max: f32,
    brush_opacity_randomness: f32,
    brush_opacity_noise_scale: f32,
    brush_opacity_seed: u32,
    rotate_with_stroke: bool,
    direction_offset: f32,
    rotation_min: f32,
    rotation_max: f32,
    rotation_randomness: f32,
    rotation_noise_scale: f32,
    rotation_seed: u32,
    reverse_direction: bool,
    texture_opacity: f32,
    stroke_blend_mode: BlendMode,
    side_color_mode: SideColorMode,
    side_color_target: StrokeSide,
    side_color_a: PixelF32,
    side_color_b: PixelF32,
    side_color_opacity: f32,
    side_color_blend_mode: BlendMode,
}

struct TextureFrame {
    pixels: Vec<PixelF32>,
    width: usize,
    height: usize,
}

struct BrushTexture {
    frames: Vec<TextureFrame>,
    has_texture_layer: bool,
}

#[derive(Clone, Copy)]
struct PathPoint {
    x: f32,
    y: f32,
    tangent_x: f32,
    tangent_y: f32,
    normal_x: f32,
    normal_y: f32,
    along: f32,
    stroke_width: f32,
}

#[derive(Clone, Copy)]
struct PathSelection {
    path_id: ae::sys::PF_PathID,
    path_index: usize,
}

#[derive(Clone, Copy)]
struct FeatherSample {
    key: f32,
    radius: f32,
}

#[derive(Clone, Default)]
struct PathFeatherProfile {
    uniform_radius: Option<f32>,
    samples: Vec<FeatherSample>,
}

#[derive(Clone, Copy)]
struct FeatherTimes {
    comp: ae::Time,
    layer: ae::Time,
}

#[derive(Clone)]
struct PathGeometry {
    open: bool,
    vertices: Vec<PathVertex>,
}

#[derive(Clone, Copy)]
struct PathVertex {
    x: f64,
    y: f64,
}

struct BrushStamp {
    x: f32,
    y: f32,
    normal_x: f32,
    normal_y: f32,
    along: f32,
    stamp_index: u32,
    size: f32,
    opacity: f32,
    rotation: f32,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

fn percent_slider(default: f64) -> FloatSliderDef<'static> {
    FloatSliderDef::setup(|d| {
        d.set_valid_min(0.0);
        d.set_valid_max(100.0);
        d.set_slider_min(0.0);
        d.set_slider_max(100.0);
        d.set_default(default);
        d.set_precision(1);
    })
}

fn noise_scale_slider() -> FloatSliderDef<'static> {
    FloatSliderDef::setup(|d| {
        d.set_valid_min(0.0);
        d.set_valid_max(100000.0);
        d.set_slider_min(0.0);
        d.set_slider_max(1000.0);
        d.set_default(0.0);
        d.set_precision(2);
    })
}

fn seed_slider(default: i32) -> SliderDef<'static> {
    SliderDef::setup(|d| {
        d.set_valid_min(0);
        d.set_valid_max(100000);
        d.set_slider_min(0);
        d.set_slider_max(10000);
        d.set_default(default);
    })
}

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
            Params::TextureLayer,
            "Texture Layer",
            LayerDef::new(),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add_group(
            Params::PathGroupStart,
            Params::PathGroupEnd,
            "Mask Path",
            false,
            |params| {
                params.add_with_flags(
                    Params::PathSource,
                    "Path Source",
                    PopupDef::setup(|d| {
                        d.set_options(&["All Paths", "Selected Path"]);
                        d.set_default(1);
                    }),
                    supervise_flags(),
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::MaskPath,
                    "Selected Mask Path",
                    PathDef::setup(|d| {
                        d.set_default(1);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_with_flags(
            Params::OutputMode,
            "Output",
            PopupDef::setup(|d| {
                d.set_options(&["Composite", "Stroke Only"]);
                d.set_default(1);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::StrokeSide,
            "Stroke Side",
            PopupDef::setup(|d| {
                d.set_options(&["Both", "Left / Outside", "Right / Inside"]);
                d.set_default(1);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::StrokeWidth,
            "Stroke Width (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(4096.0);
                d.set_slider_min(0.0);
                d.set_slider_max(256.0);
                d.set_default(24.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::TextureOpacity,
            "Texture Opacity (%)",
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
            Params::StrokeBlendMode,
            "Stroke Blend Mode",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Normal",
                    "Multiply",
                    "Screen",
                    "Add",
                    "Overlay",
                    "Difference",
                ]);
                d.set_default(1);
            }),
        )?;

        params.add_group(
            Params::MaskGroupStart,
            Params::MaskGroupEnd,
            "Stroke Edge",
            false,
            |params| {
                params.add(
                    Params::FeatherInfluence,
                    "Edge Softness (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(60.0);
                        d.set_precision(1);
                    }),
                )?;

                params.add_with_flags(
                    Params::StrokeWidthSource,
                    "Stroke Width Source",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Stroke Width",
                            "Mask Feather",
                            "Stroke Width + Mask Feather",
                        ]);
                        d.set_default(1);
                    }),
                    supervise_flags(),
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::FeatherWidthScale,
                    "Feather Width Scale (%)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(10000.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(400.0);
                        d.set_default(100.0);
                        d.set_precision(2);
                    }),
                )?;

                params.add(
                    Params::FeatherDebugLogging,
                    "Debug Feather Logging",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::TimeGroupStart,
            Params::TimeGroupEnd,
            "Texture Time",
            false,
            |params| {
                params.add_with_flags(
                    Params::TextureTimeMode,
                    "Texture Time",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Current",
                            "Fixed Frame",
                            "Along Stroke",
                            "Random Per Stamp",
                        ]);
                        d.set_default(1);
                    }),
                    supervise_flags(),
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::FixedFrame,
                    "Fixed Frame",
                    SliderDef::setup(|d| {
                        d.set_valid_min(-100000);
                        d.set_valid_max(100000);
                        d.set_slider_min(0);
                        d.set_slider_max(300);
                        d.set_default(0);
                    }),
                )?;

                params.add(
                    Params::TimeRangeFrames,
                    "Time Range (frames)",
                    SliderDef::setup(|d| {
                        d.set_valid_min(0);
                        d.set_valid_max(100000);
                        d.set_slider_min(0);
                        d.set_slider_max(300);
                        d.set_default(24);
                    }),
                )?;

                params.add(
                    Params::TimeSamples,
                    "Time Samples",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(MAX_TIME_SAMPLES as i32);
                        d.set_slider_min(1);
                        d.set_slider_max(MAX_TIME_SAMPLES as i32);
                        d.set_default(8);
                    }),
                )?;

                params.add(
                    Params::TimeSeed,
                    "Random Time Seed",
                    SliderDef::setup(|d| {
                        d.set_valid_min(0);
                        d.set_valid_max(100000);
                        d.set_slider_min(0);
                        d.set_slider_max(10000);
                        d.set_default(0);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::BrushGroupStart,
            Params::BrushGroupEnd,
            "Brush Stamp",
            false,
            |params| {
                params.add_group(
                    Params::BrushFallbackGroupStart,
                    Params::BrushFallbackGroupEnd,
                    "Fallback Brush",
                    true,
                    |params| {
                        params.add(
                            Params::FallbackBrushShape,
                            "Fallback Brush Shape",
                            PopupDef::setup(|d| {
                                d.set_options(&["Circle", "Square"]);
                                d.set_default(1);
                            }),
                        )?;

                        params.add(
                            Params::FallbackBrushSoftness,
                            "Fallback Brush Softness (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(100.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(100.0);
                                d.set_default(35.0);
                                d.set_precision(1);
                            }),
                        )?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::BrushSizeGroupStart,
                    Params::BrushSizeGroupEnd,
                    "Size",
                    true,
                    |params| {
                        params.add(
                            Params::BrushSizeMin,
                            "Size Min (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.1);
                                d.set_valid_max(10000.0);
                                d.set_slider_min(1.0);
                                d.set_slider_max(400.0);
                                d.set_default(100.0);
                                d.set_precision(2);
                            }),
                        )?;

                        params.add(
                            Params::BrushSizeMax,
                            "Size Max (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.1);
                                d.set_valid_max(10000.0);
                                d.set_slider_min(1.0);
                                d.set_slider_max(400.0);
                                d.set_default(100.0);
                                d.set_precision(2);
                            }),
                        )?;

                        params.add(
                            Params::BrushSizeRandomness,
                            "Size Randomness (%)",
                            percent_slider(0.0),
                        )?;

                        params.add(
                            Params::BrushSizeNoiseScale,
                            "Size Noise Scale (px)",
                            noise_scale_slider(),
                        )?;

                        params.add(Params::BrushSizeSeed, "Size Seed", seed_slider(11))?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::BrushSpacingGroupStart,
                    Params::BrushSpacingGroupEnd,
                    "Spacing",
                    true,
                    |params| {
                        params.add(
                            Params::StampDensity,
                            "Density (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.1);
                                d.set_valid_max(10000.0);
                                d.set_slider_min(1.0);
                                d.set_slider_max(400.0);
                                d.set_default(100.0);
                                d.set_precision(2);
                            }),
                        )?;
                        params.add(
                            Params::SpacingMin,
                            "Spacing Min (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.1);
                                d.set_valid_max(10000.0);
                                d.set_slider_min(1.0);
                                d.set_slider_max(400.0);
                                d.set_default(100.0);
                                d.set_precision(2);
                            }),
                        )?;
                        params.add(
                            Params::SpacingMax,
                            "Spacing Max (%)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.1);
                                d.set_valid_max(10000.0);
                                d.set_slider_min(1.0);
                                d.set_slider_max(400.0);
                                d.set_default(100.0);
                                d.set_precision(2);
                            }),
                        )?;
                        params.add(
                            Params::SpacingRandomness,
                            "Spacing Randomness (%)",
                            percent_slider(0.0),
                        )?;
                        params.add(
                            Params::SpacingNoiseScale,
                            "Spacing Noise Scale (px)",
                            noise_scale_slider(),
                        )?;
                        params.add(Params::SpacingSeed, "Spacing Seed", seed_slider(23))?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::BrushOpacityGroupStart,
                    Params::BrushOpacityGroupEnd,
                    "Opacity",
                    true,
                    |params| {
                        params.add(
                            Params::BrushOpacityMin,
                            "Opacity Min (%)",
                            percent_slider(100.0),
                        )?;
                        params.add(
                            Params::BrushOpacityMax,
                            "Opacity Max (%)",
                            percent_slider(100.0),
                        )?;
                        params.add(
                            Params::BrushOpacityRandomness,
                            "Opacity Randomness (%)",
                            percent_slider(0.0),
                        )?;
                        params.add(
                            Params::BrushOpacityNoiseScale,
                            "Opacity Noise Scale (px)",
                            noise_scale_slider(),
                        )?;
                        params.add(Params::BrushOpacitySeed, "Opacity Seed", seed_slider(37))?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::BrushRotationGroupStart,
                    Params::BrushRotationGroupEnd,
                    "Rotation",
                    true,
                    |params| {
                        params.add_with_flags(
                            Params::RotateWithStroke,
                            "Rotate Brush With Stroke",
                            CheckBoxDef::setup(|d| {
                                d.set_default(true);
                            }),
                            supervise_flags(),
                            ae::ParamUIFlags::empty(),
                        )?;

                        params.add(
                            Params::DirectionOffset,
                            "Base Direction Offset (deg)",
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
                            Params::RotationMin,
                            "Rotation Min (deg)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(-3600.0);
                                d.set_valid_max(3600.0);
                                d.set_slider_min(-180.0);
                                d.set_slider_max(180.0);
                                d.set_default(-15.0);
                                d.set_precision(2);
                            }),
                        )?;

                        params.add(
                            Params::RotationMax,
                            "Rotation Max (deg)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(-3600.0);
                                d.set_valid_max(3600.0);
                                d.set_slider_min(-180.0);
                                d.set_slider_max(180.0);
                                d.set_default(15.0);
                                d.set_precision(2);
                            }),
                        )?;

                        params.add(
                            Params::RotationRandomness,
                            "Rotation Randomness (%)",
                            percent_slider(0.0),
                        )?;

                        params.add(
                            Params::RotationNoiseScale,
                            "Rotation Noise Scale (px)",
                            noise_scale_slider(),
                        )?;

                        params.add(Params::RotationSeed, "Rotation Seed", seed_slider(53))?;

                        params.add(
                            Params::ReverseDirection,
                            "Reverse Direction",
                            CheckBoxDef::setup(|d| {
                                d.set_default(false);
                            }),
                        )?;

                        Ok(())
                    },
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::SideColorGroupStart,
            Params::SideColorGroupEnd,
            "Side Color",
            false,
            |params| {
                params.add_with_flags(
                    Params::SideColorMode,
                    "Side Color",
                    PopupDef::setup(|d| {
                        d.set_options(&["None", "Solid", "Gradient"]);
                        d.set_default(1);
                    }),
                    supervise_flags(),
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::SideColorTarget,
                    "Side Color Target",
                    PopupDef::setup(|d| {
                        d.set_options(&["Both", "Left / Outside", "Right / Inside"]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::SideColorA,
                    "Side Color A",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            red: 255,
                            green: 255,
                            blue: 255,
                            alpha: ae::MAX_CHANNEL8 as u8,
                        });
                    }),
                )?;

                params.add(
                    Params::SideColorB,
                    "Side Color B",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            red: 0,
                            green: 0,
                            blue: 0,
                            alpha: ae::MAX_CHANNEL8 as u8,
                        });
                    }),
                )?;

                params.add(
                    Params::SideColorOpacity,
                    "Side Color Opacity (%)",
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
                    Params::SideColorBlendMode,
                    "Side Color Blend Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Normal",
                            "Multiply",
                            "Screen",
                            "Add",
                            "Overlay",
                            "Difference",
                        ]);
                        d.set_default(1);
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
                        "AOD_TextureStroke - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag(OutFlags::PixIndependent, true);
                out_data.set_out_flag(OutFlags::UseOutputExtent, true);
                out_data.set_out_flag(OutFlags::WideTimeInput, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::AutomaticWideTimeInput, true);
                out_data.set_out_flag2(OutFlags2::DependsOnUnreferencedMasks, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_TextureStroke")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_data, in_layer, out_layer, params)?;
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
                    self.do_render(in_data, in_layer, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let changed = params.type_at(param_index);
                if matches!(
                    changed,
                    Params::PathSource
                        | Params::TextureLayer
                        | Params::TextureTimeMode
                        | Params::StrokeWidthSource
                        | Params::RotateWithStroke
                        | Params::SideColorMode
                ) {
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
        let time_mode =
            texture_time_mode_from_popup(params.get(Params::TextureTimeMode)?.as_popup()?.value());
        let path_source =
            path_source_from_popup(params.get(Params::PathSource)?.as_popup()?.value());
        let side_color_mode =
            side_color_mode_from_popup(params.get(Params::SideColorMode)?.as_popup()?.value());
        let stroke_width_source = stroke_width_source_from_popup(
            params.get(Params::StrokeWidthSource)?.as_popup()?.value(),
        );
        let rotate_with_stroke = params.get(Params::RotateWithStroke)?.as_checkbox()?.value();
        let has_texture_layer = params
            .get(Params::TextureLayer)?
            .as_layer()?
            .value()
            .is_some();

        let _ = in_data;

        Self::set_param_enabled(
            params,
            Params::MaskPath,
            matches!(path_source, PathSource::SelectedPath),
        )?;

        Self::set_param_enabled(params, Params::FallbackBrushShape, !has_texture_layer)?;
        Self::set_param_enabled(params, Params::FallbackBrushSoftness, !has_texture_layer)?;

        let feather_width_enabled = !matches!(stroke_width_source, StrokeWidthSource::StrokeWidth);
        Self::set_param_enabled(params, Params::FeatherWidthScale, feather_width_enabled)?;

        Self::set_param_enabled(
            params,
            Params::FixedFrame,
            matches!(time_mode, TextureTimeMode::FixedFrame),
        )?;
        Self::set_param_enabled(
            params,
            Params::TimeRangeFrames,
            matches!(
                time_mode,
                TextureTimeMode::AlongStroke | TextureTimeMode::RandomPerStamp
            ),
        )?;
        Self::set_param_enabled(
            params,
            Params::TimeSamples,
            matches!(
                time_mode,
                TextureTimeMode::AlongStroke | TextureTimeMode::RandomPerStamp
            ),
        )?;
        Self::set_param_enabled(
            params,
            Params::TimeSeed,
            matches!(time_mode, TextureTimeMode::RandomPerStamp),
        )?;

        Self::set_param_enabled(params, Params::DirectionOffset, rotate_with_stroke)?;
        Self::set_param_enabled(params, Params::ReverseDirection, rotate_with_stroke)?;

        let side_enabled = !matches!(side_color_mode, SideColorMode::None);
        Self::set_param_enabled(params, Params::SideColorTarget, side_enabled)?;
        Self::set_param_enabled(params, Params::SideColorA, side_enabled)?;
        Self::set_param_enabled(
            params,
            Params::SideColorB,
            matches!(side_color_mode, SideColorMode::Gradient),
        )?;
        Self::set_param_enabled(params, Params::SideColorOpacity, side_enabled)?;
        Self::set_param_enabled(params, Params::SideColorBlendMode, side_enabled)?;

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
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let settings = read_settings(params)?;
        let src = read_layer_rgba(&in_layer);
        if settings.stroke_width <= 0.0 {
            return write_layer_rgba(&mut out_layer, &src);
        }

        let paths = collect_path_selections(params, in_data, settings.path_source)?;
        if paths.is_empty() {
            return write_layer_rgba(&mut out_layer, &src);
        }

        let texture = build_texture_frames(params, in_data, &settings)?;
        let brush_stamps = build_brush_stamps(in_data, self.aegp_id, &paths, settings)?;
        if brush_stamps.is_empty() {
            return write_layer_rgba(&mut out_layer, &src);
        }

        let out = render_texture_stroke(&src, &brush_stamps, width, height, &texture, settings);

        write_layer_rgba(&mut out_layer, &out)
    }
}

fn read_settings(params: &mut Parameters<Params>) -> Result<Settings, Error> {
    Ok(Settings {
        path_source: path_source_from_popup(params.get(Params::PathSource)?.as_popup()?.value()),
        output_mode: output_mode_from_popup(params.get(Params::OutputMode)?.as_popup()?.value()),
        stroke_side: stroke_side_from_popup(params.get(Params::StrokeSide)?.as_popup()?.value()),
        stroke_width: params.get(Params::StrokeWidth)?.as_float_slider()?.value() as f32,
        feather_influence: ((params
            .get(Params::FeatherInfluence)?
            .as_float_slider()?
            .value() as f32)
            / 100.0)
            .clamp(0.0, 1.0),
        stroke_width_source: stroke_width_source_from_popup(
            params.get(Params::StrokeWidthSource)?.as_popup()?.value(),
        ),
        feather_width_scale: percent_value(params, Params::FeatherWidthScale)?.max(0.0),
        feather_debug_logging: params
            .get(Params::FeatherDebugLogging)?
            .as_checkbox()?
            .value(),
        texture_time_mode: texture_time_mode_from_popup(
            params.get(Params::TextureTimeMode)?.as_popup()?.value(),
        ),
        fixed_frame: params.get(Params::FixedFrame)?.as_slider()?.value(),
        time_range_frames: params
            .get(Params::TimeRangeFrames)?
            .as_slider()?
            .value()
            .max(0),
        time_samples: (params.get(Params::TimeSamples)?.as_slider()?.value() as usize)
            .clamp(1, MAX_TIME_SAMPLES),
        time_seed: params.get(Params::TimeSeed)?.as_slider()?.value().max(0) as u32,
        fallback_brush_shape: fallback_brush_shape_from_popup(
            params.get(Params::FallbackBrushShape)?.as_popup()?.value(),
        ),
        fallback_brush_softness: percent_value(params, Params::FallbackBrushSoftness)?
            .clamp(0.0, 1.0),
        brush_size_min: percent_value(params, Params::BrushSizeMin)?.max(0.001),
        brush_size_max: percent_value(params, Params::BrushSizeMax)?.max(0.001),
        brush_size_randomness: percent_value(params, Params::BrushSizeRandomness)?.clamp(0.0, 1.0),
        brush_size_noise_scale: params
            .get(Params::BrushSizeNoiseScale)?
            .as_float_slider()?
            .value() as f32,
        brush_size_seed: params
            .get(Params::BrushSizeSeed)?
            .as_slider()?
            .value()
            .max(0) as u32,
        stamp_density: percent_value(params, Params::StampDensity)?.max(0.001),
        spacing_min: percent_value(params, Params::SpacingMin)?.max(0.001),
        spacing_max: percent_value(params, Params::SpacingMax)?.max(0.001),
        spacing_randomness: percent_value(params, Params::SpacingRandomness)?.clamp(0.0, 1.0),
        spacing_noise_scale: params
            .get(Params::SpacingNoiseScale)?
            .as_float_slider()?
            .value() as f32,
        spacing_seed: params.get(Params::SpacingSeed)?.as_slider()?.value().max(0) as u32,
        brush_opacity_min: percent_value(params, Params::BrushOpacityMin)?.clamp(0.0, 1.0),
        brush_opacity_max: percent_value(params, Params::BrushOpacityMax)?.clamp(0.0, 1.0),
        brush_opacity_randomness: percent_value(params, Params::BrushOpacityRandomness)?
            .clamp(0.0, 1.0),
        brush_opacity_noise_scale: params
            .get(Params::BrushOpacityNoiseScale)?
            .as_float_slider()?
            .value() as f32,
        brush_opacity_seed: params
            .get(Params::BrushOpacitySeed)?
            .as_slider()?
            .value()
            .max(0) as u32,
        rotate_with_stroke: params.get(Params::RotateWithStroke)?.as_checkbox()?.value(),
        direction_offset: (params
            .get(Params::DirectionOffset)?
            .as_float_slider()?
            .value() as f32)
            .to_radians(),
        rotation_min: (params.get(Params::RotationMin)?.as_float_slider()?.value() as f32)
            .to_radians(),
        rotation_max: (params.get(Params::RotationMax)?.as_float_slider()?.value() as f32)
            .to_radians(),
        rotation_randomness: percent_value(params, Params::RotationRandomness)?.clamp(0.0, 1.0),
        rotation_noise_scale: params
            .get(Params::RotationNoiseScale)?
            .as_float_slider()?
            .value() as f32,
        rotation_seed: params
            .get(Params::RotationSeed)?
            .as_slider()?
            .value()
            .max(0) as u32,
        reverse_direction: params.get(Params::ReverseDirection)?.as_checkbox()?.value(),
        texture_opacity: ((params
            .get(Params::TextureOpacity)?
            .as_float_slider()?
            .value() as f32)
            / 100.0)
            .clamp(0.0, 1.0),
        stroke_blend_mode: blend_mode_from_popup(
            params.get(Params::StrokeBlendMode)?.as_popup()?.value(),
        ),
        side_color_mode: side_color_mode_from_popup(
            params.get(Params::SideColorMode)?.as_popup()?.value(),
        ),
        side_color_target: stroke_side_from_popup(
            params.get(Params::SideColorTarget)?.as_popup()?.value(),
        ),
        side_color_a: params
            .get(Params::SideColorA)?
            .as_color()?
            .value()
            .to_pixel32(),
        side_color_b: params
            .get(Params::SideColorB)?
            .as_color()?
            .value()
            .to_pixel32(),
        side_color_opacity: ((params
            .get(Params::SideColorOpacity)?
            .as_float_slider()?
            .value() as f32)
            / 100.0)
            .clamp(0.0, 1.0),
        side_color_blend_mode: blend_mode_from_popup(
            params.get(Params::SideColorBlendMode)?.as_popup()?.value(),
        ),
    })
}

fn build_texture_frames(
    params: &mut Parameters<Params>,
    in_data: InData,
    settings: &Settings,
) -> Result<BrushTexture, Error> {
    let sample_count = match settings.texture_time_mode {
        TextureTimeMode::Current | TextureTimeMode::FixedFrame => 1,
        TextureTimeMode::AlongStroke | TextureTimeMode::RandomPerStamp => settings.time_samples,
    };

    let mut frames = Vec::with_capacity(sample_count);
    let mut has_texture_layer = false;
    for sample in 0..sample_count {
        let time = texture_checkout_time(in_data, settings, sample, sample_count);
        let checkout = params.checkout_at(Params::TextureLayer, Some(time), None, None)?;
        let texture_layer = checkout.as_layer()?.value();

        if let Some(layer) = texture_layer.as_ref() {
            has_texture_layer = true;
            frames.push(TextureFrame {
                pixels: read_layer_rgba(layer),
                width: layer.width(),
                height: layer.height(),
            });
        } else {
            frames.push(TextureFrame {
                pixels: vec![PixelF32 {
                    red: 1.0,
                    green: 1.0,
                    blue: 1.0,
                    alpha: 1.0,
                }],
                width: 1,
                height: 1,
            });
        }
    }

    Ok(BrushTexture {
        frames,
        has_texture_layer,
    })
}

fn texture_checkout_time(
    in_data: InData,
    settings: &Settings,
    sample: usize,
    sample_count: usize,
) -> i32 {
    match settings.texture_time_mode {
        TextureTimeMode::Current => in_data.current_time(),
        TextureTimeMode::FixedFrame => settings.fixed_frame.saturating_mul(in_data.time_step()),
        TextureTimeMode::AlongStroke | TextureTimeMode::RandomPerStamp => {
            if sample_count <= 1 || settings.time_range_frames <= 0 {
                return in_data.current_time();
            }
            let t = sample as f32 / (sample_count - 1) as f32;
            let offset = ((t - 0.5) * settings.time_range_frames as f32).round() as i32;
            in_data
                .current_time()
                .saturating_add(offset.saturating_mul(in_data.time_step()))
        }
    }
}

fn collect_path_selections(
    params: &mut Parameters<Params>,
    in_data: InData,
    path_source: PathSource,
) -> Result<Vec<PathSelection>, Error> {
    let none = ae::sys::PF_PathID_NONE as ae::sys::PF_PathID;
    match path_source {
        PathSource::SelectedPath => {
            let path_id = params.get(Params::MaskPath)?.as_path()?.path_id();
            if path_id == none {
                Ok(Vec::new())
            } else {
                let effect = in_data.effect();
                Ok(vec![PathSelection {
                    path_id,
                    path_index: path_index_for_id(effect, path_id)?,
                }])
            }
        }
        PathSource::AllPaths => {
            let effect = in_data.effect();
            let count = effect.num_paths()?.max(0);
            let mut paths = Vec::new();
            for index in 0..count {
                let path_id = effect.path_info(index)?;
                if path_id != none {
                    paths.push(PathSelection {
                        path_id,
                        path_index: index as usize,
                    });
                }
            }
            Ok(paths)
        }
    }
}

fn path_index_for_id(effect: ae::pf::Effect, path_id: ae::sys::PF_PathID) -> Result<usize, Error> {
    let count = effect.num_paths()?.max(0);
    for index in 0..count {
        if effect.path_info(index)? == path_id {
            return Ok(index as usize);
        }
    }
    Ok(0)
}

fn build_brush_stamps(
    in_data: InData,
    plugin_id: Option<ae::aegp::PluginId>,
    paths: &[PathSelection],
    settings: Settings,
) -> Result<Vec<BrushStamp>, Error> {
    let feather_profiles = if matches!(settings.stroke_width_source, StrokeWidthSource::StrokeWidth)
    {
        vec![PathFeatherProfile::default(); paths.len()]
    } else {
        build_path_feather_profiles(in_data, plugin_id, paths, settings.feather_debug_logging)
    };
    let points = build_path_points(in_data, paths, &feather_profiles, settings)?;
    if points.is_empty() {
        debug_feather(
            settings.feather_debug_logging,
            "No path points were built for brush stamps.",
        );
        return Ok(Vec::new());
    }

    let mut stamps = Vec::new();
    let mut next_along = points[0].along;
    let mut stamp_index = 0u32;
    let mut point_index = 0usize;
    let total_length = points.last().map(|p| p.along).unwrap_or(0.0).max(1.0);

    while point_index < points.len() && stamps.len() < MAX_BRUSH_STAMPS {
        while point_index + 1 < points.len() && points[point_index].along < next_along {
            point_index += 1;
        }
        let point = points[point_index];
        let size = stamp_size(settings, point, stamp_index);
        let opacity = stamp_opacity(settings, point.along, stamp_index);
        let rotation = stamp_rotation(settings, point, stamp_index);

        stamps.push(BrushStamp {
            x: point.x,
            y: point.y,
            normal_x: point.normal_x,
            normal_y: point.normal_y,
            along: point.along,
            stamp_index,
            size,
            opacity,
            rotation,
        });

        let spacing = stamp_spacing(settings, size, point.along, stamp_index);
        if point_index + 1 >= points.len() {
            break;
        }
        next_along = (point.along + spacing).max(next_along + 0.25);
        if next_along >= total_length {
            break;
        }
        stamp_index = stamp_index.wrapping_add(1);
    }

    if settings.feather_debug_logging {
        let mut min_size = f32::MAX;
        let mut max_size = 0.0f32;
        let mut sum_size = 0.0f32;
        for stamp in &stamps {
            min_size = min_size.min(stamp.size);
            max_size = max_size.max(stamp.size);
            sum_size += stamp.size;
        }
        let avg_size = if stamps.is_empty() {
            0.0
        } else {
            sum_size / stamps.len() as f32
        };
        debug_feather(
            true,
            &format!(
                "Built {} brush stamps. size_min={}, size_avg={}, size_max={}",
                stamps.len(),
                min_size,
                avg_size,
                max_size
            ),
        );
    }

    Ok(stamps)
}

fn build_path_points(
    in_data: InData,
    paths: &[PathSelection],
    feather_profiles: &[PathFeatherProfile],
    settings: Settings,
) -> Result<Vec<PathPoint>, Error> {
    let effect = in_data.effect();
    let sample_step = (settings.stroke_width * 0.125).clamp(1.0, 4.0) as f64;
    let mut points = Vec::new();
    let mut total_along = 0.0f32;

    for (path_i, selection) in paths.iter().enumerate() {
        let Some(path) = effect.checkout_path(
            selection.path_id,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        )?
        else {
            continue;
        };

        let segments = path.num_segments()?.max(0);
        for seg in 0..segments {
            let mut prep = path.prepare_seg_length(seg, 100)?;
            let seg_len = prep.length()?.max(0.0);
            if seg_len <= f64::EPSILON {
                continue;
            }

            let steps = (seg_len / sample_step).ceil().max(1.0) as usize;
            for step in 0..=steps {
                let length = (seg_len * step as f64 / steps as f64).min(seg_len);
                let segment_s = if seg_len > f64::EPSILON {
                    (length / seg_len) as f32
                } else {
                    0.0
                };
                let (x, y, dx, dy) = prep.eval_deriv1(length)?;
                let (mut tangent_x, mut tangent_y) = normalize2(dx as f32, dy as f32);
                if settings.reverse_direction {
                    tangent_x = -tangent_x;
                    tangent_y = -tangent_y;
                }
                if settings.direction_offset.abs() > 1.0e-6 {
                    let cos_a = settings.direction_offset.cos();
                    let sin_a = settings.direction_offset.sin();
                    let rx = tangent_x * cos_a - tangent_y * sin_a;
                    let ry = tangent_x * sin_a + tangent_y * cos_a;
                    (tangent_x, tangent_y) = normalize2(rx, ry);
                }
                let normal_x = -tangent_y;
                let normal_y = tangent_x;

                points.push(PathPoint {
                    x: x as f32,
                    y: y as f32,
                    tangent_x,
                    tangent_y,
                    normal_x,
                    normal_y,
                    along: total_along + length as f32,
                    stroke_width: stroke_width_at(
                        settings,
                        feather_profiles.get(path_i),
                        seg,
                        segment_s,
                    ),
                });
            }

            total_along += seg_len as f32;
        }
    }

    Ok(points)
}

fn build_path_feather_profiles(
    in_data: InData,
    plugin_id: Option<ae::aegp::PluginId>,
    paths: &[PathSelection],
    debug_logging: bool,
) -> Vec<PathFeatherProfile> {
    let Some(plugin_id) = plugin_id else {
        debug_feather(debug_logging, "AEGP plugin id is not available.");
        return vec![PathFeatherProfile::default(); paths.len()];
    };

    let Ok(interface) = ae::aegp::suites::PFInterface::new() else {
        debug_feather(debug_logging, "PFInterface suite is not available.");
        return vec![PathFeatherProfile::default(); paths.len()];
    };
    let Ok(layer_handle) = interface.effect_layer(in_data.effect()) else {
        debug_feather(debug_logging, "Effect layer is not available.");
        return vec![PathFeatherProfile::default(); paths.len()];
    };
    let layer: ae::aegp::Layer = layer_handle.into();
    let mask_count = layer.num_masks().unwrap_or(0).max(0);
    let comp_time = interface
        .convert_effect_to_comp_time(
            in_data.effect(),
            in_data.current_time(),
            in_data.time_scale(),
        )
        .unwrap_or(ae::Time {
            value: in_data.current_time(),
            scale: in_data.time_scale(),
        });
    let layer_time = ae::Time {
        value: in_data.current_time(),
        scale: in_data.time_scale(),
    };
    let times = FeatherTimes {
        comp: comp_time,
        layer: layer_time,
    };

    debug_feather(
        debug_logging,
        &format!(
            "Building feather profiles. paths={}, masks={}, comp_time={}/{}, layer_time={}/{}",
            paths.len(),
            mask_count,
            times.comp.value,
            times.comp.scale,
            times.layer.value,
            times.layer.scale
        ),
    );

    paths
        .iter()
        .enumerate()
        .map(|(path_i, selection)| {
            let pf_geometry = path_geometry_for_selection(in_data, *selection);
            let Some((mask, match_method)) = mask_for_path(
                &layer,
                mask_count,
                *selection,
                plugin_id,
                times,
                pf_geometry.as_ref(),
                debug_logging,
            ) else {
                debug_feather(
                    debug_logging,
                    &format!(
                        "Path {} id={} did not match any mask.",
                        path_i, selection.path_id
                    ),
                );
                return PathFeatherProfile::default();
            };

            let profile = feather_profile_for_mask(&mask, plugin_id, times, debug_logging)
                .unwrap_or_default();
            debug_feather(
                debug_logging,
                &format!(
                    "Path {} id={} matched by {}. uniform={:?}, variable_points={}",
                    path_i,
                    selection.path_id,
                    match_method,
                    profile.uniform_radius,
                    profile.samples.len()
                ),
            );
            profile
        })
        .collect()
}

fn mask_for_path(
    layer: &ae::aegp::Layer,
    mask_count: i32,
    selection: PathSelection,
    plugin_id: ae::aegp::PluginId,
    times: FeatherTimes,
    pf_geometry: Option<&PathGeometry>,
    debug_logging: bool,
) -> Option<(ae::aegp::Mask, &'static str)> {
    for index in 0..mask_count {
        let Ok(mask) = layer.mask_by_index(index) else {
            debug_feather(
                debug_logging,
                &format!(
                    "Failed to get mask by index {} while matching by id.",
                    index
                ),
            );
            continue;
        };
        let mask_id = mask.id().ok();
        debug_feather(
            debug_logging,
            &format!(
                "Match check id: path_id={}, path_index={}, mask_index={}, mask_id={:?}",
                selection.path_id, selection.path_index, index, mask_id
            ),
        );
        if mask_id == Some(selection.path_id as i32) {
            return Some((mask, "mask-id"));
        }
    }

    if let Some(pf_geometry) = pf_geometry {
        debug_feather(
            debug_logging,
            &format!(
                "Path geometry for id={} has {} vertices, open={}.",
                selection.path_id,
                pf_geometry.vertices.len(),
                pf_geometry.open
            ),
        );
        for index in 0..mask_count {
            let Ok(mask) = layer.mask_by_index(index) else {
                debug_feather(
                    debug_logging,
                    &format!(
                        "Failed to get mask by index {} while matching by geometry.",
                        index
                    ),
                );
                continue;
            };
            if let Some(mask_geometry) = mask_geometry(&mask, plugin_id, times, debug_logging)
                && path_geometries_match(pf_geometry, &mask_geometry)
            {
                return Some((mask, "outline-geometry"));
            }
        }
    } else {
        debug_feather(
            debug_logging,
            &format!(
                "Path geometry for id={} is not available.",
                selection.path_id
            ),
        );
    }

    if selection.path_index < mask_count as usize {
        layer
            .mask_by_index(selection.path_index as i32)
            .ok()
            .map(|mask| (mask, "path-index"))
    } else {
        None
    }
}

fn feather_profile_for_mask(
    mask: &ae::aegp::Mask,
    plugin_id: ae::aegp::PluginId,
    times: FeatherTimes,
    debug_logging: bool,
) -> Option<PathFeatherProfile> {
    let mut profile = PathFeatherProfile::default();
    let mask_id = mask.id().ok();

    match mask.stream(plugin_id, ae::aegp::MaskStream::Feather) {
        Ok(stream) => {
            if let Ok(stream_type) = stream.stream_type() {
                debug_feather(
                    debug_logging,
                    &format!("Mask {:?} feather stream type: {:?}", mask_id, stream_type),
                );
            }
            profile.uniform_radius =
                uniform_feather_radius_for_stream(&stream, plugin_id, times, debug_logging);
        }
        Err(err) => debug_feather(
            debug_logging,
            &format!("Mask {:?} failed to get feather stream: {:?}", mask_id, err),
        ),
    }

    match mask.stream(plugin_id, ae::aegp::MaskStream::Outline) {
        Ok(stream) => {
            if let Some(outline) = outline_for_stream(&stream, plugin_id, times, debug_logging)
                && let Ok(count) = outline.num_feathers()
            {
                debug_feather(
                    debug_logging,
                    &format!("Mask {:?} outline feather count: {}", mask_id, count),
                );
                for index in 0..count.max(0) {
                    if let Ok(feather) = outline.feather_info(index) {
                        debug_feather(
                            debug_logging,
                            &format!(
                                "Mask {:?} variable feather {}: segment={}, s={}, radius={}",
                                mask_id,
                                index,
                                feather.segment,
                                feather.segment_sF,
                                feather.radiusF
                            ),
                        );
                        profile.samples.push(FeatherSample {
                            key: feather.segment as f32 + feather.segment_sF as f32,
                            radius: (feather.radiusF as f32).abs(),
                        });
                    }
                }
                profile.samples.sort_by(|a, b| a.key.total_cmp(&b.key));
            }
        }
        Err(err) => debug_feather(
            debug_logging,
            &format!("Mask {:?} failed to get outline stream: {:?}", mask_id, err),
        ),
    }

    if profile.uniform_radius.is_some() || !profile.samples.is_empty() {
        Some(profile)
    } else {
        None
    }
}

fn uniform_feather_radius_for_stream(
    stream: &ae::aegp::Stream,
    plugin_id: ae::aegp::PluginId,
    times: FeatherTimes,
    debug_logging: bool,
) -> Option<f32> {
    for (label, mode, time) in [
        ("CompTime", ae::aegp::TimeMode::CompTime, times.comp),
        ("LayerTime", ae::aegp::TimeMode::LayerTime, times.layer),
    ] {
        match stream.new_value(plugin_id, mode, time, false) {
            Ok(value) => {
                let radius = uniform_feather_radius(value);
                debug_feather(
                    debug_logging,
                    &format!("Feather stream {} radius: {:?}", label, radius),
                );
                if radius.is_some() {
                    return radius;
                }
            }
            Err(err) => debug_feather(
                debug_logging,
                &format!("Feather stream {} read failed: {:?}", label, err),
            ),
        }
    }
    None
}

fn outline_for_stream(
    stream: &ae::aegp::Stream,
    plugin_id: ae::aegp::PluginId,
    times: FeatherTimes,
    debug_logging: bool,
) -> Option<ae::aegp::MaskOutline> {
    for (label, mode, time) in [
        ("CompTime", ae::aegp::TimeMode::CompTime, times.comp),
        ("LayerTime", ae::aegp::TimeMode::LayerTime, times.layer),
    ] {
        match stream.new_value(plugin_id, mode, time, false) {
            Ok(ae::aegp::StreamValue::Mask(outline_handle)) => {
                debug_feather(debug_logging, &format!("Outline stream {} read ok.", label));
                return Some(outline_handle.into());
            }
            Ok(_) => debug_feather(
                debug_logging,
                &format!("Outline stream {} returned non-mask value.", label),
            ),
            Err(err) => debug_feather(
                debug_logging,
                &format!("Outline stream {} read failed: {:?}", label, err),
            ),
        }
    }
    None
}

fn path_geometry_for_selection(in_data: InData, selection: PathSelection) -> Option<PathGeometry> {
    let effect = in_data.effect();
    let path = effect
        .checkout_path(
            selection.path_id,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        )
        .ok()
        .flatten()?;
    let segments = path.num_segments().ok()?.max(0);
    let mut vertices = Vec::with_capacity(segments as usize + 1);
    for point in 0..=segments {
        let vertex = path.vertex(point).ok()?;
        vertices.push(PathVertex {
            x: vertex.x,
            y: vertex.y,
        });
    }
    Some(PathGeometry {
        open: path.is_open().unwrap_or(false),
        vertices,
    })
}

fn mask_geometry(
    mask: &ae::aegp::Mask,
    plugin_id: ae::aegp::PluginId,
    times: FeatherTimes,
    debug_logging: bool,
) -> Option<PathGeometry> {
    let stream = mask.stream(plugin_id, ae::aegp::MaskStream::Outline).ok()?;
    let outline = outline_for_stream(&stream, plugin_id, times, debug_logging)?;
    let segments = outline.num_segments().ok()?.max(0);
    let mut vertices = Vec::with_capacity(segments as usize + 1);
    for point in 0..=segments {
        let vertex = outline.vertex_info(point).ok()?;
        vertices.push(PathVertex {
            x: vertex.x,
            y: vertex.y,
        });
    }
    Some(PathGeometry {
        open: outline.is_open().unwrap_or(false),
        vertices,
    })
}

fn path_geometries_match(a: &PathGeometry, b: &PathGeometry) -> bool {
    if a.open != b.open || a.vertices.len() != b.vertices.len() || a.vertices.is_empty() {
        return false;
    }

    if vertices_match_ordered(&a.vertices, &b.vertices) {
        return true;
    }

    let mut reversed = b.vertices.clone();
    reversed.reverse();
    vertices_match_ordered(&a.vertices, &reversed)
}

fn vertices_match_ordered(a: &[PathVertex], b: &[PathVertex]) -> bool {
    const TOLERANCE: f64 = 0.5;
    a.iter()
        .zip(b.iter())
        .all(|(a, b)| (a.x - b.x).abs() <= TOLERANCE && (a.y - b.y).abs() <= TOLERANCE)
}

fn debug_feather(enabled: bool, message: &str) {
    if enabled {
        debug_log(&format!("[AOD_TextureStroke] Feather: {message}"));
    }
}

#[cfg(windows)]
fn debug_log(message: &str) {
    unsafe extern "system" {
        fn OutputDebugStringA(lp_output_string: *const i8);
    }

    let sanitized = message.replace('\0', " ");
    append_debug_file(&sanitized);
    if let Ok(c_message) = CString::new(sanitized) {
        unsafe {
            OutputDebugStringA(c_message.as_ptr());
        }
    }
}

#[cfg(not(windows))]
fn debug_log(message: &str) {
    append_debug_file(message);
    eprintln!("{message}");
}

fn append_debug_file(message: &str) {
    let path = env::temp_dir().join("AOD_TextureStroke_feather.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

fn uniform_feather_radius(value: ae::aegp::StreamValue) -> Option<f32> {
    match value {
        ae::aegp::StreamValue::TwoD { x, y } | ae::aegp::StreamValue::TwoDSpatial { x, y } => {
            Some(((x as f32).abs() + (y as f32).abs()) * 0.5)
        }
        ae::aegp::StreamValue::OneD(v) => Some((v as f32).abs()),
        _ => None,
    }
}

fn stroke_width_at(
    settings: Settings,
    profile: Option<&PathFeatherProfile>,
    segment: i32,
    segment_s: f32,
) -> f32 {
    let feather_width = profile
        .and_then(|profile| profile.radius_at(segment, segment_s))
        .map(|radius| radius * settings.feather_width_scale);

    let width = match settings.stroke_width_source {
        StrokeWidthSource::StrokeWidth => settings.stroke_width,
        StrokeWidthSource::MaskFeather => feather_width.unwrap_or(settings.stroke_width),
        StrokeWidthSource::StrokeWidthPlusMaskFeather => {
            settings.stroke_width + feather_width.unwrap_or(0.0)
        }
    };

    width.max(0.5)
}

impl PathFeatherProfile {
    fn radius_at(&self, segment: i32, segment_s: f32) -> Option<f32> {
        if self.samples.is_empty() {
            return self.uniform_radius;
        }
        if self.samples.len() == 1 {
            return Some(self.samples[0].radius);
        }

        let key = segment as f32 + segment_s.clamp(0.0, 1.0);
        if key <= self.samples[0].key {
            return Some(self.samples[0].radius);
        }

        for pair in self.samples.windows(2) {
            let a = pair[0];
            let b = pair[1];
            if key <= b.key {
                let span = (b.key - a.key).max(f32::EPSILON);
                let t = ((key - a.key) / span).clamp(0.0, 1.0);
                return Some(lerp(a.radius, b.radius, t));
            }
        }

        self.samples.last().map(|sample| sample.radius)
    }
}

fn stamp_size(settings: Settings, point: PathPoint, stamp_index: u32) -> f32 {
    let min = settings.brush_size_min.min(settings.brush_size_max);
    let max = settings.brush_size_min.max(settings.brush_size_max);
    let base_width = point.stroke_width.max(0.5);
    let base = base_width * ((min + max) * 0.5);
    let v = variation_value(
        point.along,
        stamp_index,
        settings.brush_size_seed,
        settings.brush_size_noise_scale,
    );
    let target = base_width * lerp(min, max, v);
    lerp(base, target, settings.brush_size_randomness).max(0.5)
}

fn stamp_spacing(settings: Settings, size: f32, along: f32, stamp_index: u32) -> f32 {
    let density = settings.stamp_density.max(0.001);
    let base = (size * 0.5 / density).max(0.5);
    let min = settings.spacing_min.min(settings.spacing_max);
    let max = settings.spacing_min.max(settings.spacing_max);
    let base_scale = (min + max) * 0.5;
    let v = variation_value(
        along,
        stamp_index,
        settings.spacing_seed,
        settings.spacing_noise_scale,
    );
    let random_scale = lerp(min, max, v);
    (base * lerp(base_scale, random_scale, settings.spacing_randomness)).max(0.5)
}

fn stamp_opacity(settings: Settings, along: f32, stamp_index: u32) -> f32 {
    let min = settings.brush_opacity_min.min(settings.brush_opacity_max);
    let max = settings.brush_opacity_min.max(settings.brush_opacity_max);
    let base = (min + max) * 0.5;
    let v = variation_value(
        along,
        stamp_index,
        settings.brush_opacity_seed,
        settings.brush_opacity_noise_scale,
    );
    let target = lerp(min, max, v);
    lerp(base, target, settings.brush_opacity_randomness).clamp(0.0, 1.0)
}

fn stamp_rotation(settings: Settings, point: PathPoint, stamp_index: u32) -> f32 {
    let base = if settings.rotate_with_stroke {
        point.tangent_y.atan2(point.tangent_x)
    } else {
        0.0
    } + settings.direction_offset;
    let min = settings.rotation_min.min(settings.rotation_max);
    let max = settings.rotation_min.max(settings.rotation_max);
    let v = variation_value(
        point.along,
        stamp_index,
        settings.rotation_seed,
        settings.rotation_noise_scale,
    );
    base + lerp(0.0, lerp(min, max, v), settings.rotation_randomness)
}

fn render_texture_stroke(
    src: &[PixelF32],
    stamps: &[BrushStamp],
    width: usize,
    height: usize,
    texture: &BrushTexture,
    settings: Settings,
) -> Vec<PixelF32> {
    let mut out = match settings.output_mode {
        OutputMode::Composite => src.to_vec(),
        OutputMode::StrokeOnly => vec![transparent(); src.len()],
    };

    let total_length = stamps
        .last()
        .map(|stamp| stamp.along.max(1.0))
        .unwrap_or(1.0);

    for stamp in stamps {
        let frame_idx = texture_frame_index(
            (stamp.along / total_length).clamp(0.0, 1.0),
            stamp.stamp_index,
            settings.time_seed,
            settings.texture_time_mode,
            texture.frames.len(),
        );
        let Some(frame) = texture
            .frames
            .get(frame_idx)
            .or_else(|| texture.frames.first())
        else {
            continue;
        };

        let aspect = if texture.has_texture_layer && frame.height > 0 {
            frame.width.max(1) as f32 / frame.height.max(1) as f32
        } else {
            1.0
        };
        let half_h = (stamp.size * 0.5).max(0.5);
        let half_w = if texture.has_texture_layer {
            (stamp.size * aspect * 0.5).max(0.5)
        } else {
            half_h
        };
        let radius = (half_w * half_w + half_h * half_h).sqrt() + 1.0;
        let min_x = (stamp.x - radius).floor().max(0.0) as usize;
        let max_x = (stamp.x + radius)
            .ceil()
            .min(width.saturating_sub(1) as f32) as usize;
        let min_y = (stamp.y - radius).floor().max(0.0) as usize;
        let max_y = (stamp.y + radius)
            .ceil()
            .min(height.saturating_sub(1) as f32) as usize;
        let cos_r = stamp.rotation.cos();
        let sin_r = stamp.rotation.sin();

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let idx = y * width + x;
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let dx = px - stamp.x;
                let dy = py - stamp.y;
                let signed_distance = dx * stamp.normal_x + dy * stamp.normal_y;
                let side_coverage = stroke_side_coverage(
                    settings.stroke_side,
                    signed_distance,
                    stamp.size,
                    settings,
                );
                if side_coverage <= 0.0 {
                    continue;
                }

                let local_x = dx * cos_r + dy * sin_r;
                let local_y = -dx * sin_r + dy * cos_r;
                if local_x.abs() > half_w || local_y.abs() > half_h {
                    continue;
                }

                let coverage =
                    brush_shape_coverage(local_x, local_y, half_w, half_h, texture, settings);
                if coverage <= 0.0 {
                    continue;
                }

                let opacity = coverage * side_coverage * settings.texture_opacity * stamp.opacity;
                if opacity <= ALPHA_EPSILON {
                    continue;
                }

                let mut stroke = sample_brush_texture(frame, local_x, local_y, half_w, half_h);
                stroke.alpha *= opacity;
                stroke.red *= opacity;
                stroke.green *= opacity;
                stroke.blue *= opacity;

                let distance = signed_distance.abs();
                stroke = apply_side_color(stroke, signed_distance, distance, settings);

                let base = match settings.output_mode {
                    OutputMode::Composite => out[idx],
                    OutputMode::StrokeOnly => transparent(),
                };
                out[idx] = composite_pixel(base, stroke, settings.stroke_blend_mode);
            }
        }
    }

    out
}

fn texture_frame_index(
    along_phase: f32,
    stamp_index: u32,
    seed: u32,
    mode: TextureTimeMode,
    frame_count: usize,
) -> usize {
    if frame_count <= 1 {
        return 0;
    }

    match mode {
        TextureTimeMode::AlongStroke => {
            let t = along_phase.clamp(0.0, 1.0);
            ((t * frame_count as f32).floor() as usize).min(frame_count - 1)
        }
        TextureTimeMode::RandomPerStamp => (hash_u32(stamp_index ^ seed) as usize) % frame_count,
        TextureTimeMode::Current | TextureTimeMode::FixedFrame => 0,
    }
}

fn brush_shape_coverage(
    local_x: f32,
    local_y: f32,
    half_w: f32,
    half_h: f32,
    texture: &BrushTexture,
    settings: Settings,
) -> f32 {
    if texture.has_texture_layer {
        return 1.0;
    }

    let d = match settings.fallback_brush_shape {
        FallbackBrushShape::Circle => {
            let nx = local_x / half_w.max(1.0e-6);
            let ny = local_y / half_h.max(1.0e-6);
            (nx * nx + ny * ny).sqrt()
        }
        FallbackBrushShape::Square => {
            (local_x.abs() / half_w.max(1.0e-6)).max(local_y.abs() / half_h.max(1.0e-6))
        }
    };
    if d >= 1.0 {
        return 0.0;
    }

    let softness = settings.fallback_brush_softness.clamp(0.0, 1.0);
    if softness <= 1.0e-6 {
        return 1.0;
    }
    1.0 - smoothstep((1.0 - softness).clamp(0.0, 1.0), 1.0, d)
}

fn sample_brush_texture(
    frame: &TextureFrame,
    local_x: f32,
    local_y: f32,
    half_w: f32,
    half_h: f32,
) -> PixelF32 {
    if frame.width == 0 || frame.height == 0 {
        return transparent();
    }

    let u = ((local_x / half_w.max(1.0e-6)) * 0.5 + 0.5).clamp(0.0, 1.0);
    let v = ((local_y / half_h.max(1.0e-6)) * 0.5 + 0.5).clamp(0.0, 1.0);
    let x = ((u * frame.width.saturating_sub(1) as f32).round() as usize).min(frame.width - 1);
    let y = ((v * frame.height.saturating_sub(1) as f32).round() as usize).min(frame.height - 1);
    frame.pixels[y * frame.width + x]
}

fn apply_side_color(
    stroke: PixelF32,
    signed_distance: f32,
    distance: f32,
    settings: Settings,
) -> PixelF32 {
    if matches!(settings.side_color_mode, SideColorMode::None)
        || !stroke_side_matches(settings.side_color_target, signed_distance)
    {
        return stroke;
    }

    let t = if settings.stroke_width <= ALPHA_EPSILON {
        0.0
    } else {
        (distance / settings.stroke_width).clamp(0.0, 1.0)
    };
    let side_color = match settings.side_color_mode {
        SideColorMode::None => return stroke,
        SideColorMode::Solid => settings.side_color_a,
        SideColorMode::Gradient => lerp_pixel(settings.side_color_a, settings.side_color_b, t),
    };
    let mut side_color = side_color;
    side_color.alpha *= settings.side_color_opacity;
    side_color.red *= settings.side_color_opacity;
    side_color.green *= settings.side_color_opacity;
    side_color.blue *= settings.side_color_opacity;

    composite_pixel(stroke, side_color, settings.side_color_blend_mode)
}

fn composite_pixel(base: PixelF32, blend: PixelF32, mode: BlendMode) -> PixelF32 {
    let base_a = sanitize(base.alpha).clamp(0.0, 1.0);
    let blend_a = sanitize(blend.alpha).clamp(0.0, 1.0);
    if blend_a <= ALPHA_EPSILON {
        return sanitize_pixel(base);
    }

    let base_rgb = straight_rgb(base);
    let blend_rgb = straight_rgb(blend);
    let mixed_rgb = blend_rgb_mode(base_rgb, blend_rgb, mode);
    let out_a = blend_a + base_a * (1.0 - blend_a);
    if out_a <= ALPHA_EPSILON {
        return transparent();
    }

    let out_rgb = [
        (mixed_rgb[0] * blend_a + base_rgb[0] * base_a * (1.0 - blend_a)) / out_a,
        (mixed_rgb[1] * blend_a + base_rgb[1] * base_a * (1.0 - blend_a)) / out_a,
        (mixed_rgb[2] * blend_a + base_rgb[2] * base_a * (1.0 - blend_a)) / out_a,
    ];
    premultiply(out_rgb, out_a)
}

fn blend_rgb_mode(base: [f32; 3], blend: [f32; 3], mode: BlendMode) -> [f32; 3] {
    match mode {
        BlendMode::Normal => blend,
        BlendMode::Multiply => [base[0] * blend[0], base[1] * blend[1], base[2] * blend[2]],
        BlendMode::Screen => [
            1.0 - (1.0 - base[0]) * (1.0 - blend[0]),
            1.0 - (1.0 - base[1]) * (1.0 - blend[1]),
            1.0 - (1.0 - base[2]) * (1.0 - blend[2]),
        ],
        BlendMode::Add => [base[0] + blend[0], base[1] + blend[1], base[2] + blend[2]],
        BlendMode::Overlay => [
            overlay_channel(base[0], blend[0]),
            overlay_channel(base[1], blend[1]),
            overlay_channel(base[2], blend[2]),
        ],
        BlendMode::Difference => [
            (base[0] - blend[0]).abs(),
            (base[1] - blend[1]).abs(),
            (base[2] - blend[2]).abs(),
        ],
    }
}

fn overlay_channel(base: f32, blend: f32) -> f32 {
    if base <= 0.5 {
        2.0 * base * blend
    } else {
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend)
    }
}

fn stroke_side_matches(side: StrokeSide, signed_distance: f32) -> bool {
    match side {
        StrokeSide::Both => true,
        StrokeSide::LeftOutside => signed_distance >= 0.0,
        StrokeSide::RightInside => signed_distance <= 0.0,
    }
}

fn stroke_side_coverage(
    side: StrokeSide,
    signed_distance: f32,
    size: f32,
    settings: Settings,
) -> f32 {
    match side {
        StrokeSide::Both => 1.0,
        StrokeSide::LeftOutside => {
            soft_side_step(signed_distance, size * settings.feather_influence * 0.5)
        }
        StrokeSide::RightInside => {
            soft_side_step(-signed_distance, size * settings.feather_influence * 0.5)
        }
    }
}

fn soft_side_step(value: f32, softness: f32) -> f32 {
    if softness <= 1.0e-6 {
        if value >= 0.0 { 1.0 } else { 0.0 }
    } else {
        smoothstep(-softness, softness, value)
    }
}

fn read_layer_rgba(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut out = vec![transparent(); width * height];

    for y in 0..height {
        for x in 0..width {
            out[y * width + x] = read_pixel_f32(layer, world_type, x, y);
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

fn write_layer_rgba(out_layer: &mut Layer, pixels: &[PixelF32]) -> Result<(), Error> {
    let width = out_layer.width();
    let height = out_layer.height();
    let out_world_type = out_layer.world_type();

    out_layer.iterate(0, height as i32, None, |x, y, mut dst| {
        let idx = y as usize * width + x as usize;
        let px = sanitize_pixel(pixels[idx]);
        match out_world_type {
            ae::aegp::WorldType::U8 => dst.set_from_u8(px.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(px.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(px),
        }
        Ok(())
    })?;

    Ok(())
}

fn output_mode_from_popup(value: i32) -> OutputMode {
    match value {
        2 => OutputMode::StrokeOnly,
        _ => OutputMode::Composite,
    }
}

fn stroke_side_from_popup(value: i32) -> StrokeSide {
    match value {
        2 => StrokeSide::LeftOutside,
        3 => StrokeSide::RightInside,
        _ => StrokeSide::Both,
    }
}

fn path_source_from_popup(value: i32) -> PathSource {
    match value {
        2 => PathSource::SelectedPath,
        _ => PathSource::AllPaths,
    }
}

fn stroke_width_source_from_popup(value: i32) -> StrokeWidthSource {
    match value {
        2 => StrokeWidthSource::MaskFeather,
        3 => StrokeWidthSource::StrokeWidthPlusMaskFeather,
        _ => StrokeWidthSource::StrokeWidth,
    }
}

fn fallback_brush_shape_from_popup(value: i32) -> FallbackBrushShape {
    match value {
        2 => FallbackBrushShape::Square,
        _ => FallbackBrushShape::Circle,
    }
}

fn texture_time_mode_from_popup(value: i32) -> TextureTimeMode {
    match value {
        2 => TextureTimeMode::FixedFrame,
        3 => TextureTimeMode::AlongStroke,
        4 => TextureTimeMode::RandomPerStamp,
        _ => TextureTimeMode::Current,
    }
}

fn side_color_mode_from_popup(value: i32) -> SideColorMode {
    match value {
        2 => SideColorMode::Solid,
        3 => SideColorMode::Gradient,
        _ => SideColorMode::None,
    }
}

fn blend_mode_from_popup(value: i32) -> BlendMode {
    match value {
        2 => BlendMode::Multiply,
        3 => BlendMode::Screen,
        4 => BlendMode::Add,
        5 => BlendMode::Overlay,
        6 => BlendMode::Difference,
        _ => BlendMode::Normal,
    }
}

fn percent_value(params: &mut Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok((params.get(id)?.as_float_slider()?.value() as f32) / 100.0)
}

fn variation_value(along: f32, stamp_index: u32, seed: u32, noise_scale: f32) -> f32 {
    if noise_scale > 1.0e-6 {
        value_noise_1d(along / noise_scale.max(1.0e-6), seed)
    } else {
        random_unit(stamp_index, seed)
    }
}

fn value_noise_1d(x: f32, seed: u32) -> f32 {
    let i0 = x.floor();
    let t = x - i0;
    let i0 = i0 as i32;
    let i1 = i0.saturating_add(1);
    let a = random_unit(i0 as u32, seed);
    let b = random_unit(i1 as u32, seed);
    lerp(a, b, t * t * (3.0 - 2.0 * t))
}

fn random_unit(index: u32, seed: u32) -> f32 {
    hash_u32(index ^ seed.rotate_left(13)) as f32 / u32::MAX as f32
}

fn straight_rgb(px: PixelF32) -> [f32; 3] {
    if px.alpha > ALPHA_EPSILON {
        [
            sanitize(px.red / px.alpha),
            sanitize(px.green / px.alpha),
            sanitize(px.blue / px.alpha),
        ]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn premultiply(rgb: [f32; 3], alpha: f32) -> PixelF32 {
    PixelF32 {
        red: sanitize(rgb[0]) * alpha,
        green: sanitize(rgb[1]) * alpha,
        blue: sanitize(rgb[2]) * alpha,
        alpha,
    }
}

fn transparent() -> PixelF32 {
    PixelF32 {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.0,
    }
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    let t = t.clamp(0.0, 1.0);
    PixelF32 {
        red: lerp(a.red, b.red, t),
        green: lerp(a.green, b.green, t),
        blue: lerp(a.blue, b.blue, t),
        alpha: lerp(a.alpha, b.alpha, t),
    }
}

fn normalize2(x: f32, y: f32) -> (f32, f32) {
    let len = (x * x + y * y).sqrt();
    if len > 1.0e-6 {
        (x / len, y / len)
    } else {
        (1.0, 0.0)
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

fn sanitize_pixel(px: PixelF32) -> PixelF32 {
    PixelF32 {
        red: sanitize(px.red),
        green: sanitize(px.green),
        blue: sanitize(px.blue),
        alpha: sanitize(px.alpha).clamp(0.0, 1.0),
    }
}

fn sanitize(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}
