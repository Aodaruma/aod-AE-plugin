#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;
use std::f32::consts::{FRAC_PI_4, PI, TAU};

use ae::pf::*;
use utils::ToPixel;
use utils::image::{
    SampleEdge, TRANSPARENT, luma, read_layer, sample_bilinear, sample_nearest, sanitize_pixel,
};

const PLUGIN_DESCRIPTION: &str = "Converts images between Cartesian, polar, spiral, elliptic, parabolic, bipolar, Radon, and line Hough representations.";
const MAX_INTEGRAL_SAMPLE_EVALUATIONS: usize = 12_000_000;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TransformStart,
    Space,
    Direction,
    CenterMode,
    Center,
    RadiusMode,
    Radius,
    LogMinRadius,
    AngleOffset,
    TransformEnd,
    AnalysisStart,
    AngleSamples,
    DetectorSamples,
    RaySamples,
    HoughThreshold,
    WeightedHough,
    FilterRadius,
    AnalysisEnd,
    SamplingStart,
    Interpolation,
    Edge,
    OutputGain,
    OutputBias,
    ClampOutput,
    SamplingEnd,
    SpaceSpecificStart,
    SpiralTurns,
    FocusRatio,
    CoordinateExtent,
    HoughChannels,
    SpaceSpecificEnd,
    // Post-transform parameters are deliberately append-only. Reordering any
    // parameter above this point would break projects saved by earlier builds.
    PostTransformStart,
    PostAnchorPoint,
    PostPosition,
    PostSeparateScale,
    PostScale,
    PostScaleX,
    PostScaleY,
    PostRotation,
    PostSkew,
    PostSkewAxis,
    PostTransformEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Space {
    Polar,
    LogPolar,
    SquareDisc,
    Radon,
    LineHough,
    SpiralPolar,
    Elliptic,
    Parabolic,
    Bipolar,
}

impl Space {
    fn from_popup(value: i32) -> Self {
        match value {
            2 => Self::LogPolar,
            3 => Self::SquareDisc,
            4 => Self::Radon,
            5 => Self::LineHough,
            6 => Self::SpiralPolar,
            7 => Self::Elliptic,
            8 => Self::Parabolic,
            9 => Self::Bipolar,
            _ => Self::Polar,
        }
    }

