#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const DESCRIPTION: &str = "Adds rim-light style light wrapping to alpha silhouettes using blur displacement or SDF masks.";
const ALPHA_EPSILON: f32 = 1.0e-6;
const DISPLACEMENT_NEUTRAL: f32 = 136.0 / 255.0;
const DISPLACEMENT_INVERSE_ITERATIONS: usize = 3;
const DIST_INF: f32 = 1.0e20;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    LightColor,
    Opacity,
    Direction,
    AlphaThreshold,
    Antialias,

    MaskGroupStart,
    MaskLayer,
    MaskChannel,
    MaskInvert,
    MaskGroupEnd,

    AlphaBlurRadius,
    AlphaDisplacement,
    AlphaWarpQuality,
    AlphaMapGamma,
    AlphaMatteMode,
    ShowDisplacementMap,

    SdfRadius,
    SdfFalloff,
    SdfDirectionality,

    InnerGroupStart,
    InnerMode,
    InnerRadius,
    InnerSeparateRadii,
    InnerBlurRadius,
    InnerScatterRadius,
    InnerSamples,
    InnerGroupEnd,

    CompositeGroupStart,
    BlendMode,
    PreserveSourceAlpha,
    Clamp32,
    CompositeGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WrapMode {
    AlphaBlurDisplace,
    Sdf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InnerMode {
    Off,
    Blur,
    Scatter,
    BlurScatter,
}

#[derive(Clone, Copy)]
enum MaskChannel {
    Alpha,
    Luminance,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy)]
enum AlphaMatteMode {
    InsideInverted,
    OutsideOnly,
    DisplacedAlpha,
}

#[derive(Clone, Copy)]
enum AlphaWarpQuality {
    Fast,
    Smooth,
    Accurate,
}

#[derive(Clone, Copy)]
enum BlendMode {
    Normal,
    Add,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    Lighten,
}

#[derive(Clone, Copy)]
struct Settings {
    mode: WrapMode,
    light_rgb: [f32; 3],
    opacity: f32,
    dir_x: f32,
    dir_y: f32,
    alpha_threshold: f32,
    antialias: bool,
    mask_channel: MaskChannel,
    mask_invert: bool,
    alpha_blur_radius: f32,
    alpha_displacement: f32,
    alpha_warp_quality: AlphaWarpQuality,
    alpha_map_gamma: f32,
    alpha_matte_mode: AlphaMatteMode,
    show_displacement_map: bool,
    sdf_radius: f32,
    sdf_falloff: f32,
    sdf_directionality: f32,
    inner_mode: InnerMode,
    inner_radius: f32,
    inner_separate_radii: bool,
    inner_blur_radius: f32,
    inner_scatter_radius: f32,
    inner_samples: usize,
    blend_mode: BlendMode,
    preserve_source_alpha: bool,
    clamp32: bool,
}

struct AlphaDisplaceMasks {
    effect_mask: Vec<f32>,
    displacement_map: Vec<f32>,
}

struct MaskMap {
    values: Vec<f32>,
    active: bool,
}

