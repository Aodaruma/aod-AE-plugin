#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

#[cfg(feature = "gpu_wgpu")]
use std::sync::{Arc, OnceLock};

use ae::pf::*;
use utils::ToPixel;

#[cfg(feature = "gpu_wgpu")]
mod gpu;
#[cfg(feature = "gpu_wgpu")]
use crate::gpu::wgpu::{WgpuContext, WgpuRenderParams};

const GABOR_IMPULSES_COUNT: i32 = 8;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TextureType,
    OutputType,
    Scale,
    UseScaleMap,
    ScaleMapLayer,
    Frequency,
    UseFrequencyMap,
    FrequencyMapLayer,
    Anisotropy,
    AnisotropyMapMode,
    AnisotropyMapLayer,
    Orientation2D,
    OrientationAzimuth,
    OrientationElevation,
    SliceW,
    SliceScale,
    Offset,
    Seed,
    Gain,
    Bias,
    Clamp32,
    UseOriginalAlpha,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextureType {
    Type2D,
    Type3D,
}

#[derive(Clone, Copy)]
enum OutputType {
    Value,
    Phase,
    Intensity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnisotropyMapMode {
    None,
    Value,
    DivergenceDirection,
    DivergenceRotation,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

#[derive(Clone, Copy)]
struct RenderSettings {
    texture_type: TextureType,
    output_type: OutputType,
    scale: f32,
    frequency: f32,
    isotropy: f32,
    orientation_2d: f32,
    orientation_azimuth: f32,
    orientation_elevation: f32,
    slice_w: f32,
    slice_scale: f32,
    offset_x: f32,
    offset_y: f32,
    seed: u32,
    gain: f32,
    bias: f32,
    clamp_32: bool,
    use_original_alpha: bool,
}

#[derive(Clone, Copy, Default)]
struct Phasor {
    re: f32,
    im: f32,
}

#[derive(Clone, Copy)]
struct GaborSignals {
    value: f32,
    phase: f32,
    intensity: f32,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Generates Blender-style Gabor texture maps.";

#[cfg(feature = "gpu_wgpu")]
static WGPU_CONTEXT: OnceLock<Result<Arc<WgpuContext>, ()>> = OnceLock::new();

#[cfg(feature = "gpu_wgpu")]
fn wgpu_context() -> Option<Arc<WgpuContext>> {
    match WGPU_CONTEXT.get_or_init(|| WgpuContext::new().map(Arc::new).map_err(|_| ())) {
        Ok(ctx) => Some(ctx.clone()),
        Err(_) => None,
    }
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::TextureType,
            "Type",
            PopupDef::setup(|d| {
                d.set_options(&["2D", "3D"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::OutputType,
            "Output",
            PopupDef::setup(|d| {
                d.set_options(&["Value", "Phase", "Intensity"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::Scale,
            "Scale",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.001);
                d.set_valid_max(1000.0);
                d.set_slider_min(0.1);
                d.set_slider_max(50.0);
                d.set_default(5.0);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::UseScaleMap,
            "Use Scale Map Layer",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(Params::ScaleMapLayer, "Scale Map Layer", LayerDef::new())?;

        params.add(
            Params::Frequency,
            "Frequency",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(128.0);
                d.set_slider_min(0.0);
                d.set_slider_max(16.0);
                d.set_default(2.0);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::UseFrequencyMap,
            "Use Frequency Map Layer",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::FrequencyMapLayer,
            "Frequency Map Layer",
            LayerDef::new(),
        )?;

        params.add(
            Params::Anisotropy,
            "Anisotropy",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::AnisotropyMapMode,
            "Anisotropy Map Mode",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "None",
                    "Value (Multiply)",
                    "Divergence Direction",
                    "Divergence Rotation",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::AnisotropyMapLayer,
            "Anisotropy Map Layer",
            LayerDef::new(),
        )?;

        params.add(
            Params::Orientation2D,
            "Orientation 2D (deg)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-3600.0);
                d.set_valid_max(3600.0);
                d.set_slider_min(-180.0);
                d.set_slider_max(180.0);
                d.set_default(45.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::OrientationAzimuth,
            "Orientation 3D Azimuth (deg)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-3600.0);
                d.set_valid_max(3600.0);
                d.set_slider_min(-180.0);
                d.set_slider_max(180.0);
                d.set_default(45.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::OrientationElevation,
            "Orientation 3D Elevation (deg)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-1800.0);
                d.set_valid_max(1800.0);
                d.set_slider_min(-90.0);
                d.set_slider_max(90.0);
                d.set_default(0.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::SliceW,
            "W",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-100000.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(-5.0);
                d.set_slider_max(5.0);
                d.set_default(0.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::SliceScale,
            "Scale W",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.001);
                d.set_valid_max(1000.0);
                d.set_slider_min(0.1);
                d.set_slider_max(10.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Offset,
            "Offset",
            PointDef::setup(|p| {
                p.set_default((0.0, 0.0));
            }),
        )?;

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
            Params::Gain,
            "Gain",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-8.0);
                d.set_valid_max(8.0);
                d.set_slider_min(0.0);
                d.set_slider_max(2.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Bias,
            "Bias",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-4.0);
                d.set_valid_max(4.0);
                d.set_slider_min(-1.0);
                d.set_slider_max(1.0);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Clamp32,
            "Clamp (32bpc)",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::UseOriginalAlpha,
            "Use Original Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
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
                        "AOD_GaborGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_GaborGenerate")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                #[cfg(feature = "gpu_wgpu")]
                {
                    if let Some(ctx) = wgpu_context()
                        && self
                            .do_render_wgpu(&in_layer, &mut out_layer, params, &ctx)
                            .is_ok()
                    {
                        return Ok(());
                    }
                }

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
                let t = params.type_at(param_index);
                if t == Params::TextureType
                    || t == Params::UseScaleMap
                    || t == Params::UseFrequencyMap
                    || t == Params::AnisotropyMapMode
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
        let texture_type =
            texture_type_from_popup(params.get(Params::TextureType)?.as_popup()?.value());
        let is_3d = matches!(texture_type, TextureType::Type3D);

        self.set_param_visible(in_data, params, Params::Orientation2D, !is_3d)?;
        self.set_param_visible(in_data, params, Params::OrientationAzimuth, is_3d)?;
        self.set_param_visible(in_data, params, Params::OrientationElevation, is_3d)?;
        self.set_param_visible(in_data, params, Params::SliceW, is_3d)?;
        self.set_param_visible(in_data, params, Params::SliceScale, is_3d)?;

        let use_scale_map = params.get(Params::UseScaleMap)?.as_checkbox()?.value();
        let use_frequency_map = params.get(Params::UseFrequencyMap)?.as_checkbox()?.value();
        let anisotropy_map_mode = anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        );
        let use_anisotropy_map = !matches!(anisotropy_map_mode, AnisotropyMapMode::None);

        self.set_param_visible(in_data, params, Params::ScaleMapLayer, use_scale_map)?;
        self.set_param_visible(
            in_data,
            params,
            Params::FrequencyMapLayer,
            use_frequency_map,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::AnisotropyMapLayer,
            use_anisotropy_map,
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

    #[cfg(feature = "gpu_wgpu")]
    fn do_render_wgpu(
        &self,
        in_layer: &Layer,
        out_layer: &mut Layer,
        params: &mut Parameters<Params>,
        ctx: &WgpuContext,
    ) -> Result<(), Error> {
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if out_w == 0 || out_h == 0 {
            return Ok(());
        }

        let use_scale_map = params.get(Params::UseScaleMap)?.as_checkbox()?.value();
        let use_frequency_map = params.get(Params::UseFrequencyMap)?.as_checkbox()?.value();
        let anisotropy_map_mode = anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        );
        let use_anisotropy_map = !matches!(anisotropy_map_mode, AnisotropyMapMode::None);

        if use_scale_map || use_frequency_map || use_anisotropy_map {
            return Err(Error::BadCallbackParameter);
        }

        let render = Self::read_render_settings(params)?;

        let gpu_params = WgpuRenderParams {
            out_w: out_w as u32,
            out_h: out_h as u32,
            texture_type: match render.texture_type {
                TextureType::Type2D => 0,
                TextureType::Type3D => 1,
            },
            output_type: match render.output_type {
                OutputType::Value => 0,
                OutputType::Phase => 1,
                OutputType::Intensity => 2,
            },
            scale: render.scale,
            frequency: render.frequency,
            isotropy: render.isotropy,
            orientation_2d: render.orientation_2d,
            orientation_azimuth: render.orientation_azimuth,
            orientation_elevation: render.orientation_elevation,
            slice_w: render.slice_w,
            slice_scale: render.slice_scale,
            offset_x: render.offset_x,
            offset_y: render.offset_y,
            seed: render.seed,
            gain: render.gain,
            bias: render.bias,
        };

        let output = ctx.render(&gpu_params)?;

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        out_layer.iterate(0, out_h as i32, None, |x, y, mut dst| {
            let idx = ((y as usize) * out_w + x as usize) * 4;

            let mut out_px = PixelF32 {
                red: sanitize_output(output.data[idx], out_is_f32, render.clamp_32),
                green: sanitize_output(output.data[idx + 1], out_is_f32, render.clamp_32),
                blue: sanitize_output(output.data[idx + 2], out_is_f32, render.clamp_32),
                alpha: 1.0,
            };

            if render.use_original_alpha {
                let mut a = read_pixel_f32(in_layer, in_world_type, x as usize, y as usize).alpha;
                if !a.is_finite() {
                    a = 0.0;
                }
                a = a.clamp(0.0, 1.0);
                out_px.red *= a;
                out_px.green *= a;
                out_px.blue *= a;
                out_px.alpha = a;
            }

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(out_px),
            }

            Ok(())
        })?;

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
        if out_w == 0 || out_h == 0 {
            return Ok(());
        }

        let render = Self::read_render_settings(params)?;
        let span = out_w.min(out_h).max(1) as f32;
        let base_anisotropy = 1.0 - render.isotropy;

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        let use_scale_map = params.get(Params::UseScaleMap)?.as_checkbox()?.value();
        let use_frequency_map = params.get(Params::UseFrequencyMap)?.as_checkbox()?.value();
        let anisotropy_map_mode = anisotropy_map_mode_from_popup(
            params.get(Params::AnisotropyMapMode)?.as_popup()?.value(),
        );

        let scale_map_checkout = params.checkout_at(Params::ScaleMapLayer, None, None, None)?;
        let scale_map_layer = scale_map_checkout.as_layer()?.value();
        let scale_map_world_type = scale_map_layer.as_ref().map(|layer| layer.world_type());
        let scale_map_enabled = use_scale_map && scale_map_layer.is_some();

        let frequency_map_checkout =
            params.checkout_at(Params::FrequencyMapLayer, None, None, None)?;
        let frequency_map_layer = frequency_map_checkout.as_layer()?.value();
        let frequency_map_world_type = frequency_map_layer.as_ref().map(|layer| layer.world_type());
        let frequency_map_enabled = use_frequency_map && frequency_map_layer.is_some();

        let anisotropy_map_checkout =
            params.checkout_at(Params::AnisotropyMapLayer, None, None, None)?;
        let anisotropy_map_layer = anisotropy_map_checkout.as_layer()?.value();
        let anisotropy_map_world_type = anisotropy_map_layer
            .as_ref()
            .map(|layer| layer.world_type());
        let anisotropy_map_enabled = !matches!(anisotropy_map_mode, AnisotropyMapMode::None)
            && anisotropy_map_layer.is_some();

        out_layer.iterate(0, out_h as i32, None, |x, y, mut dst| {
            let mut render_px = render;
            if scale_map_enabled {
                let scale_factor = map_value_at(
                    scale_map_layer.as_ref(),
                    scale_map_world_type,
                    x as usize,
                    y as usize,
                    out_w,
                    out_h,
                );
                render_px.scale = (render.scale * scale_factor.max(0.0)).max(0.001);
            }
            if frequency_map_enabled {
                let frequency_factor = map_value_at(
                    frequency_map_layer.as_ref(),
                    frequency_map_world_type,
                    x as usize,
                    y as usize,
                    out_w,
                    out_h,
                );
                render_px.frequency = (render.frequency * frequency_factor.max(0.0)).max(0.001);
            }
            if anisotropy_map_enabled {
                match anisotropy_map_mode {
                    AnisotropyMapMode::None => {}
                    AnisotropyMapMode::Value => {
                        let anisotropy_factor = map_value_at(
                            anisotropy_map_layer.as_ref(),
                            anisotropy_map_world_type,
                            x as usize,
                            y as usize,
                            out_w,
                            out_h,
                        );
                        let anisotropy = (base_anisotropy * anisotropy_factor).clamp(0.0, 1.0);
                        render_px.isotropy = (1.0 - anisotropy).clamp(0.0, 1.0);
                    }
                    AnisotropyMapMode::DivergenceDirection
                    | AnisotropyMapMode::DivergenceRotation => {
                        let x0 = (x as usize).saturating_sub(1);
                        let x1 = (x as usize + 1).min(out_w.saturating_sub(1));
                        let y0 = (y as usize).saturating_sub(1);
                        let y1 = (y as usize + 1).min(out_h.saturating_sub(1));

                        let gx = map_value_at(
                            anisotropy_map_layer.as_ref(),
                            anisotropy_map_world_type,
                            x1,
                            y as usize,
                            out_w,
                            out_h,
                        ) - map_value_at(
                            anisotropy_map_layer.as_ref(),
                            anisotropy_map_world_type,
                            x0,
                            y as usize,
                            out_w,
                            out_h,
                        );
                        let gy = map_value_at(
                            anisotropy_map_layer.as_ref(),
                            anisotropy_map_world_type,
                            x as usize,
                            y1,
                            out_w,
                            out_h,
                        ) - map_value_at(
                            anisotropy_map_layer.as_ref(),
                            anisotropy_map_world_type,
                            x as usize,
                            y0,
                            out_w,
                            out_h,
                        );

                        let (dir_x, dir_y) =
                            if matches!(anisotropy_map_mode, AnisotropyMapMode::DivergenceRotation)
                            {
                                (-gy, gx)
                            } else {
                                (gx, gy)
                            };

                        let len2 = dir_x * dir_x + dir_y * dir_y;
                        if len2 > 1.0e-12 {
                            let angle = dir_y.atan2(dir_x);
                            render_px.orientation_2d = angle;
                            render_px.orientation_azimuth = angle;
                        }
                    }
                }
            }

            let coord_x = (x as f32 + 0.5 - render.offset_x) / span;
            let coord_y = (y as f32 + 0.5 - render.offset_y) / span;
            let coord_z = render.slice_w * render.slice_scale;

            let signals = sample_gabor(coord_x, coord_y, coord_z, &render_px);

            let mut v = match render.output_type {
                OutputType::Value => signals.value,
                OutputType::Phase => signals.phase,
                OutputType::Intensity => signals.intensity,
            };

            v = v * render.gain + render.bias;
            v = sanitize_output(v, out_is_f32, render.clamp_32);

            let mut out_px = PixelF32 {
                red: v,
                green: v,
                blue: v,
                alpha: 1.0,
            };

            if render.use_original_alpha {
                let mut a = read_pixel_f32(&in_layer, in_world_type, x as usize, y as usize).alpha;
                if !a.is_finite() {
                    a = 0.0;
                }
                a = a.clamp(0.0, 1.0);
                out_px.red *= a;
                out_px.green *= a;
                out_px.blue *= a;
                out_px.alpha = a;
            }

            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(out_px),
            }

            Ok(())
        })?;

        Ok(())
    }

    fn read_render_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
        let texture_type =
            texture_type_from_popup(params.get(Params::TextureType)?.as_popup()?.value());
        let output_type =
            output_type_from_popup(params.get(Params::OutputType)?.as_popup()?.value());

        let scale = params.get(Params::Scale)?.as_float_slider()?.value() as f32;
        let frequency = params.get(Params::Frequency)?.as_float_slider()?.value() as f32;
        let anisotropy = params.get(Params::Anisotropy)?.as_float_slider()?.value() as f32;
        let orientation_2d = params
            .get(Params::Orientation2D)?
            .as_float_slider()?
            .value() as f32;
        let orientation_azimuth = params
            .get(Params::OrientationAzimuth)?
            .as_float_slider()?
            .value() as f32;
        let orientation_elevation = params
            .get(Params::OrientationElevation)?
            .as_float_slider()?
            .value() as f32;
        let slice_w = params.get(Params::SliceW)?.as_float_slider()?.value() as f32;
        let slice_scale = params.get(Params::SliceScale)?.as_float_slider()?.value() as f32;
        let offset = params.get(Params::Offset)?;
        let (offset_x, offset_y) = point_value_f32(&offset.as_point()?);
        let seed = params.get(Params::Seed)?.as_slider()?.value() as u32;
        let gain = params.get(Params::Gain)?.as_float_slider()?.value() as f32;
        let bias = params.get(Params::Bias)?.as_float_slider()?.value() as f32;
        let clamp_32 = params.get(Params::Clamp32)?.as_checkbox()?.value();
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();

        Ok(RenderSettings {
            texture_type,
            output_type,
            scale: scale.max(0.001),
            frequency: frequency.max(0.001),
            isotropy: (1.0 - anisotropy).clamp(0.0, 1.0),
            orientation_2d: orientation_2d.to_radians(),
            orientation_azimuth: orientation_azimuth.to_radians(),
            orientation_elevation: orientation_elevation.to_radians(),
            slice_w,
            slice_scale: slice_scale.max(0.001),
            offset_x,
            offset_y,
            seed,
            gain,
            bias,
            clamp_32,
            use_original_alpha,
        })
    }
}

fn texture_type_from_popup(value: i32) -> TextureType {
    match value {
        2 => TextureType::Type3D,
        _ => TextureType::Type2D,
    }
}

fn output_type_from_popup(value: i32) -> OutputType {
    match value {
        2 => OutputType::Phase,
        3 => OutputType::Intensity,
        _ => OutputType::Value,
    }
}

fn anisotropy_map_mode_from_popup(value: i32) -> AnisotropyMapMode {
    match value {
        2 => AnisotropyMapMode::Value,
        3 => AnisotropyMapMode::DivergenceDirection,
        4 => AnisotropyMapMode::DivergenceRotation,
        _ => AnisotropyMapMode::None,
    }
}

fn sample_gabor(x: f32, y: f32, z: f32, render: &RenderSettings) -> GaborSignals {
    match render.texture_type {
        TextureType::Type2D => {
            let noise = gabor_2d(
                [x * render.scale, y * render.scale],
                render.frequency,
                render.isotropy,
                render.orientation_2d,
                render.seed,
            );

            let normalization = 6.0 * compute_2d_gabor_standard_deviation();
            let phase = noise.im.atan2(noise.re);
            let value = (noise.im / normalization) * 0.5 + 0.5;
            let intensity = (noise.re * noise.re + noise.im * noise.im).sqrt() / normalization;
            GaborSignals {
                value,
                phase: (phase + std::f32::consts::PI) / std::f32::consts::TAU,
                intensity,
            }
        }
        TextureType::Type3D => {
            let noise = gabor_3d(
                [x * render.scale, y * render.scale, z * render.scale],
                render.frequency,
                render.isotropy,
                orientation_3d(render.orientation_azimuth, render.orientation_elevation),
                render.seed,
            );

            let normalization = 6.0 * compute_3d_gabor_standard_deviation();
            let phase = noise.im.atan2(noise.re);
            let value = (noise.im / normalization) * 0.5 + 0.5;
            let intensity = (noise.re * noise.re + noise.im * noise.im).sqrt() / normalization;
            GaborSignals {
                value,
                phase: (phase + std::f32::consts::PI) / std::f32::consts::TAU,
                intensity,
            }
        }
    }
}

fn gabor_2d(
    coordinates: [f32; 2],
    frequency: f32,
    isotropy: f32,
    base_orientation: f32,
    seed: u32,
) -> Phasor {
    let cell_x = coordinates[0].floor() as i32;
    let cell_y = coordinates[1].floor() as i32;
    let local_x = coordinates[0] - cell_x as f32;
    let local_y = coordinates[1] - cell_y as f32;

    let mut sum = Phasor::default();

    for j in -1..=1 {
        for i in -1..=1 {
            let current_cell_x = cell_x + i;
            let current_cell_y = cell_y + j;
            let cell_noise = gabor_2d_cell(
                current_cell_x,
                current_cell_y,
                [local_x - i as f32, local_y - j as f32],
                frequency,
                isotropy,
                base_orientation,
                seed,
            );
            sum.re += cell_noise.re;
            sum.im += cell_noise.im;
        }
    }

    sum
}

fn gabor_2d_cell(
    cell_x: i32,
    cell_y: i32,
    position: [f32; 2],
    frequency: f32,
    isotropy: f32,
    base_orientation: f32,
    seed: u32,
) -> Phasor {
    let mut sum = Phasor::default();

    for impulse in 0..GABOR_IMPULSES_COUNT {
        let random_orientation =
            (rand01(hash_2d(cell_x, cell_y, impulse, 0, seed)) - 0.5) * std::f32::consts::PI;
        let orientation = base_orientation + random_orientation * isotropy;

        let kernel_center_x = rand01(hash_2d(cell_x, cell_y, impulse, 1, seed));
        let kernel_center_y = rand01(hash_2d(cell_x, cell_y, impulse, 2, seed));

        let dx = position[0] - kernel_center_x;
        let dy = position[1] - kernel_center_y;
        let radius2 = dx * dx + dy * dy;

        if radius2 >= 1.0 {
            continue;
        }

        let weight = if rand01(hash_2d(cell_x, cell_y, impulse, 3, seed)) < 0.5 {
            -1.0
        } else {
            1.0
        };

        let kernel = gabor_kernel_2d(dx, dy, frequency, orientation, radius2);
        sum.re += weight * kernel.re;
        sum.im += weight * kernel.im;
    }

    sum
}

fn gabor_kernel_2d(dx: f32, dy: f32, frequency: f32, orientation: f32, radius2: f32) -> Phasor {
    let hann = 0.5 + 0.5 * (std::f32::consts::PI * radius2).cos();
    let gaussian = (-std::f32::consts::PI * radius2).exp();
    let envelope = gaussian * hann;

    let dir_x = orientation.cos();
    let dir_y = orientation.sin();
    let angle = std::f32::consts::TAU * frequency * (dx * dir_x + dy * dir_y);

    Phasor {
        re: envelope * angle.cos(),
        im: envelope * angle.sin(),
    }
}

fn gabor_3d(
    coordinates: [f32; 3],
    frequency: f32,
    isotropy: f32,
    base_orientation: [f32; 3],
    seed: u32,
) -> Phasor {
    let cell_x = coordinates[0].floor() as i32;
    let cell_y = coordinates[1].floor() as i32;
    let cell_z = coordinates[2].floor() as i32;

    let local_x = coordinates[0] - cell_x as f32;
    let local_y = coordinates[1] - cell_y as f32;
    let local_z = coordinates[2] - cell_z as f32;

    let mut sum = Phasor::default();

    for k in -1..=1 {
        for j in -1..=1 {
            for i in -1..=1 {
                let current_cell_x = cell_x + i;
                let current_cell_y = cell_y + j;
                let current_cell_z = cell_z + k;
                let cell_noise = gabor_3d_cell(
                    current_cell_x,
                    current_cell_y,
                    current_cell_z,
                    [local_x - i as f32, local_y - j as f32, local_z - k as f32],
                    frequency,
                    isotropy,
                    base_orientation,
                    seed,
                );
                sum.re += cell_noise.re;
                sum.im += cell_noise.im;
            }
        }
    }

    sum
}

fn gabor_3d_cell(
    cell_x: i32,
    cell_y: i32,
    cell_z: i32,
    position: [f32; 3],
    frequency: f32,
    isotropy: f32,
    base_orientation: [f32; 3],
    seed: u32,
) -> Phasor {
    let mut sum = Phasor::default();

    for impulse in 0..GABOR_IMPULSES_COUNT {
        let orientation = compute_3d_orientation(
            cell_x,
            cell_y,
            cell_z,
            impulse,
            isotropy,
            base_orientation,
            seed,
        );

        let kernel_center_x = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 2, seed));
        let kernel_center_y = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 3, seed));
        let kernel_center_z = rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 4, seed));

        let dx = position[0] - kernel_center_x;
        let dy = position[1] - kernel_center_y;
        let dz = position[2] - kernel_center_z;
        let radius2 = dx * dx + dy * dy + dz * dz;

        if radius2 >= 1.0 {
            continue;
        }

        let weight = if rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 5, seed)) < 0.5 {
            -1.0
        } else {
            1.0
        };

        let kernel = gabor_kernel_3d(dx, dy, dz, frequency, orientation, radius2);
        sum.re += weight * kernel.re;
        sum.im += weight * kernel.im;
    }

