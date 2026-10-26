#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::image::{
    SampleEdge, TRANSPARENT, lerp_pixel, read_layer, sample_bilinear, sample_nearest,
    sanitize_pixel,
};

mod temporal;
mod transform;
use temporal::{
    FrameRequest, PreRenderData, SourceFrame, TimeSettings, render_temporal, required_times,
};

#[cfg(test)]
mod pattern_tests;

use transform::{
    DeltaMode, FeedbackSettings, IndexAxis, PolarSettings, Repeat, StripClipping, StripDelta,
    StripMapping, TileMapping, TileShape, Transform, Vec2, feedback_coordinate, polar_coordinate,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    Center,
    Mix,
    Sampling,
    Edge,

    FeedbackGroupStart,
    FeedbackIterations,
    FeedbackStepScale,
    FeedbackStepRotation,
    FeedbackOffsetX,
    FeedbackOffsetY,
    FeedbackDecay,
    FeedbackOpacity,
    FeedbackCompositeOrder,
    FeedbackVortex,
    FeedbackGroupEnd,

    PolarGroupStart,
    PolarSectors,
    PolarMirror,
    PolarRings,
    PolarTwist,
    PolarGroupEnd,

    StripGroupStart,
    StripAngle,
    StripWidth,
    StripRepeat,
    StripGroupEnd,

    TileGroupStart,
    TileShape,
    TileWidth,
    TileHeight,
    TileRepeat,
    TileRotation,
    TileScaleX,
    TileScaleY,
    TileSkew,
    TileSkewAxis,
    TileGroupEnd,
    TileAffect,
    StripRotation,
    StripScaleX,
    StripScaleY,
    StripSkew,
    StripSkewAxis,
    StripAffect,
    StripEdgeDirection,
    StripDeltaAngle,
    StripDeltaWidth,
    StripDeltaX,
    StripDeltaY,
    StripDeltaMode,
    StripDeltaCycle,
    StripDeltaSeed,
    StripCopies,
    StripSplitOverlaps,
    StripClipping,
    StripOverlapOrder,
    TimeGroupStart,
    TimeDelta,
    TimeOrigin,
    TimeIndexAxis,
    TimeIndexOffset,
    TimeEdge,
    TimeGroupEnd,
}

const FEEDBACK_PARAMS: [Params; 11] = [
    Params::FeedbackGroupStart,
    Params::FeedbackIterations,
    Params::FeedbackStepScale,
    Params::FeedbackStepRotation,
    Params::FeedbackOffsetX,
    Params::FeedbackOffsetY,
    Params::FeedbackDecay,
    Params::FeedbackOpacity,
    Params::FeedbackCompositeOrder,
    Params::FeedbackVortex,
    Params::FeedbackGroupEnd,
];

const POLAR_PARAMS: [Params; 6] = [
    Params::PolarGroupStart,
    Params::PolarSectors,
    Params::PolarMirror,
    Params::PolarRings,
    Params::PolarTwist,
    Params::PolarGroupEnd,
];

const STRIP_PARAMS: &[Params] = &[
    Params::StripGroupStart,
    Params::StripAngle,
    Params::StripWidth,
    Params::StripRepeat,
    Params::StripRotation,
    Params::StripScaleX,
    Params::StripScaleY,
    Params::StripSkew,
    Params::StripSkewAxis,
    Params::StripAffect,
    Params::StripEdgeDirection,
    Params::StripDeltaAngle,
    Params::StripDeltaWidth,
    Params::StripDeltaX,
    Params::StripDeltaY,
    Params::StripDeltaMode,
    Params::StripDeltaCycle,
    Params::StripDeltaSeed,
    Params::StripCopies,
    Params::StripSplitOverlaps,
    Params::StripClipping,
    Params::StripGroupEnd,
];