struct AlphaWarpContext<'a> {
    displacement_amounts: &'a [f32],
    width: usize,
    height: usize,
    displacement: f32,
    dir_x: f32,
    dir_y: f32,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = DESCRIPTION;

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::Mode,
            "Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Alpha Blur Displace", "SDF"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::LightColor,
            "Light Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 235,
                    blue: 205,
                });
            }),
        )?;

        params.add(
            Params::Opacity,
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
            Params::Direction,
            "Direction",
            AngleDef::setup(|d| {
                d.set_default(0.0);
            }),
        )?;

        params.add(
            Params::AlphaThreshold,
            "Alpha Threshold",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.01);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Antialias,
            "Antialias",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::AlphaBlurRadius,
            "Blur Radius (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(0.0);
                d.set_slider_max(128.0);
                d.set_default(16.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::AlphaDisplacement,
            "Displacement (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-2048.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(-256.0);
                d.set_slider_max(256.0);
                d.set_default(20.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::AlphaWarpQuality,
            "Warp Quality",
            PopupDef::setup(|d| {
                d.set_options(&["Fast", "Smooth", "Accurate"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::AlphaMapGamma,
            "Map Gamma",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.05);
                d.set_valid_max(16.0);
                d.set_slider_min(0.1);
                d.set_slider_max(4.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::AlphaMatteMode,
            "Alpha Matte",
            PopupDef::setup(|d| {
                d.set_options(&["Inside Inverted", "Outside Only", "Displaced Alpha"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::ShowDisplacementMap,
            "Show Displacement Map",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add_with_flags(
            Params::SdfRadius,
            "Radius (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(0.0);
                d.set_slider_max(128.0);
                d.set_default(24.0);
                d.set_precision(2);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::SdfFalloff,
            "Falloff",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.05);
                d.set_valid_max(16.0);
                d.set_slider_min(0.25);
                d.set_slider_max(6.0);
                d.set_default(2.0);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::SdfDirectionality,
            "Directionality",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.8);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_group(
            Params::MaskGroupStart,
            Params::MaskGroupEnd,
            "Mask",
            false,
            |params| {
                params.add(Params::MaskLayer, "Mask Layer", LayerDef::new())?;

                params.add(
                    Params::MaskChannel,
                    "Mask Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Alpha", "Luminance", "Red", "Green", "Blue"]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::MaskInvert,
                    "Invert Mask",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::InnerGroupStart,
            Params::InnerGroupEnd,
            "Boundary Blur / Scatter",
            false,
            |params| {
                params.add_with_flags(
                    Params::InnerMode,
                    "Boundary Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Off",
                            "Directional Blur",
                            "Directional Scatter",
                            "Blur + Scatter",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::InnerRadius,
                    "Spread (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(2048.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(128.0);
                        d.set_default(16.0);
                        d.set_precision(2);
                    }),
                )?;

                params.add_with_flags(
                    Params::InnerSeparateRadii,
                    "Separate Blur/Scatter",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::InnerBlurRadius,
                    "Blur Spread (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(2048.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(128.0);
                        d.set_default(16.0);
                        d.set_precision(2);
                    }),
                )?;

                params.add(
                    Params::InnerScatterRadius,
                    "Scatter Spread (px)",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(2048.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(128.0);
                        d.set_default(16.0);
                        d.set_precision(2);
                    }),
                )?;

                params.add(
                    Params::InnerSamples,
                    "Scatter Samples",
                    SliderDef::setup(|d| {
                        d.set_valid_min(2);
                        d.set_valid_max(128);
                        d.set_slider_min(2);
                        d.set_slider_max(64);
                        d.set_default(24);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::CompositeGroupStart,
            Params::CompositeGroupEnd,
            "Composite",
            false,
            |params| {
                params.add(
                    Params::BlendMode,
                    "Blend Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Normal",
                            "Add (Linear Dodge)",
                            "Screen",
                            "Overlay",
                            "Soft Light",
                            "Hard Light",
                            "Lighten",
                        ]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::PreserveSourceAlpha,
                    "Preserve Source Alpha",
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
                        "AOD_LightWrap - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_LightWrap")
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
                if changed == Params::Mode
                    || changed == Params::InnerMode
                    || changed == Params::InnerSeparateRadii
                {
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
        let mode = wrap_mode_from_popup(params.get(Params::Mode)?.as_popup()?.value());
        let alpha_visible = matches!(mode, WrapMode::AlphaBlurDisplace);
        let sdf_visible = matches!(mode, WrapMode::Sdf);

        for id in [
            Params::AlphaBlurRadius,
            Params::AlphaDisplacement,
            Params::AlphaWarpQuality,
            Params::AlphaMapGamma,
            Params::AlphaMatteMode,
            Params::ShowDisplacementMap,
        ] {
            self.set_param_visible(in_data, params, id, alpha_visible)?;
        }

        for id in [
            Params::SdfRadius,
            Params::SdfFalloff,
            Params::SdfDirectionality,
        ] {
            self.set_param_visible(in_data, params, id, sdf_visible)?;
        }

        let inner_mode = inner_mode_from_popup(params.get(Params::InnerMode)?.as_popup()?.value());
        let inner_enabled = !matches!(inner_mode, InnerMode::Off);
        let has_scatter = matches!(inner_mode, InnerMode::Scatter | InnerMode::BlurScatter);
        let separate_allowed = matches!(inner_mode, InnerMode::BlurScatter);
        let separate_radii = separate_allowed
            && params
                .get(Params::InnerSeparateRadii)?
                .as_checkbox()?
                .value();

        Self::set_param_enabled(
            params,
            Params::InnerRadius,
            inner_enabled && !separate_radii,
        )?;
        Self::set_param_enabled(params, Params::InnerSeparateRadii, separate_allowed)?;
        Self::set_param_enabled(params, Params::InnerBlurRadius, separate_radii)?;
        Self::set_param_enabled(params, Params::InnerScatterRadius, separate_radii)?;
        Self::set_param_enabled(params, Params::InnerSamples, has_scatter)?;

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
        params: &mut Parameters<Params>,
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
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let src = read_layer_rgba(&in_layer);
        let alpha: Vec<f32> = src
            .iter()
            .map(|px| sanitize_non_finite(px.alpha).clamp(0.0, 1.0))
            .collect();
        let mask_map = read_mask_map(
            params,
            width,
            height,
            settings.mask_channel,
            settings.mask_invert,
        )?;

        let needs_sdf = matches!(settings.mode, WrapMode::Sdf);
        let sdf = if needs_sdf {
            Some(build_signed_distance(
                &alpha,
                width,
                height,
                settings.alpha_threshold,
            ))
        } else {
            None
        };

        let (outer_mask, displacement_map) = match settings.mode {
            WrapMode::AlphaBlurDisplace => {
                let maps =
                    build_alpha_displace_masks(&alpha, &mask_map.values, width, height, &settings);
                (maps.effect_mask, Some(maps.displacement_map))
            }
            WrapMode::Sdf => {
                let mut sdf_mask = build_sdf_mask(
                    sdf.as_ref().expect("SDF is required for SDF mode"),
                    width,
                    height,
                    &settings,
                );
                if mask_map.active {
                    let influence = build_sdf_mask_influence(
                        &alpha,
                        &mask_map.values,
                        width,
                        height,
                        &settings,
                    );
                    multiply_mask_in_place(&mut sdf_mask, &influence);
                }
                (sdf_mask, None)
            }
        };

        let processed_mask = apply_boundary_processing(outer_mask, width, height, &settings);

        out_layer.iterate(0, height as i32, None, |x, y, mut dst| {
            let xi = x as usize;
            let yi = y as usize;
            let idx = yi * width + xi;
            let out_px = if settings.show_displacement_map
                && matches!(settings.mode, WrapMode::AlphaBlurDisplace)
            {
                let value = displacement_map
                    .as_ref()
                    .map(|map| map[idx])
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                PixelF32 {
                    alpha: 1.0,
                    red: value,
                    green: value,
                    blue: value,
                }
            } else {
                let effect_mask = apply_antialias(
                    sanitize_non_finite(processed_mask[idx]).max(0.0),
                    settings.antialias,
                );
                composite_light(src[idx], effect_mask, out_is_f32, &settings)
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

fn read_settings(params: &mut Parameters<Params>) -> Result<Settings, Error> {
    let mode = wrap_mode_from_popup(params.get(Params::Mode)?.as_popup()?.value());
    let color = params.get(Params::LightColor)?.as_color()?.float_value()?;
    let direction_degrees = params.get(Params::Direction)?.as_angle()?.float_value()? as f32;
    let angle = (direction_degrees - 90.0).to_radians();

    Ok(Settings {
        mode,
        light_rgb: [
            sanitize_non_finite(color.red).max(0.0),
            sanitize_non_finite(color.green).max(0.0),
            sanitize_non_finite(color.blue).max(0.0),
        ],
        opacity: ((params.get(Params::Opacity)?.as_float_slider()?.value() as f32) / 100.0)
            .clamp(0.0, 1.0),
        dir_x: angle.cos(),
        dir_y: angle.sin(),
        alpha_threshold: (params
            .get(Params::AlphaThreshold)?
            .as_float_slider()?
            .value() as f32)
            .clamp(0.0, 1.0),
        antialias: params.get(Params::Antialias)?.as_checkbox()?.value(),
        mask_channel: mask_channel_from_popup(params.get(Params::MaskChannel)?.as_popup()?.value()),
        mask_invert: params.get(Params::MaskInvert)?.as_checkbox()?.value(),
        alpha_blur_radius: params
            .get(Params::AlphaBlurRadius)?
            .as_float_slider()?
            .value() as f32,
        alpha_displacement: params
            .get(Params::AlphaDisplacement)?
            .as_float_slider()?
            .value() as f32,
        alpha_warp_quality: alpha_warp_quality_from_popup(
            params.get(Params::AlphaWarpQuality)?.as_popup()?.value(),
        ),
        alpha_map_gamma: (params
            .get(Params::AlphaMapGamma)?
            .as_float_slider()?
            .value() as f32)
            .max(0.05),
        alpha_matte_mode: alpha_matte_mode_from_popup(
            params.get(Params::AlphaMatteMode)?.as_popup()?.value(),
        ),
        show_displacement_map: params
            .get(Params::ShowDisplacementMap)?
            .as_checkbox()?
            .value(),
        sdf_radius: params.get(Params::SdfRadius)?.as_float_slider()?.value() as f32,
        sdf_falloff: (params.get(Params::SdfFalloff)?.as_float_slider()?.value() as f32).max(0.05),
        sdf_directionality: (params
            .get(Params::SdfDirectionality)?
            .as_float_slider()?
            .value() as f32)
            .clamp(0.0, 1.0),
        inner_mode: inner_mode_from_popup(params.get(Params::InnerMode)?.as_popup()?.value()),
        inner_radius: params.get(Params::InnerRadius)?.as_float_slider()?.value() as f32,
        inner_separate_radii: params
            .get(Params::InnerSeparateRadii)?
            .as_checkbox()?
            .value(),
        inner_blur_radius: params
            .get(Params::InnerBlurRadius)?
            .as_float_slider()?
            .value() as f32,
        inner_scatter_radius: params
            .get(Params::InnerScatterRadius)?
            .as_float_slider()?
            .value() as f32,
        inner_samples: params
            .get(Params::InnerSamples)?
            .as_slider()?
            .value()
            .clamp(2, 128) as usize,
        blend_mode: blend_mode_from_popup(params.get(Params::BlendMode)?.as_popup()?.value()),
        preserve_source_alpha: params
            .get(Params::PreserveSourceAlpha)?
            .as_checkbox()?
            .value(),
        clamp32: params.get(Params::Clamp32)?.as_checkbox()?.value(),
    })
}

fn read_mask_map(
    params: &mut Parameters<Params>,
    width: usize,
    height: usize,
    channel: MaskChannel,
    invert: bool,
) -> Result<MaskMap, Error> {
    let mask_checkout = params.checkout_at(Params::MaskLayer, None, None, None)?;
    let mask_layer = mask_checkout.as_layer()?.value();
    let mask = if let Some(layer) = mask_layer.as_ref() {
        MaskMap {
            values: read_layer_mask(layer, width, height, channel, invert),
            active: true,
        }
    } else {
        MaskMap {
            values: vec![1.0; width * height],
            active: false,
        }
    };

    Ok(mask)
}

fn read_layer_mask(
    layer: &Layer,
    out_width: usize,
    out_height: usize,
    channel: MaskChannel,
    invert: bool,
) -> Vec<f32> {
    let layer_width = layer.width();
    let layer_height = layer.height();
    if out_width == 0 || out_height == 0 || layer_width == 0 || layer_height == 0 {
        return vec![0.0; out_width * out_height];
    }

    let world_type = layer.world_type();
    let mut out = vec![0.0; out_width * out_height];

    for y in 0..out_height {
        for x in 0..out_width {
            let sx = remap_coord_to_layer(x, out_width, layer_width);
            let sy = remap_coord_to_layer(y, out_height, layer_height);
            let px = read_pixel_f32(layer, world_type, sx, sy);
            let mut value = mask_value(px, channel).clamp(0.0, 1.0);
            if invert {
                value = 1.0 - value;
            }
            out[y * out_width + x] = value;
        }
    }

    out
}

fn mask_value(px: PixelF32, channel: MaskChannel) -> f32 {
    let rgb = pixel_to_straight_rgb(px);
    match channel {
        MaskChannel::Alpha => sanitize_non_finite(px.alpha),
        MaskChannel::Luminance => {
            sanitize_non_finite(0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2])
        }
        MaskChannel::Red => rgb[0],
        MaskChannel::Green => rgb[1],
        MaskChannel::Blue => rgb[2],
    }
}

fn remap_coord_to_layer(coord: usize, out_len: usize, layer_len: usize) -> usize {
    if out_len == 0 || layer_len == 0 {
        return 0;
    }
    if out_len == layer_len {
        return coord.min(layer_len - 1);
    }
    let src = ((coord as f32 + 0.5) * layer_len as f32 / out_len as f32) - 0.5;
    src.round().clamp(0.0, layer_len.saturating_sub(1) as f32) as usize
}

fn multiply_mask_in_place(values: &mut [f32], mask: &[f32]) {
    for (value, mask_value) in values.iter_mut().zip(mask.iter().copied()) {
        *value *= mask_value.clamp(0.0, 1.0);
    }
}

fn build_sdf_mask_influence(
    alpha: &[f32],
    mask: &[f32],
    width: usize,
    height: usize,
    settings: &Settings,
) -> Vec<f32> {
    let masked_alpha: Vec<f32> = alpha
        .iter()
        .copied()
        .zip(mask.iter().copied())
        .map(|(a, m)| a.clamp(0.0, 1.0) * m.clamp(0.0, 1.0))
        .collect();
    gaussian_blur_scalar(&masked_alpha, width, height, settings.sdf_radius.max(0.0))
        .into_iter()
        .map(|v| v.clamp(0.0, 1.0))
        .collect()
}

fn build_alpha_displace_masks(
    alpha: &[f32],
    mask: &[f32],
    width: usize,
    height: usize,
    settings: &Settings,
) -> AlphaDisplaceMasks {
    let masked_alpha: Vec<f32> = alpha
        .iter()
        .copied()
        .zip(mask.iter().copied())
        .map(|(a, m)| a.clamp(0.0, 1.0) * m.clamp(0.0, 1.0))
        .collect();
    let source_map: Vec<f32> = masked_alpha
        .iter()
        .copied()
        .map(|coverage| {
            let remapped = coverage.powf(settings.alpha_map_gamma.max(0.05));
            lerp(DISPLACEMENT_NEUTRAL, 0.0, remapped)
        })
        .collect();
    let displacement_map = box_blur_scalar_repeat(
        &source_map,
        width,
        height,
        settings.alpha_blur_radius.max(0.0),
        3,
    );
    let displacement_amounts: Vec<f32> = displacement_map
        .iter()
        .copied()
        .map(displacement_value_to_coverage)
        .collect();
    let displaced_alpha = displace_alpha_with_map(
        &masked_alpha,
        &displacement_amounts,
        width,
        height,
        settings,
    );
    let mut out = vec![0.0; width * height];

    for idx in 0..out.len() {
        let source_alpha = masked_alpha[idx].clamp(0.0, 1.0);
        let displaced = displaced_alpha[idx].clamp(0.0, 1.0);
        out[idx] = match settings.alpha_matte_mode {
            AlphaMatteMode::InsideInverted => source_alpha * (1.0 - displaced),
            AlphaMatteMode::OutsideOnly => (displaced - source_alpha).max(0.0),
            AlphaMatteMode::DisplacedAlpha => displaced,
        };
    }

    AlphaDisplaceMasks {
        effect_mask: out,
        displacement_map,
    }
}

fn displace_alpha_with_map(
    alpha: &[f32],
    displacement_amounts: &[f32],
    width: usize,
    height: usize,
    settings: &Settings,
) -> Vec<f32> {
    let displacement = sanitize_non_finite(settings.alpha_displacement);
    if displacement.abs() <= ALPHA_EPSILON || width == 0 || height == 0 {
        return alpha.to_vec();
    }

    match settings.alpha_warp_quality {
        AlphaWarpQuality::Fast => displace_alpha_fast(
            alpha,
            displacement_amounts,
            width,
            height,
            displacement,
            settings,
        ),
        AlphaWarpQuality::Smooth => displace_alpha_supersampled(
            alpha,
            displacement_amounts,
            width,
            height,
            displacement,
            settings,
            false,
        ),
        AlphaWarpQuality::Accurate => displace_alpha_supersampled(
            alpha,
            displacement_amounts,
            width,
            height,
            displacement,
            settings,
            true,
        ),
    }
}

fn displace_alpha_fast(
    alpha: &[f32],
    displacement_amounts: &[f32],
    width: usize,
    height: usize,
    displacement: f32,
    settings: &Settings,
) -> Vec<f32> {
    let mut out = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let offset = displacement_amounts[idx].clamp(0.0, 1.0) * displacement;
            let src_x = x as f32 - settings.dir_x * offset;
            let src_y = y as f32 - settings.dir_y * offset;
            out[idx] = sample_scalar_bilinear_default(alpha, width, height, src_x, src_y, 0.0)
                .clamp(0.0, 1.0);
        }
    }
    out
}

fn displace_alpha_supersampled(
    alpha: &[f32],
    displacement_amounts: &[f32],
    width: usize,
    height: usize,
    displacement: f32,
    settings: &Settings,
    iterative: bool,
) -> Vec<f32> {
    let subpixels = [(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)];
    let ctx = AlphaWarpContext {
        displacement_amounts,
        width,
        height,
        displacement,
        dir_x: settings.dir_x,
        dir_y: settings.dir_y,
    };
    let mut out = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut sum = 0.0;

            for (offset_x, offset_y) in subpixels {
                let dst_x = x as f32 + offset_x;
                let dst_y = y as f32 + offset_y;
                let (src_x, src_y) = displacement_source(&ctx, dst_x, dst_y, iterative);
                sum += sample_scalar_bilinear_default(alpha, width, height, src_x, src_y, 0.0);
            }

            out[idx] = (sum / subpixels.len() as f32).clamp(0.0, 1.0);
        }
    }

    out
}

fn displacement_source(
    ctx: &AlphaWarpContext,
    dst_x: f32,
    dst_y: f32,
    iterative: bool,
) -> (f32, f32) {
    let amount = sample_scalar_bilinear_default(
        ctx.displacement_amounts,
        ctx.width,
        ctx.height,
        dst_x,
        dst_y,
        0.0,
    );
    let mut src_x = dst_x - ctx.dir_x * amount * ctx.displacement;
    let mut src_y = dst_y - ctx.dir_y * amount * ctx.displacement;

    if !iterative {
        return (src_x, src_y);
    }

    for _ in 0..DISPLACEMENT_INVERSE_ITERATIONS {
        let amount = sample_scalar_bilinear_default(
            ctx.displacement_amounts,
            ctx.width,
            ctx.height,
            src_x,
            src_y,
            0.0,
        );
        let offset = amount * ctx.displacement;
        src_x = dst_x - ctx.dir_x * offset;
        src_y = dst_y - ctx.dir_y * offset;
    }

    (src_x, src_y)
}

fn displacement_value_to_coverage(value: f32) -> f32 {
    ((DISPLACEMENT_NEUTRAL - value) / DISPLACEMENT_NEUTRAL).clamp(0.0, 1.0)
}

fn apply_antialias(value: f32, antialias: bool) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if antialias {
        value
    } else if value >= 0.5 {
        1.0
    } else {
        0.0
    }
}

fn build_sdf_mask(sdf: &[f32], width: usize, height: usize, settings: &Settings) -> Vec<f32> {
    let mut out = vec![0.0; width * height];
    let radius = settings.sdf_radius.max(0.0);
    if radius <= ALPHA_EPSILON {
        return out;
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if sdf[idx] >= 0.0 {
                continue;
            }
            let outside_distance = -sdf[idx];
            if outside_distance > radius {
                continue;
            }

            let t = 1.0 - smoothstep(0.0, radius, outside_distance);
            let falloff = t.powf(settings.sdf_falloff.max(0.05));
            let dir_weight = directional_weight(sdf, width, height, x, y, settings);
            out[idx] = falloff * dir_weight;
        }
    }

    out
}

fn apply_boundary_processing(
    source_mask: Vec<f32>,
    width: usize,
    height: usize,
    settings: &Settings,
) -> Vec<f32> {
    match settings.inner_mode {
        InnerMode::Off => source_mask,
        InnerMode::Blur => directional_blur_scalar(
            &source_mask,
            width,
            height,
            settings.dir_x,
            settings.dir_y,
            boundary_blur_radius(settings),
        ),
        InnerMode::Scatter => directional_scatter_scalar(
            &source_mask,
            width,
            height,
            settings.dir_x,
            settings.dir_y,
            boundary_scatter_radius(settings),
            settings.inner_samples,
        ),
        InnerMode::BlurScatter => {
            let blurred = directional_blur_scalar(
                &source_mask,
                width,
                height,
                settings.dir_x,
                settings.dir_y,
                boundary_blur_radius(settings),
            );
            directional_scatter_scalar(
                &blurred,
                width,
                height,
                settings.dir_x,
                settings.dir_y,
                boundary_scatter_radius(settings),
                settings.inner_samples,
            )
        }
    }
}

fn boundary_blur_radius(settings: &Settings) -> f32 {
    if settings.inner_separate_radii && matches!(settings.inner_mode, InnerMode::BlurScatter) {
        settings.inner_blur_radius
    } else {
        settings.inner_radius
    }
}

fn boundary_scatter_radius(settings: &Settings) -> f32 {
    if settings.inner_separate_radii && matches!(settings.inner_mode, InnerMode::BlurScatter) {
        settings.inner_scatter_radius
    } else {
        settings.inner_radius
    }
}

fn build_signed_distance(alpha: &[f32], width: usize, height: usize, threshold: f32) -> Vec<f32> {
    let inside: Vec<bool> = alpha.iter().map(|&a| a >= threshold).collect();
    let any_inside = inside.iter().any(|&v| v);
    let any_outside = inside.iter().any(|&v| !v);
    if !any_inside || !any_outside {
        return vec![0.0; width * height];
    }

    let dist_to_outside = chamfer_distance(&inside, width, height, false);
    let dist_to_inside = chamfer_distance(&inside, width, height, true);

    dist_to_outside
        .into_iter()
        .zip(dist_to_inside)
        .map(|(inside_dist, outside_dist)| inside_dist - outside_dist)
        .collect()
}

fn chamfer_distance(inside: &[bool], width: usize, height: usize, seed_inside: bool) -> Vec<f32> {
    let mut dist = vec![DIST_INF; width * height];
    for (idx, is_inside) in inside.iter().copied().enumerate() {
        if is_inside == seed_inside {
            dist[idx] = 0.0;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut best = dist[idx];
            if x > 0 {
                best = best.min(dist[idx - 1] + 1.0);
            }
            if y > 0 {
                best = best.min(dist[idx - width] + 1.0);
                if x > 0 {
                    best = best.min(dist[idx - width - 1] + std::f32::consts::SQRT_2);
                }
                if x + 1 < width {
                    best = best.min(dist[idx - width + 1] + std::f32::consts::SQRT_2);
                }
            }
            dist[idx] = best;
        }
    }

    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let idx = y * width + x;
            let mut best = dist[idx];
            if x + 1 < width {
                best = best.min(dist[idx + 1] + 1.0);
            }
            if y + 1 < height {
                best = best.min(dist[idx + width] + 1.0);
                if x + 1 < width {
                    best = best.min(dist[idx + width + 1] + std::f32::consts::SQRT_2);
                }
                if x > 0 {
                    best = best.min(dist[idx + width - 1] + std::f32::consts::SQRT_2);
                }
            }
            dist[idx] = best;
        }
    }

    dist
}

fn directional_weight(
    sdf: &[f32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    settings: &Settings,
) -> f32 {
    let left = sample_sdf(sdf, width, height, x as i32 - 1, y as i32);
    let right = sample_sdf(sdf, width, height, x as i32 + 1, y as i32);
    let up = sample_sdf(sdf, width, height, x as i32, y as i32 - 1);
    let down = sample_sdf(sdf, width, height, x as i32, y as i32 + 1);
    let gx = right - left;
    let gy = down - up;
    let len = (gx * gx + gy * gy).sqrt();
    if len <= ALPHA_EPSILON {
        return 1.0;
    }

    let outward_x = -gx / len;
    let outward_y = -gy / len;
    let facing = (outward_x * settings.dir_x + outward_y * settings.dir_y).max(0.0);
    lerp(1.0, facing, settings.sdf_directionality)
}

fn box_blur_scalar_repeat(
    src: &[f32],
    width: usize,
    height: usize,
    radius: f32,
    passes: usize,
) -> Vec<f32> {
    let mut out = src.to_vec();
    for _ in 0..passes {
        out = box_blur_scalar(&out, width, height, radius);
    }
    out
}

fn box_blur_scalar(src: &[f32], width: usize, height: usize, radius: f32) -> Vec<f32> {
    let radius = sanitize_non_finite(radius).max(0.0);
    if radius <= ALPHA_EPSILON || width == 0 || height == 0 {
        return src.to_vec();
    }

    let radius = radius.ceil().min(2048.0) as i32;
    if radius <= 0 {
        return src.to_vec();
    }

    let width_i = width as i32;
    let height_i = height as i32;
    let denom = (radius * 2 + 1) as f32;
    let mut tmp = vec![0.0; width * height];
    let mut out = vec![0.0; width * height];

    for y in 0..height {
        let row = y * width;
        let mut sum = 0.0;
        for offset in -radius..=radius {
            let sx = offset.clamp(0, width_i - 1) as usize;
            sum += src[row + sx];
        }

        for x in 0..width {
            tmp[row + x] = sum / denom;
            let remove_x = (x as i32 - radius).clamp(0, width_i - 1) as usize;
            let add_x = (x as i32 + radius + 1).clamp(0, width_i - 1) as usize;
            sum += src[row + add_x] - src[row + remove_x];
        }
    }

    for x in 0..width {
        let mut sum = 0.0;
        for offset in -radius..=radius {
            let sy = offset.clamp(0, height_i - 1) as usize;
            sum += tmp[sy * width + x];
        }

        for y in 0..height {
            out[y * width + x] = sum / denom;
            let remove_y = (y as i32 - radius).clamp(0, height_i - 1) as usize;
            let add_y = (y as i32 + radius + 1).clamp(0, height_i - 1) as usize;
            sum += tmp[add_y * width + x] - tmp[remove_y * width + x];
        }
    }

    out
}

fn gaussian_blur_scalar(src: &[f32], width: usize, height: usize, radius: f32) -> Vec<f32> {
    if radius <= ALPHA_EPSILON || width == 0 || height == 0 {
        return src.to_vec();
    }

    let kernel = gaussian_kernel(radius);
    let half = kernel.len() as i32 / 2;
    let mut tmp = vec![0.0; width * height];
    let mut out = vec![0.0; width * height];

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (k, weight) in kernel.iter().copied().enumerate() {
                let offset = k as i32 - half;
                let sx = (x as i32 + offset).clamp(0, width as i32 - 1) as usize;
                sum += src[y * width + sx] * weight;
            }
            tmp[y * width + x] = sum;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (k, weight) in kernel.iter().copied().enumerate() {
                let offset = k as i32 - half;
                let sy = (y as i32 + offset).clamp(0, height as i32 - 1) as usize;
                sum += tmp[sy * width + x] * weight;
            }
            out[y * width + x] = sum;
        }
    }

    out
}

fn gaussian_kernel(radius: f32) -> Vec<f32> {
    let sigma = (radius * 0.5).max(0.5);
    let half = radius.ceil().clamp(1.0, 512.0) as i32;
    let mut kernel = Vec::with_capacity((half * 2 + 1) as usize);
    let mut sum = 0.0;

    for i in -half..=half {
        let x = i as f32;
        let w = (-0.5 * (x / sigma).powi(2)).exp();
        kernel.push(w);
        sum += w;
    }

    if sum > ALPHA_EPSILON {
        for w in &mut kernel {
            *w /= sum;
        }
    }

    kernel
}

fn directional_blur_scalar(
    src: &[f32],
    width: usize,
    height: usize,
    dir_x: f32,
    dir_y: f32,
    radius: f32,
) -> Vec<f32> {
    let radius = sanitize_non_finite(radius).max(0.0);
    if radius <= ALPHA_EPSILON || width == 0 || height == 0 {
        return src.to_vec();
    }

    let samples = ((radius.ceil() as usize) * 2 + 1).clamp(3, 96);
    let denom = samples.saturating_sub(1).max(1) as f32;
    let sigma = (radius * 0.45).max(0.5);
    let mut out = vec![0.0; width * height];

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut weight_sum = 0.0;
            for i in 0..samples {
                let t = i as f32 / denom;
                let distance = radius * t;
                let weight = (-0.5 * (distance / sigma).powi(2)).exp();
                let sx = x as f32 - dir_x * distance;
                let sy = y as f32 - dir_y * distance;
                sum += sample_scalar_bilinear(src, width, height, sx, sy) * weight;
                weight_sum += weight;
            }

            out[y * width + x] = if weight_sum > ALPHA_EPSILON {
                sum / weight_sum
            } else {
                0.0
            };
        }
    }

    out
}

fn directional_scatter_scalar(
    src: &[f32],
    width: usize,
    height: usize,
    dir_x: f32,
    dir_y: f32,
    radius: f32,
    samples: usize,
) -> Vec<f32> {
    let radius = sanitize_non_finite(radius).max(0.0);
    if radius <= ALPHA_EPSILON || width == 0 || height == 0 {
        return src.to_vec();
    }

    let samples = samples.clamp(2, 128);
    let mut out = vec![0.0; width * height];
    let denom = (samples.saturating_sub(1)).max(1) as f32;

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut weight_sum = 0.0;
            for i in 0..samples {
                let t = i as f32 / denom;
                let weight = 1.0 - t * 0.75;
                let sx = x as f32 - dir_x * radius * t;
                let sy = y as f32 - dir_y * radius * t;
                sum += sample_scalar_bilinear(src, width, height, sx, sy) * weight;
                weight_sum += weight;
            }

            out[y * width + x] = if weight_sum > ALPHA_EPSILON {
                sum / weight_sum
            } else {
                0.0
            };
        }
    }

    out
}

