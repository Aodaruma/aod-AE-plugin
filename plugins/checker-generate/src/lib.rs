#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    PatternGroupStart,
    SeparateWidthHeight,
    CellWidth,
    CellHeight,
    Center,
    ColorA,
    ColorAOpacity,
    ColorB,
    ColorBOpacity,
    EdgeInterpolation,
    UseFeather,
    FeatherWidth,
    PatternGroupEnd,
    CompositeGroupStart,
    BlendMode,
    BlendOpacity,
    PreserveOriginalAlpha,
    Clamp32,
    CompositeGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlendMode {
    Normal,
    Add,
    Subtract,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    ColorDodge,
    ColorBurn,
    LinearBurn,
    LinearLight,
    Difference,
    Exclusion,
    Divide,
    Darken,
    Lighten,
}

#[derive(Clone, Copy)]
struct PatternSettings {
    center_x: f32,
    center_y: f32,
    cell_width: f32,
    cell_height: f32,
    softness_x: f32,
    softness_y: f32,
    color_a: [f32; 3],
    opacity_a: f32,
    color_b: [f32; 3],
    opacity_b: f32,
}

#[derive(Clone, Copy)]
struct PatternSample {
    rgb: [f32; 3],
    alpha: f32,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Generates customizable two-color checkerboard patterns with adjustable geometry, edge treatment, and compositing.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_group(
            Params::PatternGroupStart,
            Params::PatternGroupEnd,
            "Pattern",
            false, // Start expanded.
            |params| {
                params.add_with_flags(
                    Params::SeparateWidthHeight,
                    "Separate Width / Height",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::CellWidth,
                    "Cell Size (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.01);
                        d.set_valid_max(32768.0);
                        d.set_slider_min(1.0);
                        d.set_slider_max(1024.0);
                        d.set_default(8.0);
                        d.set_precision(2);
                    }),
                )?;

                params.add_with_flags(
                    Params::CellHeight,
                    "Cell Height (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.01);
                        d.set_valid_max(32768.0);
                        d.set_slider_min(1.0);
                        d.set_slider_max(1024.0);
                        d.set_default(8.0);
                        d.set_precision(2);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::DISABLED,
                )?;