    fn is_integral(self) -> bool {
        matches!(self, Self::Radon | Self::LineHough)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    Forward,
    Inverse,
}

impl Direction {
    fn from_popup(value: i32) -> Self {
        if value == 2 {
            Self::Inverse
        } else {
            Self::Forward
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CenterMode {
    LayerCenter,
    Custom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RadiusMode {
    Auto,
    Inscribed,
    Custom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Interpolation {
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HoughChannels {
    Luminance,
    Rgba,
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix2 {
    m00: f32,
    m01: f32,
    m10: f32,
    m11: f32,
}

impl Matrix2 {
    fn rotation(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            m00: cos,
            m01: -sin,
            m10: sin,
            m11: cos,
        }
    }

    const fn scale(x: f32, y: f32) -> Self {
        Self {
            m00: x,
            m01: 0.0,
            m10: 0.0,
            m11: y,
        }
    }

    fn shear_x(angle: f32) -> Self {
        Self {
            m00: 1.0,
            m01: angle.tan(),
            m10: 0.0,
            m11: 1.0,
        }
    }

    const fn mul(self, rhs: Self) -> Self {
        Self {
            m00: self.m00 * rhs.m00 + self.m01 * rhs.m10,
            m01: self.m00 * rhs.m01 + self.m01 * rhs.m11,
            m10: self.m10 * rhs.m00 + self.m11 * rhs.m10,
            m11: self.m10 * rhs.m01 + self.m11 * rhs.m11,
        }
    }

    fn transform(self, point: (f32, f32)) -> (f32, f32) {
        (
            self.m00 * point.0 + self.m01 * point.1,
            self.m10 * point.0 + self.m11 * point.1,
        )
    }

    fn inverse(self) -> Option<Self> {
        let determinant = self.m00 * self.m11 - self.m01 * self.m10;
        if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
            return None;
        }
        let reciprocal = determinant.recip();
        Some(Self {
            m00: self.m11 * reciprocal,
            m01: -self.m01 * reciprocal,
            m10: -self.m10 * reciprocal,
            m11: self.m00 * reciprocal,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct PostTransform {
    anchor: (f32, f32),
    position: (f32, f32),
    scale: (f32, f32),
    rotation: f32,
    skew: f32,
    skew_axis: f32,
}

impl PostTransform {
    fn is_identity(self) -> bool {
        self.anchor.0 == self.position.0
            && self.anchor.1 == self.position.1
            && self.scale.0 == 1.0
            && self.scale.1 == 1.0
            && self.rotation == 0.0
            && self.skew == 0.0
    }

    fn matrix(self) -> Matrix2 {
        let scale = Matrix2::scale(self.scale.0, self.scale.1);
        let skew = Matrix2::rotation(self.skew_axis)
            .mul(Matrix2::shear_x(self.skew))
            .mul(Matrix2::rotation(-self.skew_axis));
        Matrix2::rotation(self.rotation).mul(skew).mul(scale)
    }
}

impl HoughChannels {
    fn from_popup(value: i32) -> Self {
        match value {
            2 => Self::Rgba,
            3 => Self::Red,
            4 => Self::Green,
            5 => Self::Blue,
            6 => Self::Alpha,
            _ => Self::Luminance,
        }
    }

    fn is_rgba(self) -> bool {
        self == Self::Rgba
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    space: Space,
    direction: Direction,
    center_mode: CenterMode,
    center: (f32, f32),
    radius_mode: RadiusMode,
    radius: f32,
    log_min_radius: f32,
    angle_offset: f32,
    angle_samples: usize,
    detector_samples: usize,
    ray_samples: usize,
    hough_threshold: f32,
    weighted_hough: bool,
    filter_radius: usize,
    interpolation: Interpolation,
    edge: SampleEdge,
    output_gain: f32,
    output_bias: f32,
    clamp_output: bool,
    pixel_scale: (f32, f32),
    spiral_turns: f32,
    focus_ratio: f32,
    coordinate_extent: f32,
    hough_channels: HoughChannels,
    post_transform: PostTransform,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

struct LayerCheckoutGuard<const N: usize> {
    callbacks: SmartRenderCallbacks,
    checkout_ids: [u32; N],
    count: usize,
}

impl<const N: usize> LayerCheckoutGuard<N> {
    fn new(callbacks: SmartRenderCallbacks) -> Self {
        Self {
            callbacks,
            checkout_ids: [0; N],
            count: 0,
        }
    }

    fn checkout(&mut self, checkout_id: u32) -> Result<Option<Layer>, Error> {
        assert!(self.count < N, "layer checkout guard capacity exceeded");
        let layer = self.callbacks.checkout_layer_pixels(checkout_id)?;
        self.checkout_ids[self.count] = checkout_id;
        self.count += 1;
        Ok(layer)
    }

    fn checkin_all(&mut self) -> Result<(), Error> {
        let mut first_error = None;
        while self.count > 0 {
            self.count -= 1;
            if let Err(error) = self
                .callbacks
                .checkin_layer_pixels(self.checkout_ids[self.count])
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<const N: usize> Drop for LayerCheckoutGuard<N> {
    fn drop(&mut self) {
        let _ = self.checkin_all();
    }
}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let supervise = || {
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP
        };

        params.add_group(
            Params::TransformStart,
            Params::TransformEnd,
            "Transform",
            false,
            |params| {
                params.add_with_flags(
                    Params::Space,
                    "Space",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Polar",
                            "Log-Polar",
                            "Square-Disc",
                            "Radon",
                            "Line Hough",
                            "Spiral Polar",
                            "Elliptic Coordinates",
                            "Parabolic Coordinates",
                            "Bipolar Coordinates",
                        ]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ae::ParamUIFlags::empty(),
                )?;
                params.add_with_flags(
                    Params::Direction,
                    "Direction",
                    PopupDef::setup(|d| {
                        d.set_options(&["Forward", "Inverse (Approximate)"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ae::ParamUIFlags::empty(),
                )?;
                params.add_with_flags(
                    Params::CenterMode,
                    "Center",
                    PopupDef::setup(|d| {
                        d.set_options(&["Layer Center", "Custom"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::Center,
                    "Custom Center",
                    PointDef::setup(|d| {
                        d.set_default((0.0, 0.0));
                    }),
                )?;
                params.add_with_flags(
                    Params::RadiusMode,
                    "Radius",
                    PopupDef::setup(|d| {
                        d.set_options(&["Auto (Full Coverage)", "Inscribed", "Custom"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::Radius,
                    "Custom Radius",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.01);
                        d.set_valid_max(100000.0);
                        d.set_slider_min(1.0);
                        d.set_slider_max(4000.0);
                        d.set_default(500.0);
                        d.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::LogMinRadius,
                    "Log Minimum Radius",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.001);
                        d.set_valid_max(10000.0);
                        d.set_slider_min(0.1);
                        d.set_slider_max(100.0);
                        d.set_default(1.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::AngleOffset,
                    "Angle Offset",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::AnalysisStart,
            Params::AnalysisEnd,
            "Analysis Quality",
            true,
            |params| {
                add_integer_slider(
                    params,
                    Params::AngleSamples,
                    "Angle Samples",
                    8.0,
                    720.0,
                    180.0,
                )?;
                add_integer_slider(
                    params,
                    Params::DetectorSamples,
                    "Detector Samples",
                    16.0,
                    2048.0,
                    256.0,
                )?;
                add_integer_slider(
                    params,
                    Params::RaySamples,
                    "Samples per Ray",
                    8.0,
                    2048.0,
                    256.0,
                )?;
                params.add(
                    Params::HoughThreshold,
                    "Edge Threshold",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.15);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::WeightedHough,
                    "Weighted Votes",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;
                add_integer_slider(
                    params,
                    Params::FilterRadius,
                    "Ram-Lak Radius",
                    1.0,
                    127.0,
                    31.0,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::SamplingStart,
            Params::SamplingEnd,
            "Sampling / Output",
            true,
            |params| {
                params.add(
                    Params::Interpolation,
                    "Interpolation",
                    PopupDef::setup(|d| {
                        d.set_options(&["Nearest", "Bilinear"]);
                        d.set_default(2);
                    }),
                )?;
                params.add(
                    Params::Edge,
                    "Edge",
                    PopupDef::setup(|d| {
                        d.set_options(&["Transparent", "Clamp", "Tile", "Mirror"]);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::OutputGain,
                    "Output Gain",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(-100.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(4.0);
                        d.set_default(1.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::OutputBias,
                    "Output Bias",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(-100.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(-1.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::ClampOutput,
                    "Clamp Output",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;
                Ok(())
            },
        )?;

        // Appended after the original parameter block to preserve the indices used by
        // projects created with the first release.
        params.add_group(
            Params::SpaceSpecificStart,
            Params::SpaceSpecificEnd,
            "Space-specific",
            true,
            |params| {
                params.add(
                    Params::SpiralTurns,
                    "Spiral Turns",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(-100.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(-12.0);
                        d.set_slider_max(12.0);
                        d.set_default(1.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::FocusRatio,
                    "Focus / Radius",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.001);
                        d.set_valid_max(0.999);
                        d.set_slider_min(0.05);
                        d.set_slider_max(0.95);
                        d.set_default(0.5);
                        d.set_precision(3);
                    }),
                )?;
                params.add(
                    Params::CoordinateExtent,
                    "Coordinate Extent",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.05);
                        d.set_valid_max(20.0);
                        d.set_slider_min(0.25);
                        d.set_slider_max(8.0);
                        d.set_default(3.0);
                        d.set_precision(3);
                    }),
                )?;
                params.add_with_flags(
                    Params::HoughChannels,
                    "Line Hough Channels",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luminance", "RGBA", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ae::ParamUIFlags::empty(),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::PostTransformStart,
            Params::PostTransformEnd,
            "Post Transform",
            true,
            |params| {
                params.add(
                    Params::PostAnchorPoint,
                    "Anchor Point",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                params.add(
                    Params::PostPosition,
                    "Position",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                params.add_with_flags(
                    Params::PostSeparateScale,
                    "Separate Dimensions",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                add_post_scale_slider(params, Params::PostScale, "Scale", true)?;
                add_post_scale_slider(params, Params::PostScaleX, "Scale X", false)?;
                add_post_scale_slider(params, Params::PostScaleY, "Scale Y", false)?;
                params.add(
                    Params::PostRotation,
                    "Rotation",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                params.add(
                    Params::PostSkew,
                    "Skew",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                params.add(
                    Params::PostSkewAxis,
                    "Skew Axis",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
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
            ae::Command::About => out_data.set_return_msg(
                format!(
                    "AOD_SpaceConvert - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{year} Aodaruma",
                    version = env!("CARGO_PKG_VERSION"),
                    year = env!("BUILD_YEAR")
                )
                .as_str(),
            ),
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_SpaceConvert")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render { in_layer, out_layer } => {
                self.do_render(in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let request = extra.output_request();
                let callbacks = extra.callbacks();
                let query = callbacks.checkout_layer(
                    0,
                    1000,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let mut full_request = request;
                full_request.rect = query.max_result_rect;
                let result = callbacks.checkout_layer(
                    0,
                    0,
                    &full_request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let full_rect: ae::Rect = result.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let mut checkouts = LayerCheckoutGuard::<1>::new(cb);
                let render_result = (|| -> Result<(), Error> {
                    let input = checkouts.checkout(0)?;
                    let output = cb.checkout_output()?;
                    if let (Some(input), Some(output)) = (input, output) {
                        self.do_render(in_data, input, output, params)
                    } else {
                        Ok(())
                    }
                })();
                let checkin_result = checkouts.checkin_all();
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if matches!(
                    params.type_at(param_index),
                    Params::Space
                        | Params::Direction
                        | Params::CenterMode
                        | Params::RadiusMode
                        | Params::PostSeparateScale
                ) {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut cloned = params.cloned();
                self.update_params_ui(in_data, &mut cloned)?;
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
        let space = Space::from_popup(params.get(Params::Space)?.as_popup()?.value());
        let direction = Direction::from_popup(params.get(Params::Direction)?.as_popup()?.value());
        let custom_center = params.get(Params::CenterMode)?.as_popup()?.value() == 2;
        let custom_radius = params.get(Params::RadiusMode)?.as_popup()?.value() == 3;
        let integral = space.is_integral();
        let hough_forward = space == Space::LineHough && direction == Direction::Forward;
        let radon_inverse = space == Space::Radon && direction == Direction::Inverse;

        self.set_param_visible(in_data, params, Params::Center, custom_center)?;
        self.set_param_visible(in_data, params, Params::Radius, custom_radius)?;
        self.set_param_visible(
            in_data,
            params,
            Params::LogMinRadius,
            space == Space::LogPolar,
        )?;
        for id in [Params::AngleSamples, Params::DetectorSamples] {
            self.set_param_visible(in_data, params, id, integral)?;
        }
        self.set_param_visible(
            in_data,
            params,
            Params::RaySamples,
            integral && direction == Direction::Forward,
        )?;
        self.set_param_visible(in_data, params, Params::HoughThreshold, hough_forward)?;
        self.set_param_visible(in_data, params, Params::WeightedHough, hough_forward)?;
        self.set_param_visible(in_data, params, Params::FilterRadius, radon_inverse)?;
        self.set_param_visible(in_data, params, Params::Interpolation, !integral)?;
        self.set_param_visible(in_data, params, Params::Edge, !integral)?;
        self.set_param_visible(
            in_data,
            params,
            Params::SpiralTurns,
            space == Space::SpiralPolar,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::FocusRatio,
            matches!(space, Space::Elliptic | Space::Bipolar),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::CoordinateExtent,
            space == Space::Bipolar,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::HoughChannels,
            space == Space::LineHough,
        )?;
        let separate_scale = params
            .get(Params::PostSeparateScale)?
            .as_checkbox()?
            .value();
        self.set_param_visible(in_data, params, Params::PostScale, !separate_scale)?;
        self.set_param_visible(in_data, params, Params::PostScaleX, separate_scale)?;
        self.set_param_visible(in_data, params, Params::PostScaleY, separate_scale)?;
        Self::set_param_enabled(params, Params::PostScale, !separate_scale)?;
        Self::set_param_enabled(params, Params::PostScaleX, separate_scale)?;
        Self::set_param_enabled(params, Params::PostScaleY, separate_scale)?;
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

    fn set_param_enabled(
        params: &mut Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ParamUIFlags::DISABLED, !enabled)
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let mut settings = read_settings(params, in_data)?;
        let src_width = in_layer.width();
        let src_height = in_layer.height();
        let out_width = out_layer.width();
        let out_height = out_layer.height();
        if src_width == 0 || src_height == 0 || out_width == 0 || out_height == 0 {
            return Ok(());
        }
        if settings.center_mode == CenterMode::Custom {
            settings.center = point_in_checkout_world(settings.center, in_layer.origin());
        }
        settings.post_transform.anchor =
            point_in_checkout_world(settings.post_transform.anchor, out_layer.origin());
        settings.post_transform.position =
            point_in_checkout_world(settings.post_transform.position, out_layer.origin());

        let source = read_layer(&in_layer);
        let output = if settings.space.is_integral() {
            render_integral(
                &source, src_width, src_height, out_width, out_height, settings,
            )
        } else {
            render_coordinates(
                &source, src_width, src_height, out_width, out_height, settings,
            )
        };
        let out_world_type = out_layer.world_type();
        out_layer.iterate(0, out_height as i32, None, |x, y, mut dst| {
            let pixel = output[y as usize * out_width + x as usize];
            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(pixel.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(pixel.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(pixel),
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn add_integer_slider(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    default: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(min);
            d.set_slider_max(max);
            d.set_default(default);
            d.set_precision(0);
        }),
    )?;
    Ok(())
}

fn add_post_scale_slider(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    visible: bool,
) -> Result<(), Error> {
    let mut ui_flags = ParamUIFlags::empty();
    if !visible {
        ui_flags |= ParamUIFlags::INVISIBLE | ParamUIFlags::DISABLED;
    }
    params.add_with_flags(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(-10000.0);
            d.set_valid_max(10000.0);
            d.set_slider_min(-1000.0);
            d.set_slider_max(1000.0);
            d.set_default(100.0);
            d.set_precision(2);
        }),
        ParamFlag::empty(),
        ui_flags,
    )?;
    Ok(())
}

fn read_settings(params: &Parameters<Params>, in_data: InData) -> Result<Settings, Error> {
    let popup = |id| params.get(id)?.as_popup().map(|p| p.value());
    let slider = |id| params.get(id)?.as_float_slider().map(|p| p.value() as f32);
    let angle = |id| {
        params
            .get(id)?
            .as_angle()?
            .float_value()
            .map(|value| finite_or(value as f32, 0.0).to_radians())
    };
    let point = |id| {
        params
            .get(id)?
            .as_point()
            .map(|point| point.value())
            .map(|(x, y)| (finite_or(x, 0.0), finite_or(y, 0.0)))
    };
    let separate_scale = params
        .get(Params::PostSeparateScale)?
        .as_checkbox()?
        .value();
    let post_scale = if separate_scale {
        (
            finite_or(slider(Params::PostScaleX)?, 100.0) * 0.01,
            finite_or(slider(Params::PostScaleY)?, 100.0) * 0.01,
        )
    } else {
        let uniform = finite_or(slider(Params::PostScale)?, 100.0) * 0.01;
        (uniform, uniform)
    };
    Ok(Settings {
        space: Space::from_popup(popup(Params::Space)?),
        direction: Direction::from_popup(popup(Params::Direction)?),
        center_mode: if popup(Params::CenterMode)? == 2 {
            CenterMode::Custom
        } else {
            CenterMode::LayerCenter
        },
        center: params.get(Params::Center)?.as_point()?.value(),
        radius_mode: match popup(Params::RadiusMode)? {
            2 => RadiusMode::Inscribed,
            3 => RadiusMode::Custom,
            _ => RadiusMode::Auto,
        },
        radius: slider(Params::Radius)?.max(0.01),
        log_min_radius: slider(Params::LogMinRadius)?.max(0.001),
        angle_offset: (params.get(Params::AngleOffset)?.as_angle()?.float_value()? as f32)
            .to_radians(),
        angle_samples: slider(Params::AngleSamples)?.round().clamp(8.0, 720.0) as usize,
        detector_samples: slider(Params::DetectorSamples)?.round().clamp(16.0, 2048.0) as usize,
        ray_samples: slider(Params::RaySamples)?.round().clamp(8.0, 2048.0) as usize,
        hough_threshold: slider(Params::HoughThreshold)?.clamp(0.0, 1.0),
        weighted_hough: params.get(Params::WeightedHough)?.as_checkbox()?.value(),
        filter_radius: slider(Params::FilterRadius)?.round().clamp(1.0, 127.0) as usize,
        interpolation: if popup(Params::Interpolation)? == 1 {
            Interpolation::Nearest
        } else {
            Interpolation::Bilinear
        },
        edge: match popup(Params::Edge)? {
            2 => SampleEdge::Clamp,
            3 => SampleEdge::Tile,
            4 => SampleEdge::Mirror,
            _ => SampleEdge::Transparent,
        },
        output_gain: slider(Params::OutputGain)?,
        output_bias: slider(Params::OutputBias)?,
        clamp_output: params.get(Params::ClampOutput)?.as_checkbox()?.value(),
        pixel_scale: render_pixel_scale(in_data),
        spiral_turns: slider(Params::SpiralTurns)?,
        focus_ratio: slider(Params::FocusRatio)?.clamp(0.001, 0.999),
        coordinate_extent: slider(Params::CoordinateExtent)?.clamp(0.05, 20.0),
        hough_channels: HoughChannels::from_popup(popup(Params::HoughChannels)?),
        post_transform: PostTransform {
            anchor: point(Params::PostAnchorPoint)?,
            position: point(Params::PostPosition)?,
            scale: post_scale,
            rotation: angle(Params::PostRotation)?,
            skew: angle(Params::PostSkew)?,
            skew_axis: angle(Params::PostSkewAxis)?,
        },
    })
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn render_pixel_scale(in_data: InData) -> (f32, f32) {
    square_pixel_scale(
        rational_or_one(in_data.pixel_aspect_ratio()),
        rational_or_one(in_data.downsample_x()),
        rational_or_one(in_data.downsample_y()),
    )
}

fn rational_or_one(value: RationalScale) -> f32 {
    if value.num > 0 && value.den > 0 {
        let ratio = value.num as f32 / value.den as f32;
        if ratio.is_finite() && ratio > 0.0 {
            return ratio;
        }
    }
    1.0
}

fn square_pixel_scale(pixel_aspect: f32, downsample_x: f32, downsample_y: f32) -> (f32, f32) {
    (
        pixel_aspect.max(1.0e-6) / downsample_x.max(1.0e-6),
        1.0 / downsample_y.max(1.0e-6),
    )
}

fn point_in_checkout_world(point: (f32, f32), origin: ae::Point) -> (f32, f32) {
    (point.0 - origin.h as f32, point.1 - origin.v as f32)
}

fn inverse_post_transform_point(
    destination: (f32, f32),
    post_transform: PostTransform,
    pixel_scale: (f32, f32),
    inverse: Matrix2,
) -> (f32, f32) {
    let destination_physical = (
        (destination.0 - post_transform.position.0) * pixel_scale.0,
        (destination.1 - post_transform.position.1) * pixel_scale.1,
    );
    let source_physical = inverse.transform(destination_physical);
    (
        post_transform.anchor.0 + source_physical.0 / pixel_scale.0,
        post_transform.anchor.1 + source_physical.1 / pixel_scale.1,
    )
}

#[cfg(test)]
fn forward_post_transform_point(
    source: (f32, f32),
    post_transform: PostTransform,
    pixel_scale: (f32, f32),
) -> (f32, f32) {
    let source_physical = (
        (source.0 - post_transform.anchor.0) * pixel_scale.0,
        (source.1 - post_transform.anchor.1) * pixel_scale.1,
    );
    let destination_physical = post_transform.matrix().transform(source_physical);
    (
        post_transform.position.0 + destination_physical.0 / pixel_scale.0,
        post_transform.position.1 + destination_physical.1 / pixel_scale.1,
    )
}

fn resolve_geometry(width: usize, height: usize, settings: Settings) -> ((f32, f32), f32) {
    let center = match settings.center_mode {
        CenterMode::LayerCenter => ((width as f32 - 1.0) * 0.5, (height as f32 - 1.0) * 0.5),
        CenterMode::Custom => settings.center,
    };
    let left = center.0.abs() * settings.pixel_scale.0;
    let right = (width as f32 - 1.0 - center.0).abs() * settings.pixel_scale.0;
    let top = center.1.abs() * settings.pixel_scale.1;
    let bottom = (height as f32 - 1.0 - center.1).abs() * settings.pixel_scale.1;
    let radius = match settings.radius_mode {
        RadiusMode::Auto if settings.space == Space::SquareDisc => {
            left.min(right).min(top).min(bottom)
        }
        RadiusMode::Auto => [
            left.hypot(top),
            left.hypot(bottom),
            right.hypot(top),
            right.hypot(bottom),
        ]
        .into_iter()
        .fold(0.0, f32::max),
        RadiusMode::Inscribed => left.min(right).min(top).min(bottom),
        RadiusMode::Custom => settings.radius,
    };
    (center, radius.max(0.001))
}

fn sample(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    interpolation: Interpolation,
    edge: SampleEdge,
) -> PixelF32 {
    match interpolation {
        Interpolation::Nearest => sample_nearest(pixels, width, height, x, y, edge),
        Interpolation::Bilinear => sample_bilinear(pixels, width, height, x, y, edge),
    }
}

fn render_coordinates(
    source: &[PixelF32],
    src_width: usize,
    src_height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let (center, radius) = resolve_geometry(out_width, out_height, settings);
    let post_identity = settings.post_transform.is_identity();
    let post_inverse = if post_identity {
        None
    } else {
        settings.post_transform.matrix().inverse()
    };
    if !post_identity && post_inverse.is_none() {
        return vec![TRANSPARENT; out_width * out_height];
    }
    let (forward_polar_trigonometry, forward_polar_radii) = if matches!(
        (settings.space, settings.direction),
        (Space::Polar | Space::LogPolar, Direction::Forward)
    ) && post_identity
    {
        let trigonometry = (0..out_width)
            .map(|x| {
                let theta = settings.angle_offset + normalized_index(x, out_width) * TAU;
                (theta.cos(), theta.sin())
            })
            .collect::<Vec<_>>();
        let radii = (0..out_height)
            .map(|y| {
                let v = normalized_index(y, out_height);
                if settings.space == Space::LogPolar {
                    let min_radius = settings.log_min_radius.min(radius * 0.999).max(0.001);
                    min_radius * (radius / min_radius).powf(v)
                } else {
                    v * radius
                }
            })
            .collect::<Vec<_>>();
        (trigonometry, radii)
    } else {
        (Vec::new(), Vec::new())
    };
    let (inverse_polar_dx, inverse_polar_dy) = if matches!(
        (settings.space, settings.direction),
        (
            Space::Polar | Space::LogPolar | Space::SpiralPolar,
            Direction::Inverse
        )
    ) && post_identity
    {
        let dx = (0..out_width)
            .map(|x| (x as f32 - center.0) * settings.pixel_scale.0)
            .collect::<Vec<_>>();
        let dy = (0..out_height)
            .map(|y| (y as f32 - center.1) * settings.pixel_scale.1)
            .collect::<Vec<_>>();
        (dx, dy)
    } else {
        (Vec::new(), Vec::new())
    };
    let (inverse_log_min_radius, inverse_log_radius_range) =
        if settings.space == Space::LogPolar && settings.direction == Direction::Inverse {
            let min_radius = settings.log_min_radius.min(radius * 0.999).max(0.001);
            (min_radius, (radius / min_radius).ln())
        } else {
            (0.0, 0.0)
        };
    let mut output = vec![TRANSPARENT; out_width * out_height];
    for y in 0..out_height {
        for x in 0..out_width {
            let (post_x, post_y) = if post_identity {
                (x as f32, y as f32)
            } else {
                inverse_post_transform_point(
                    (x as f32, y as f32),
                    settings.post_transform,
                    settings.pixel_scale,
                    post_inverse.expect("non-identity transform was checked for invertibility"),
                )
            };
            let u = normalized_coordinate(post_x, out_width);
            let v = normalized_coordinate(post_y, out_height);
            let coordinate = match (settings.space, settings.direction) {
                (Space::Polar, Direction::Forward) => {
                    let ((cos, sin), r) = if post_identity {
                        (forward_polar_trigonometry[x], forward_polar_radii[y])
                    } else {
                        let theta = settings.angle_offset + u * TAU;
                        ((theta.cos(), theta.sin()), v * radius)
                    };
                    Some((
                        center.0 + r * cos / settings.pixel_scale.0,
                        center.1 + r * sin / settings.pixel_scale.1,
                    ))
                }
                (Space::LogPolar, Direction::Forward) => {
                    let ((cos, sin), r) = if post_identity {
                        (forward_polar_trigonometry[x], forward_polar_radii[y])
                    } else {
                        let theta = settings.angle_offset + u * TAU;
                        let min_radius = settings.log_min_radius.min(radius * 0.999).max(0.001);
                        (
                            (theta.cos(), theta.sin()),
                            min_radius * (radius / min_radius).powf(v),
                        )
                    };
                    Some((
                        center.0 + r * cos / settings.pixel_scale.0,
                        center.1 + r * sin / settings.pixel_scale.1,
                    ))
                }
                (Space::SpiralPolar, Direction::Forward) => {
                    let theta = settings.angle_offset + (u + v * settings.spiral_turns) * TAU;
                    let r = v * radius;
                    Some((
                        center.0 + r * theta.cos() / settings.pixel_scale.0,
                        center.1 + r * theta.sin() / settings.pixel_scale.1,
                    ))
                }
                (Space::Polar | Space::LogPolar | Space::SpiralPolar, Direction::Inverse) => {
                    let (dx, dy) = if post_identity {
                        (inverse_polar_dx[x], inverse_polar_dy[y])
                    } else {
                        (
                            (post_x - center.0) * settings.pixel_scale.0,
                            (post_y - center.1) * settings.pixel_scale.1,
                        )
                    };
                    let r = dx.hypot(dy);
                    if r > radius {
                        None
                    } else {
                        let radial_position = r / radius;
                        let spiral_phase = if settings.space == Space::SpiralPolar {
                            radial_position * settings.spiral_turns * TAU
                        } else {
                            0.0
                        };
                        let angle_u = (dy.atan2(dx) - settings.angle_offset - spiral_phase)
                            .rem_euclid(TAU)
                            / TAU;
                        let radius_v = if settings.space == Space::LogPolar {
                            if r < inverse_log_min_radius {
                                return_coordinate_none()
                            } else {
                                (r / inverse_log_min_radius).ln() / inverse_log_radius_range
                            }
                        } else {
                            r / radius
                        };
                        if radius_v.is_finite() {
                            Some((
                                angle_u * (src_width.saturating_sub(1) as f32),
                                radius_v * (src_height.saturating_sub(1) as f32),
                            ))
                        } else {
                            None
                        }
                    }
                }
                (Space::SquareDisc, Direction::Forward) => {
                    let dx = (post_x - center.0) * settings.pixel_scale.0 / radius;
                    let dy = (post_y - center.1) * settings.pixel_scale.1 / radius;
                    if dx * dx + dy * dy > 1.0 {
                        None
                    } else {
                        let (sx, sy) = disc_to_square(dx, dy);
                        Some((
                            (sx * 0.5 + 0.5) * src_width.saturating_sub(1) as f32,
                            (sy * 0.5 + 0.5) * src_height.saturating_sub(1) as f32,
                        ))
                    }
                }
                (Space::SquareDisc, Direction::Inverse) => {
                    let sx = u * 2.0 - 1.0;
                    let sy = v * 2.0 - 1.0;
                    let (dx, dy) = square_to_disc(sx, sy);
                    Some((
                        center.0 + dx * radius / settings.pixel_scale.0,
                        center.1 + dy * radius / settings.pixel_scale.1,
                    ))
                }
                (Space::Elliptic, Direction::Forward) => {
                    let focus = (radius * settings.focus_ratio).max(1.0e-6);
                    let mu_max = (radius / focus).max(1.0).acosh().max(1.0e-6);
                    let mu = v * mu_max;
                    let nu = settings.angle_offset + u * TAU;
                    Some((
                        center.0 + focus * mu.cosh() * nu.cos() / settings.pixel_scale.0,
                        center.1 + focus * mu.sinh() * nu.sin() / settings.pixel_scale.1,
                    ))
                }
                (Space::Elliptic, Direction::Inverse) => {
                    let dx = (post_x - center.0) * settings.pixel_scale.0;
                    let dy = (post_y - center.1) * settings.pixel_scale.1;
                    let focus = (radius * settings.focus_ratio).max(1.0e-6);
                    let d_positive = (dx - focus).hypot(dy);
                    let d_negative = (dx + focus).hypot(dy);
                    let mu = ((d_positive + d_negative) / (2.0 * focus)).max(1.0).acosh();
                    let mu_max = (radius / focus).max(1.0).acosh().max(1.0e-6);
                    if mu > mu_max + 1.0e-5 {
                        None
                    } else {
                        let cosine = ((d_negative - d_positive) / (2.0 * focus)).clamp(-1.0, 1.0);
                        let mut nu = cosine.acos();
                        if dy < 0.0 {
                            nu = TAU - nu;
                        }
                        let angle_u = (nu - settings.angle_offset).rem_euclid(TAU) / TAU;
                        Some((
                            angle_u * src_width.saturating_sub(1) as f32,
                            (mu / mu_max) * src_height.saturating_sub(1) as f32,
                        ))
                    }
                }
                (Space::Parabolic, Direction::Forward) => {
                    let coordinate_radius = (2.0 * radius).sqrt();
                    let tau = (u * 2.0 - 1.0) * coordinate_radius;
                    let sigma = v * coordinate_radius;
                    let dx = 0.5 * (sigma * sigma - tau * tau);
                    let dy = sigma * tau;
                    Some((
                        center.0 + dx / settings.pixel_scale.0,
                        center.1 + dy / settings.pixel_scale.1,
                    ))
                }
                (Space::Parabolic, Direction::Inverse) => {
                    let dx = (post_x - center.0) * settings.pixel_scale.0;
                    let dy = (post_y - center.1) * settings.pixel_scale.1;
                    let radial = dx.hypot(dy);
                    let sigma = (radial + dx).max(0.0).sqrt();
                    let tau_magnitude = (radial - dx).max(0.0).sqrt();
                    // The negative X axis is the parabolic branch cut. Choose
                    // the positive branch at exactly y=0 so it stays mapped
                    // instead of collapsing to the origin.
                    let tau = if dy < 0.0 {
                        -tau_magnitude
                    } else {
                        tau_magnitude
                    };
                    let coordinate_radius = (2.0 * radius).sqrt().max(1.0e-6);
                    let source_u = tau / (2.0 * coordinate_radius) + 0.5;
                    let source_v = sigma / coordinate_radius;
                    if !(0.0..=1.0).contains(&source_u) || !(0.0..=1.0).contains(&source_v) {
                        None
                    } else {
                        Some((
                            source_u * src_width.saturating_sub(1) as f32,
                            source_v * src_height.saturating_sub(1) as f32,
                        ))
                    }
                }
                (Space::Bipolar, Direction::Forward) => {
                    let focus = (radius * settings.focus_ratio).max(1.0e-6);
                    let tau = (u * 2.0 - 1.0) * settings.coordinate_extent;
                    let sigma = (v * 2.0 - 1.0) * PI + settings.angle_offset;
                    let denominator = tau.cosh() - sigma.cos();
                    if denominator.abs() < 1.0e-6 {
                        None
                    } else {
                        let dx = focus * tau.sinh() / denominator;
                        let dy = focus * sigma.sin() / denominator;
                        Some((
                            center.0 + dx / settings.pixel_scale.0,
                            center.1 + dy / settings.pixel_scale.1,
                        ))
                    }
                }
                (Space::Bipolar, Direction::Inverse) => {
                    let dx = (post_x - center.0) * settings.pixel_scale.0;
                    let dy = (post_y - center.1) * settings.pixel_scale.1;
                    let focus = (radius * settings.focus_ratio).max(1.0e-6);
                    let numerator = (dx + focus) * (dx + focus) + dy * dy;
                    let denominator = (dx - focus) * (dx - focus) + dy * dy;
                    if numerator <= 1.0e-12 || denominator <= 1.0e-12 {
                        None
                    } else {
                        let tau = 0.5 * (numerator / denominator).ln();
                        let sigma = (2.0 * focus * dy).atan2(dx * dx + dy * dy - focus * focus);
                        let source_u = tau / (2.0 * settings.coordinate_extent) + 0.5;
                        let source_v = (sigma - settings.angle_offset).rem_euclid(TAU) / TAU;
                        if !(0.0..=1.0).contains(&source_u) {
                            None
                        } else {
                            Some((
                                source_u * src_width.saturating_sub(1) as f32,
                                source_v * src_height.saturating_sub(1) as f32,
                            ))
                        }
                    }
                }
                _ => None,
            };
            let pixel = coordinate
                .map(|(sx, sy)| {
                    sample(
                        source,
                        src_width,
                        src_height,
                        sx,
                        sy,
                        settings.interpolation,
                        settings.edge,
                    )
                })
                .unwrap_or(TRANSPARENT);
            output[y * out_width + x] = finish_pixel(pixel, settings);
        }
    }
    output
}

#[inline]
fn return_coordinate_none() -> f32 {
    f32::NAN
}

fn render_integral(
    source: &[PixelF32],
    src_width: usize,
    src_height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let analysis = match (settings.space, settings.direction) {
        (Space::Radon, Direction::Forward) if !settings.post_transform.is_identity() => {
            forward_radon_post_transformed(
                source, src_width, src_height, out_width, out_height, settings,
            )
        }
        (Space::Radon, Direction::Forward) => {
            forward_radon(source, src_width, src_height, settings)
        }
        (Space::LineHough, Direction::Forward) if !settings.post_transform.is_identity() => {
            forward_hough_post_transformed(
                source, src_width, src_height, out_width, out_height, settings,
            )
        }
        (Space::LineHough, Direction::Forward) => {
            forward_hough(source, src_width, src_height, settings)
        }
        (Space::Radon, Direction::Inverse) => inverse_radon(
            source, src_width, src_height, out_width, out_height, settings,
        ),
        (Space::LineHough, Direction::Inverse) => inverse_hough(
            source, src_width, src_height, out_width, out_height, settings,
        ),
        _ => vec![TRANSPARENT; out_width * out_height],
    };

    if settings.direction == Direction::Inverse {
        return analysis
            .into_iter()
            .map(|pixel| finish_pixel(pixel, settings))
            .collect();
    }
    resize_image(
        &analysis,
        settings.angle_samples,
        settings.detector_samples,
        out_width,
        out_height,
    )
    .into_iter()
    .map(|pixel| finish_pixel(pixel, settings))
    .collect()
}

fn effective_ray_samples(settings: Settings) -> usize {
    let projection_count = settings
        .angle_samples
        .saturating_mul(settings.detector_samples)
        .max(1);
    let budgeted = (MAX_INTEGRAL_SAMPLE_EVALUATIONS / projection_count).max(8);
    settings.ray_samples.min(budgeted)
}

struct ProjectionGeometry {
    trigonometry: Vec<(f32, f32)>,
    detector_rhos: Vec<f32>,
    ray_positions: Vec<f32>,
}

fn projection_trigonometry(angle_offset: f32, angle_samples: usize) -> Vec<(f32, f32)> {
    (0..angle_samples)
        .map(|angle| {
            let theta = angle_offset + angle as f32 * PI / angle_samples as f32;
            (theta.cos(), theta.sin())
        })
        .collect()
}

impl ProjectionGeometry {
    fn new(
        angle_offset: f32,
        angle_samples: usize,
        detector_samples: usize,
        ray_samples: usize,
        radius: f32,
    ) -> Self {
        let trigonometry = projection_trigonometry(angle_offset, angle_samples);
        let detector_rhos = (0..detector_samples)
            .map(|detector| detector_rho(detector, detector_samples, radius))
            .collect();
        let ray_positions = (0..ray_samples)
            .map(|ray| detector_rho(ray, ray_samples, radius))
            .collect();
        Self {
            trigonometry,
            detector_rhos,
            ray_positions,
        }
    }
}

fn transformed_projection_coordinate(
    angle_index: usize,
    detector_index: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
    inverse: Matrix2,
) -> (f32, f32) {
    let destination = (
        resize_coordinate(angle_index, settings.angle_samples, out_width),
        resize_coordinate(detector_index, settings.detector_samples, out_height),
    );
    let source = inverse_post_transform_point(
        destination,
        settings.post_transform,
        settings.pixel_scale,
        inverse,
    );
    (
        settings.angle_offset + normalized_coordinate(source.0, out_width) * PI,
        normalized_coordinate(source.1, out_height),
    )
}

fn forward_radon_post_transformed(
    source: &[PixelF32],
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let Some(inverse) = settings.post_transform.matrix().inverse() else {
        return vec![TRANSPARENT; settings.angle_samples * settings.detector_samples];
    };
    let (center, radius) = resolve_geometry(width, height, settings);
    let ray_samples = effective_ray_samples(settings);
    let ray_positions = (0..ray_samples)
        .map(|ray| detector_rho(ray, ray_samples, radius))
        .collect::<Vec<_>>();
    let mut result = vec![TRANSPARENT; settings.angle_samples * settings.detector_samples];
    for detector in 0..settings.detector_samples {
        for angle in 0..settings.angle_samples {
            let (theta, detector_v) = transformed_projection_coordinate(
                angle, detector, out_width, out_height, settings, inverse,
            );
            let rho = (detector_v * 2.0 - 1.0) * radius;
            let (sin, cos) = theta.sin_cos();
            let mut sum = TRANSPARENT;
            for (ray, &t) in ray_positions.iter().enumerate() {
                let pixel = sample_bilinear(
                    source,
                    width,
                    height,
                    center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                    center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                    SampleEdge::Transparent,
                );
                let endpoint_weight = if ray == 0 || ray + 1 == ray_samples {
                    0.5
                } else {
                    1.0
                };
                sum = add_pixel(sum, scale_pixel(pixel, endpoint_weight));
            }
            result[detector * settings.angle_samples + angle] =
                scale_pixel(sum, 1.0 / ray_samples.saturating_sub(1).max(1) as f32);
        }
    }
    result
}

fn forward_hough_post_transformed(
    source: &[PixelF32],
    width: usize,
    height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let Some(inverse) = settings.post_transform.matrix().inverse() else {
        return vec![TRANSPARENT; settings.angle_samples * settings.detector_samples];
    };
    let (center, radius) = resolve_geometry(width, height, settings);
    let ray_samples = effective_ray_samples(settings);
    let ray_positions = (0..ray_samples)
        .map(|ray| detector_rho(ray, ray_samples, radius))
        .collect::<Vec<_>>();
    let edges = sobel_edge_channels(
        source,
        width,
        height,
        settings.pixel_scale,
        settings.hough_channels,
    );
    let mut votes = vec![[0.0_f32; 4]; settings.angle_samples * settings.detector_samples];
    let mut maximum = [0.0_f32; 4];
    let alpha_fallback = if source.is_empty() {
        0.0
    } else {
        source.iter().map(|pixel| pixel.alpha).sum::<f32>() / source.len() as f32
    };
    for detector in 0..settings.detector_samples {
        for angle in 0..settings.angle_samples {
            let (theta, detector_v) = transformed_projection_coordinate(
                angle, detector, out_width, out_height, settings, inverse,
            );
            let rho = (detector_v * 2.0 - 1.0) * radius;
            let (sin, cos) = theta.sin_cos();
            let mut sum = [0.0_f32; 4];
            for &t in &ray_positions {
                let magnitude = sample_channel_bilinear(
                    &edges,
                    width,
                    height,
                    center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                    center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                );
                for channel in 0..4 {
                    if magnitude[channel] >= settings.hough_threshold {
                        sum[channel] += if settings.weighted_hough {
                            magnitude[channel]
                        } else {
                            1.0
                        };
                    }
                }
            }
            let vote = sum.map(|value| value / ray_samples as f32);
            votes[detector * settings.angle_samples + angle] = vote;
            for channel in 0..4 {
                maximum[channel] = maximum[channel].max(vote[channel]);
            }
        }
    }
    votes
        .into_iter()
        .map(|value| {
            let normalized = [
                value[0] / maximum[0].max(1.0e-8),
                value[1] / maximum[1].max(1.0e-8),
                value[2] / maximum[2].max(1.0e-8),
                if maximum[3] <= 1.0e-8 {
                    alpha_fallback
                } else {
                    value[3] / maximum[3]
                },
            ];
            if settings.hough_channels.is_rgba() {
                PixelF32 {
                    alpha: normalized[3],
                    red: normalized[0],
                    green: normalized[1],
                    blue: normalized[2],
                }
            } else {
                gray_pixel(normalized[0])
            }
        })
        .collect()
}

fn forward_radon(
    source: &[PixelF32],
    width: usize,
    height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let (center, radius) = resolve_geometry(width, height, settings);
    let ray_samples = effective_ray_samples(settings);
    let geometry = ProjectionGeometry::new(
        settings.angle_offset,
        settings.angle_samples,
        settings.detector_samples,
        ray_samples,
        radius,
    );
    let mut result = vec![TRANSPARENT; settings.angle_samples * settings.detector_samples];
    for detector in 0..settings.detector_samples {
        let rho = geometry.detector_rhos[detector];
        for angle in 0..settings.angle_samples {
            let (cos, sin) = geometry.trigonometry[angle];
            let mut sum = TRANSPARENT;
            for ray in 0..ray_samples {
                let t = geometry.ray_positions[ray];
                let pixel = sample_bilinear(
                    source,
                    width,
                    height,
                    center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                    center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                    SampleEdge::Transparent,
                );
                let endpoint_weight = if ray == 0 || ray + 1 == ray_samples {
                    0.5
                } else {
                    1.0
                };
                sum = add_pixel(sum, scale_pixel(pixel, endpoint_weight));
            }
            result[detector * settings.angle_samples + angle] =
                scale_pixel(sum, 1.0 / ray_samples.saturating_sub(1).max(1) as f32);
        }
    }
    result
}

fn forward_hough(
    source: &[PixelF32],
    width: usize,
    height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let (center, radius) = resolve_geometry(width, height, settings);
    let ray_samples = effective_ray_samples(settings);
    let geometry = ProjectionGeometry::new(
        settings.angle_offset,
        settings.angle_samples,
        settings.detector_samples,
        ray_samples,
        radius,
    );
    let edges = sobel_edge_channels(
        source,
        width,
        height,
        settings.pixel_scale,
        settings.hough_channels,
    );
    let mut votes = vec![[0.0_f32; 4]; settings.angle_samples * settings.detector_samples];
    let mut maximum = [0.0_f32; 4];
    let alpha_fallback = if source.is_empty() {
        0.0
    } else {
        source.iter().map(|pixel| pixel.alpha).sum::<f32>() / source.len() as f32
    };
    for detector in 0..settings.detector_samples {
        let rho = geometry.detector_rhos[detector];
        for angle in 0..settings.angle_samples {
            let (cos, sin) = geometry.trigonometry[angle];
            let mut sum = [0.0_f32; 4];
            for ray in 0..ray_samples {
                let t = geometry.ray_positions[ray];
                let magnitude = sample_channel_bilinear(
                    &edges,
                    width,
                    height,
                    center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                    center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                );
                for channel in 0..4 {
                    if magnitude[channel] >= settings.hough_threshold {
                        sum[channel] += if settings.weighted_hough {
                            magnitude[channel]
                        } else {
                            1.0
                        };
                    }
                }
            }
            let vote = sum.map(|value| value / ray_samples as f32);
            votes[detector * settings.angle_samples + angle] = vote;
            for channel in 0..4 {
                maximum[channel] = maximum[channel].max(vote[channel]);
            }
        }
    }
    votes
        .into_iter()
        .map(|value| {
            let normalized = [
                value[0] / maximum[0].max(1.0e-8),
                value[1] / maximum[1].max(1.0e-8),
                value[2] / maximum[2].max(1.0e-8),
                if maximum[3] <= 1.0e-8 {
                    alpha_fallback
                } else {
                    value[3] / maximum[3]
                },
            ];
            if settings.hough_channels.is_rgba() {
                PixelF32 {
                    alpha: normalized[3],
                    red: normalized[0],
                    green: normalized[1],
                    blue: normalized[2],
                }
            } else {
                gray_pixel(normalized[0])
            }
        })
        .collect()
}

fn inverse_radon(
    source: &[PixelF32],
    src_width: usize,
    src_height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let sinogram = resize_image(
        source,
        src_width,
        src_height,
        settings.angle_samples,
        settings.detector_samples,
    );
    let (analysis_width, analysis_height) = reconstruction_dimensions(
        out_width,
        out_height,
        settings.detector_samples,
        settings.angle_samples,
    );
    let analysis_settings = scaled_geometry_settings(
        settings,
        analysis_width as f32 / out_width.max(1) as f32,
        analysis_height as f32 / out_height.max(1) as f32,
    );
    let filter_radius = effective_filter_radius(
        settings.angle_samples,
        settings.detector_samples,
        settings.filter_radius,
    );
    let filtered = ram_lak_filter(
        &sinogram,
        settings.angle_samples,
        settings.detector_samples,
        filter_radius,
    );
    let mut reconstruction = backproject(
        &filtered,
        settings.angle_samples,
        settings.detector_samples,
        analysis_width,
        analysis_height,
        analysis_settings,
        false,
    );
    let (_, analysis_radius) = resolve_geometry(analysis_width, analysis_height, analysis_settings);
    let normalization = fbp_normalization(settings.detector_samples, analysis_radius);
    for pixel in &mut reconstruction {
        *pixel = scale_pixel(*pixel, normalization);
    }
    resize_image(
        &reconstruction,
        analysis_width,
        analysis_height,
        out_width,
        out_height,
    )
}

fn inverse_hough(
    source: &[PixelF32],
    src_width: usize,
    src_height: usize,
    out_width: usize,
    out_height: usize,
    settings: Settings,
) -> Vec<PixelF32> {
    let channel_source = analysis_channel_pixels(source, settings.hough_channels);
    let accumulator = resize_image(
        &channel_source,
        src_width,
        src_height,
        settings.angle_samples,
        settings.detector_samples,
    );
    let (analysis_width, analysis_height) = reconstruction_dimensions(
        out_width,
        out_height,
        settings.detector_samples,
        settings.angle_samples,
    );
    let analysis_settings = scaled_geometry_settings(
        settings,
        analysis_width as f32 / out_width.max(1) as f32,
        analysis_height as f32 / out_height.max(1) as f32,
    );
    let reconstruction = backproject(
        &accumulator,
        settings.angle_samples,
        settings.detector_samples,
        analysis_width,
        analysis_height,
        analysis_settings,
        true,
    );
    resize_image(
        &reconstruction,
        analysis_width,
        analysis_height,
        out_width,
        out_height,
    )
}

fn reconstruction_dimensions(
    width: usize,
    height: usize,
    detector_samples: usize,
    angle_samples: usize,
) -> (usize, usize) {
    let longest = width.max(height).max(1);
    let analysis_longest = detector_samples.min(longest).max(1);
    let scale = analysis_longest as f32 / longest as f32;
    let mut dimensions = (
        ((width as f32 * scale).round() as usize).max(1),
        ((height as f32 * scale).round() as usize).max(1),
    );
    let pixel_budget = (MAX_INTEGRAL_SAMPLE_EVALUATIONS / angle_samples.max(1)).max(1);
    let pixel_count = dimensions.0.saturating_mul(dimensions.1);
    if pixel_count > pixel_budget {
        let budget_scale = (pixel_budget as f64 / pixel_count as f64).sqrt() as f32;
        dimensions.0 = ((dimensions.0 as f32 * budget_scale).floor() as usize).max(1);
        dimensions.1 = ((dimensions.1 as f32 * budget_scale).floor() as usize).max(1);
        while dimensions.0.saturating_mul(dimensions.1) > pixel_budget {
            if dimensions.0 >= dimensions.1 && dimensions.0 > 1 {
                dimensions.0 -= 1;
            } else if dimensions.1 > 1 {
                dimensions.1 -= 1;
            } else {
                break;
            }
        }
    }
    dimensions
}

fn scaled_geometry_settings(mut settings: Settings, scale_x: f32, scale_y: f32) -> Settings {
    if settings.center_mode == CenterMode::Custom {
        settings.center.0 *= scale_x;
        settings.center.1 *= scale_y;
    }
    settings.post_transform.anchor.0 *= scale_x;
    settings.post_transform.anchor.1 *= scale_y;
    settings.post_transform.position.0 *= scale_x;
    settings.post_transform.position.1 *= scale_y;
    settings.pixel_scale.0 /= scale_x.max(1.0e-6);
    settings.pixel_scale.1 /= scale_y.max(1.0e-6);
    settings
}

#[allow(clippy::too_many_arguments)]
fn backproject(
    sinogram: &[PixelF32],
    angles: usize,
    detectors: usize,
    width: usize,
    height: usize,
    settings: Settings,
    normalize: bool,
) -> Vec<PixelF32> {
    let (center, radius) = resolve_geometry(width, height, settings);
    let trigonometry = projection_trigonometry(settings.angle_offset, angles);
    let post_identity = settings.post_transform.is_identity();
    let post_inverse = if post_identity {
        None
    } else {
        settings.post_transform.matrix().inverse()
    };
    if !post_identity && post_inverse.is_none() {
        return vec![TRANSPARENT; width * height];
    }
    let mut result = vec![TRANSPARENT; width * height];
    let mut maximum = 0.0_f32;
    for y in 0..height {
        for x in 0..width {
            let (post_x, post_y) = if post_identity {
                (x as f32, y as f32)
            } else {
                inverse_post_transform_point(
                    (x as f32, y as f32),
                    settings.post_transform,
                    settings.pixel_scale,
                    post_inverse.expect("non-identity transform was checked for invertibility"),
                )
            };
            let dx = (post_x - center.0) * settings.pixel_scale.0;
            let dy = (post_y - center.1) * settings.pixel_scale.1;
            let mut sum = TRANSPARENT;
            for (angle, &(cos, sin)) in trigonometry.iter().enumerate() {
                let rho = dx * cos + dy * sin;
                let detector = rho_to_detector(rho, detectors, radius);
                sum = add_pixel(
                    sum,
                    sample_sinogram_detector(sinogram, angles, detectors, angle, detector),
                );
            }
            let pixel = scale_pixel(sum, PI / angles as f32);
            maximum = maximum.max(pixel.red.max(pixel.green).max(pixel.blue).abs());
            result[y * width + x] = pixel;
        }
    }
    if normalize {
        let scale = 1.0 / maximum.max(1.0e-8);
        for pixel in &mut result {
            *pixel = scale_pixel(*pixel, scale);
            pixel.alpha = 1.0;
        }
    }
    result
}

fn effective_filter_radius(angles: usize, detectors: usize, requested: usize) -> usize {
    let sample_count = angles.saturating_mul(detectors).max(1);
    let tap_budget = (MAX_INTEGRAL_SAMPLE_EVALUATIONS / sample_count).max(3);
    let budgeted_radius = tap_budget.saturating_sub(1) / 2;
    requested.min(budgeted_radius.max(1))
}

fn fbp_normalization(detectors: usize, reconstruction_radius: f32) -> f32 {
    let detector_intervals = detectors.saturating_sub(1).max(1) as f32;
    detector_intervals * detector_intervals / (2.0 * reconstruction_radius.max(0.001))
}

fn ram_lak_filter(
    sinogram: &[PixelF32],
    angles: usize,
    detectors: usize,
    radius: usize,
) -> Vec<PixelF32> {
    let mut result = vec![TRANSPARENT; sinogram.len()];
    let coefficients = (-(radius as isize)..=(radius as isize))
        .map(ram_lak_coefficient)
        .collect::<Vec<_>>();
    for detector in 0..detectors {
        for angle in 0..angles {
            let mut sum = TRANSPARENT;
            for offset in -(radius as isize)..=(radius as isize) {
                let source_detector = detector as isize + offset;
                if !(0..detectors as isize).contains(&source_detector) {
                    continue;
                }
                let coefficient = coefficients[(offset + radius as isize) as usize];
                sum = add_pixel(
                    sum,
                    scale_pixel(
                        sinogram[source_detector as usize * angles + angle],
                        coefficient,
                    ),
                );
            }
            result[detector * angles + angle] = sum;
        }
    }
    result
}

fn ram_lak_coefficient(index: isize) -> f32 {
    if index == 0 {
        0.25
    } else if index % 2 == 0 {
        0.0
    } else {
        -1.0 / (PI * PI * (index * index) as f32)
    }
}

fn resize_image(
    source: &[PixelF32],
    src_width: usize,
    src_height: usize,
    width: usize,
    height: usize,
) -> Vec<PixelF32> {
    let mut result = vec![TRANSPARENT; width * height];
    for y in 0..height {
        let sy = resize_coordinate(y, height, src_height);
        for x in 0..width {
            let sx = resize_coordinate(x, width, src_width);
            result[y * width + x] =
                sample_bilinear(source, src_width, src_height, sx, sy, SampleEdge::Clamp);
        }
    }
    result
}

fn resize_coordinate(index: usize, destination: usize, source: usize) -> f32 {
    if destination <= 1 || source <= 1 {
        0.0
    } else {
        index as f32 * (source - 1) as f32 / (destination - 1) as f32
    }
}

#[cfg(test)]
fn sobel_edges(
    source: &[PixelF32],
    width: usize,
    height: usize,
    pixel_scale: (f32, f32),
) -> Vec<f32> {
    sobel_edge_channels(source, width, height, pixel_scale, HoughChannels::Luminance)
        .into_iter()
        .map(|channels| channels[0])
        .collect()
}

fn sobel_edge_channels(
    source: &[PixelF32],
    width: usize,
    height: usize,
    pixel_scale: (f32, f32),
    mode: HoughChannels,
) -> Vec<[f32; 4]> {
    let mut result = vec![[0.0; 4]; width * height];
    if width == 0 || height == 0 {
        return result;
    }
    let components = (0..width * height)
        .map(|index| analysis_components(source.get(index).copied().unwrap_or(TRANSPARENT), mode))
        .collect::<Vec<_>>();
    let get = |x: i64, y: i64| {
        let x = x.clamp(0, width as i64 - 1) as usize;
        let y = y.clamp(0, height as i64 - 1) as usize;
        components[y * width + x]
    };
    for y in 0..height {
        for x in 0..width {
            let x = x as i64;
            let y = y as i64;
            let p00 = get(x - 1, y - 1);
            let p10 = get(x, y - 1);
            let p20 = get(x + 1, y - 1);
            let p01 = get(x - 1, y);
            let p21 = get(x + 1, y);
            let p02 = get(x - 1, y + 1);
            let p12 = get(x, y + 1);
            let p22 = get(x + 1, y + 1);
            let mut magnitude = [0.0; 4];
            for channel in 0..4 {
                let gx = (-p00[channel] + p20[channel] - 2.0 * p01[channel] + 2.0 * p21[channel]
                    - p02[channel]
                    + p22[channel])
                    / pixel_scale.0.max(1.0e-6);
                let gy = (-p00[channel] - 2.0 * p10[channel] - p20[channel]
                    + p02[channel]
                    + 2.0 * p12[channel]
                    + p22[channel])
                    / pixel_scale.1.max(1.0e-6);
                magnitude[channel] = (gx.hypot(gy) * 0.25).clamp(0.0, 1.0);
            }
            result[y as usize * width + x as usize] = magnitude;
        }
    }
    result
}

fn analysis_components(pixel: PixelF32, mode: HoughChannels) -> [f32; 4] {
    match mode {
        HoughChannels::Rgba => [pixel.red, pixel.green, pixel.blue, pixel.alpha],
        HoughChannels::Red => [pixel.red; 4],
        HoughChannels::Green => [pixel.green; 4],
        HoughChannels::Blue => [pixel.blue; 4],
        HoughChannels::Alpha => [pixel.alpha; 4],
        HoughChannels::Luminance => [luma(pixel); 4],
    }
}

fn analysis_channel_pixels(source: &[PixelF32], mode: HoughChannels) -> Vec<PixelF32> {
    if mode.is_rgba() {
        return source.to_vec();
    }
    source
        .iter()
        .map(|pixel| gray_pixel(analysis_components(*pixel, mode)[0]))
        .collect()
}

fn sample_channel_bilinear(
    values: &[[f32; 4]],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
) -> [f32; 4] {
    if x < 0.0
        || y < 0.0
        || x > width.saturating_sub(1) as f32
        || y > height.saturating_sub(1) as f32
    {
        return [0.0; 4];
    }
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let mut result = [0.0; 4];
    for channel in 0..4 {
        let top =
            values[y0 * width + x0][channel] * (1.0 - tx) + values[y0 * width + x1][channel] * tx;
        let bottom =
            values[y1 * width + x0][channel] * (1.0 - tx) + values[y1 * width + x1][channel] * tx;
        result[channel] = top * (1.0 - ty) + bottom * ty;
    }
    result
}

fn sample_sinogram_detector(
    pixels: &[PixelF32],
    angles: usize,
    detectors: usize,
    angle: usize,
    detector: f32,
) -> PixelF32 {
    if detector < 0.0 || detector > detectors.saturating_sub(1) as f32 {
        return TRANSPARENT;
    }
    let d0 = detector.floor() as usize;
    let d1 = (d0 + 1).min(detectors - 1);
    let t = detector - d0 as f32;
    lerp_pixel_local(pixels[d0 * angles + angle], pixels[d1 * angles + angle], t)
}

fn square_to_disc(x: f32, y: f32) -> (f32, f32) {
    if x.abs() < 1.0e-8 && y.abs() < 1.0e-8 {
        return (0.0, 0.0);
    }
    let (radius, theta) = if x.abs() > y.abs() {
        (x, FRAC_PI_4 * y / x)
    } else {
        (y, PI * 0.5 - FRAC_PI_4 * x / y)
    };
    (radius * theta.cos(), radius * theta.sin())
}

fn disc_to_square(x: f32, y: f32) -> (f32, f32) {
    let radius = x.hypot(y);
    if radius < 1.0e-8 {
        return (0.0, 0.0);
    }
    let mut phi = y.atan2(x);
    if phi < -FRAC_PI_4 {
        phi += TAU;
    }
    if phi < FRAC_PI_4 {
        (radius, radius * phi / FRAC_PI_4)
    } else if phi < 3.0 * FRAC_PI_4 {
        let square_y = radius;
        (square_y * (2.0 - phi / FRAC_PI_4), square_y)
    } else if phi < 5.0 * FRAC_PI_4 {
        let square_x = -radius;
        (square_x, square_x * (phi - PI) / FRAC_PI_4)
    } else {
        let square_y = -radius;
        (square_y * (2.0 - (phi - PI) / FRAC_PI_4), square_y)
    }
}

fn normalized_index(index: usize, length: usize) -> f32 {
    if length <= 1 {
        0.0
    } else {
        index as f32 / (length - 1) as f32
    }
}

fn normalized_coordinate(coordinate: f32, length: usize) -> f32 {
    if length <= 1 {
        0.0
    } else {
        coordinate / (length - 1) as f32
    }
}

fn detector_rho(index: usize, count: usize, radius: f32) -> f32 {
    (normalized_index(index, count) * 2.0 - 1.0) * radius
}

fn rho_to_detector(rho: f32, detectors: usize, radius: f32) -> f32 {
    (rho / radius * 0.5 + 0.5) * detectors.saturating_sub(1) as f32
}

fn add_pixel(a: PixelF32, b: PixelF32) -> PixelF32 {
    PixelF32 {
        alpha: a.alpha + b.alpha,
        red: a.red + b.red,
        green: a.green + b.green,
        blue: a.blue + b.blue,
    }
}

fn scale_pixel(pixel: PixelF32, scale: f32) -> PixelF32 {
    PixelF32 {
        alpha: pixel.alpha * scale,
        red: pixel.red * scale,
        green: pixel.green * scale,
        blue: pixel.blue * scale,
    }
}

fn lerp_pixel_local(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    add_pixel(scale_pixel(a, 1.0 - t), scale_pixel(b, t))
}

fn gray_pixel(value: f32) -> PixelF32 {
    PixelF32 {
        alpha: 1.0,
        red: value,
        green: value,
        blue: value,
    }
}

fn finish_pixel(mut pixel: PixelF32, settings: Settings) -> PixelF32 {
    pixel.red = pixel.red * settings.output_gain + settings.output_bias * pixel.alpha;
    pixel.green = pixel.green * settings.output_gain + settings.output_bias * pixel.alpha;
    pixel.blue = pixel.blue * settings.output_gain + settings.output_bias * pixel.alpha;
    sanitize_pixel(pixel, settings.clamp_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pixels_bits_eq(actual: &[PixelF32], expected: &[PixelF32]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                actual.alpha.to_bits(),
                expected.alpha.to_bits(),
                "pixel {index} alpha"
            );
            assert_eq!(
                actual.red.to_bits(),
                expected.red.to_bits(),
                "pixel {index} red"
            );
            assert_eq!(
                actual.green.to_bits(),
                expected.green.to_bits(),
                "pixel {index} green"
            );
            assert_eq!(
                actual.blue.to_bits(),
                expected.blue.to_bits(),
                "pixel {index} blue"
            );
        }
    }

    fn render_polar_reference(
        source: &[PixelF32],
        src_width: usize,
        src_height: usize,
        out_width: usize,
        out_height: usize,
        settings: Settings,
    ) -> Vec<PixelF32> {
        let (center, radius) = resolve_geometry(out_width, out_height, settings);
        let mut output = vec![TRANSPARENT; out_width * out_height];
        for y in 0..out_height {
            let v = normalized_index(y, out_height);
            for x in 0..out_width {
                let u = normalized_index(x, out_width);
                let coordinate = match (settings.space, settings.direction) {
                    (Space::Polar, Direction::Forward) => {
                        let theta = settings.angle_offset + u * TAU;
                        let r = v * radius;
                        Some((
                            center.0 + r * theta.cos() / settings.pixel_scale.0,
                            center.1 + r * theta.sin() / settings.pixel_scale.1,
                        ))
                    }
                    (Space::LogPolar, Direction::Forward) => {
                        let theta = settings.angle_offset + u * TAU;
                        let min_radius = settings.log_min_radius.min(radius * 0.999).max(0.001);
                        let r = min_radius * (radius / min_radius).powf(v);
                        Some((
                            center.0 + r * theta.cos() / settings.pixel_scale.0,
                            center.1 + r * theta.sin() / settings.pixel_scale.1,
                        ))
                    }
                    (Space::SpiralPolar, Direction::Forward) => {
                        let theta = settings.angle_offset + (u + v * settings.spiral_turns) * TAU;
                        let r = v * radius;
                        Some((
                            center.0 + r * theta.cos() / settings.pixel_scale.0,
                            center.1 + r * theta.sin() / settings.pixel_scale.1,
                        ))
                    }
                    (Space::Polar | Space::LogPolar | Space::SpiralPolar, Direction::Inverse) => {
                        let dx = (x as f32 - center.0) * settings.pixel_scale.0;
                        let dy = (y as f32 - center.1) * settings.pixel_scale.1;
                        let r = dx.hypot(dy);
                        if r > radius {
                            None
                        } else {
                            let radial_position = r / radius;
                            let spiral_phase = if settings.space == Space::SpiralPolar {
                                radial_position * settings.spiral_turns * TAU
                            } else {
                                0.0
                            };
                            let angle_u = (dy.atan2(dx) - settings.angle_offset - spiral_phase)
                                .rem_euclid(TAU)
                                / TAU;
                            let radius_v = if settings.space == Space::LogPolar {
                                let min_radius =
                                    settings.log_min_radius.min(radius * 0.999).max(0.001);
                                if r < min_radius {
                                    return_coordinate_none()
                                } else {
                                    (r / min_radius).ln() / (radius / min_radius).ln()
                                }
                            } else {
                                r / radius
                            };
                            if radius_v.is_finite() {
                                Some((
                                    angle_u * src_width.saturating_sub(1) as f32,
                                    radius_v * src_height.saturating_sub(1) as f32,
                                ))
                            } else {
                                None
                            }
                        }
                    }
                    _ => unreachable!(),
                };
                let pixel = coordinate
                    .map(|(sx, sy)| {
                        sample(
                            source,
                            src_width,
                            src_height,
                            sx,
                            sy,
                            settings.interpolation,
                            settings.edge,
                        )
                    })
                    .unwrap_or(TRANSPARENT);
                output[y * out_width + x] = finish_pixel(pixel, settings);
            }
        }
        output
    }

    fn sobel_edge_channels_reference(
        source: &[PixelF32],
        width: usize,
        height: usize,
        pixel_scale: (f32, f32),
        mode: HoughChannels,
    ) -> Vec<[f32; 4]> {
        let mut result = vec![[0.0; 4]; width * height];
        let get = |x: i64, y: i64| {
            analysis_components(
                sample_nearest(source, width, height, x as f32, y as f32, SampleEdge::Clamp),
                mode,
            )
        };
        for y in 0..height {
            for x in 0..width {
                let x = x as i64;
                let y = y as i64;
                let p00 = get(x - 1, y - 1);
                let p10 = get(x, y - 1);
                let p20 = get(x + 1, y - 1);
                let p01 = get(x - 1, y);
                let p21 = get(x + 1, y);
                let p02 = get(x - 1, y + 1);
                let p12 = get(x, y + 1);
                let p22 = get(x + 1, y + 1);
                let mut magnitude = [0.0; 4];
                for channel in 0..4 {
                    let gx = (-p00[channel] + p20[channel] - 2.0 * p01[channel]
                        + 2.0 * p21[channel]
                        - p02[channel]
                        + p22[channel])
                        / pixel_scale.0.max(1.0e-6);
                    let gy = (-p00[channel] - 2.0 * p10[channel] - p20[channel]
                        + p02[channel]
                        + 2.0 * p12[channel]
                        + p22[channel])
                        / pixel_scale.1.max(1.0e-6);
                    magnitude[channel] = (gx.hypot(gy) * 0.25).clamp(0.0, 1.0);
                }
                result[y as usize * width + x as usize] = magnitude;
            }
        }
        result
    }

    fn forward_radon_reference(
        source: &[PixelF32],
        width: usize,
        height: usize,
        settings: Settings,
    ) -> Vec<PixelF32> {
        let (center, radius) = resolve_geometry(width, height, settings);
        let ray_samples = effective_ray_samples(settings);
        let mut result = vec![TRANSPARENT; settings.angle_samples * settings.detector_samples];
        for detector in 0..settings.detector_samples {
            let rho = detector_rho(detector, settings.detector_samples, radius);
            for angle in 0..settings.angle_samples {
                let theta =
                    settings.angle_offset + angle as f32 * PI / settings.angle_samples as f32;
                let cos = theta.cos();
                let sin = theta.sin();
                let mut sum = TRANSPARENT;
                for ray in 0..ray_samples {
                    let t = detector_rho(ray, ray_samples, radius);
                    let pixel = sample_bilinear(
                        source,
                        width,
                        height,
                        center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                        center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                        SampleEdge::Transparent,
                    );
                    let endpoint_weight = if ray == 0 || ray + 1 == ray_samples {
                        0.5
                    } else {
                        1.0
                    };
                    sum = add_pixel(sum, scale_pixel(pixel, endpoint_weight));
                }
                result[detector * settings.angle_samples + angle] =
                    scale_pixel(sum, 1.0 / ray_samples.saturating_sub(1).max(1) as f32);
            }
        }
        result
    }

    fn forward_hough_reference(
        source: &[PixelF32],
        width: usize,
        height: usize,
        settings: Settings,
    ) -> Vec<PixelF32> {
        let (center, radius) = resolve_geometry(width, height, settings);
        let ray_samples = effective_ray_samples(settings);
        let edges = sobel_edge_channels_reference(
            source,
            width,
            height,
            settings.pixel_scale,
            settings.hough_channels,
        );
        let mut votes = vec![[0.0_f32; 4]; settings.angle_samples * settings.detector_samples];
        let mut maximum = [0.0_f32; 4];
        let alpha_fallback = if source.is_empty() {
            0.0
        } else {
            source.iter().map(|pixel| pixel.alpha).sum::<f32>() / source.len() as f32
        };
        for detector in 0..settings.detector_samples {
            let rho = detector_rho(detector, settings.detector_samples, radius);
            for angle in 0..settings.angle_samples {
                let theta =
                    settings.angle_offset + angle as f32 * PI / settings.angle_samples as f32;
                let cos = theta.cos();
                let sin = theta.sin();
                let mut sum = [0.0_f32; 4];
                for ray in 0..ray_samples {
                    let t = detector_rho(ray, ray_samples, radius);
                    let magnitude = sample_channel_bilinear(
                        &edges,
                        width,
                        height,
                        center.0 + (rho * cos - t * sin) / settings.pixel_scale.0,
                        center.1 + (rho * sin + t * cos) / settings.pixel_scale.1,
                    );
                    for channel in 0..4 {
                        if magnitude[channel] >= settings.hough_threshold {
                            sum[channel] += if settings.weighted_hough {
                                magnitude[channel]
                            } else {
                                1.0
                            };
                        }
                    }
                }
                let vote = sum.map(|value| value / ray_samples as f32);
                votes[detector * settings.angle_samples + angle] = vote;
                for channel in 0..4 {
                    maximum[channel] = maximum[channel].max(vote[channel]);
                }
            }
        }
        votes
            .into_iter()
            .map(|value| {
                let normalized = [
                    value[0] / maximum[0].max(1.0e-8),
                    value[1] / maximum[1].max(1.0e-8),
                    value[2] / maximum[2].max(1.0e-8),
                    if maximum[3] <= 1.0e-8 {
                        alpha_fallback
                    } else {
                        value[3] / maximum[3]
                    },
                ];
                if settings.hough_channels.is_rgba() {
                    PixelF32 {
                        alpha: normalized[3],
                        red: normalized[0],
                        green: normalized[1],
                        blue: normalized[2],
                    }
                } else {
                    gray_pixel(normalized[0])
                }
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn backproject_reference(
        sinogram: &[PixelF32],
        angles: usize,
        detectors: usize,
        width: usize,
        height: usize,
        settings: Settings,
        normalize: bool,
    ) -> Vec<PixelF32> {
        let (center, radius) = resolve_geometry(width, height, settings);
        let mut result = vec![TRANSPARENT; width * height];
        let mut maximum = 0.0_f32;
        for y in 0..height {
            for x in 0..width {
                let dx = (x as f32 - center.0) * settings.pixel_scale.0;
                let dy = (y as f32 - center.1) * settings.pixel_scale.1;
                let mut sum = TRANSPARENT;
                for angle in 0..angles {
                    let theta = settings.angle_offset + angle as f32 * PI / angles as f32;
                    let rho = dx * theta.cos() + dy * theta.sin();
                    let detector = rho_to_detector(rho, detectors, radius);
                    sum = add_pixel(
                        sum,
                        sample_sinogram_detector(sinogram, angles, detectors, angle, detector),
                    );
                }
                let pixel = scale_pixel(sum, PI / angles as f32);
                maximum = maximum.max(pixel.red.max(pixel.green).max(pixel.blue).abs());
                result[y * width + x] = pixel;
            }
        }
        if normalize {
            let scale = 1.0 / maximum.max(1.0e-8);
            for pixel in &mut result {
                *pixel = scale_pixel(*pixel, scale);
                pixel.alpha = 1.0;
            }
        }
        result
    }

    fn ram_lak_filter_reference(
        sinogram: &[PixelF32],
        angles: usize,
        detectors: usize,
        radius: usize,
    ) -> Vec<PixelF32> {
        let mut result = vec![TRANSPARENT; sinogram.len()];
        for detector in 0..detectors {
            for angle in 0..angles {
                let mut sum = TRANSPARENT;
                for offset in -(radius as isize)..=(radius as isize) {
                    let source_detector = detector as isize + offset;
                    if !(0..detectors as isize).contains(&source_detector) {
                        continue;
                    }
                    let coefficient = ram_lak_coefficient(offset);
                    sum = add_pixel(
                        sum,
                        scale_pixel(
                            sinogram[source_detector as usize * angles + angle],
                            coefficient,
                        ),
                    );
                }
                result[detector * angles + angle] = sum;
            }
        }
        result
    }

    fn test_settings(space: Space) -> Settings {
        Settings {
            space,
            direction: Direction::Forward,
            center_mode: CenterMode::LayerCenter,
            center: (0.0, 0.0),
            radius_mode: RadiusMode::Auto,
            radius: 1.0,
            log_min_radius: 1.0,
            angle_offset: 0.0,
            angle_samples: 90,
            detector_samples: 48,
            ray_samples: 64,
            hough_threshold: 0.15,
            weighted_hough: true,
            filter_radius: 15,
            interpolation: Interpolation::Bilinear,
            edge: SampleEdge::Transparent,
            output_gain: 1.0,
            output_bias: 0.0,
            clamp_output: false,
            pixel_scale: (1.0, 1.0),
            spiral_turns: 1.0,
            focus_ratio: 0.5,
            coordinate_extent: 3.0,
            hough_channels: HoughChannels::Luminance,
            post_transform: PostTransform {
                anchor: (4.0, 3.0),
                position: (4.0, 3.0),
                scale: (1.0, 1.0),
                rotation: 0.0,
                skew: 0.0,
                skew_axis: 0.0,
            },
        }
    }

    fn representative_pixels(width: usize, height: usize) -> Vec<PixelF32> {
        (0..width * height)
            .map(|index| {
                let x = index % width;
                let y = index / width;
                PixelF32 {
                    alpha: 0.35 + ((x * 7 + y * 3) % 11) as f32 / 17.0,
                    red: ((x * 5 + y * 2) % 13) as f32 / 12.0,
                    green: ((x * 3 + y * 7) % 17) as f32 / 16.0,
                    blue: ((x * 11 + y * 5) % 19) as f32 / 18.0,
                }
            })
            .collect()
    }

    #[test]
    fn post_transform_inverse_undoes_full_affine_mapping() {
        let post_transform = PostTransform {
            anchor: (47.0, 31.0),
            position: (123.0, -18.0),
            scale: (-1.25, 0.65),
            rotation: 37.0_f32.to_radians(),
            skew: -21.0_f32.to_radians(),
            skew_axis: 14.0_f32.to_radians(),
        };
        let pixel_scale = (2.4, 3.0);
        let inverse = post_transform
            .matrix()
            .inverse()
            .expect("non-zero scale must be invertible");
        for source in [(0.0, 0.0), (47.0, 31.0), (103.0, -18.0)] {
            let destination = forward_post_transform_point(source, post_transform, pixel_scale);
            let rebuilt =
                inverse_post_transform_point(destination, post_transform, pixel_scale, inverse);
            assert!((rebuilt.0 - source.0).abs() < 1.0e-4);
            assert!((rebuilt.1 - source.1).abs() < 1.0e-4);
        }
    }

    #[test]
    fn identity_post_transform_is_bitwise_compatible_in_every_space() {
        let width = 9;
        let height = 7;
        let source = representative_pixels(width, height);
        for space in [
            Space::Polar,
            Space::LogPolar,
            Space::SquareDisc,
            Space::SpiralPolar,
            Space::Elliptic,
            Space::Parabolic,
            Space::Bipolar,
            Space::Radon,
            Space::LineHough,
        ] {
            for direction in [Direction::Forward, Direction::Inverse] {
                let mut baseline_settings = test_settings(space);
                baseline_settings.direction = direction;
                baseline_settings.angle_samples = width;
                baseline_settings.detector_samples = height;
                baseline_settings.ray_samples = 9;
                baseline_settings.filter_radius = 3;
                let baseline = if space.is_integral() {
                    render_integral(&source, width, height, width, height, baseline_settings)
                } else {
                    render_coordinates(&source, width, height, width, height, baseline_settings)
                };

                let moved_identity = Settings {
                    post_transform: PostTransform {
                        anchor: (812.5, -209.25),
                        position: (812.5, -209.25),
                        ..baseline_settings.post_transform
                    },
                    ..baseline_settings
                };
                let actual = if space.is_integral() {
                    render_integral(&source, width, height, width, height, moved_identity)
                } else {
                    render_coordinates(&source, width, height, width, height, moved_identity)
                };
                assert_pixels_bits_eq(&actual, &baseline);
            }
        }
    }

    #[test]
    fn post_translation_is_composed_before_each_coordinate_formula() {
        let width = 9;
        let height = 7;
        let source = representative_pixels(width, height);
        for space in [
            Space::Polar,
            Space::LogPolar,
            Space::SquareDisc,
            Space::SpiralPolar,
            Space::Elliptic,
            Space::Parabolic,
            Space::Bipolar,
        ] {
            for direction in [Direction::Forward, Direction::Inverse] {
                let baseline_settings = Settings {
                    direction,
                    ..test_settings(space)
                };
                let baseline =
                    render_coordinates(&source, width, height, width, height, baseline_settings);
                let translated_settings = Settings {
                    post_transform: PostTransform {
                        position: (
                            baseline_settings.post_transform.position.0 + 1.0,
                            baseline_settings.post_transform.position.1,
                        ),
                        ..baseline_settings.post_transform
                    },
                    ..baseline_settings
                };
                let translated =
                    render_coordinates(&source, width, height, width, height, translated_settings);
                for y in 0..height {
                    for x in 1..width {
                        assert_pixels_bits_eq(
                            &translated[y * width + x..=y * width + x],
                            &baseline[y * width + x - 1..=y * width + x - 1],
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn transformed_radon_evaluates_unclamped_projection_coordinates() {
        let width = 9;
        let height = 7;
        let settings = Settings {
            angle_samples: width,
            detector_samples: height,
            ray_samples: 9,
            post_transform: PostTransform {
                position: (13.0, 3.0),
                ..test_settings(Space::Radon).post_transform
            },
            ..test_settings(Space::Radon)
        };
        let inverse = settings
            .post_transform
            .matrix()
            .inverse()
            .expect("translation is invertible");
        let (theta, detector_v) =
            transformed_projection_coordinate(0, 0, width, height, settings, inverse);
        // A nine-pixel right translation maps the first output sample to x=-9.
        // The angle remains outside the nominal [0, PI] interval instead of
        // being clamped or tiled from a finite accumulator image.
        assert!((theta - (-9.0 / 8.0 * PI)).abs() < 1.0e-6);
        assert_eq!(detector_v.to_bits(), 0.0_f32.to_bits());

        let source = representative_pixels(width, height);
        let transformed =
            forward_radon_post_transformed(&source, width, height, width, height, settings);
        assert_eq!(transformed.len(), width * height);
        assert!(transformed.iter().all(|pixel| {
            pixel.alpha.is_finite()
                && pixel.red.is_finite()
                && pixel.green.is_finite()
                && pixel.blue.is_finite()
        }));
    }

    #[test]
    fn polar_coordinate_caches_match_reference_bitwise() {
        let width = 11;
        let height = 9;
        let source = representative_pixels(width, height);
        for space in [Space::Polar, Space::LogPolar, Space::SpiralPolar] {
            for direction in [Direction::Forward, Direction::Inverse] {
                let settings = Settings {
                    direction,
                    center_mode: CenterMode::Custom,
                    center: (4.25, 3.75),
                    radius_mode: RadiusMode::Custom,
                    radius: 5.125,
                    log_min_radius: 0.625,
                    angle_offset: 0.371,
                    pixel_scale: (1.125, 0.875),
                    ..test_settings(space)
                };
                let actual = render_coordinates(&source, width, height, width, height, settings);
                let expected =
                    render_polar_reference(&source, width, height, width, height, settings);
                assert_pixels_bits_eq(&actual, &expected);
            }
        }
    }

    #[test]
    fn projection_geometry_caches_match_reference_bitwise() {
        let width = 9;
        let height = 7;
        let source = representative_pixels(width, height);
        let settings = Settings {
            angle_samples: 13,
            detector_samples: 11,
            ray_samples: 15,
            angle_offset: -0.217,
            center_mode: CenterMode::Custom,
            center: (3.75, 2.625),
            radius_mode: RadiusMode::Custom,
            radius: 4.875,
            pixel_scale: (1.25, 0.8),
            ..test_settings(Space::Radon)
        };

        let actual_sinogram = forward_radon(&source, width, height, settings);
        let expected_sinogram = forward_radon_reference(&source, width, height, settings);
        assert_pixels_bits_eq(&actual_sinogram, &expected_sinogram);

        let actual_backprojection = backproject(
            &actual_sinogram,
            settings.angle_samples,
            settings.detector_samples,
            width,
            height,
            settings,
            false,
        );
        let expected_backprojection = backproject_reference(
            &expected_sinogram,
            settings.angle_samples,
            settings.detector_samples,
            width,
            height,
            settings,
            false,
        );
        assert_pixels_bits_eq(&actual_backprojection, &expected_backprojection);

        let actual_filtered = ram_lak_filter(
            &actual_sinogram,
            settings.angle_samples,
            settings.detector_samples,
            5,
        );
        let expected_filtered = ram_lak_filter_reference(
            &expected_sinogram,
            settings.angle_samples,
            settings.detector_samples,
            5,
        );
        assert_pixels_bits_eq(&actual_filtered, &expected_filtered);
    }

    #[test]
    fn rgba_hough_and_sobel_cache_match_reference_bitwise() {
        let width = 9;
        let height = 7;
        let source = representative_pixels(width, height);
        let settings = Settings {
            angle_samples: 13,
            detector_samples: 11,
            ray_samples: 15,
            angle_offset: 0.413,
            hough_threshold: 0.08,
            hough_channels: HoughChannels::Rgba,
            pixel_scale: (1.125, 0.75),
            ..test_settings(Space::LineHough)
        };

        for mode in [
            HoughChannels::Luminance,
            HoughChannels::Rgba,
            HoughChannels::Red,
            HoughChannels::Green,
            HoughChannels::Blue,
            HoughChannels::Alpha,
        ] {
            let actual_edges =
                sobel_edge_channels(&source, width, height, settings.pixel_scale, mode);
            let expected_edges =
                sobel_edge_channels_reference(&source, width, height, settings.pixel_scale, mode);
            assert_eq!(actual_edges.len(), expected_edges.len());
            for (index, (actual, expected)) in actual_edges.iter().zip(&expected_edges).enumerate()
            {
                for channel in 0..4 {
                    assert_eq!(
                        actual[channel].to_bits(),
                        expected[channel].to_bits(),
                        "mode {mode:?}, edge {index}, channel {channel}"
                    );
                }
            }
        }

        let actual = forward_hough(&source, width, height, settings);
        let expected = forward_hough_reference(&source, width, height, settings);
        assert_pixels_bits_eq(&actual, &expected);
    }

    #[test]
    fn concentric_square_disc_round_trip() {
        for point in [
            (-1.0, -1.0),
            (-0.8, 0.2),
            (0.3, -0.7),
            (1.0, 0.5),
            (0.0, 0.0),
        ] {
            let disc = square_to_disc(point.0, point.1);
            let square = disc_to_square(disc.0, disc.1);
            assert!((square.0 - point.0).abs() < 1.0e-5);
            assert!((square.1 - point.1).abs() < 1.0e-5);
        }
    }

    #[test]
    fn parabolic_inverse_keeps_negative_x_branch() {
        let width = 9;
        let height = 9;
        let source: Vec<PixelF32> = (0..width * height)
            .map(|index| {
                let value = (index % width) as f32 / (width - 1) as f32;
                PixelF32 {
                    alpha: 1.0,
                    red: value,
                    green: value,
                    blue: value,
                }
            })
            .collect();
        let output = render_coordinates(
            &source,
            width,
            height,
            width,
            height,
            Settings {
                direction: Direction::Inverse,
                ..test_settings(Space::Parabolic)
            },
        );
        assert!(output[(height / 2) * width].red > 0.75);
    }

    #[test]
    fn square_disc_auto_radius_stays_inside_canvas() {
        let settings = test_settings(Space::SquareDisc);
        let (center, radius) = resolve_geometry(16, 10, settings);
        assert!((radius - 4.5).abs() < 1.0e-6);

        for sx in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for sy in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                let (dx, dy) = square_to_disc(sx, sy);
                let x = center.0 + dx * radius / settings.pixel_scale.0;
                let y = center.1 + dy * radius / settings.pixel_scale.1;
                assert!((0.0..=15.0).contains(&x));
                assert!((0.0..=9.0).contains(&y));
            }
        }
    }

    #[test]
    fn pixel_geometry_accounts_for_par_and_downsample() {
        assert_eq!(square_pixel_scale(2.0, 0.5, 0.25), (4.0, 4.0));
    }

    #[test]
    fn extended_bounds_origin_localizes_custom_center() {
        let local = point_in_checkout_world((50.0, 25.0), ae::Point { h: -20, v: 10 });
        assert_eq!(local, (70.0, 15.0));
        let local_positive = point_in_checkout_world((50.0, 25.0), ae::Point { h: 12, v: 8 });
        assert_eq!(local_positive, (38.0, 17.0));
    }

    #[test]
    fn physical_radius_is_resolution_invariant() {
        let full_settings = test_settings(Space::Polar);
        let (_, full_radius) = resolve_geometry(101, 51, full_settings);

        let mut half_settings = full_settings;
        half_settings.pixel_scale = (2.0, 2.0);
        let (_, half_radius) = resolve_geometry(51, 26, half_settings);
        assert!((full_radius - half_radius).abs() < 1.0e-5);

        let mut custom = full_settings;
        custom.center_mode = CenterMode::Custom;
        custom.center = (50.0, 25.0);
        custom.radius_mode = RadiusMode::Custom;
        custom.radius = 80.0;
        custom.log_min_radius = 4.0;
        let scaled = scaled_geometry_settings(custom, 0.5, 0.5);
        assert_eq!(scaled.center, (25.0, 12.5));
        assert_eq!(scaled.pixel_scale, (2.0, 2.0));
        assert_eq!(scaled.radius, 80.0);
        assert_eq!(scaled.log_min_radius, 4.0);
    }

    #[test]
    fn hough_edge_strength_is_resolution_invariant() {
        let width = 7;
        let height = 3;
        let full: Vec<PixelF32> = (0..width * height)
            .map(|index| gray_pixel((index % width) as f32 * 0.05))
            .collect();
        let half: Vec<PixelF32> = (0..width * height)
            .map(|index| gray_pixel((index % width) as f32 * 0.1))
            .collect();
        let full_edges = sobel_edges(&full, width, height, (1.0, 1.0));
        let half_edges = sobel_edges(&half, width, height, (2.0, 2.0));
        assert!((full_edges[width + 3] - half_edges[width + 3]).abs() < 1.0e-6);
    }

    #[test]
    fn line_hough_rgba_channels_are_analyzed_independently() {
        let width = 7;
        let height = 7;
        let pixels: Vec<PixelF32> = (0..width * height)
            .map(|index| {
                let x = index % width;
                let y = index / width;
                PixelF32 {
                    alpha: 1.0,
                    red: if x >= 3 { 1.0 } else { 0.0 },
                    green: if y >= 3 { 1.0 } else { 0.0 },
                    blue: if x + y >= 7 { 1.0 } else { 0.0 },
                }
            })
            .collect();
        let rgba = sobel_edge_channels(&pixels, width, height, (1.0, 1.0), HoughChannels::Rgba);
        assert!(rgba[width + 2][0] > rgba[width + 2][1]);
        assert!(rgba[2 * width + 1][1] > rgba[2 * width + 1][0]);

        let red = sobel_edge_channels(&pixels, width, height, (1.0, 1.0), HoughChannels::Red);
        assert!(red.iter().all(|channels| {
            channels
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 1.0e-7)
        }));
    }

    #[test]
    fn detector_coordinate_round_trip() {
        let radius = 123.0;
        for detector in [0, 1, 37, 255] {
            let rho = detector_rho(detector, 256, radius);
            let rebuilt = rho_to_detector(rho, 256, radius);
            assert!((rebuilt - detector as f32).abs() < 1.0e-4);
        }
    }

    #[test]
    fn ram_lak_kernel_is_even_and_finite() {
        for index in 1..32 {
            let positive = ram_lak_coefficient(index);
            let negative = ram_lak_coefficient(-index);
            assert!(positive.is_finite());
            assert!((positive - negative).abs() < 1.0e-8);
        }
    }

    #[test]
    fn inverse_analysis_resolution_preserves_aspect_ratio() {
        let (width, height) = reconstruction_dimensions(1920, 1080, 256, 180);
        assert_eq!(width, 256);
        assert_eq!(height, 144);
        let (small_width, small_height) = reconstruction_dimensions(100, 50, 256, 180);
        assert_eq!((small_width, small_height), (100, 50));
    }

    #[test]
    fn integral_quality_is_bounded_by_work_budget() {
        let mut settings = test_settings(Space::Radon);
        settings.angle_samples = 720;
        settings.detector_samples = 2048;
        settings.ray_samples = 2048;
        settings.filter_radius = 127;

        let rays = effective_ray_samples(settings);
        assert!(
            settings
                .angle_samples
                .saturating_mul(settings.detector_samples)
                .saturating_mul(rays)
                <= MAX_INTEGRAL_SAMPLE_EVALUATIONS
        );

        let filter_radius = effective_filter_radius(
            settings.angle_samples,
            settings.detector_samples,
            settings.filter_radius,
        );
        let filter_taps = filter_radius * 2 + 1;
        assert!(
            settings
                .angle_samples
                .saturating_mul(settings.detector_samples)
                .saturating_mul(filter_taps)
                <= MAX_INTEGRAL_SAMPLE_EVALUATIONS
        );

        let dimensions = reconstruction_dimensions(
            4096,
            2160,
            settings.detector_samples,
            settings.angle_samples,
        );
        assert!(
            dimensions
                .0
                .saturating_mul(dimensions.1)
                .saturating_mul(settings.angle_samples)
                <= MAX_INTEGRAL_SAMPLE_EVALUATIONS
        );
    }

    #[test]
    fn radon_fbp_round_trip_preserves_phantom_level() {
        let width = 32;
        let height = 32;
        let mut phantom = vec![TRANSPARENT; width * height];
        let center = ((width as f32 - 1.0) * 0.5, (height as f32 - 1.0) * 0.5);
        for y in 0..height {
            for x in 0..width {
                if (x as f32 - center.0).hypot(y as f32 - center.1) <= 7.0 {
                    phantom[y * width + x] = PixelF32 {
                        alpha: 1.0,
                        red: 1.0,
                        green: 0.5,
                        blue: 0.25,
                    };
                }
            }
        }

        let settings = test_settings(Space::Radon);
        let sinogram = forward_radon(&phantom, width, height, settings);
        let reconstruction = inverse_radon(
            &sinogram,
            settings.angle_samples,
            settings.detector_samples,
            width,
            height,
            Settings {
                direction: Direction::Inverse,
                ..settings
            },
        );

        let mut inside_sum = 0.0;
        let mut inside_count = 0;
        for y in 0..height {
            for x in 0..width {
                if (x as f32 - center.0).hypot(y as f32 - center.1) <= 4.0 {
                    inside_sum += reconstruction[y * width + x].red;
                    inside_count += 1;
                }
            }
        }
        let inside_mean = inside_sum / inside_count as f32;
        assert!(
            (0.8..=1.2).contains(&inside_mean),
            "FBP level was {inside_mean}"
        );
        assert!(reconstruction[0].red.abs() < 0.35);
    }
}