fn sample_scalar_bilinear(src: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
        return 0.0;
    }

    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = sample_scalar(src, width, height, x0, y0);
    let p10 = sample_scalar(src, width, height, x1, y0);
    let p01 = sample_scalar(src, width, height, x0, y1);
    let p11 = sample_scalar(src, width, height, x1, y1);

    let top = lerp(p00, p10, tx);
    let bottom = lerp(p01, p11, tx);
    lerp(top, bottom, ty)
}

fn sample_scalar_bilinear_default(
    src: &[f32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    default: f32,
) -> f32 {
    if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
        return default;
    }

    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = sample_scalar_default(src, width, height, x0, y0, default);
    let p10 = sample_scalar_default(src, width, height, x1, y0, default);
    let p01 = sample_scalar_default(src, width, height, x0, y1, default);
    let p11 = sample_scalar_default(src, width, height, x1, y1, default);

    let top = lerp(p00, p10, tx);
    let bottom = lerp(p01, p11, tx);
    lerp(top, bottom, ty)
}

fn sample_scalar(src: &[f32], width: usize, height: usize, x: i32, y: i32) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let xi = x.clamp(0, width as i32 - 1) as usize;
    let yi = y.clamp(0, height as i32 - 1) as usize;
    src[yi * width + xi]
}

