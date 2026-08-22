#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Falloff,
    NearDistance,
    FarDistance,
    PowerExponent,
    Density,
    ClampOutput,
    UseOriginalAlpha,
    UseColorMap,
    NearColor,
    FogColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Falloff {
    Linear,
    Quadratic,
    InverseQuadratic,
    Power,
    BeerLambert,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Converts monochrome depth maps into realistic fog falloff maps.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::Falloff,
            "Falloff",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Linear",
                    "Quadratic",
                    "Inverse Quadratic",
                    "Power",
                    "Beer-Lambert",
                ]);
                d.set_default(5);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::NearDistance,
            "Near Distance (32bpc only)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-1000000.0);
                d.set_valid_max(1000000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1000.0);
                d.set_default(0.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::FarDistance,
            "Far Distance (32bpc only)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-1000000.0);
                d.set_valid_max(1000000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1000.0);
                d.set_default(100.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::PowerExponent,
            "Power Exponent",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.01);
                d.set_valid_max(64.0);
                d.set_slider_min(0.1);
                d.set_slider_max(8.0);
                d.set_default(2.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Density,
            "Beer-Lambert Density",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(10.0);
                d.set_default(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::ClampOutput,
            "Clamp Output 0..1",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

        params.add(
            Params::UseOriginalAlpha,
            "Use Original Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add_with_flags(
            Params::UseColorMap,
            "Use Color Map",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::NearColor,
            "Near Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: 255,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
            }),
        )?;

        params.add(
            Params::FogColor,
            "Fog Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: 255,
                    red: 255,
                    green: 255,
                    blue: 255,
                });
            }),
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
                        "AOD_DepthFog - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_layer, out_data, out_layer, params)?;
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
                    self.do_render(in_layer, out_data, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let t = params.type_at(param_index);
                if t == Params::Falloff || t == Params::UseColorMap {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params_copy = params.cloned();
                self.update_params_ui(&mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn update_params_ui(&self, params: &mut Parameters<Params>) -> Result<(), Error> {
        let falloff = falloff_from_popup(params.get(Params::Falloff)?.as_popup()?.value());
        let use_color_map = params.get(Params::UseColorMap)?.as_checkbox()?.value();
        let is_32bpc = project_is_32bpc();

        Self::set_param_enabled(params, Params::NearDistance, is_32bpc)?;
        Self::set_param_enabled(params, Params::FarDistance, is_32bpc)?;
        Self::set_param_enabled(params, Params::PowerExponent, falloff == Falloff::Power)?;
        Self::set_param_enabled(params, Params::Density, falloff == Falloff::BeerLambert)?;
        Self::set_param_enabled(params, Params::NearColor, use_color_map)?;
        Self::set_param_enabled(params, Params::FogColor, use_color_map)?;

        Ok(())
    }

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        let flag = ae::pf::ParamUIFlags::DISABLED;
        let flag_bits = flag.bits();
        let status = !enabled;
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
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let in_is_f32 = matches!(
            in_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        let falloff = falloff_from_popup(params.get(Params::Falloff)?.as_popup()?.value());
        let near_distance = params.get(Params::NearDistance)?.as_float_slider()?.value() as f32;
        let far_distance = params.get(Params::FarDistance)?.as_float_slider()?.value() as f32;
        let power = (params
            .get(Params::PowerExponent)?
            .as_float_slider()?
            .value() as f32)
            .max(0.01);
        let density = (params.get(Params::Density)?.as_float_slider()?.value() as f32).max(0.0);
        let clamp_output = params.get(Params::ClampOutput)?.as_checkbox()?.value();
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();
        let use_color_map = params.get(Params::UseColorMap)?.as_checkbox()?.value();
        let near_color = params
            .get(Params::NearColor)?
            .as_color()?
            .value()
            .to_pixel32();
        let fog_color = params
            .get(Params::FogColor)?
            .as_color()?
            .value()
            .to_pixel32();
        let progress_final = height as i32;

        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let src = read_pixel_f32(&in_layer, in_world_type, x as usize, y as usize);
            let depth = luminance(src);
            let (distance, normalized_depth) =
                map_depth(depth, near_distance, far_distance, in_is_f32);
            let mut fog = apply_falloff(falloff, normalized_depth, distance, power, density);

            let force_clamp = clamp_output || !out_is_f32;
            if !fog.is_finite() {
                fog = 0.0;
            }
            if force_clamp {
                fog = fog.clamp(0.0, 1.0);
            }

            let mut out_px = if use_color_map {
                lerp_color(near_color, fog_color, fog)
            } else {
                PixelF32 {
                    alpha: 1.0,
                    red: fog,
                    green: fog,
                    blue: fog,
                }
            };

            if use_original_alpha {
                let alpha = sanitize_alpha(src.alpha);
                out_px.alpha = alpha;
                out_px.red *= alpha;
                out_px.green *= alpha;
                out_px.blue *= alpha;
            } else {
                out_px.alpha = 1.0;
            }

            if force_clamp {
                out_px.red = out_px.red.clamp(0.0, 1.0);
                out_px.green = out_px.green.clamp(0.0, 1.0);
                out_px.blue = out_px.blue.clamp(0.0, 1.0);
                out_px.alpha = out_px.alpha.clamp(0.0, 1.0);
            }

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                    dst.set_from_f32(out_px);
                }
            }

            Ok(())
        })?;

        Ok(())
    }
}

fn project_is_32bpc() -> bool {
    let Ok(project_suite) = ae::aegp::suites::Project::new() else {
        return false;
    };
    let Ok(project_count) = project_suite.num_projects() else {
        return false;
    };
    if project_count <= 0 {
        return false;
    }
    let Ok(project) = project_suite.project_by_index(0) else {
        return false;
    };
    matches!(
        project_suite.project_bit_depth(project),
        Ok(ae::aegp::ProjectBitDepth::BitDepthF32)
    )
}

fn falloff_from_popup(value: i32) -> Falloff {
    match value {
        2 => Falloff::Quadratic,
        3 => Falloff::InverseQuadratic,
        4 => Falloff::Power,
        5 => Falloff::BeerLambert,
        _ => Falloff::Linear,
    }
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn luminance(px: PixelF32) -> f32 {
    (px.red + px.green + px.blue) / 3.0
}

fn map_depth(
    depth: f32,
    near_distance: f32,
    far_distance: f32,
    use_distance_range: bool,
) -> (f32, f32) {
    if !use_distance_range {
        let normalized = depth.clamp(0.0, 1.0);
        return (normalized, normalized);
    }

    let range = far_distance - near_distance;
    if range.abs() <= 1.0e-6 {
        return (0.0, 0.0);
    }

    let distance = depth - near_distance;
    let normalized = (distance / range).clamp(0.0, 1.0);
    (distance.max(0.0), normalized)
}

fn apply_falloff(
    falloff: Falloff,
    normalized_depth: f32,
    distance: f32,
    power: f32,
    density: f32,
) -> f32 {
    let t = normalized_depth.max(0.0);
    match falloff {
        Falloff::Linear => t,
        Falloff::Quadratic => t * t,
        Falloff::InverseQuadratic => 1.0 - (1.0 - t).max(0.0).powi(2),
        Falloff::Power => t.powf(power),
        Falloff::BeerLambert => 1.0 - (-density * distance.max(0.0)).exp(),
    }
}

fn lerp_color(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: a.alpha + (b.alpha - a.alpha) * t,
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
    }
}

fn sanitize_alpha(alpha: f32) -> f32 {
    if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