const TILE_PARAMS: &[Params] = &[
    Params::TileGroupStart,
    Params::TileShape,
    Params::TileWidth,
    Params::TileHeight,
    Params::TileRepeat,
    Params::TileRotation,
    Params::TileScaleX,
    Params::TileScaleY,
    Params::TileSkew,
    Params::TileSkewAxis,
    Params::TileAffect,
    Params::TileGroupEnd,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Feedback,
    Polar,
    Strip,
    Tile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Sampling {
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImageEdge {
    Standard(SampleEdge),
    Black,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FeedbackCompositeOrder {
    FrontToBack,
    BackToFront,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StripOverlapOrder {
    Over,
    Under,
}

#[derive(Clone, Debug)]
struct RenderSettings {
    mode: Mode,
    space: PatternSpace,
    center: Vec2,
    mix: f32,
    sampling: Sampling,
    edge: ImageEdge,
    feedback_iterations: usize,
    feedback_decay: f32,
    feedback_opacity: f32,
    feedback_composite_order: FeedbackCompositeOrder,
    feedback: FeedbackSettings,
    polar: PolarSettings,
    strip: StripMapping,
    strip_edge_along: bool,
    strip_clipping: StripClipping,
    strip_overlap_order: StripOverlapOrder,
    time: TimeSettings,
    tile: TileMapping,
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderLayout {
    input_rect: Option<ae::Rect>,
    output_rect: Option<ae::Rect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BufferBounds {
    origin_x: i64,
    origin_y: i64,
    width: usize,
    height: usize,
}

impl BufferBounds {
    const fn new(origin: (i64, i64), width: usize, height: usize) -> Self {
        Self {
            origin_x: origin.0,
            origin_y: origin.1,
            width,
            height,
        }
    }

    fn layer_coordinate(self, local_x: usize, local_y: usize) -> (i64, i64) {
        (
            self.origin_x + local_x as i64,
            self.origin_y + local_y as i64,
        )
    }

    fn local_coordinate(self, layer_x: i64, layer_y: i64) -> Option<(usize, usize)> {
        let local_x = layer_x.checked_sub(self.origin_x)?;
        let local_y = layer_y.checked_sub(self.origin_y)?;
        if local_x < 0
            || local_y < 0
            || local_x >= self.width as i64
            || local_y >= self.height as i64
        {
            return None;
        }
        Some((local_x as usize, local_y as usize))
    }
}

#[derive(Clone, Copy, Debug)]
struct PatternSpace {
    source_anchor: Vec2,
    buffer_to_pattern: Vec2,
    pattern_to_buffer: Vec2,
    extent: Vec2,
    pixel_aspect: f32,
}

impl PatternSpace {
    fn new(
        source_anchor: Vec2,
        downsample: Vec2,
        pixel_aspect: f32,
        full_width: f32,
        full_height: f32,
    ) -> Self {
        let downsample_x = positive_finite(downsample.x, 1.0);
        let downsample_y = positive_finite(downsample.y, 1.0);
        let pixel_aspect = positive_finite(pixel_aspect, 1.0);
        Self {
            source_anchor,
            buffer_to_pattern: Vec2::new(pixel_aspect / downsample_x, downsample_y.recip()),
            pattern_to_buffer: Vec2::new(downsample_x / pixel_aspect, downsample_y),
            extent: Vec2::new(full_width.max(1.0) * pixel_aspect, full_height.max(1.0)),
            pixel_aspect,
        }
    }

    fn buffer_point_to_pattern(self, point: Vec2) -> Vec2 {
        Vec2::new(
            (point.x - self.source_anchor.x) * self.buffer_to_pattern.x,
            (point.y - self.source_anchor.y) * self.buffer_to_pattern.y,
        )
    }

    fn layer_point_to_pattern(self, point: Vec2, source_origin: Vec2) -> Vec2 {
        self.buffer_point_to_pattern(point - source_origin)
    }

    fn pattern_point_to_buffer(self, point: Vec2) -> Vec2 {
        Vec2::new(
            point.x * self.pattern_to_buffer.x + self.source_anchor.x,
            point.y * self.pattern_to_buffer.y + self.source_anchor.y,
        )
    }

    fn pixel_offset_to_pattern(self, offset: Vec2) -> Vec2 {
        Vec2::new(offset.x * self.pixel_aspect, offset.y)
    }

    #[cfg(test)]
    fn layer_center(self) -> Vec2 {
        Vec2::new(
            (self.extent.x - self.pixel_aspect) * 0.5,
            (self.extent.y - 1.0) * 0.5,
        )
    }

    fn max_radius(self) -> f32 {
        self.extent.x.hypot(self.extent.y) * 0.5
    }
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Creates feedback, kaleidoscope, strip repeats, and seamless polygon image patterns with per-copy time offsets.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::Mode,
            "Pattern Mode",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Feedback",
                    "Polar Kaleidoscope",
                    "Strip Repeat",
                    "Polygon Tile",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE | ae::ParamFlag::CANNOT_TIME_VARY,
            ParamUIFlags::empty(),
        )?;

        params.add(
            Params::Center,
            "Center",
            PointDef::setup(|d| {
                d.set_default((50.0, 50.0));
            }),
        )?;

        params.add(
            Params::Mix,
            "Mix (%)",
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
            Params::Sampling,
            "Sampling",
            PopupDef::setup(|d| {
                d.set_options(&["Nearest", "Bilinear"]);
                d.set_default(2);
            }),
        )?;

        params.add(
            Params::Edge,
            "Edge",
            PopupDef::setup(|d| {
                // Append new choices to preserve saved popup values.
                d.set_options(&["None (Transparent)", "Extend", "Tile", "Mirror", "Black"]);
                d.set_default(1);
            }),
        )?;

        params.add_group(
            Params::FeedbackGroupStart,
            Params::FeedbackGroupEnd,
            "Feedback",
            false,
            |params| {
                params.add(
                    Params::FeedbackIterations,
                    "Iterations",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(16);
                        d.set_slider_min(1);
                        d.set_slider_max(16);
                        d.set_default(6);
                    }),
                )?;
                params.add(
                    Params::FeedbackStepScale,
                    "Step Scale",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.05);
                        d.set_valid_max(8.0);
                        d.set_slider_min(0.25);
                        d.set_slider_max(2.0);
                        d.set_default(0.88);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::FeedbackStepRotation,
                    "Step Rotation",
                    AngleDef::setup(|d| {
                        d.set_default(12.0);
                    }),
                )?;
                for (id, name) in [
                    (Params::FeedbackOffsetX, "Step Offset X"),
                    (Params::FeedbackOffsetY, "Step Offset Y"),
                ] {
                    params.add(
                        id,
                        name,
                        FloatSliderDef::setup(|d| {
                            d.set_valid_min(-20_000.0);
                            d.set_valid_max(20_000.0);
                            d.set_slider_min(-200.0);
                            d.set_slider_max(200.0);
                            d.set_default(0.0);
                            d.set_precision(2);
                        }),
                    )?;
                }
                params.add(
                    Params::FeedbackDecay,
                    "Decay",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::FeedbackOpacity,
                    "Opacity (%)",
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
                    Params::FeedbackCompositeOrder,
                    "Composite Order",
                    PopupDef::setup(|d| {
                        d.set_options(&["Front to Back", "Back to Front"]);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::FeedbackVortex,
                    "Vortex",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::PolarGroupStart,
            Params::PolarGroupEnd,
            "Polar Pattern",
            false,
            |params| {
                params.add(
                    Params::PolarSectors,
                    "Sectors",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(128);
                        d.set_slider_min(1);
                        d.set_slider_max(32);
                        d.set_default(8);
                    }),
                )?;
                params.add(
                    Params::PolarMirror,
                    "Mirror Alternate Sectors",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;
                params.add(
                    Params::PolarRings,
                    "Radial Repeats",
                    SliderDef::setup(|d| {
                        d.set_valid_min(0);
                        d.set_valid_max(64);
                        d.set_slider_min(0);
                        d.set_slider_max(16);
                        d.set_default(0);
                    }),
                )?;
                params.add(
                    Params::PolarTwist,
                    "Radial Twist",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::StripGroupStart,
            Params::StripGroupEnd,
            "Strip Repeat",
            false,
            |params| {
                params.add(
                    Params::StripAngle,
                    "Angle",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                add_distance(params, Params::StripWidth, "Width (px)", 100.0)?;
                add_repeat(params, Params::StripRepeat)?;
                params.add_with_flags(
                    Params::StripClipping,
                    "Clipping",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Both Edges",
                            "Negative Edge Only",
                            "Positive Edge Only",
                            "None",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ParamUIFlags::empty(),
                )?;
                add_popup(
                    params,
                    Params::StripOverlapOrder,
                    "Overlap Order",
                    &["Over", "Under"],
                    1,
                )?;
                add_transform_controls(
                    params,
                    [
                        Params::StripRotation,
                        Params::StripScaleX,
                        Params::StripScaleY,
                        Params::StripSkew,
                        Params::StripSkewAxis,
                        Params::StripAffect,
                    ],
                )?;
                add_popup(
                    params,
                    Params::StripEdgeDirection,
                    "Edge Direction",
                    &["Along Strip", "Image Axes"],
                    1,
                )?;
                params.add(
                    Params::StripDeltaAngle,
                    "Delta Angle",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                add_float(
                    params,
                    Params::StripDeltaWidth,
                    "Delta Width (px)",
                    -10000.0,
                    10000.0,
                    100.0,
                    0.0,
                )?;
                for (id, name) in [
                    (Params::StripDeltaX, "Delta X (px)"),
                    (Params::StripDeltaY, "Delta Y (px)"),
                ] {
                    add_float(params, id, name, -10000.0, 10000.0, 100.0, 0.0)?;
                }
                add_popup(
                    params,
                    Params::StripDeltaMode,
                    "Delta Mode",
                    &["Forward", "Reverse", "Outward", "Ping Pong", "Random"],
                    1,
                )?;
                add_integer(
                    params,
                    Params::StripDeltaCycle,
                    "Delta Cycle / Range",
                    1,
                    256,
                    32,
                    4,
                )?;
                add_integer(
                    params,
                    Params::StripDeltaSeed,
                    "Random Seed",
                    0,
                    100000,
                    1000,
                    0,
                )?;
                add_integer(
                    params,
                    Params::StripCopies,
                    "Copies Each Side",
                    1,
                    256,
                    64,
                    32,
                )?;
                params.add(
                    Params::StripSplitOverlaps,
                    "Split Overlaps",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::TileGroupStart,
            Params::TileGroupEnd,
            "Polygon Tile",
            false,
            |params| {
                params.add(
                    Params::TileShape,
                    "Shape",
                    PopupDef::setup(|d| {
                        d.set_options(&["Triangle", "Quadrilateral", "Hexagon"]);
                        d.set_default(2);
                    }),
                )?;
                add_distance(params, Params::TileWidth, "Tile Width (px)", 200.0)?;
                add_distance(params, Params::TileHeight, "Tile Height (px)", 200.0)?;
                add_repeat(params, Params::TileRepeat)?;
                params.add(
                    Params::TileRotation,
                    "Rotation",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                for (id, name) in [
                    (Params::TileScaleX, "Scale X (%)"),
                    (Params::TileScaleY, "Scale Y (%)"),
                ] {
                    params.add(
                        id,
                        name,
                        FloatSliderDef::setup(|d| {
                            d.set_valid_min(0.1);
                            d.set_valid_max(10_000.0);
                            d.set_slider_min(1.0);
                            d.set_slider_max(400.0);
                            d.set_default(100.0);
                            d.set_precision(2);
                        }),
                    )?;
                }
                params.add(
                    Params::TileSkew,
                    "Skew (deg)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(-85.0);
                        d.set_valid_max(85.0);
                        d.set_slider_min(-60.0);
                        d.set_slider_max(60.0);
                        d.set_default(0.0);
                        d.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::TileSkewAxis,
                    "Skew Axis",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                add_float(
                    params,
                    Params::TileAffect,
                    "Image Affect (%)",
                    0.0,
                    100.0,
                    100.0,
                    100.0,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::TimeGroupStart,
            Params::TimeGroupEnd,
            "Time Offset",
            false,
            |params| {
                add_float(
                    params,
                    Params::TimeDelta,
                    "Time Delta (frames)",
                    -10000.0,
                    10000.0,
                    30.0,
                    0.0,
                )?;
                params.add(
                    Params::TimeOrigin,
                    "Time Origin (i = 0)",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                add_popup(
                    params,
                    Params::TimeIndexAxis,
                    "Tile Time Direction",
                    &["From Origin", "X", "Y"],
                    1,
                )?;
                add_integer(
                    params,
                    Params::TimeIndexOffset,
                    "Time Index Offset",
                    -10000,
                    10000,
                    32,
                    0,
                )?;
                add_popup(
                    params,
                    Params::TimeEdge,
                    "Time Edge",
                    &["Transparent", "Extend", "Loop", "Ping Pong"],
                    2,
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
                        "AOD_ImagePattern - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::WideTimeInput, true);
                out_data.set_out_flag2(OutFlags2::AutomaticWideTimeInput, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImagePattern")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => self.do_render(
                in_data,
                in_layer,
                out_layer,
                params,
                RenderLayout::default(),
                &mut |time| {
                    let checkout = ParamDef::checkout(
                        in_data,
                        0,
                        time,
                        in_data.time_step(),
                        in_data.time_scale(),
                        Some(ParamType::Layer),
                    )?;
                    let layer = checkout.as_layer()?.value();
                    let origin = in_data.pre_effect_source_origin();
                    Ok(layer.as_ref().map(|layer| {
                        SourceFrame::read(layer, None, (-(origin.h as i64), -(origin.v as i64)))
                    }))
                },
            )?,
            ae::Command::SmartPreRender { mut extra } => {
                let mut request = extra.output_request();
                request.preserve_rgb_of_zero_alpha = 0;
                let callbacks = extra.callbacks();
                let query = callbacks.checkout_layer(
                    0,
                    1_000_000_000,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let mut full_request = request;
                full_request.rect = query.max_result_rect;
                let input = callbacks.checkout_layer(
                    0,
                    0,
                    &full_request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let full_rect: ae::Rect = input.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);
                let bounds = BufferBounds::new(
                    (full_rect.left as i64, full_rect.top as i64),
                    full_rect.width().max(0) as usize,
                    full_rect.height().max(0) as usize,
                );
                let settings = Self::read_settings(params, in_data, bounds)?;
                let times = required_times(in_data, &settings, bounds, bounds)?;
                let mut frames = Vec::with_capacity(times.len());
                for (n, time) in times.into_iter().enumerate() {
                    in_data.interact().abort()?;
                    let id = n as i32 + 1;
                    let query = callbacks.checkout_layer(
                        0,
                        1_000_000_000 + id,
                        &request,
                        time,
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                    let mut frame_request = full_request;
                    frame_request.rect = query.max_result_rect;
                    let input = callbacks.checkout_layer(
                        0,
                        id,
                        &frame_request,
                        time,
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                    frames.push(FrameRequest {
                        time,
                        id,
                        rect: input.max_result_rect.into(),
                    });
                }
                extra.set_pre_render_data(PreRenderData {
                    layout: RenderLayout {
                        input_rect: Some(full_rect),
                        output_rect: Some(full_rect),
                    },
                    frames,
                });
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let data = extra
                    .pre_render_data::<PreRenderData>()
                    .cloned()
                    .unwrap_or_default();
                let mut used = vec![false; data.frames.len()];
                let in_layer = cb.checkout_layer_pixels(0)?;
                let render_result: Result<(), Error> = (|| {
                    let out_layer = cb.checkout_output()?;
                    if let (Some(in_layer), Some(out_layer)) = (in_layer, out_layer) {
                        self.do_render(
                            in_data,
                            in_layer,
                            out_layer,
                            params,
                            data.layout,
                            &mut |time| {
                                let n = data
                                    .frames
                                    .iter()
                                    .enumerate()
                                    .position(|(n, frame)| frame.time == time && !used[n])
                                    .ok_or(Error::InvalidIndex)?;
                                used[n] = true;
                                let frame = &data.frames[n];
                                let layer = cb.checkout_layer_pixels(frame.id as u32)?;
                                let source = layer.as_ref().map(|layer| {
                                    SourceFrame::read(
                                        layer,
                                        Some(frame.rect),
                                        (frame.rect.left as i64, frame.rect.top as i64),
                                    )
                                });
                                cb.checkin_layer_pixels(frame.id as u32)?;
                                Ok(source)
                            },
                        )?;
                    }
                    Ok(())
                })();
                let checkin_result = cb.checkin_layer_pixels(0);
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let changed = params.type_at(param_index);
                if matches!(changed, Params::Mode | Params::StripClipping) {
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
        let mode = mode_from_popup(params.get(Params::Mode)?.as_popup()?.value());
        self.set_param_visible(in_data, params, Params::Center, true)?;

        self.set_many_visible(
            in_data,
            params,
            &FEEDBACK_PARAMS,
            matches!(mode, Mode::Feedback),
        )?;
        self.set_many_visible(in_data, params, &POLAR_PARAMS, matches!(mode, Mode::Polar))?;
        self.set_many_visible(in_data, params, STRIP_PARAMS, matches!(mode, Mode::Strip))?;
        let overlap = params.get(Params::StripClipping)?.as_popup()?.value() != 1;
        self.set_param_visible(
            in_data,
            params,
            Params::StripOverlapOrder,
            mode == Mode::Strip && overlap,
        )?;
        self.set_many_visible(in_data, params, TILE_PARAMS, matches!(mode, Mode::Tile))?;
        self.set_param_visible(in_data, params, Params::TimeIndexAxis, mode == Mode::Tile)?;
        self.set_param_visible(in_data, params, Params::TimeOrigin, mode != Mode::Feedback)?;
        Ok(())
    }

    fn set_many_visible(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
        ids: &[Params],
        visible: bool,
    ) -> Result<(), Error> {
        for id in ids {
            self.set_param_visible(in_data, params, *id, visible)?;
        }
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
            return Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible);
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
        Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_ui_flag(
        params: &mut Parameters<Params>,
        id: Params,
        flag: ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let current = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current == status {
            return Ok(());
        }
        let mut param = params.get_mut(id)?;
        param.set_ui_flag(flag, status);
        param.update_param_ui()?;
        Ok(())
    }

    fn read_settings(
        params: &mut Parameters<Params>,
        in_data: InData,
        source_bounds: BufferBounds,
    ) -> Result<RenderSettings, Error> {
        let downsample = Vec2::new(
            positive_finite(f32::from(in_data.downsample_x()), 1.0),
            positive_finite(f32::from(in_data.downsample_y()), 1.0),
        );
        let full_width = if in_data.width() > 0 {
            in_data.width() as f32
        } else {
            source_bounds.width as f32 / downsample.x
        };
        let full_height = if in_data.height() > 0 {
            in_data.height() as f32
        } else {
            source_bounds.height as f32 / downsample.y
        };
        let pre_effect_origin = in_data.pre_effect_source_origin();
        let space = PatternSpace::new(
            Vec2::new(pre_effect_origin.h as f32, pre_effect_origin.v as f32),
            downsample,
            f32::from(in_data.pixel_aspect_ratio()),
            full_width,
            full_height,
        );
        let (center_x, center_y) = params.get(Params::Center)?.as_point()?.value();
        let center = space.layer_point_to_pattern(
            Vec2::new(center_x, center_y),
            Vec2::new(source_bounds.origin_x as f32, source_bounds.origin_y as f32),
        );
        let (time_x, time_y) = params.get(Params::TimeOrigin)?.as_point()?.value();
        let time_origin = space.layer_point_to_pattern(
            Vec2::new(time_x, time_y),
            Vec2::new(source_bounds.origin_x as f32, source_bounds.origin_y as f32),
        );
        let feedback_offset = space.pixel_offset_to_pattern(Vec2::new(
            params
                .get(Params::FeedbackOffsetX)?
                .as_float_slider()?
                .value() as f32,
            params
                .get(Params::FeedbackOffsetY)?
                .as_float_slider()?
                .value() as f32,
        ));
        Ok(RenderSettings {
            mode: mode_from_popup(params.get(Params::Mode)?.as_popup()?.value()),
            space,
            center,
            mix: (params.get(Params::Mix)?.as_float_slider()?.value() as f32 / 100.0)
                .clamp(0.0, 1.0),
            sampling: sampling_from_popup(params.get(Params::Sampling)?.as_popup()?.value()),
            edge: image_edge_from_popup(params.get(Params::Edge)?.as_popup()?.value()),
            feedback_iterations: params
                .get(Params::FeedbackIterations)?
                .as_slider()?
                .value()
                .clamp(1, 16) as usize,
            feedback_decay: (params
                .get(Params::FeedbackDecay)?
                .as_float_slider()?
                .value() as f32)
                .clamp(0.0, 1.0),
            feedback_opacity: (params
                .get(Params::FeedbackOpacity)?
                .as_float_slider()?
                .value() as f32
                / 100.0)
                .clamp(0.0, 1.0),
            feedback_composite_order: feedback_composite_order_from_popup(
                params
                    .get(Params::FeedbackCompositeOrder)?
                    .as_popup()?
                    .value(),
            ),
            feedback: FeedbackSettings {
                step_scale: params
                    .get(Params::FeedbackStepScale)?
                    .as_float_slider()?
                    .value() as f32,
                step_rotation: angle_radians(params, Params::FeedbackStepRotation)?,
                step_offset: feedback_offset,
                vortex: angle_radians(params, Params::FeedbackVortex)?,
            },
            polar: PolarSettings {
                sectors: params
                    .get(Params::PolarSectors)?
                    .as_slider()?
                    .value()
                    .clamp(1, 128) as usize,
                mirror: params.get(Params::PolarMirror)?.as_checkbox()?.value(),
                rings: params
                    .get(Params::PolarRings)?
                    .as_slider()?
                    .value()
                    .clamp(0, 64) as usize,
                twist: angle_radians(params, Params::PolarTwist)?,
            },
            strip: StripMapping::with_controls(
                angle_radians(params, Params::StripAngle)?,
                params.get(Params::StripWidth)?.as_float_slider()?.value() as f32,
                repeat_from_popup(params.get(Params::StripRepeat)?.as_popup()?.value()),
                read_transform(
                    params,
                    [
                        Params::StripRotation,
                        Params::StripScaleX,
                        Params::StripScaleY,
                        Params::StripSkew,
                        Params::StripSkewAxis,
                    ],
                )?,
                float_value(params, Params::StripAffect)? / 100.0,
                StripDelta {
                    angle: angle_radians(params, Params::StripDeltaAngle)?,
                    width: float_value(params, Params::StripDeltaWidth)?,
                    offset: Vec2::new(
                        float_value(params, Params::StripDeltaX)?,
                        float_value(params, Params::StripDeltaY)?,
                    ),
                    mode: match params.get(Params::StripDeltaMode)?.as_popup()?.value() {
                        2 => DeltaMode::Reverse,
                        3 => DeltaMode::Outward,
                        4 => DeltaMode::PingPong,
                        5 => DeltaMode::Random,
                        _ => DeltaMode::Forward,
                    },
                    cycle: params.get(Params::StripDeltaCycle)?.as_slider()?.value(),
                    seed: params.get(Params::StripDeltaSeed)?.as_slider()?.value() as u32,
                    copies: params.get(Params::StripCopies)?.as_slider()?.value(),
                    split_overlaps: params
                        .get(Params::StripSplitOverlaps)?
                        .as_checkbox()?
                        .value(),
                },
            ),
            strip_edge_along: params.get(Params::StripEdgeDirection)?.as_popup()?.value() == 1,
            strip_clipping: strip_clipping_from_popup(
                params.get(Params::StripClipping)?.as_popup()?.value(),
            ),
            strip_overlap_order: if params.get(Params::StripOverlapOrder)?.as_popup()?.value() == 2
            {
                StripOverlapOrder::Under
            } else {
                StripOverlapOrder::Over
            },
            time: TimeSettings {
                delta: float_value(params, Params::TimeDelta)?,
                origin: time_origin,
                axis: match params.get(Params::TimeIndexAxis)?.as_popup()?.value() {
                    2 => IndexAxis::X,
                    3 => IndexAxis::Y,
                    _ => IndexAxis::Distance,
                },
                offset: params.get(Params::TimeIndexOffset)?.as_slider()?.value(),
                edge: edge_from_popup(params.get(Params::TimeEdge)?.as_popup()?.value()),
            },
            tile: TileMapping::new(
                tile_shape_from_popup(params.get(Params::TileShape)?.as_popup()?.value()),
                Vec2::new(
                    params.get(Params::TileWidth)?.as_float_slider()?.value() as f32,
                    params.get(Params::TileHeight)?.as_float_slider()?.value() as f32,
                ),
                repeat_from_popup(params.get(Params::TileRepeat)?.as_popup()?.value()),
                Transform {
                    rotation: angle_radians(params, Params::TileRotation)?,
                    scale: Vec2::new(
                        params.get(Params::TileScaleX)?.as_float_slider()?.value() as f32 / 100.0,
                        params.get(Params::TileScaleY)?.as_float_slider()?.value() as f32 / 100.0,
                    ),
                    skew: (params.get(Params::TileSkew)?.as_float_slider()?.value() as f32)
                        .to_radians(),
                    skew_axis: angle_radians(params, Params::TileSkewAxis)?,
                },
                float_value(params, Params::TileAffect)? / 100.0,
            ),
        })
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
        layout: RenderLayout,
        checkout: &mut dyn FnMut(i32) -> Result<Option<SourceFrame>, Error>,
    ) -> Result<(), Error> {
        let source_width = in_layer.width();
        let source_height = in_layer.height();
        let output_width = out_layer.width();
        let output_height = out_layer.height();
        if source_width == 0 || source_height == 0 || output_width == 0 || output_height == 0 {
            return Ok(());
        }

        let out_world_type = out_layer.world_type();
        let (source_bounds, output_bounds) =
            render_buffer_bounds(in_data, &in_layer, &out_layer, layout);
        let settings = Self::read_settings(params, in_data, source_bounds)?;
        let max_radius = settings.space.max_radius();
        let current = SourceFrame::read(
            &in_layer,
            layout.input_rect,
            (source_bounds.origin_x, source_bounds.origin_y),
        );
        let temporal_pixels = if settings.time.delta != 0.0 && settings.mix != 0.0 {
            Some(render_temporal(
                in_data,
                &current,
                output_bounds,
                &settings,
                checkout,
            )?)
        } else {
            None
        };
        let source = &current.pixels;

        out_layer.iterate(0, output_height as i32, None, |x, y, mut destination| {
            let layer_coordinate = output_bounds.layer_coordinate(x as usize, y as usize);
            let source_x = layer_coordinate.0 - source_bounds.origin_x;
            let source_y = layer_coordinate.1 - source_bounds.origin_y;
            let source_point = Vec2::new(source_x as f32, source_y as f32);
            let point = settings.space.buffer_point_to_pattern(source_point);
            let original = source_bounds
                .local_coordinate(layer_coordinate.0, layer_coordinate.1)
                .map_or(TRANSPARENT, |(source_x, source_y)| {
                    source[source_y * source_width + source_x]
                });
            let transformed = if let Some(ref temporal) = temporal_pixels {
                temporal[y as usize * output_width + x as usize]
            } else {
                match settings.mode {
                    Mode::Feedback => feedback_pixel(
                        source,
                        source_width,
                        source_height,
                        point,
                        max_radius,
                        &settings,
                    ),
                    Mode::Polar => {
                        let coordinate =
                            polar_coordinate(point, settings.center, settings.polar, max_radius);
                        sample_pixel(source, source_width, source_height, coordinate, &settings)
                    }
                    Mode::Strip => {
                        strip_pixel(source, source_width, source_height, point, &settings)
                    }
                    Mode::Tile => sample_pixel(
                        source,
                        source_width,
                        source_height,
                        settings.tile.coordinate(point, settings.center),
                        &settings,
                    ),
                }
            };
            let output = sanitize_pixel(lerp_pixel(original, transformed, settings.mix), false);
            write_pixel(&mut destination, out_world_type, output);
            Ok(())
        })?;

        Ok(())
    }
}

fn render_buffer_bounds(
    in_data: InData,
    in_layer: &Layer,
    out_layer: &Layer,
    layout: RenderLayout,
) -> (BufferBounds, BufferBounds) {
    let pre_effect_origin = in_data.pre_effect_source_origin();
    let source_fallback = (-(pre_effect_origin.h as i64), -(pre_effect_origin.v as i64));
    let source_origin = layer_buffer_origin(in_layer, layout.input_rect, source_fallback);

    // output_origin is where input buffer (0, 0) is placed inside output.
    // Converting output-local pixels to layer space therefore uses the inverse
    // shift from the input buffer's layer-space origin.
    let output_shift = in_data.output_origin();
    let output_fallback = (
        source_origin.0 - output_shift.h as i64,
        source_origin.1 - output_shift.v as i64,
    );
    let output_origin = layer_buffer_origin(out_layer, layout.output_rect, output_fallback);

    (
        BufferBounds::new(source_origin, in_layer.width(), in_layer.height()),
        BufferBounds::new(output_origin, out_layer.width(), out_layer.height()),
    )
}

fn layer_buffer_origin(
    layer: &Layer,
    requested_rect: Option<ae::Rect>,
    fallback: (i64, i64),
) -> (i64, i64) {
    let native = layer.origin();
    if native.h != 0 || native.v != 0 {
        return (native.h as i64, native.v as i64);
    }
    if let Some(rect) = requested_rect
        && rect.width() == layer.width() as i32
        && rect.height() == layer.height() as i32
    {
        return (rect.left as i64, rect.top as i64);
    }
    fallback
}

fn positive_finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value.abs() > 1.0e-6 {
        value.abs()
    } else {
        fallback
    }
}

fn mode_from_popup(value: i32) -> Mode {
    match value {
        2 => Mode::Polar,
        3 => Mode::Strip,
        4 => Mode::Tile,
        _ => Mode::Feedback,
    }
}

fn sampling_from_popup(value: i32) -> Sampling {
    if value == 1 {
        Sampling::Nearest
    } else {
        Sampling::Bilinear
    }
}

fn edge_from_popup(value: i32) -> SampleEdge {
    match value {
        2 => SampleEdge::Clamp,
        3 => SampleEdge::Tile,
        4 => SampleEdge::Mirror,
        _ => SampleEdge::Transparent,
    }
}

fn image_edge_from_popup(value: i32) -> ImageEdge {
    if value == 5 {
        ImageEdge::Black
    } else {
        ImageEdge::Standard(edge_from_popup(value))
    }
}

fn feedback_composite_order_from_popup(value: i32) -> FeedbackCompositeOrder {
    if value == 2 {
        FeedbackCompositeOrder::BackToFront
    } else {
        FeedbackCompositeOrder::FrontToBack
    }
}

fn repeat_from_popup(value: i32) -> Repeat {
    if value == 2 {
        Repeat::Mirror
    } else {
        Repeat::Tile
    }
}

fn strip_clipping_from_popup(value: i32) -> StripClipping {
    match value {
        2 => StripClipping::NegativeEdge,
        3 => StripClipping::PositiveEdge,
        4 => StripClipping::None,
        _ => StripClipping::Both,
    }
}

fn tile_shape_from_popup(value: i32) -> TileShape {
    match value {
        1 => TileShape::Triangle,
        3 => TileShape::Hexagon,
        _ => TileShape::Quadrilateral,
    }
}

fn add_distance(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    default: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(1.0);
            d.set_valid_max(100_000.0);
            d.set_slider_min(1.0);
            d.set_slider_max(1000.0);
            d.set_default(default);
            d.set_precision(2);
        }),
    )?;
    Ok(())
}

fn add_repeat(params: &mut Parameters<Params>, id: Params) -> Result<(), Error> {
    params.add(
        id,
        "Repeat",
        PopupDef::setup(|d| {
            d.set_options(&["Tile", "Mirror"]);
            d.set_default(1);
        }),
    )?;
    Ok(())
}

fn add_popup(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    options: &[&str],
    default: i32,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        PopupDef::setup(|d| {
            d.set_options(options);
            d.set_default(default);
        }),
    )?;
    Ok(())
}

fn add_integer(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: i32,
    max: i32,
    slider_max: i32,
    default: i32,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        SliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(min.max(-slider_max));
            d.set_slider_max(slider_max);
            d.set_default(default);
        }),
    )?;
    Ok(())
}

fn add_transform_controls(params: &mut Parameters<Params>, ids: [Params; 6]) -> Result<(), Error> {
    params.add(
        ids[0],
        "Strip Rotation",
        AngleDef::setup(|d| {
            d.set_default(0.0);
        }),
    )?;
    add_float(
        params,
        ids[1],
        "Strip Scale X (%)",
        0.1,
        10000.0,
        400.0,
        100.0,
    )?;
    add_float(
        params,
        ids[2],
        "Strip Scale Y (%)",
        0.1,
        10000.0,
        400.0,
        100.0,
    )?;
    add_float(params, ids[3], "Strip Skew (deg)", -85.0, 85.0, 60.0, 0.0)?;
    params.add(
        ids[4],
        "Strip Skew Axis",
        AngleDef::setup(|d| {
            d.set_default(0.0);
        }),
    )?;
    add_float(
        params,
        ids[5],
        "Strip Image Affect (%)",
        0.0,
        100.0,
        100.0,
        100.0,
    )
}

fn read_transform(params: &mut Parameters<Params>, ids: [Params; 5]) -> Result<Transform, Error> {
    Ok(Transform {
        rotation: angle_radians(params, ids[0])?,
        scale: Vec2::new(
            float_value(params, ids[1])? / 100.0,
            float_value(params, ids[2])? / 100.0,
        ),
        skew: float_value(params, ids[3])?.to_radians(),
        skew_axis: angle_radians(params, ids[4])?,
    })
}

fn add_float(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    slider_max: f32,
    default: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(min.max(-slider_max));
            d.set_slider_max(slider_max);
            d.set_default(default);
            d.set_precision(2);
        }),
    )?;
    Ok(())
}

fn float_value(params: &mut Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok(params.get(id)?.as_float_slider()?.value() as f32)
}

fn angle_radians(params: &mut Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok((params.get(id)?.as_angle()?.float_value()? as f32).to_radians())
}

fn normalized_sample_coordinate(value: f32, length: usize, edge: SampleEdge) -> Option<f32> {
    if length == 0 || !value.is_finite() {
        return None;
    }
    let normalized = match edge {
        SampleEdge::Transparent => {
            if value <= -1.0 || value >= length as f32 {
                return None;
            }
            value
        }
        SampleEdge::Clamp => value.clamp(0.0, length.saturating_sub(1) as f32),
        SampleEdge::Tile => value.rem_euclid(length as f32),
        SampleEdge::Mirror => {
            if length == 1 {
                0.0
            } else {
                let period = length.saturating_mul(2).saturating_sub(2);
                value.rem_euclid(period as f32)
            }
        }
    };
    normalized.is_finite().then_some(normalized)
}

fn sample_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    coordinate: Vec2,
    settings: &RenderSettings,
) -> ae::PixelF32 {
    sample_buffer(
        source,
        width,
        height,
        settings.space.pattern_point_to_buffer(coordinate),
        None,
        settings.sampling,
        settings.edge,
    )
}

fn strip_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    point: Vec2,
    settings: &RenderSettings,
) -> ae::PixelF32 {
    let mut pixel = TRANSPARENT;
    settings
        .strip
        .visit_samples(point, settings.center, settings.strip_clipping, |sample| {
            let color = sample_strip_pixel(
                source,
                width,
                height,
                sample.coordinate,
                sample.direction,
                settings,
            );
            pixel = if settings.strip_clipping == StripClipping::Both {
                color
            } else {
                composite_strip_sample(pixel, color, settings.strip_overlap_order)
            };
        });
    pixel
}

fn composite_strip_sample(
    destination: ae::PixelF32,
    source: ae::PixelF32,
    order: StripOverlapOrder,
) -> ae::PixelF32 {
    let (back, front) = match order {
        StripOverlapOrder::Over => (destination, source),
        StripOverlapOrder::Under => (source, destination),
    };
    let back = prepare_source_pixel(back);
    let front = prepare_source_pixel(front);
    let back_weight = back.alpha * (1.0 - front.alpha);
    let alpha = front.alpha + back_weight;
    if alpha <= 1.0e-6 {
        return TRANSPARENT;
    }
    // Match native AE layer compositing: weight straight sample RGB by alpha
    // and return straight RGB, avoiding a second opacity boost at output.
    ae::PixelF32 {
        alpha,
        red: (front.red * front.alpha + back.red * back_weight) / alpha,
        green: (front.green * front.alpha + back.green * back_weight) / alpha,
        blue: (front.blue * front.alpha + back.blue * back_weight) / alpha,
    }
}

fn sample_strip_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    coordinate: Vec2,
    direction: Vec2,
    settings: &RenderSettings,
) -> ae::PixelF32 {
    let direction = settings.strip_edge_along.then_some(Vec2::new(
        direction.x * settings.space.pattern_to_buffer.x,
        direction.y * settings.space.pattern_to_buffer.y,
    ));
    sample_buffer(
        source,
        width,
        height,
        settings.space.pattern_point_to_buffer(coordinate),
        direction,
        settings.sampling,
        settings.edge,
    )
}

// Move only along the band tangent. Independent XY clamping would bend the
// extension toward corners and destroy the cross-strip coordinate.
fn along_strip_edge(
    point: Vec2,
    direction: Vec2,
    width: usize,
    height: usize,
    edge: SampleEdge,
) -> Option<Vec2> {
    if width == 0 || height == 0 || !point.x.is_finite() || !point.y.is_finite() {
        return None;
    }
    if matches!(edge, SampleEdge::Transparent) {
        return Some(point);
    }
    let length = direction.x.hypot(direction.y);
    if !length.is_finite() || length < 1.0e-8 {
        return None;
    }
    let direction = direction / length;
    let mut low = f32::NEG_INFINITY;
    let mut high = f32::INFINITY;
    for (p, d, maximum) in [
        (point.x, direction.x, (width - 1) as f32),
        (point.y, direction.y, (height - 1) as f32),
    ] {
        if d.abs() < 1.0e-6 {
            if p < 0.0 || p > maximum {
                return None;
            }
        } else {
            let a = -p / d;
            let b = (maximum - p) / d;
            low = low.max(a.min(b));
            high = high.min(a.max(b));
        }
    }
    if low > high {
        return None;
    }
    if low <= 0.0 && high >= 0.0 {
        return Some(point);
    }
    let extent = high - low;
    let shift = match edge {
        SampleEdge::Clamp => 0.0_f32.clamp(low, high),
        _ if extent < 1.0e-5 => low,
        SampleEdge::Tile => low + (-low).rem_euclid(extent),
        SampleEdge::Mirror => {
            let phase = (-low).rem_euclid(extent * 2.0);
            low + phase.min(extent * 2.0 - phase)
        }
        SampleEdge::Transparent => 0.0,
    };
    Some(point + direction * shift)
}

fn sample_black_edge(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    coordinate: Vec2,
    sampling: Sampling,
) -> ae::PixelF32 {
    if width == 0 || height == 0 || !coordinate.x.is_finite() || !coordinate.y.is_finite() {
        return TRANSPARENT;
    }
    let black = ae::PixelF32 {
        alpha: 1.0,
        ..TRANSPARENT
    };
    let (Some(x), Some(y)) = (
        normalized_sample_coordinate(coordinate.x, width, SampleEdge::Transparent),
        normalized_sample_coordinate(coordinate.y, height, SampleEdge::Transparent),
    ) else {
        return black;
    };
    match sampling {
        Sampling::Nearest => {
            if x.round() < 0.0
                || y.round() < 0.0
                || x.round() >= width as f32
                || y.round() >= height as f32
            {
                black
            } else {
                sample_nearest(source, width, height, x, y, SampleEdge::Transparent)
            }
        }
        Sampling::Bilinear => {
            let mut pixel = sample_bilinear(source, width, height, x, y, SampleEdge::Transparent);
            // Black padding changes only out-of-bounds taps, not transparent
            // pixels inside the source. Preserve the same RGB interpolation.
            let coverage_x = (x + 1.0).clamp(0.0, 1.0) * (width as f32 - x).clamp(0.0, 1.0);
            let coverage_y = (y + 1.0).clamp(0.0, 1.0) * (height as f32 - y).clamp(0.0, 1.0);
            let outside = 1.0 - coverage_x * coverage_y;
            pixel.alpha = (pixel.alpha + outside).clamp(0.0, 1.0);
            pixel
        }
    }
}

fn sample_buffer(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    coordinate: Vec2,
    direction: Option<Vec2>,
    sampling: Sampling,
    edge: ImageEdge,
) -> ae::PixelF32 {
    let ImageEdge::Standard(edge) = edge else {
        return sample_black_edge(source, width, height, coordinate, sampling);
    };
    let (coordinate, edge) = if let Some(direction) = direction {
        let Some(coordinate) = along_strip_edge(coordinate, direction, width, height, edge) else {
            return TRANSPARENT;
        };
        (
            coordinate,
            if matches!(edge, SampleEdge::Transparent) {
                edge
            } else {
                SampleEdge::Clamp
            },
        )
    } else {
        (coordinate, edge)
    };
    let Some(x) = normalized_sample_coordinate(coordinate.x, width, edge) else {
        return TRANSPARENT;
    };
    let Some(y) = normalized_sample_coordinate(coordinate.y, height, edge) else {
        return TRANSPARENT;
    };
    match sampling {
        Sampling::Nearest => sample_nearest(source, width, height, x, y, edge),
        Sampling::Bilinear => sample_bilinear(source, width, height, x, y, edge),
    }
}

fn feedback_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    point: Vec2,
    max_radius: f32,
    settings: &RenderSettings,
) -> ae::PixelF32 {
    let mut accumulated = ae::PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    };
    for stack_index in 0..settings.feedback_iterations {
        let iteration = feedback_iteration(
            stack_index,
            settings.feedback_iterations,
            settings.feedback_composite_order,
        );
        let coordinate = feedback_coordinate(
            point,
            settings.center,
            iteration,
            settings.feedback,
            max_radius,
        );
        let sample = sample_pixel(source, width, height, coordinate, settings);
        let weight = feedback_tap_weight(
            iteration,
            settings.feedback_decay,
            settings.feedback_opacity,
        );
        accumulated = composite_feedback_sample(accumulated, sample, weight);
    }
    accumulated
}

fn feedback_tap_weight(iteration: usize, decay: f32, opacity: f32) -> f32 {
    if iteration == 0 {
        return 1.0;
    }
    let decay = if decay.is_finite() { decay } else { 0.0 }.clamp(0.0, 1.0);
    let opacity = if opacity.is_finite() { opacity } else { 0.0 }.clamp(0.0, 1.0);
    opacity * decay.powi(iteration.saturating_sub(1).min(i32::MAX as usize) as i32)
}

fn feedback_iteration(
    stack_index: usize,
    iteration_count: usize,
    order: FeedbackCompositeOrder,
) -> usize {
    match order {
        FeedbackCompositeOrder::FrontToBack => stack_index,
        FeedbackCompositeOrder::BackToFront => iteration_count.saturating_sub(1 + stack_index),
    }
}

fn prepare_source_pixel(pixel: ae::PixelF32) -> ae::PixelF32 {
    let alpha = if pixel.alpha.is_finite() {
        pixel.alpha
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    if alpha <= 1.0e-6 {
        return TRANSPARENT;
    }
    ae::PixelF32 {
        alpha,
        red: if pixel.red.is_finite() {
            pixel.red
        } else {
            0.0
        },
        green: if pixel.green.is_finite() {
            pixel.green
        } else {
            0.0
        },
        blue: if pixel.blue.is_finite() {
            pixel.blue
        } else {
            0.0
        },
    }
}

fn composite_feedback_sample(
    destination: ae::PixelF32,
    source: ae::PixelF32,
    opacity: f32,
) -> ae::PixelF32 {
    let opacity = if opacity.is_finite() { opacity } else { 0.0 }.clamp(0.0, 1.0);
    let source = prepare_source_pixel(source);
    let source_alpha = source.alpha * opacity;
    if source_alpha <= 1.0e-6 {
        return prepare_source_pixel(destination);
    }
    let destination = prepare_source_pixel(destination);
    let destination_weight = 1.0 - source_alpha;
    ae::PixelF32 {
        alpha: source_alpha + destination.alpha * destination_weight,
        red: source.red * opacity + destination.red * destination_weight,
        green: source.green * opacity + destination.green * destination_weight,
        blue: source.blue * opacity + destination.blue * destination_weight,
    }
}

fn write_pixel(
    destination: &mut ae::GenericPixelMut<'_>,
    world_type: ae::aegp::WorldType,
    pixel: ae::PixelF32,
) {
    match world_type {
        ae::aegp::WorldType::U8 => destination.set_from_u8(ae::Pixel8 {
            red: integer_channel(pixel.red, ae::MAX_CHANNEL8) as u8,
            green: integer_channel(pixel.green, ae::MAX_CHANNEL8) as u8,
            blue: integer_channel(pixel.blue, ae::MAX_CHANNEL8) as u8,
            alpha: integer_channel(pixel.alpha, ae::MAX_CHANNEL8) as u8,
        }),
        ae::aegp::WorldType::U15 => destination.set_from_u16(ae::Pixel16 {
            red: integer_channel(pixel.red, ae::MAX_CHANNEL16) as u16,
            green: integer_channel(pixel.green, ae::MAX_CHANNEL16) as u16,
            blue: integer_channel(pixel.blue, ae::MAX_CHANNEL16) as u16,
            alpha: integer_channel(pixel.alpha, ae::MAX_CHANNEL16) as u16,
        }),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => destination.set_from_f32(pixel),
    }
}

fn integer_channel(value: f32, maximum: u32) -> u32 {
    (if value.is_finite() { value } else { 0.0 }.clamp(0.0, 1.0) * maximum as f32).round() as u32
}

#[cfg(test)]
mod host_tests {
    #[test]
    fn strip_over_and_under_weight_straight_color_by_alpha_and_reverse_stack_order() {
        use super::*;
        let red = ae::PixelF32 {
            alpha: 0.5,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        };
        let blue = ae::PixelF32 {
            alpha: 0.5,
            red: 0.0,
            green: 0.0,
            blue: 1.0,
        };
        for (order, expected_red, expected_blue) in [
            (StripOverlapOrder::Over, 1.0 / 3.0, 2.0 / 3.0),
            (StripOverlapOrder::Under, 2.0 / 3.0, 1.0 / 3.0),
        ] {
            let pixel = composite_strip_sample(red, blue, order);
            assert_eq!(
                (pixel.alpha, pixel.red, pixel.green, pixel.blue),
                (0.75, expected_red, 0.0, expected_blue)
            );
            let transparent = ae::PixelF32 {
                alpha: 0.0,
                red: 100.0,
                green: 100.0,
                blue: 100.0,
            };
            let pixel = composite_strip_sample(red, transparent, order);
            assert_eq!(
                (pixel.alpha, pixel.red, pixel.green, pixel.blue),
                (0.5, 1.0, 0.0, 0.0)
            );
        }
    }
    use super::*;

    fn assert_vec_near(actual: Vec2, expected: Vec2) {
        assert!((actual.x - expected.x).abs() < 1.0e-4, "x: {actual:?}");
        assert!((actual.y - expected.y).abs() < 1.0e-4, "y: {actual:?}");
    }

    fn assert_pixel_near(actual: ae::PixelF32, expected: ae::PixelF32) {
        assert!((actual.alpha - expected.alpha).abs() < 1.0e-6);
        assert!((actual.red - expected.red).abs() < 1.0e-6);
        assert!((actual.green - expected.green).abs() < 1.0e-6);
        assert!((actual.blue - expected.blue).abs() < 1.0e-6);
    }

    #[test]
    fn buffer_bounds_align_negative_and_nonzero_origins() {
        let source = BufferBounds::new((-12, 7), 100, 80);
        let output = BufferBounds::new((-20, 4), 120, 90);
        let layer_coordinate = output.layer_coordinate(8, 3);

        assert_eq!(layer_coordinate, (-12, 7));
        assert_eq!(source.local_coordinate(-12, 7), Some((0, 0)));
        assert_eq!(source.local_coordinate(87, 86), Some((99, 79)));
        assert_eq!(source.local_coordinate(-13, 7), None);
    }

    #[test]
    fn pattern_space_is_invariant_to_downsample_and_grow_bounds() {
        let full = PatternSpace::new(
            Vec2::new(20.0, 10.0),
            Vec2::new(1.0, 1.0),
            1.0,
            1920.0,
            1080.0,
        );
        let half = PatternSpace::new(
            Vec2::new(10.0, 5.0),
            Vec2::new(0.5, 0.5),
            1.0,
            1920.0,
            1080.0,
        );

        assert_vec_near(
            full.buffer_point_to_pattern(Vec2::new(220.0, 110.0)),
            half.buffer_point_to_pattern(Vec2::new(110.0, 55.0)),
        );
        assert_vec_near(full.layer_center(), half.layer_center());
    }

    #[test]
    fn custom_center_localizes_negative_and_positive_source_origins() {
        let unshifted = PatternSpace::new(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            1.0,
            1920.0,
            1080.0,
        );
        let negative_origin = PatternSpace::new(
            Vec2::new(20.0, 10.0),
            Vec2::new(1.0, 1.0),
            1.0,
            1920.0,
            1080.0,
        );
        let positive_origin = PatternSpace::new(
            Vec2::new(-12.0, 7.0),
            Vec2::new(1.0, 1.0),
            1.0,
            1920.0,
            1080.0,
        );
        let layer_point = Vec2::new(300.0, 200.0);
        let expected = unshifted.layer_point_to_pattern(layer_point, Vec2::new(0.0, 0.0));

        assert_vec_near(
            negative_origin.layer_point_to_pattern(layer_point, Vec2::new(-20.0, -10.0)),
            expected,
        );
        assert_vec_near(
            positive_origin.layer_point_to_pattern(layer_point, Vec2::new(12.0, -7.0)),
            expected,
        );
    }

    #[test]
    fn pattern_space_accounts_for_pixel_aspect_and_axis_downsample() {
        let space = PatternSpace::new(Vec2::new(8.0, 3.0), Vec2::new(0.5, 0.25), 2.0, 720.0, 576.0);
        let buffer_point = Vec2::new(33.0, 28.0);
        let pattern_point = space.buffer_point_to_pattern(buffer_point);
        assert_vec_near(pattern_point, Vec2::new(100.0, 100.0));
        assert_vec_near(space.pattern_point_to_buffer(pattern_point), buffer_point);
        assert_vec_near(
            space.pixel_offset_to_pattern(Vec2::new(24.0, 16.0)),
            Vec2::new(48.0, 16.0),
        );
    }

    #[test]
    fn extreme_coordinates_are_bounded_before_shared_sampling() {
        assert_eq!(
            normalized_sample_coordinate(f32::MAX, 1920, SampleEdge::Transparent),
            None
        );
        for edge in [SampleEdge::Clamp, SampleEdge::Tile, SampleEdge::Mirror] {
            let coordinate = normalized_sample_coordinate(f32::MAX, 1920, edge).unwrap();
            assert!(coordinate.is_finite());
            assert!((-1.0..3840.0).contains(&coordinate));
        }
    }

    #[test]
    fn extend_edge_clamps_to_the_nearest_boundary_pixel() {
        assert_eq!(edge_from_popup(2), SampleEdge::Clamp);
        assert_eq!(
            normalized_sample_coordinate(-50.0, 10, edge_from_popup(2)),
            Some(0.0)
        );
        assert_eq!(
            normalized_sample_coordinate(50.0, 10, edge_from_popup(2)),
            Some(9.0)
        );
    }

    #[test]
    fn image_edge_choices_preserve_existing_values_and_time_edge_semantics() {
        for (value, edge) in [
            (1, SampleEdge::Transparent),
            (2, SampleEdge::Clamp),
            (3, SampleEdge::Tile),
            (4, SampleEdge::Mirror),
        ] {
            assert_eq!(image_edge_from_popup(value), ImageEdge::Standard(edge));
            assert_eq!(edge_from_popup(value), edge);
        }
        assert_eq!(image_edge_from_popup(5), ImageEdge::Black);
    }

    #[test]
    fn black_edge_filters_match_opaque_padding_without_filling_source_transparency() {
        let black = ae::PixelF32 {
            alpha: 1.0,
            ..TRANSPARENT
        };
        let source = [
            TRANSPARENT,
            ae::PixelF32 {
                alpha: 0.25,
                red: 0.8,
                green: 0.2,
                blue: 0.0,
            },
            ae::PixelF32 {
                alpha: 1.0,
                red: 0.0,
                green: 0.4,
                blue: 1.5,
            },
            ae::PixelF32 {
                alpha: 0.7,
                red: 0.3,
                green: 0.6,
                blue: 0.9,
            },
        ];
        let mut padded = [black; 16];
        padded[5] = source[0];
        padded[6] = source[1];
        padded[9] = source[2];
        padded[10] = source[3];
        for sampling in [Sampling::Nearest, Sampling::Bilinear] {
            for y in -6..=10 {
                for x in -6..=10 {
                    let point = Vec2::new(x as f32 * 0.25, y as f32 * 0.25);
                    let expected = match sampling {
                        Sampling::Nearest => {
                            // Round in original coordinates, including negative half pixels.
                            sample_nearest(
                                &padded,
                                4,
                                4,
                                point.x.round() + 1.0,
                                point.y.round() + 1.0,
                                SampleEdge::Clamp,
                            )
                        }
                        Sampling::Bilinear => sample_bilinear(
                            &padded,
                            4,
                            4,
                            point.x + 1.0,
                            point.y + 1.0,
                            SampleEdge::Clamp,
                        ),
                    };
                    for direction in [None, Some(Vec2::new(1.0, 1.0))] {
                        assert_pixel_near(
                            sample_buffer(
                                &source,
                                2,
                                2,
                                point,
                                direction,
                                sampling,
                                ImageEdge::Black,
                            ),
                            expected,
                        );
                    }
                }
            }
            for point in [Vec2::new(f32::MAX, 0.0), Vec2::new(0.0, f32::MIN)] {
                assert_pixel_near(sample_black_edge(&source, 2, 2, point, sampling), black);
            }
            assert_pixel_near(
                sample_black_edge(&source, 2, 2, Vec2::new(f32::NAN, 0.0), sampling),
                TRANSPARENT,
            );
            assert_pixel_near(
                sample_black_edge(&[], 0, 0, Vec2::default(), sampling),
                TRANSPARENT,
            );
        }
    }

    #[test]
    fn feedback_weights_apply_opacity_then_decay() {
        assert_eq!(feedback_tap_weight(0, 0.25, 0.0), 1.0);
        assert!((feedback_tap_weight(1, 0.5, 0.8) - 0.8).abs() < 1.0e-6);
        assert!((feedback_tap_weight(2, 0.5, 0.8) - 0.4).abs() < 1.0e-6);
        assert!((feedback_tap_weight(3, 0.5, 0.8) - 0.2).abs() < 1.0e-6);
        assert_eq!(feedback_tap_weight(4, 0.5, 0.0), 0.0);
    }

    #[test]
    fn feedback_compositing_keeps_the_base_opaque_and_fades_repeats() {
        let base = ae::PixelF32 {
            alpha: 1.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let white = ae::PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
        };
        let over_opaque = composite_feedback_sample(base, white, 0.25);
        assert!((over_opaque.alpha - 1.0).abs() < 1.0e-6);
        assert!((over_opaque.red - 0.25).abs() < 1.0e-6);

        let over_transparent = composite_feedback_sample(TRANSPARENT, white, 0.25);
        assert!((over_transparent.alpha - 0.25).abs() < 1.0e-6);
        assert!((over_transparent.red - 0.25).abs() < 1.0e-6);
    }

    #[test]
    fn zero_alpha_rgb_from_set_matte_does_not_enter_feedback() {
        let hidden_rgb = ae::PixelF32 {
            alpha: 0.0,
            red: 8.0,
            green: 4.0,
            blue: 2.0,
        };
        assert_pixel_near(prepare_source_pixel(hidden_rgb), TRANSPARENT);
        assert_pixel_near(
            composite_feedback_sample(TRANSPARENT, hidden_rgb, 1.0),
            TRANSPARENT,
        );
    }

    #[test]
    fn overlapping_feedback_samples_use_source_over_instead_of_addition() {
        let half_red = ae::PixelF32 {
            alpha: 0.5,
            red: 0.5,
            green: 0.0,
            blue: 0.0,
        };
        let first = composite_feedback_sample(TRANSPARENT, half_red, 1.0);
        let second = composite_feedback_sample(first, half_red, 1.0);
        assert!((second.alpha - 0.75).abs() < 1.0e-6);
        assert!((second.red - 0.75).abs() < 1.0e-6);
        assert!(second.green.abs() < 1.0e-6);
        assert!(second.blue.abs() < 1.0e-6);
    }

    #[test]
    fn feedback_composite_order_reverses_iteration_stacking() {
        let front_to_back = (0..4)
            .map(|index| feedback_iteration(index, 4, FeedbackCompositeOrder::FrontToBack))
            .collect::<Vec<_>>();
        let back_to_front = (0..4)
            .map(|index| feedback_iteration(index, 4, FeedbackCompositeOrder::BackToFront))
            .collect::<Vec<_>>();
        assert_eq!(front_to_back, [0, 1, 2, 3]);
        assert_eq!(back_to_front, [3, 2, 1, 0]);
    }
}