fn sample_scalar_default(
    src: &[f32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    default: f32,
) -> f32 {
    if width == 0 || height == 0 || x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return default;
    }
    src[y as usize * width + x as usize]
}

fn sample_sdf(sdf: &[f32], width: usize, height: usize, x: i32, y: i32) -> f32 {
    sample_scalar(sdf, width, height, x, y)
}

fn composite_light(
    base_px: PixelF32,
    mask: f32,
    out_is_f32: bool,
    settings: &Settings,
) -> PixelF32 {
    let source_alpha = sanitize_non_finite(base_px.alpha).clamp(0.0, 1.0);
    let base_rgb = pixel_to_straight_rgb(base_px);
    let opacity = settings.opacity;
    let light_strength = (mask * opacity).max(0.0);

    if light_strength <= ALPHA_EPSILON {
        return sanitize_pixel(base_px, out_is_f32, settings.clamp32);
    }

    let blend_rgb = settings.light_rgb;
    let blended = blend_rgb_values(base_rgb, blend_rgb, settings.blend_mode);
    let mix = light_strength.clamp(0.0, 1.0);
    let out_rgb = [
        lerp(base_rgb[0], blended[0], mix),
        lerp(base_rgb[1], blended[1], mix),
        lerp(base_rgb[2], blended[2], mix),
    ];

    let out_alpha = if settings.preserve_source_alpha {
        source_alpha
    } else {
        (source_alpha + mix * (1.0 - source_alpha)).clamp(0.0, 1.0)
    };

    sanitize_pixel(
        straight_rgb_to_pixel(out_rgb, out_alpha),
        out_is_f32,
        settings.clamp32,
    )
}