    sum
}

fn gabor_kernel_3d(
    dx: f32,
    dy: f32,
    dz: f32,
    frequency: f32,
    orientation: [f32; 3],
    radius2: f32,
) -> Phasor {
    let hann = 0.5 + 0.5 * (std::f32::consts::PI * radius2).cos();
    let gaussian = (-std::f32::consts::PI * radius2).exp();
    let envelope = gaussian * hann;

    let dot = dx * orientation[0] + dy * orientation[1] + dz * orientation[2];
    let angle = std::f32::consts::TAU * frequency * dot;

    Phasor {
        re: envelope * angle.cos(),
        im: envelope * angle.sin(),
    }
}

fn orientation_3d(azimuth: f32, elevation: f32) -> [f32; 3] {
    let cos_e = elevation.cos();
    normalize3([
        cos_e * azimuth.cos(),
        cos_e * azimuth.sin(),
        elevation.sin(),
    ])
}

fn compute_3d_orientation(
    cell_x: i32,
    cell_y: i32,
    cell_z: i32,
    impulse: i32,
    isotropy: f32,
    base_orientation: [f32; 3],
    seed: u32,
) -> [f32; 3] {
    if isotropy <= 1.0e-6 {
        return base_orientation;
    }

    let mut inclination = base_orientation[2].clamp(-1.0, 1.0).acos();
    let mut azimuth = base_orientation[1].atan2(base_orientation[0]);

    let random_inclination =
        rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 0, seed)) * std::f32::consts::PI;
    let random_azimuth =
        rand01(hash_3d(cell_x, cell_y, cell_z, impulse, 1, seed)) * std::f32::consts::PI;

    inclination += random_inclination * isotropy;
    azimuth += random_azimuth * isotropy;

    normalize3([
        inclination.sin() * azimuth.cos(),
        inclination.sin() * azimuth.sin(),
        inclination.cos(),
    ])
}