                params.add(
                    Params::Center,
                    "Center",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;

                params.add(
                    Params::ColorA,
                    "Color A",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            alpha: 255,
                            red: 204,
                            green: 204,
                            blue: 204,
                        });
                    }),
                )?;

                params.add(
                    Params::ColorAOpacity,
                    "Color A Opacity (%)",
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
                    Params::ColorB,
                    "Color B",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                )?;

                params.add(
                    Params::ColorBOpacity,
                    "Color B Opacity (%)",
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
                    Params::EdgeInterpolation,
                    "Edge Interpolation",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;

                params.add_with_flags(
                    Params::UseFeather,
                    "Apply Feather",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add_with_flags(
                    Params::FeatherWidth,
                    "Feather Width (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(8192.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(256.0);
                        d.set_default(8.0);
                        d.set_precision(2);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::DISABLED,
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::CompositeGroupStart,
            Params::CompositeGroupEnd,
            "Composite",
            false, // Start expanded.
            |params| {
                params.add(
                    Params::BlendMode,
                    "Blend Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Normal",
                            "Add (Linear Dodge)",
                            "Subtract",
                            "Multiply",
                            "Screen",
                            "Overlay",
                            "Soft Light",
                            "Hard Light",
                            "Color Dodge",
                            "Color Burn",
                            "Linear Burn",
                            "Linear Light",
                            "Difference",
                            "Exclusion",
                            "Divide",
                            "Darken",
                            "Lighten",
                        ]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::BlendOpacity,
                    "Blend Opacity",
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
                    Params::PreserveOriginalAlpha,
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
                        "AOD_CheckerGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
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
                let changed_param = params.type_at(param_index);
                if changed_param == Params::SeparateWidthHeight
                    || changed_param == Params::UseFeather
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
        let separate_width_height = params
            .get(Params::SeparateWidthHeight)?
            .as_checkbox()?
            .value();
        Self::set_param_name(
            params,
            Params::CellWidth,
            if separate_width_height {
                "Cell Width (px)"
            } else {
                "Cell Size (px)"
            },
        )?;
        Self::set_param_enabled(params, Params::CellHeight, separate_width_height)?;

        let use_feather = params.get(Params::UseFeather)?.as_checkbox()?.value();
        Self::set_param_enabled(params, Params::FeatherWidth, use_feather)
    }

    fn set_param_name(
        params: &mut Parameters<Params>,
        id: Params,
        name: &str,
    ) -> Result<(), Error> {
        let mut param = params.get_mut(id)?;
        param.set_name(name)?;
        param.update_param_ui()?;
        Ok(())
    }

    fn set_param_enabled(
        params: &mut Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        let flag = ae::ParamUIFlags::DISABLED;
        let flag_bits = flag.bits();
        let status = !enabled;
        let current_status = (params.get(id)?.ui_flags().bits() & flag_bits) != 0;
        if current_status == status {
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
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = out_layer.width();
        let height = out_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let downsample_x = f32::from(in_data.downsample_x()).abs().max(1.0e-6);
        let downsample_y = f32::from(in_data.downsample_y()).abs().max(1.0e-6);

        let separate_width_height = params
            .get(Params::SeparateWidthHeight)?
            .as_checkbox()?
            .value();
        let cell_width_value = finite_or(
            params.get(Params::CellWidth)?.as_float_slider()?.value() as f32,
            8.0,
        )
        .max(0.01);
        let cell_height_value = if separate_width_height {
            finite_or(
                params.get(Params::CellHeight)?.as_float_slider()?.value() as f32,
                8.0,
            )
            .max(0.01)
        } else {
            cell_width_value
        };
        let cell_width = cell_width_value * downsample_x;
        let cell_height = cell_height_value * downsample_y;

        let center_param = params.get(Params::Center)?;
        let center = center_param.as_point()?;
        let (center_x, center_y) = point_value_f32(&center);

        let color_a = color_rgb(params.get(Params::ColorA)?.as_color()?.float_value()?);
        let opacity_a = finite_or(
            params
                .get(Params::ColorAOpacity)?
                .as_float_slider()?
                .value() as f32,
            100.0,
        )
        .clamp(0.0, 100.0)
            * 0.01;
        let color_b = color_rgb(params.get(Params::ColorB)?.as_color()?.float_value()?);
        let opacity_b = finite_or(
            params
                .get(Params::ColorBOpacity)?
                .as_float_slider()?
                .value() as f32,
            100.0,
        )
        .clamp(0.0, 100.0)
            * 0.01;

        let interpolate = params
            .get(Params::EdgeInterpolation)?
            .as_checkbox()?
            .value();
        let use_feather = params.get(Params::UseFeather)?.as_checkbox()?.value();
        let feather_width = finite_or(
            params.get(Params::FeatherWidth)?.as_float_slider()?.value() as f32,
            0.0,
        )
        .max(0.0);
        let interpolation_softness: f32 = if interpolate { 0.5 } else { 0.0 };
        let softness_x = interpolation_softness.max(if use_feather {
            feather_width * downsample_x * 0.5
        } else {
            0.0
        });
        let softness_y = interpolation_softness.max(if use_feather {
            feather_width * downsample_y * 0.5
        } else {
            0.0
        });

        let settings = PatternSettings {
            center_x,
            center_y,
            cell_width,
            cell_height,
            softness_x,
            softness_y,
            color_a,
            opacity_a,
            color_b,
            opacity_b,
        };

        let blend_mode = blend_mode_from_popup(params.get(Params::BlendMode)?.as_popup()?.value());
        let blend_opacity = finite_or(
            params.get(Params::BlendOpacity)?.as_float_slider()?.value() as f32,
            1.0,
        )
        .clamp(0.0, 1.0);
        let preserve_alpha = params
            .get(Params::PreserveOriginalAlpha)?
            .as_checkbox()?
            .value();
        let clamp_32 = params.get(Params::Clamp32)?.as_checkbox()?.value();

        let origin = in_data.output_origin();
        let pre_origin = in_data.pre_effect_source_origin();
        let origin_x = origin.h as f32 + pre_origin.h as f32;
        let origin_y = origin.v as f32 + pre_origin.v as f32;

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        out_layer.iterate(0, height as i32, None, |x, y, mut dst| {
            let sample_x = x as f32 + 0.5 + origin_x;
            let sample_y = y as f32 + 0.5 + origin_y;
            let pattern = checker_sample(sample_x, sample_y, settings);
            let base = read_pixel_f32(&in_layer, in_world_type, x as usize, y as usize);
            let mut output =
                composite_pattern(base, pattern, blend_mode, blend_opacity, preserve_alpha);
            output = sanitize_output(output, out_is_f32, clamp_32);

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(output.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(output.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                    dst.set_from_f32(output);
                }
            }

            Ok(())
        })?;

        Ok(())
    }
}

fn point_value_f32(point: &PointDef<'_>) -> (f32, f32) {
    match point.float_value() {
        Ok(point) => (point.x as f32, point.y as f32),
        Err(_) => point.value(),
    }
}

fn color_rgb(color: PixelF32) -> [f32; 3] {
    [
        finite_or(color.red, 0.0),
        finite_or(color.green, 0.0),
        finite_or(color.blue, 0.0),
    ]
}

fn checker_sample(x: f32, y: f32, settings: PatternSettings) -> PatternSample {
    let weight = checker_weight(x, y, settings);
    let alpha_a = finite_or(settings.opacity_a, 1.0).clamp(0.0, 1.0);
    let alpha_b = finite_or(settings.opacity_b, 1.0).clamp(0.0, 1.0);
    let alpha = lerp(alpha_a, alpha_b, weight);
    let premultiplied = [
        lerp(
            settings.color_a[0] * alpha_a,
            settings.color_b[0] * alpha_b,
            weight,
        ),
        lerp(
            settings.color_a[1] * alpha_a,
            settings.color_b[1] * alpha_b,
            weight,
        ),
        lerp(
            settings.color_a[2] * alpha_a,
            settings.color_b[2] * alpha_b,
            weight,
        ),
    ];
    let rgb = if alpha > 1.0e-6 {
        [
            premultiplied[0] / alpha,
            premultiplied[1] / alpha,
            premultiplied[2] / alpha,
        ]
    } else {
        [0.0; 3]
    };

    PatternSample { rgb, alpha }
}

fn checker_weight(x: f32, y: f32, settings: PatternSettings) -> f32 {
    let wave_x = axis_wave(
        x,
        settings.center_x,
        settings.cell_width,
        settings.softness_x,
    );
    let wave_y = axis_wave(
        y,
        settings.center_y,
        settings.cell_height,
        settings.softness_y,
    );
    (0.5 - 0.5 * wave_x * wave_y).clamp(0.0, 1.0)
}

fn axis_wave(position: f32, center: f32, cell_size: f32, softness: f32) -> f32 {
    let cell_size = finite_or(cell_size, 1.0).abs().max(1.0e-6);
    let center = finite_or(center, 0.0);
    let cell_coordinate = (finite_or(position, center) - center) / cell_size + 0.5;
    let cell_floor = cell_coordinate.floor();
    let fraction = cell_coordinate - cell_floor;
    let cell_index = cell_floor as i64;
    let sign = if cell_index.rem_euclid(2) == 0 {
        1.0
    } else {
        -1.0
    };

    let softness = finite_or(softness, 0.0).max(0.0);
    if softness <= 1.0e-6 {
        return sign;
    }

    let distance_to_edge = fraction.min(1.0 - fraction) * cell_size;
    sign * smoothstep(distance_to_edge / softness)
}

fn composite_pattern(
    base: PixelF32,
    pattern: PatternSample,
    mode: BlendMode,
    opacity: f32,
    preserve_alpha: bool,
) -> PixelF32 {
    let opacity = finite_or(opacity, 1.0).clamp(0.0, 1.0);
    let source_alpha = finite_or(pattern.alpha, 0.0).clamp(0.0, 1.0) * opacity;
    if source_alpha <= 0.0 {
        return base;
    }

    let base_alpha = finite_or(base.alpha, 0.0).clamp(0.0, 1.0);
    let base_straight = if base_alpha > 1.0e-6 {
        [
            finite_or(base.red, 0.0) / base_alpha,
            finite_or(base.green, 0.0) / base_alpha,
            finite_or(base.blue, 0.0) / base_alpha,
        ]
    } else {
        [0.0; 3]
    };
    let blended = [
        blend_channel(base_straight[0], pattern.rgb[0], mode),
        blend_channel(base_straight[1], pattern.rgb[1], mode),
        blend_channel(base_straight[2], pattern.rgb[2], mode),
    ];

    if preserve_alpha {
        let mixed_straight = [
            lerp(base_straight[0], blended[0], source_alpha),
            lerp(base_straight[1], blended[1], source_alpha),
            lerp(base_straight[2], blended[2], source_alpha),
        ];
        PixelF32 {
            alpha: base_alpha,
            red: mixed_straight[0] * base_alpha,
            green: mixed_straight[1] * base_alpha,
            blue: mixed_straight[2] * base_alpha,
        }
    } else {
        // Blend modes operate where the backdrop exists. In transparent areas, the
        // generated pattern remains the source color.
        let target = [
            lerp(pattern.rgb[0], blended[0], base_alpha),
            lerp(pattern.rgb[1], blended[1], base_alpha),
            lerp(pattern.rgb[2], blended[2], base_alpha),
        ];
        PixelF32 {
            alpha: source_alpha + base_alpha * (1.0 - source_alpha),
            red: lerp(finite_or(base.red, 0.0), target[0], source_alpha),
            green: lerp(finite_or(base.green, 0.0), target[1], source_alpha),
            blue: lerp(finite_or(base.blue, 0.0), target[2], source_alpha),
        }
    }
}

fn blend_mode_from_popup(value: i32) -> BlendMode {
    match value {
        2 => BlendMode::Add,
        3 => BlendMode::Subtract,
        4 => BlendMode::Multiply,
        5 => BlendMode::Screen,
        6 => BlendMode::Overlay,
        7 => BlendMode::SoftLight,
        8 => BlendMode::HardLight,
        9 => BlendMode::ColorDodge,
        10 => BlendMode::ColorBurn,
        11 => BlendMode::LinearBurn,
        12 => BlendMode::LinearLight,
        13 => BlendMode::Difference,
        14 => BlendMode::Exclusion,
        15 => BlendMode::Divide,
        16 => BlendMode::Darken,
        17 => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

fn blend_channel(base: f32, source: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => source,
        BlendMode::Add => base + source,
        BlendMode::Subtract => base - source,
        BlendMode::Multiply => base * source,
        BlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - source),
        BlendMode::Overlay => {
            if base <= 0.5 {
                2.0 * base * source
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - source)
            }
        }
        BlendMode::SoftLight => soft_light(base, source),
        BlendMode::HardLight => {
            if source <= 0.5 {
                2.0 * base * source
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - source)
            }
        }
        BlendMode::ColorDodge => {
            if source >= 1.0 {
                1.0
            } else {
                base / (1.0 - source).max(1.0e-6)
            }
        }
        BlendMode::ColorBurn => {
            if source <= 0.0 {
                0.0
            } else {
                1.0 - (1.0 - base) / source.max(1.0e-6)
            }
        }
        BlendMode::LinearBurn => base + source - 1.0,
        BlendMode::LinearLight => base + 2.0 * source - 1.0,
        BlendMode::Difference => (base - source).abs(),
        BlendMode::Exclusion => base + source - 2.0 * base * source,
        BlendMode::Divide => {
            if source.abs() <= 1.0e-6 {
                1.0
            } else {
                base / source
            }
        }
        BlendMode::Darken => base.min(source),
        BlendMode::Lighten => base.max(source),
    }
}

fn soft_light(base: f32, source: f32) -> f32 {
    if source <= 0.5 {
        base - (1.0 - 2.0 * source) * base * (1.0 - base)
    } else {
        let d = if base <= 0.25 {
            ((16.0 * base - 12.0) * base + 4.0) * base
        } else {
            base.sqrt()
        };
        base + (2.0 * source - 1.0) * (d - base)
    }
}

fn sanitize_output(mut pixel: PixelF32, out_is_f32: bool, clamp_32: bool) -> PixelF32 {
    pixel.alpha = finite_or(pixel.alpha, 0.0).clamp(0.0, 1.0);
    pixel.red = finite_or(pixel.red, 0.0);
    pixel.green = finite_or(pixel.green, 0.0);
    pixel.blue = finite_or(pixel.blue, 0.0);

    if !out_is_f32 || clamp_32 {
        pixel.red = pixel.red.clamp(0.0, 1.0);
        pixel.green = pixel.green.clamp(0.0, 1.0);
        pixel.blue = pixel.blue.clamp(0.0, 1.0);
    }

    pixel
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    if x >= layer.width() || y >= layer.height() {
        return PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
    }

    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(softness: f32) -> PatternSettings {
        PatternSettings {
            center_x: 0.0,
            center_y: 0.0,
            cell_width: 10.0,
            cell_height: 10.0,
            softness_x: softness,
            softness_y: softness,
            color_a: [0.0, 0.0, 0.0],
            opacity_a: 1.0,
            color_b: [1.0, 1.0, 1.0],
            opacity_b: 1.0,
        }
    }

    fn sample(rgb: [f32; 3], alpha: f32) -> PatternSample {
        PatternSample { rgb, alpha }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn hard_checker_alternates_in_all_quadrants() {
        let settings = settings(0.0);
        assert_close(checker_weight(0.0, 0.0, settings), 0.0);
        assert_close(checker_weight(6.0, 0.0, settings), 1.0);
        assert_close(checker_weight(-6.0, 0.0, settings), 1.0);
        assert_close(checker_weight(6.0, 6.0, settings), 0.0);
        assert_close(checker_weight(-6.0, -6.0, settings), 0.0);
    }

    #[test]
    fn interpolation_mixes_exactly_on_an_edge() {
        assert_close(checker_weight(5.0, 0.0, settings(0.5)), 0.5);
    }

    #[test]
    fn feather_blends_across_the_requested_transition() {
        let settings = settings(0.5);
        assert_close(checker_weight(4.75, 0.0, settings), 0.25);
        assert_close(checker_weight(5.25, 0.0, settings), 0.75);
    }

    #[test]
    fn each_checker_color_has_independent_opacity() {
        let mut settings = settings(0.0);
        settings.opacity_a = 0.25;
        settings.opacity_b = 0.75;
        assert_close(checker_sample(0.0, 0.0, settings).alpha, 0.25);
        assert_close(checker_sample(6.0, 0.0, settings).alpha, 0.75);
    }

    #[test]
    fn transparent_colors_are_interpolated_without_color_bleeding() {
        let mut settings = settings(0.5);
        settings.color_a = [1.0, 0.0, 0.0];
        settings.opacity_a = 0.0;
        settings.color_b = [0.0, 0.0, 1.0];
        settings.opacity_b = 1.0;

        let sample = checker_sample(5.0, 0.0, settings);
        assert_close(sample.alpha, 0.5);
        assert_close(sample.rgb[0], 0.0);
        assert_close(sample.rgb[1], 0.0);
        assert_close(sample.rgb[2], 1.0);
    }

    #[test]
    fn normal_composite_uses_premultiplied_alpha() {
        let base = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let result = composite_pattern(
            base,
            sample([1.0, 0.0, 0.0], 1.0),
            BlendMode::Normal,
            0.25,
            false,
        );
        assert_close(result.alpha, 0.25);
        assert_close(result.red, 0.25);
        assert_close(result.green, 0.0);
        assert_close(result.blue, 0.0);
    }

    #[test]
    fn non_normal_modes_keep_the_pattern_over_transparency() {
        let base = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let result = composite_pattern(
            base,
            sample([0.8, 0.4, 0.2], 1.0),
            BlendMode::Multiply,
            0.25,
            false,
        );
        assert_close(result.alpha, 0.25);
        assert_close(result.red, 0.2);
        assert_close(result.green, 0.1);
        assert_close(result.blue, 0.05);
    }

    #[test]
    fn preserving_alpha_keeps_rgb_premultiplied() {
        let base = PixelF32 {
            alpha: 0.5,
            red: 0.1,
            green: 0.1,
            blue: 0.1,
        };
        let result = composite_pattern(
            base,
            sample([1.0, 1.0, 1.0], 1.0),
            BlendMode::Normal,
            0.5,
            true,
        );
        assert_close(result.alpha, 0.5);
        assert_close(result.red, 0.3);
        assert_close(result.green, 0.3);
        assert_close(result.blue, 0.3);
    }

    #[test]
    fn color_opacity_controls_source_over_compositing() {
        let base = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let result = composite_pattern(
            base,
            sample([1.0, 0.5, 0.0], 0.4),
            BlendMode::Normal,
            0.5,
            false,
        );
        assert_close(result.alpha, 0.2);
        assert_close(result.red, 0.2);
        assert_close(result.green, 0.1);
        assert_close(result.blue, 0.0);
    }
}
