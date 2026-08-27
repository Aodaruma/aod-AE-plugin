#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;
use utils::image::{SampleEdge, SampleFilter, read_layer, sample_filtered, sanitize_pixel};

const PLUGIN_DESCRIPTION: &str = "Applies an AE-style affine image transform with selectable reconstruction and outside-image sampling.";

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TransformStart,
    AnchorPoint,
    Position,
    SeparateScale,
    Scale,
    ScaleX,
    ScaleY,
    Rotation,
    Skew,
    SkewAxis,
    Opacity,
    TransformEnd,
    SamplingStart,
    Interpolation,
    MitchellB,
    MitchellC,
    LanczosLobes,
    EwaRadius,
    SampleOutside,
    OutsideMode,
    SamplingEnd,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix2 {
    m00: f32,
    m01: f32,
    m10: f32,
    m11: f32,
}

impl Matrix2 {
    #[cfg(test)]
    const fn identity() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
        }
    }

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

    /// Matrix multiplication in application order: `self * rhs`.
    const fn mul(self, rhs: Self) -> Self {
        Self {
            m00: self.m00 * rhs.m00 + self.m01 * rhs.m10,
            m01: self.m00 * rhs.m01 + self.m01 * rhs.m11,
            m10: self.m10 * rhs.m00 + self.m11 * rhs.m10,
            m11: self.m10 * rhs.m01 + self.m11 * rhs.m11,
        }
    }

    fn transform(self, point: [f32; 2]) -> [f32; 2] {
        [
            self.m00 * point[0] + self.m01 * point[1],
            self.m10 * point[0] + self.m11 * point[1],
        ]
    }

    fn inverse(self) -> Option<Self> {
        let determinant = self.m00 * self.m11 - self.m01 * self.m10;
        if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
            return None;
        }
        let inverse = determinant.recip();
        Some(Self {
            m00: self.m11 * inverse,
            m01: -self.m01 * inverse,
            m10: -self.m10 * inverse,
            m11: self.m00 * inverse,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct TransformSettings {
    anchor: [f32; 2],
    position: [f32; 2],
    scale: [f32; 2],
    rotation: f32,
    skew: f32,
    skew_axis: f32,
    opacity: f32,
    filter: SampleFilter,
    edge: SampleEdge,
    pixel_scale: [f32; 2],
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_group(
            Params::TransformStart,
            Params::TransformEnd,
            "Transform",
            false,
            |params| {
                params.add(
                    Params::AnchorPoint,
                    "Anchor Point",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                params.add(
                    Params::Position,
                    "Position",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                params.add_with_flags(
                    Params::SeparateScale,
                    "Separate Dimensions",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ParamFlag::SUPERVISE,
                    ParamUIFlags::empty(),
                )?;
                add_scale_slider(params, Params::Scale, "Scale", true)?;
                add_scale_slider(params, Params::ScaleX, "Scale X", false)?;
                add_scale_slider(params, Params::ScaleY, "Scale Y", false)?;
                params.add(
                    Params::Rotation,
                    "Rotation",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                params.add(
                    Params::Skew,
                    "Skew",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                params.add(
                    Params::SkewAxis,
                    "Skew Axis",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                    }),
                )?;
                params.add(
                    Params::Opacity,
                    "Opacity",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(100.0);
                        d.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::SamplingStart,
            Params::SamplingEnd,
            "Sampling",
            true,
            |params| {
                params.add_with_flags(
                    Params::Interpolation,
                    "Interpolation",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Nearest",
                            "Bilinear",
                            "Bicubic",
                            "Mitchell-Netravali",
                            "Lanczos",
                            "Cubic B-Spline",
                            "EWA Quadratic",
                        ]);
                        d.set_default(2);
                    }),
                    ParamFlag::SUPERVISE,
                    ParamUIFlags::empty(),
                )?;
                add_hidden_float_slider(
                    params,
                    Params::MitchellB,
                    "Mitchell B",
                    0.0,
                    1.0,
                    1.0 / 3.0,
                    3,
                )?;
                add_hidden_float_slider(
                    params,
                    Params::MitchellC,
                    "Mitchell C",
                    0.0,
                    1.0,
                    1.0 / 3.0,
                    3,
                )?;
                add_hidden_float_slider(
                    params,
                    Params::LanczosLobes,
                    "Lanczos Lobes",
                    1.0,
                    8.0,
                    3.0,
                    2,
                )?;
                add_hidden_float_slider(params, Params::EwaRadius, "EWA Radius", 0.5, 8.0, 2.0, 2)?;
                params.add_with_flags(
                    Params::SampleOutside,
                    "Sample Outside Image",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ParamFlag::SUPERVISE,
                    ParamUIFlags::empty(),
                )?;
                params.add_with_flags(
                    Params::OutsideMode,
                    "Outside Pixels",
                    PopupDef::setup(|d| {
                        d.set_options(&["Clamp", "Tile", "Mirror"]);
                        d.set_default(1);
                    }),
                    ParamFlag::empty(),
                    ParamUIFlags::INVISIBLE | ParamUIFlags::DISABLED,
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
                        "AOD_ImageTransform - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImageTransform")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_data, in_layer, out_layer, params, RenderPath::Legacy)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let mut request = extra.output_request();
                request.preserve_rgb_of_zero_alpha = 1;
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
            }
            ae::Command::SmartRender { extra } => {
                let callbacks = extra.callbacks();
                let input = callbacks.checkout_layer_pixels(0)?;
                let render_result = (|| -> Result<(), Error> {
                    let output = callbacks.checkout_output()?;
                    if let (Some(input), Some(output)) = (input, output) {
                        self.do_render(in_data, input, output, params, RenderPath::Smart)?;
                    }
                    Ok(())
                })();
                let checkin_result = callbacks.checkin_layer_pixels(0);
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if matches!(
                    params.type_at(param_index),
                    Params::SeparateScale | Params::Interpolation | Params::SampleOutside
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
        let separate = params.get(Params::SeparateScale)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::Scale, !separate)?;
        self.set_param_visible(in_data, params, Params::ScaleX, separate)?;
        self.set_param_visible(in_data, params, Params::ScaleY, separate)?;
        Self::set_param_enabled(params, Params::Scale, !separate)?;
        Self::set_param_enabled(params, Params::ScaleX, separate)?;
        Self::set_param_enabled(params, Params::ScaleY, separate)?;

        let interpolation = params.get(Params::Interpolation)?.as_popup()?.value();
        self.set_param_visible(in_data, params, Params::MitchellB, interpolation == 4)?;
        self.set_param_visible(in_data, params, Params::MitchellC, interpolation == 4)?;
        self.set_param_visible(in_data, params, Params::LanczosLobes, interpolation == 5)?;
        self.set_param_visible(in_data, params, Params::EwaRadius, interpolation == 7)?;

        let sample_outside = params.get(Params::SampleOutside)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::OutsideMode, sample_outside)?;
        Self::set_param_enabled(params, Params::OutsideMode, sample_outside)?;
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

    fn set_param_enabled(
        params: &mut Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ParamUIFlags::DISABLED, !enabled)
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

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
        render_path: RenderPath,
    ) -> Result<(), Error> {
        let src_width = in_layer.width();
        let src_height = in_layer.height();
        let out_height = out_layer.height();
        if src_width == 0 || src_height == 0 || out_layer.width() == 0 || out_height == 0 {
            return Ok(());
        }

        let settings = read_settings(params, in_data)?;
        let inverse = transform_matrix(settings).inverse();
        let source = read_layer(&in_layer);
        let source_origin = source_buffer_origin(in_data, &in_layer, render_path);
        let output_origin = output_buffer_origin(in_data, &out_layer, source_origin, render_path);
        let out_world_type = out_layer.world_type();

        out_layer.iterate(0, out_height as i32, None, |x, y, mut destination| {
            let layer_point = [x as f32 + output_origin[0], y as f32 + output_origin[1]];
            let source_point = inverse
                .map(|matrix| inverse_map(layer_point, settings, matrix))
                .unwrap_or([f32::INFINITY; 2]);
            let local = layer_to_local(source_point, source_origin);
            let sampled = sample_filtered(
                &source,
                src_width,
                src_height,
                local[0],
                local[1],
                settings.edge,
                settings.filter,
            );
            let pixel = apply_opacity(sampled, settings.opacity);
            match out_world_type {
                ae::aegp::WorldType::U8 => destination.set_from_u8(pixel.to_pixel8()),
                ae::aegp::WorldType::U15 => destination.set_from_u16(pixel.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                    destination.set_from_f32(pixel)
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn add_scale_slider(
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
    )
}

#[allow(clippy::too_many_arguments)]
fn add_hidden_float_slider(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    minimum: f32,
    maximum: f32,
    default: f64,
    precision: i16,
) -> Result<(), Error> {
    params.add_with_flags(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(minimum);
            d.set_valid_max(maximum);
            d.set_slider_min(minimum);
            d.set_slider_max(maximum);
            d.set_default(default);
            d.set_precision(precision);
        }),
        ParamFlag::empty(),
        ParamUIFlags::INVISIBLE,
    )
}

fn read_settings(params: &Parameters<Params>, in_data: InData) -> Result<TransformSettings, Error> {
    let separate = params.get(Params::SeparateScale)?.as_checkbox()?.value();
    let scale = if separate {
        [
            slider(params, Params::ScaleX)? * 0.01,
            slider(params, Params::ScaleY)? * 0.01,
        ]
    } else {
        let uniform = slider(params, Params::Scale)? * 0.01;
        [uniform, uniform]
    };
    let interpolation = params.get(Params::Interpolation)?.as_popup()?.value();
    let filter = match interpolation {
        1 => SampleFilter::Nearest,
        3 => SampleFilter::Bicubic,
        4 => SampleFilter::Mitchell {
            b: slider(params, Params::MitchellB)?.clamp(0.0, 1.0),
            c: slider(params, Params::MitchellC)?.clamp(0.0, 1.0),
        },
        5 => SampleFilter::Lanczos {
            lobes: slider(params, Params::LanczosLobes)?.clamp(1.0, 8.0),
        },
        6 => SampleFilter::CubicBSpline,
        7 => SampleFilter::EwaQuadratic {
            radius: slider(params, Params::EwaRadius)?.clamp(0.5, 8.0),
        },
        _ => SampleFilter::Bilinear,
    };
    let requested_edge = match params.get(Params::OutsideMode)?.as_popup()?.value() {
        2 => SampleEdge::Tile,
        3 => SampleEdge::Mirror,
        _ => SampleEdge::Clamp,
    };
    let edge = selected_edge(
        params.get(Params::SampleOutside)?.as_checkbox()?.value(),
        requested_edge,
    );
    let [pixel_scale_x, pixel_scale_y] = render_pixel_scale(in_data);
    Ok(TransformSettings {
        anchor: point(params, Params::AnchorPoint)?,
        position: point(params, Params::Position)?,
        scale: [finite_or(scale[0], 1.0), finite_or(scale[1], 1.0)],
        rotation: angle(params, Params::Rotation)?.to_radians(),
        skew: angle(params, Params::Skew)?.to_radians(),
        skew_axis: angle(params, Params::SkewAxis)?.to_radians(),
        opacity: (slider(params, Params::Opacity)? * 0.01).clamp(0.0, 1.0),
        filter,
        edge,
        pixel_scale: [pixel_scale_x, pixel_scale_y],
    })
}

fn slider(params: &Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok(finite_or(
        params.get(id)?.as_float_slider()?.value() as f32,
        0.0,
    ))
}

fn angle(params: &Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok(finite_or(
        params.get(id)?.as_angle()?.float_value()? as f32,
        0.0,
    ))
}

fn point(params: &Parameters<Params>, id: Params) -> Result<[f32; 2], Error> {
    let param = params.get(id)?;
    let point = param.as_point()?;
    let value = match point.float_value() {
        Ok(value) => [value.x as f32, value.y as f32],
        Err(_) => {
            let value = point.value();
            [value.0, value.1]
        }
    };
    Ok([finite_or(value[0], 0.0), finite_or(value[1], 0.0)])
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn render_pixel_scale(in_data: InData) -> [f32; 2] {
    let pixel_aspect = rational_or_one(in_data.pixel_aspect_ratio());
    let downsample_x = rational_or_one(in_data.downsample_x());
    let downsample_y = rational_or_one(in_data.downsample_y());
    [
        pixel_aspect / downsample_x.max(1.0e-6),
        1.0 / downsample_y.max(1.0e-6),
    ]
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

fn transform_matrix(settings: TransformSettings) -> Matrix2 {
    let scale = Matrix2::scale(settings.scale[0], settings.scale[1]);
    let skew = Matrix2::rotation(settings.skew_axis)
        .mul(Matrix2::shear_x(settings.skew))
        .mul(Matrix2::rotation(-settings.skew_axis));
    Matrix2::rotation(settings.rotation).mul(skew).mul(scale)
}

fn inverse_map(destination: [f32; 2], settings: TransformSettings, inverse: Matrix2) -> [f32; 2] {
    let destination_physical = [
        (destination[0] - settings.position[0]) * settings.pixel_scale[0],
        (destination[1] - settings.position[1]) * settings.pixel_scale[1],
    ];
    let source_physical = inverse.transform(destination_physical);
    [
        settings.anchor[0] + source_physical[0] / settings.pixel_scale[0],
        settings.anchor[1] + source_physical[1] / settings.pixel_scale[1],
    ]
}

#[cfg(test)]
fn forward_map(source: [f32; 2], settings: TransformSettings) -> [f32; 2] {
    let source_physical = [
        (source[0] - settings.anchor[0]) * settings.pixel_scale[0],
        (source[1] - settings.anchor[1]) * settings.pixel_scale[1],
    ];
    let destination_physical = transform_matrix(settings).transform(source_physical);
    [
        settings.position[0] + destination_physical[0] / settings.pixel_scale[0],
        settings.position[1] + destination_physical[1] / settings.pixel_scale[1],
    ]
}

fn layer_to_local(point: [f32; 2], origin: [f32; 2]) -> [f32; 2] {
    [point[0] - origin[0], point[1] - origin[1]]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderPath {
    Legacy,
    Smart,
}

fn resolve_world_origin(
    native: ae::Point,
    fallback: [f32; 2],
    render_path: RenderPath,
) -> [f32; 2] {
    // Smart Render checkouts carry their requested rectangle's layer-space
    // top-left directly in PF_EffectWorld::origin. It is more precise than
    // PF_InData's legacy frame-level offsets, especially for tiles and extra
    // pixels. Zero is also a valid Smart Render tile origin and must be trusted.
    // Legacy Render worlds commonly leave both fields at zero, so only that
    // path deliberately falls through to the PF_InData mapping.
    if render_path == RenderPath::Smart || native.h != 0 || native.v != 0 {
        [native.h as f32, native.v as f32]
    } else {
        fallback
    }
}

fn source_origin_from_pre_effect(pre_effect_origin: ae::Point) -> [f32; 2] {
    // pre_effect_source_origin is the input-buffer position at which layer
    // point (0, 0) appears. Therefore input-buffer point (0, 0) represents
    // layer point (-pre_effect_source_origin).
    [-(pre_effect_origin.h as f32), -(pre_effect_origin.v as f32)]
}

fn output_origin_from_shift(source_origin: [f32; 2], output_shift: ae::Point) -> [f32; 2] {
    // output_origin is the output-buffer position of input-buffer point
    // (0, 0). Consequently output-buffer point (0, 0) is that same layer
    // coordinate shifted in the opposite direction.
    [
        source_origin[0] - output_shift.h as f32,
        source_origin[1] - output_shift.v as f32,
    ]
}

fn source_buffer_origin(in_data: InData, layer: &Layer, render_path: RenderPath) -> [f32; 2] {
    resolve_world_origin(
        layer.origin(),
        source_origin_from_pre_effect(in_data.pre_effect_source_origin()),
        render_path,
    )
}

fn output_buffer_origin(
    in_data: InData,
    layer: &Layer,
    source_origin: [f32; 2],
    render_path: RenderPath,
) -> [f32; 2] {
    resolve_world_origin(
        layer.origin(),
        output_origin_from_shift(source_origin, in_data.output_origin()),
        render_path,
    )
}

fn selected_edge(sample_outside: bool, requested: SampleEdge) -> SampleEdge {
    if sample_outside {
        requested
    } else {
        SampleEdge::Transparent
    }
}

fn apply_opacity(pixel: PixelF32, opacity: f32) -> PixelF32 {
    let opacity = finite_or(opacity, 1.0).clamp(0.0, 1.0);
    sanitize_pixel(
        PixelF32 {
            alpha: pixel.alpha * opacity,
            red: pixel.red * opacity,
            green: pixel.green * opacity,
            blue: pixel.blue * opacity,
        },
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> TransformSettings {
        TransformSettings {
            anchor: [50.0, 40.0],
            position: [50.0, 40.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            skew: 0.0,
            skew_axis: 0.0,
            opacity: 1.0,
            filter: SampleFilter::Bilinear,
            edge: SampleEdge::Transparent,
            pixel_scale: [1.0, 1.0],
        }
    }

    fn assert_point_close(actual: [f32; 2], expected: [f32; 2]) {
        assert!((actual[0] - expected[0]).abs() < 1.0e-4);
        assert!((actual[1] - expected[1]).abs() < 1.0e-4);
    }

    #[test]
    fn identity_inverse_mapping_preserves_coordinates() {
        let settings = settings();
        let inverse = transform_matrix(settings)
            .inverse()
            .expect("identity is invertible");
        for point in [[0.0, 0.0], [50.0, 40.0], [123.5, -17.25]] {
            assert_point_close(inverse_map(point, settings, inverse), point);
        }
    }

    #[test]
    fn inverse_mapping_undoes_full_affine_transform() {
        let mut settings = settings();
        settings.position = [170.0, -30.0];
        settings.scale = [-1.25, 0.65];
        settings.rotation = 37.0_f32.to_radians();
        settings.skew = -21.0_f32.to_radians();
        settings.skew_axis = 14.0_f32.to_radians();
        settings.pixel_scale = [2.4, 3.0];
        let inverse = transform_matrix(settings)
            .inverse()
            .expect("non-zero scales are invertible");
        for source in [[0.0, 0.0], [50.0, 40.0], [103.0, -18.0]] {
            let destination = forward_map(source, settings);
            assert_point_close(inverse_map(destination, settings, inverse), source);
        }
    }

    #[test]
    fn anchor_maps_to_position() {
        let mut settings = settings();
        settings.position = [300.0, 125.0];
        settings.rotation = 90.0_f32.to_radians();
        settings.scale = [2.0, 3.0];
        assert_point_close(forward_map(settings.anchor, settings), settings.position);
    }

    #[test]
    fn layer_origins_map_layer_space_to_checkout_space() {
        assert_eq!(layer_to_local([45.0, 12.0], [-20.0, 10.0]), [65.0, 2.0]);
        assert_eq!(layer_to_local([45.0, 12.0], [30.0, -8.0]), [15.0, 20.0]);
    }

    #[test]
    fn legacy_origin_fallbacks_follow_pf_indata_sign_conventions() {
        // The original source point (0, 0) appears at input local (20, 5), so
        // input local (0, 0) is layer point (-20, -5).
        let source_origin = source_origin_from_pre_effect(ae::Point { h: 20, v: 5 });
        assert_eq!(source_origin, [-20.0, -5.0]);

        // Input local (0, 0) appears at output local (7, 3), so output local
        // (0, 0) is seven/three pixels earlier in layer space.
        let output_origin = output_origin_from_shift(source_origin, ae::Point { h: 7, v: 3 });
        assert_eq!(output_origin, [-27.0, -8.0]);

        // At the documented output shift, identity mapping lands exactly on
        // input local (0, 0).
        let layer_point = [output_origin[0] + 7.0, output_origin[1] + 3.0];
        assert_eq!(layer_to_local(layer_point, source_origin), [0.0, 0.0]);
    }

    #[test]
    fn smart_world_origin_overrides_legacy_fallback_including_zero() {
        let fallback = [-20.0, -5.0];
        assert_eq!(
            resolve_world_origin(ae::Point { h: 12, v: -8 }, fallback, RenderPath::Smart),
            [12.0, -8.0]
        );
        assert_eq!(
            resolve_world_origin(ae::Point { h: 0, v: 0 }, fallback, RenderPath::Smart),
            [0.0, 0.0]
        );
        assert_eq!(
            resolve_world_origin(ae::Point { h: 0, v: 0 }, fallback, RenderPath::Legacy),
            fallback
        );
    }

    #[test]
    fn outside_checkbox_selects_transparent_or_requested_mapping() {
        assert_eq!(
            selected_edge(false, SampleEdge::Mirror),
            SampleEdge::Transparent
        );
        assert_eq!(selected_edge(true, SampleEdge::Tile), SampleEdge::Tile);

        let pixels = [PixelF32 {
            alpha: 1.0,
            red: 0.25,
            green: 0.5,
            blue: 0.75,
        }];
        let transparent = sample_filtered(
            &pixels,
            1,
            1,
            -2.0,
            0.0,
            selected_edge(false, SampleEdge::Clamp),
            SampleFilter::Nearest,
        );
        let clamped = sample_filtered(
            &pixels,
            1,
            1,
            -2.0,
            0.0,
            selected_edge(true, SampleEdge::Clamp),
            SampleFilter::Nearest,
        );
        assert_eq!(transparent.alpha, 0.0);
        assert_eq!(clamped.red, 0.25);
    }

    #[test]
    fn zero_scale_has_no_inverse() {
        let mut settings = settings();
        settings.scale[0] = 0.0;
        assert!(transform_matrix(settings).inverse().is_none());
    }

    #[test]
    fn opacity_preserves_premultiplication() {
        let pixel = PixelF32 {
            alpha: 0.8,
            red: 0.4,
            green: 0.2,
            blue: 0.1,
        };
        let output = apply_opacity(pixel, 0.25);
        assert!((output.alpha - 0.2).abs() < 1.0e-6);
        assert!((output.red - 0.1).abs() < 1.0e-6);
    }

    #[test]
    fn matrix_identity_is_neutral() {
        assert_eq!(Matrix2::identity().transform([3.0, -2.0]), [3.0, -2.0]);
    }
}