fn blend_rgb_values(base: [f32; 3], blend: [f32; 3], mode: BlendMode) -> [f32; 3] {
    [
        blend_channel(base[0], blend[0], mode),
        blend_channel(base[1], blend[1], mode),
        blend_channel(base[2], blend[2], mode),
    ]
}

fn blend_channel(b: f32, s: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => s,
        BlendMode::Add => b + s,
        BlendMode::Screen => 1.0 - (1.0 - b) * (1.0 - s),
        BlendMode::Overlay => {
            if b <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        BlendMode::SoftLight => soft_light(b, s),
        BlendMode::HardLight => {
            if s <= 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        BlendMode::Lighten => b.max(s),
    }
}

fn soft_light(b: f32, s: f32) -> f32 {
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

fn read_layer_rgba(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut out = vec![
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

fn pixel_to_straight_rgb(px: PixelF32) -> [f32; 3] {
    if px.alpha > ALPHA_EPSILON {
        [
            sanitize_non_finite(px.red / px.alpha),
            sanitize_non_finite(px.green / px.alpha),
            sanitize_non_finite(px.blue / px.alpha),
        ]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn straight_rgb_to_pixel(rgb: [f32; 3], alpha: f32) -> PixelF32 {
    if alpha > ALPHA_EPSILON {
        PixelF32 {
            alpha,
            red: sanitize_non_finite(rgb[0]) * alpha,
            green: sanitize_non_finite(rgb[1]) * alpha,
            blue: sanitize_non_finite(rgb[2]) * alpha,
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

fn sanitize_pixel(px: PixelF32, out_is_f32: bool, clamp32: bool) -> PixelF32 {
    let mut out = PixelF32 {
        alpha: sanitize_non_finite(px.alpha),
        red: sanitize_non_finite(px.red),
        green: sanitize_non_finite(px.green),
        blue: sanitize_non_finite(px.blue),
    };

    if !out_is_f32 || clamp32 {
        out.alpha = out.alpha.clamp(0.0, 1.0);
        out.red = out.red.clamp(0.0, 1.0);
        out.green = out.green.clamp(0.0, 1.0);
        out.blue = out.blue.clamp(0.0, 1.0);
    }

    out
}

fn wrap_mode_from_popup(value: i32) -> WrapMode {
    match value {
        2 => WrapMode::Sdf,
        _ => WrapMode::AlphaBlurDisplace,
    }
}

fn mask_channel_from_popup(value: i32) -> MaskChannel {
    match value {
        2 => MaskChannel::Luminance,
        3 => MaskChannel::Red,
        4 => MaskChannel::Green,
        5 => MaskChannel::Blue,
        _ => MaskChannel::Alpha,
    }
}

fn alpha_matte_mode_from_popup(value: i32) -> AlphaMatteMode {
    match value {
        2 => AlphaMatteMode::OutsideOnly,
        3 => AlphaMatteMode::DisplacedAlpha,
        _ => AlphaMatteMode::InsideInverted,
    }
}

fn alpha_warp_quality_from_popup(value: i32) -> AlphaWarpQuality {
    match value {
        2 => AlphaWarpQuality::Smooth,
        3 => AlphaWarpQuality::Accurate,
        _ => AlphaWarpQuality::Fast,
    }
}

fn inner_mode_from_popup(value: i32) -> InnerMode {
    match value {
        2 => InnerMode::Blur,
        3 => InnerMode::Scatter,
        4 => InnerMode::BlurScatter,
        _ => InnerMode::Off,
    }
}

fn blend_mode_from_popup(value: i32) -> BlendMode {
    match value {
        2 => BlendMode::Add,
        3 => BlendMode::Screen,
        4 => BlendMode::Overlay,
        5 => BlendMode::SoftLight,
        6 => BlendMode::HardLight,
        7 => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn sanitize_non_finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}