fn compute_2d_gabor_standard_deviation() -> f32 {
    let integral_of_squared = 0.25_f32;
    let second_moment = 0.5_f32;
    (GABOR_IMPULSES_COUNT as f32 * second_moment * integral_of_squared).sqrt()
}

fn compute_3d_gabor_standard_deviation() -> f32 {
    let integral_of_squared = 1.0_f32 / (4.0 * 2.0_f32.sqrt());
    let second_moment = 0.5_f32;
    (GABOR_IMPULSES_COUNT as f32 * second_moment * integral_of_squared).sqrt()
}

fn hash_2d(cell_x: i32, cell_y: i32, impulse: i32, channel: u32, seed: u32) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    h = h.wrapping_add((cell_x as u32).wrapping_mul(0x85EB_CA6B));
    h = h.wrapping_add((cell_y as u32).wrapping_mul(0xC2B2_AE35));
    h = h.wrapping_add((impulse as u32).wrapping_mul(0x27D4_EB2D));
    h = h.wrapping_add(channel.wrapping_mul(0x1656_67B1));
    hash_u32(h)
}

fn hash_3d(cell_x: i32, cell_y: i32, cell_z: i32, impulse: i32, channel: u32, seed: u32) -> u32 {
    let mut h = seed ^ 0x517C_C1B7;
    h = h.wrapping_add((cell_x as u32).wrapping_mul(0x85EB_CA6B));
    h = h.wrapping_add((cell_y as u32).wrapping_mul(0xC2B2_AE35));
    h = h.wrapping_add((cell_z as u32).wrapping_mul(0x9E37_79B9));
    h = h.wrapping_add((impulse as u32).wrapping_mul(0x27D4_EB2D));
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

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len2 <= 1.0e-20 {
        return [1.0, 0.0, 0.0];
    }
    let inv = len2.sqrt().recip();
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

fn map_value_at(
    layer: Option<&Layer>,
    world_type: Option<ae::aegp::WorldType>,
    x: usize,
    y: usize,
    out_w: usize,
    out_h: usize,
) -> f32 {
    if let (Some(layer), Some(world_type)) = (layer, world_type) {
        let layer_w = layer.width().max(1);
        let layer_h = layer.height().max(1);
        let norm_x = (x as f32 + 0.5) / out_w.max(1) as f32;
        let norm_y = (y as f32 + 0.5) / out_h.max(1) as f32;
        let map_x = (norm_x * layer_w as f32)
            .floor()
            .clamp(0.0, (layer_w - 1) as f32) as usize;
        let map_y = (norm_y * layer_h as f32)
            .floor()
            .clamp(0.0, (layer_h - 1) as f32) as usize;
        let p = read_pixel_f32(layer, world_type, map_x, map_y);
        let v = (p.red + p.green + p.blue) * (1.0 / 3.0);
        if v.is_finite() { v.max(0.0) } else { 1.0 }
    } else {
        1.0
    }
}

fn sanitize_output(mut v: f32, out_is_f32: bool, clamp_32: bool) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    if !out_is_f32 || clamp_32 {
        v = v.clamp(0.0, 1.0);
    }
    v
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn point_value_f32(point: &PointDef<'_>) -> (f32, f32) {
    match point.float_value() {
        Ok(p) => (p.x as f32, p.y as f32),
        Err(_) => point.value(),
    }
}
