#![allow(clippy::drop_non_drop, clippy::question_mark)]

mod gradient;
mod params;

use after_effects as ae;
use std::env;

use ae::pf::*;
use gradient::{
    ColorModel, Easing, ExtendMode, Geometry, Rainbow, Shape, aspect_ratio_from_balance,
    geometry_value, sample_rainbow,
};
use params::Params;
use utils::ToPixel;
use utils::image::{finite_or, read_pixel, sanitize_pixel, unpremultiplied_rgb};

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Generates parametric rainbows directly in perceptual and cylindrical color models across multiple geometric shapes.";

const AFFINE_COLOR_PARAMS: [Params; 6] = [
    Params::HueScale,
    Params::HueOffset,
    Params::Component2Scale,
    Params::Component2Offset,
    Params::Component3Scale,
    Params::Component3Offset,
];
const ENDPOINT_COLOR_PARAMS: [Params; 6] = [
    Params::StartHue,
    Params::StartComponent2,
    Params::StartComponent3,
    Params::EndHue,
    Params::EndComponent2,
    Params::EndComponent3,
];

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params::setup(params)
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
                        "AOD_RainbowGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::PixIndependent, true);
                out_data.set_out_flag(OutFlags::UseOutputExtent, true);
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::WideTimeInput, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsThreadedRendering, true);
                out_data.set_out_flag2(OutFlags2::AutomaticWideTimeInput, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_RainbowGenerate")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => self.do_render(in_data, in_layer, out_layer, params)?,
            ae::Command::SmartPreRender { mut extra } => {
                let request = extra.output_request();
                let input = extra.callbacks().checkout_layer(
                    0,
                    0,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let _ = extra.union_result_rect(input.result_rect.into());
                let _ = extra.union_max_result_rect(input.max_result_rect.into());
            }
            ae::Command::SmartRender { extra } => {
                let callbacks = extra.callbacks();
                let input = callbacks.checkout_layer_pixels(0)?;
                let render_result: Result<(), Error> = (|| {
                    let output = callbacks.checkout_output()?;
                    if let (Some(input), Some(output)) = (input, output) {
                        self.do_render(in_data, input, output, params)?;
                    }
                    Ok(())
                })();
                let checkin_result = callbacks.checkin_layer_pixels(0);
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index }
                if matches!(
                    params.type_at(param_index),
                    Params::Shape
                        | Params::CoordinateMode
                        | Params::ColorModel
                        | Params::Easing
                        | Params::SplitRangeEnds
                ) =>
            {
                out_data.set_out_flag(OutFlags::RefreshUi, true);
            }
            ae::Command::UpdateParamsUi => {
                let mut copy = params.cloned();
                self.update_params_ui(in_data, &mut copy)?;
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
        let shape = shape_from_popup(params.get(Params::Shape)?.as_popup()?.value());
        let two_points = params.get(Params::CoordinateMode)?.as_popup()?.value() == 1;
        let color_model =
            color_model_from_popup(params.get(Params::ColorModel)?.as_popup()?.value());
        let custom_easing = params.get(Params::Easing)?.as_popup()?.value() == 7;
        let split_range_ends = params.get(Params::SplitRangeEnds)?.as_checkbox()?.value();
        Self::set_component_names(params, color_model)?;

        Self::set_param_name(
            params,
            Params::PointA,
            if shape == Shape::Linear {
                "Start Point"
            } else {
                "Center"
            },
        )?;
        Self::set_param_name(
            params,
            Params::PointB,
            if shape == Shape::Linear {
                "End Point"
            } else if shape == Shape::ReflectedLinear {
                "Axis / Scale"
            } else {
                "Radius / Direction"
            },
        )?;

        for id in [Params::PointA, Params::PointB] {
            self.set_param_visible(in_data, params, id, two_points)?;
            Self::set_param_enabled(params, id, two_points)?;
        }
        for id in [Params::Center, Params::Angle] {
            self.set_param_visible(in_data, params, id, !two_points)?;
            Self::set_param_enabled(params, id, !two_points)?;
        }
        let uses_length = !two_points && !matches!(shape, Shape::Conic | Shape::Starburst);
        self.set_param_visible(in_data, params, Params::Length, uses_length)?;
        Self::set_param_enabled(params, Params::Length, uses_length)?;

        let uses_aspect = matches!(
            shape,
            Shape::Radial
                | Shape::Diamond
                | Shape::Box
                | Shape::Minkowski
                | Shape::Spiral
                | Shape::Starburst
        );
        self.set_param_visible(in_data, params, Params::Aspect, uses_aspect)?;
        Self::set_param_enabled(params, Params::Aspect, uses_aspect)?;

        for (id, visible) in [
            (Params::ShapeExponent, shape == Shape::Minkowski),
            (Params::SpiralTurns, shape == Shape::Spiral),
            (Params::RayCount, shape == Shape::Starburst),
        ] {
            self.set_param_visible(in_data, params, id, visible)?;
            Self::set_param_enabled(params, id, visible)?;
        }
        for id in AFFINE_COLOR_PARAMS {
            self.set_param_visible(in_data, params, id, !split_range_ends)?;
            Self::set_param_enabled(params, id, !split_range_ends)?;
        }
        for id in ENDPOINT_COLOR_PARAMS {
            self.set_param_visible(in_data, params, id, split_range_ends)?;
            Self::set_param_enabled(params, id, split_range_ends)?;
        }

        for id in [
            Params::BezierX1,
            Params::BezierY1,
            Params::BezierX2,
            Params::BezierY2,
        ] {
            self.set_param_visible(in_data, params, id, custom_easing)?;
            Self::set_param_enabled(params, id, custom_easing)?;
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

    fn set_param_name(
        params: &mut Parameters<Params>,
        id: Params,
        name: &str,
    ) -> Result<(), Error> {
        let unchanged = {
            let param = params.get(id)?;
            param
                .as_ref()
                .name
                .iter()
                .take_while(|&&byte| byte != 0)
                .map(|&byte| byte as u8)
                .eq(name.bytes())
        };
        if unchanged {
            return Ok(());
        }
        let mut param = params.get_mut(id)?;
        param.set_name(name)?;
        param.update_param_ui()?;
        Ok(())
    }

    fn set_component_names(
        params: &mut Parameters<Params>,
        color_model: ColorModel,
    ) -> Result<(), Error> {
        let (secondary, tertiary) = component_names(color_model);
        for (id, prefix, component, suffix) in [
            (Params::Component2Scale, "", secondary, "Scale (%)"),
            (Params::Component2Offset, "", secondary, "Offset (%)"),
            (Params::Component3Scale, "", tertiary, "Scale (%)"),
            (Params::Component3Offset, "", tertiary, "Offset (%)"),
            (Params::StartComponent2, "Start ", secondary, "(%)"),
            (Params::StartComponent3, "Start ", tertiary, "(%)"),
            (Params::EndComponent2, "End ", secondary, "(%)"),
            (Params::EndComponent3, "End ", tertiary, "(%)"),
        ] {
            let name = format!("{prefix}{component} {suffix}");
            Self::set_param_name(params, id, &name)?;
        }
        Ok(())
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = out_layer.width();
        let height = out_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let shape = shape_from_popup(params.get(Params::Shape)?.as_popup()?.value());
        let two_points = params.get(Params::CoordinateMode)?.as_popup()?.value() == 1;
        let downsample = downsample(in_data);
        let geometry = read_geometry(params, shape, two_points, downsample)?;
        let rainbow = read_rainbow(params)?;
        let mix = slider(params, Params::Mix, 100.0).clamp(0.0, 100.0) * 0.01;
        let preserve_alpha = params
            .get(Params::PreserveInputAlpha)?
            .as_checkbox()?
            .value();
        // PF_PointDef values are already adjusted for both downsampling and any
        // pre-effect buffer expansion. PF_EffectWorld::origin is the matching
        // layer-space origin for a Smart Render tile/full-frame checkout.
        let origin = out_layer.origin();
        let origin_x = origin.h as f32;
        let origin_y = origin.v as f32;
        let out_world_type = out_layer.world_type();

        out_layer.iterate(0, height as i32, None, |x, y, mut destination| {
            let point = [
                (x as f32 + origin_x + 0.5) / downsample[0],
                (y as f32 + origin_y + 0.5) / downsample[1],
            ];
            let rainbow_rgb = sample_rainbow(geometry_value(point, geometry), rainbow);
            let input = read_pixel(&in_layer, x as usize, y as usize);
            let output =
                composite_rainbow(input, rainbow_rgb, mix, preserve_alpha, rainbow.clamp_gamut);
            match out_world_type {
                ae::aegp::WorldType::U8 => destination.set_from_u8(output.to_pixel8()),
                ae::aegp::WorldType::U15 => destination.set_from_u16(output.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                    destination.set_from_f32(output)
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn read_geometry(
    params: &Parameters<Params>,
    shape: Shape,
    two_points: bool,
    downsample: [f32; 2],
) -> Result<Geometry, Error> {
    let (start, end) = if two_points {
        (
            point_to_full(point(params, Params::PointA)?, downsample),
            point_to_full(point(params, Params::PointB)?, downsample),
        )
    } else {
        let center = point_to_full(point(params, Params::Center)?, downsample);
        let angle = finite_or(
            params.get(Params::Angle)?.as_angle()?.float_value()? as f32,
            0.0,
        )
        .to_radians();
        let length = slider(params, Params::Length, 500.0).max(0.01);
        let direction = [angle.cos() * length, angle.sin() * length];
        if shape == Shape::Linear {
            (
                [
                    center[0] - direction[0] * 0.5,
                    center[1] - direction[1] * 0.5,
                ],
                [
                    center[0] + direction[0] * 0.5,
                    center[1] + direction[1] * 0.5,
                ],
            )
        } else {
            (center, [center[0] + direction[0], center[1] + direction[1]])
        }
    };
    Ok(Geometry {
        shape,
        start,
        end,
        aspect: aspect_ratio_from_balance(slider(params, Params::Aspect, 0.0)),
        exponent: slider(params, Params::ShapeExponent, 2.0).clamp(0.25, 64.0),
        spiral_turns: slider(params, Params::SpiralTurns, 3.0).clamp(-128.0, 128.0),
        ray_count: slider(params, Params::RayCount, 12.0).clamp(1.0, 512.0),
    })
}

fn read_rainbow(params: &Parameters<Params>) -> Result<Rainbow, Error> {
    let split = params.get(Params::SplitRangeEnds)?.as_checkbox()?.value();
    let (start, end) = if split {
        (
            [
                slider(params, Params::StartHue, 0.0) / 360.0,
                slider(params, Params::StartComponent2, 100.0) * 0.01,
                slider(params, Params::StartComponent3, 75.0) * 0.01,
            ],
            [
                slider(params, Params::EndHue, 360.0) / 360.0,
                slider(params, Params::EndComponent2, 100.0) * 0.01,
                slider(params, Params::EndComponent3, 75.0) * 0.01,
            ],
        )
    } else {
        affine_rainbow_endpoints(
            slider(params, Params::HueScale, 100.0),
            slider(params, Params::HueOffset, 0.0),
            slider(params, Params::Component2Scale, 0.0),
            slider(params, Params::Component2Offset, 100.0),
            slider(params, Params::Component3Scale, 0.0),
            slider(params, Params::Component3Offset, 75.0),
        )
    };
    Ok(Rainbow {
        start,
        end,
        color_model: color_model_from_popup(params.get(Params::ColorModel)?.as_popup()?.value()),
        extend: extend_from_popup(params.get(Params::Extend)?.as_popup()?.value()),
        easing: easing_from_popup(params.get(Params::Easing)?.as_popup()?.value()),
        bezier: [
            slider(params, Params::BezierX1, 0.25),
            slider(params, Params::BezierY1, 0.1),
            slider(params, Params::BezierX2, 0.25),
            slider(params, Params::BezierY2, 1.0),
        ],
        clamp_gamut: params.get(Params::ClampGamut)?.as_checkbox()?.value(),
    })
}

fn affine_rainbow_endpoints(
    hue_scale: f32,
    hue_offset: f32,
    component2_scale: f32,
    component2_offset: f32,
    component3_scale: f32,
    component3_offset: f32,
) -> ([f32; 3], [f32; 3]) {
    let start = [
        finite_or(hue_offset, 0.0) / 360.0,
        finite_or(component2_offset, 100.0) * 0.01,
        finite_or(component3_offset, 75.0) * 0.01,
    ];
    let end = [
        start[0] + finite_or(hue_scale, 100.0) * 0.01,
        start[1] + finite_or(component2_scale, 0.0) * 0.01,
        start[2] + finite_or(component3_scale, 0.0) * 0.01,
    ];
    (start, end)
}

fn point(params: &Parameters<Params>, id: Params) -> Result<[f32; 2], Error> {
    let param = params.get(id)?;
    let point = param.as_point()?;
    Ok(match point.float_value() {
        Ok(value) => [value.x as f32, value.y as f32],
        Err(_) => {
            let (x, y) = point.value();
            [x, y]
        }
    })
}

fn downsample(in_data: InData) -> [f32; 2] {
    [
        f32::from(in_data.downsample_x()).abs().max(1.0e-6),
        f32::from(in_data.downsample_y()).abs().max(1.0e-6),
    ]
}

fn point_to_full(point: [f32; 2], downsample: [f32; 2]) -> [f32; 2] {
    [point[0] / downsample[0], point[1] / downsample[1]]
}

fn slider(params: &Parameters<Params>, id: Params, fallback: f32) -> f32 {
    let Ok(param) = params.get(id) else {
        return fallback;
    };
    let Ok(slider) = param.as_float_slider() else {
        return fallback;
    };
    finite_or(slider.value() as f32, fallback)
}

fn shape_from_popup(value: i32) -> Shape {
    match value {
        2 => Shape::Radial,
        3 => Shape::Diamond,
        4 => Shape::Conic,
        5 => Shape::Box,
        6 => Shape::Minkowski,
        7 => Shape::ReflectedLinear,
        8 => Shape::Spiral,
        9 => Shape::Starburst,
        _ => Shape::Linear,
    }
}

fn color_model_from_popup(value: i32) -> ColorModel {
    match value {
        2 => ColorModel::Hsv,
        3 => ColorModel::Hsl,
        4 => ColorModel::CielchAb,
        5 => ColorModel::CielchUv,
        6 => ColorModel::Jzczhz,
        7 => ColorModel::IptIch,
        _ => ColorModel::Oklch,
    }
}

fn component_names(color_model: ColorModel) -> (&'static str, &'static str) {
    match color_model {
        ColorModel::Oklch => ("Chroma", "Lightness"),
        ColorModel::Hsv => ("Saturation", "Brightness"),
        ColorModel::Hsl => ("Saturation", "Lightness"),
        ColorModel::CielchAb | ColorModel::CielchUv => ("Chroma", "Lightness"),
        ColorModel::Jzczhz => ("Chroma (Cz)", "Lightness (Jz)"),
        ColorModel::IptIch => ("Chroma", "Intensity"),
    }
}

fn extend_from_popup(value: i32) -> ExtendMode {
    match value {
        2 => ExtendMode::Repeat,
        3 => ExtendMode::Mirror,
        _ => ExtendMode::Clamp,
    }
}

fn easing_from_popup(value: i32) -> Easing {
    match value {
        2 => Easing::EaseIn,
        3 => Easing::EaseOut,
        4 => Easing::EaseInOut,
        5 => Easing::Smoothstep,
        6 => Easing::Smootherstep,
        7 => Easing::Custom,
        _ => Easing::Linear,
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn composite_rainbow(
    input: PixelF32,
    rainbow_rgb: [f32; 3],
    mix: f32,
    preserve_alpha: bool,
    clamp_gamut: bool,
) -> PixelF32 {
    let mix = finite_or(mix, 1.0).clamp(0.0, 1.0);
    let input_alpha = finite_or(input.alpha, 0.0).clamp(0.0, 1.0);
    let mut output = if preserve_alpha {
        let input_rgb = unpremultiplied_rgb(input);
        let rgb = [
            lerp(input_rgb[0], rainbow_rgb[0], mix),
            lerp(input_rgb[1], rainbow_rgb[1], mix),
            lerp(input_rgb[2], rainbow_rgb[2], mix),
        ];
        PixelF32 {
            alpha: input_alpha,
            red: rgb[0] * input_alpha,
            green: rgb[1] * input_alpha,
            blue: rgb[2] * input_alpha,
        }
    } else {
        PixelF32 {
            alpha: lerp(input_alpha, 1.0, mix),
            red: lerp(
                finite_or(input.red, 0.0),
                finite_or(rainbow_rgb[0], 0.0),
                mix,
            ),
            green: lerp(
                finite_or(input.green, 0.0),
                finite_or(rainbow_rgb[1], 0.0),
                mix,
            ),
            blue: lerp(
                finite_or(input.blue, 0.0),
                finite_or(rainbow_rgb[2], 0.0),
                mix,
            ),
        }
    };
    output = sanitize_pixel(output, false);
    if clamp_gamut {
        output.red = output.red.clamp(0.0, output.alpha);
        output.green = output.green.clamp(0.0, output.alpha);
        output.blue = output.blue.clamp(0.0, output.alpha);
    }
    output
}

#[cfg(test)]
mod render_tests {
    use super::*;

    #[test]
    fn rainbow_mix_interpolates_associated_color() {
        let transparent = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let output = composite_rainbow(transparent, [1.0, 0.0, 0.0], 0.5, false, false);
        assert!((output.alpha - 0.5).abs() < 1.0e-6);
        assert!((output.red - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn display_gamut_clamps_in_straight_color_space() {
        let output = composite_rainbow(
            PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            },
            [4.0, -1.0, 0.5],
            1.0,
            false,
            true,
        );
        assert_eq!(output.alpha, 1.0);
        assert_eq!(output.red, 1.0);
        assert_eq!(output.green, 0.0);
        assert_eq!(output.blue, 0.5);
    }

    #[test]
    fn point_coordinates_restore_each_downsample_axis() {
        assert_eq!(point_to_full([50.0, 25.0], [0.5, 0.25]), [100.0, 100.0]);
    }

    #[test]
    fn affine_defaults_define_one_full_rainbow_cycle() {
        let (start, end) = affine_rainbow_endpoints(100.0, 0.0, 0.0, 100.0, 0.0, 75.0);
        assert_eq!(start, [0.0, 1.0, 0.75]);
        assert_eq!(end, [1.0, 1.0, 0.75]);
    }

    #[test]
    fn affine_scales_are_range_deltas() {
        let (start, end) = affine_rainbow_endpoints(250.0, -90.0, -40.0, 100.0, 30.0, 50.0);
        assert_eq!(start, [-0.25, 1.0, 0.5]);
        for (actual, expected) in end.into_iter().zip([2.25, 0.6, 0.8]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }

    #[test]
    fn color_model_popup_indices_are_append_only() {
        assert_eq!(color_model_from_popup(1), ColorModel::Oklch);
        assert_eq!(color_model_from_popup(2), ColorModel::Hsv);
        assert_eq!(color_model_from_popup(3), ColorModel::Hsl);
        assert_eq!(color_model_from_popup(4), ColorModel::CielchAb);
        assert_eq!(color_model_from_popup(5), ColorModel::CielchUv);
        assert_eq!(color_model_from_popup(6), ColorModel::Jzczhz);
        assert_eq!(color_model_from_popup(7), ColorModel::IptIch);
    }

    #[test]
    fn dynamic_component_names_follow_each_model() {
        assert_eq!(component_names(ColorModel::Oklch), ("Chroma", "Lightness"));
        assert_eq!(
            component_names(ColorModel::Hsv),
            ("Saturation", "Brightness")
        );
        assert_eq!(
            component_names(ColorModel::Hsl),
            ("Saturation", "Lightness")
        );
        assert_eq!(
            component_names(ColorModel::CielchAb),
            ("Chroma", "Lightness")
        );
        assert_eq!(
            component_names(ColorModel::CielchUv),
            ("Chroma", "Lightness")
        );
        assert_eq!(
            component_names(ColorModel::Jzczhz),
            ("Chroma (Cz)", "Lightness (Jz)")
        );
        assert_eq!(component_names(ColorModel::IptIch), ("Chroma", "Intensity"));
    }
}
