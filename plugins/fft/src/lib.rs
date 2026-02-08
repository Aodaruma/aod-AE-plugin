#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

#[cfg(feature = "gpu_wgpu")]
use std::sync::{Arc, OnceLock};

use ae::pf::*;
use utils::ToPixel;

#[cfg(feature = "gpu_wgpu")]
use utils::spectral_wgpu::SpectralWgpuContext;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Output,
    ProcessWidth,
    ProcessHeight,
    Offset,
    Scale,
    RgbOnly,
    Raw32,
}

#[derive(Clone, Copy)]
enum OutputMode {
    Real,
    Imag,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Performs 2D FFT on RGBA channels and outputs real or imaginary spectra.";

#[cfg(feature = "gpu_wgpu")]
static WGPU_CONTEXT: OnceLock<Result<Arc<SpectralWgpuContext>, ()>> = OnceLock::new();

#[cfg(feature = "gpu_wgpu")]
fn wgpu_context() -> Option<Arc<SpectralWgpuContext>> {
    match WGPU_CONTEXT.get_or_init(|| SpectralWgpuContext::new().map(Arc::new).map_err(|_| ())) {
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
        params.add(
            Params::Output,
            "Output",
            PopupDef::setup(|d| {
                d.set_options(&["Real", "Imaginary"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::ProcessWidth,
            "Process Width (0=Source)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(8192);
                d.set_slider_min(0);
                d.set_slider_max(4096);
                d.set_default(0);
            }),
        )?;

        params.add(
            Params::ProcessHeight,
            "Process Height (0=Source)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(8192);
                d.set_slider_min(0);
                d.set_slider_max(4096);
                d.set_default(0);
            }),
        )?;

        params.add(
            Params::Offset,
            "Offset",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-32.0);
                d.set_valid_max(32.0);
                d.set_slider_min(-2.0);
                d.set_slider_max(2.0);
                d.set_default(0.5);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::Scale,
            "Scale",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-1024.0);
                d.set_valid_max(1024.0);
                d.set_slider_min(-16.0);
                d.set_slider_max(16.0);
                d.set_default(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::RgbOnly,
            "RGB Only (Keep Alpha)",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::Raw32,
            "Raw Mode (32bpc only)",
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
                        "AOD_FFT - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
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
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let in_w = in_layer.width();
        let in_h = in_layer.height();
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if in_w == 0 || in_h == 0 || out_w == 0 || out_h == 0 {
            return Ok(());
        }

        let output_mode = output_mode_from_popup(params.get(Params::Output)?.as_popup()?.value());
        let process_w =
            resolve_process_dim(params.get(Params::ProcessWidth)?.as_slider()?.value(), in_w);
        let process_h = resolve_process_dim(
            params.get(Params::ProcessHeight)?.as_slider()?.value(),
            in_h,
        );
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
        let scale = params.get(Params::Scale)?.as_float_slider()?.value() as f32;
        let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value();
        let raw_32 = params.get(Params::Raw32)?.as_checkbox()?.value();

        let in_world_type = in_layer.world_type();
        let mut centered = sample_layer_rgba(&in_layer, in_world_type, process_w, process_h);
        for v in &mut centered {
            *v -= 0.5;
        }

        let (real, imag) = {
            #[cfg(feature = "gpu_wgpu")]
            {
                if let Some(ctx) = wgpu_context()
                    && let Ok(out) = ctx.forward_rgba(process_w as u32, process_h as u32, &centered)
                {
                    (out.real, out.imag)
                } else {
                    let spectrum = utils::spectral::fft2_rgba(&centered, process_w, process_h)
                        .map_err(|_| Error::BadCallbackParameter)?;
                    (spectrum.real, spectrum.imag)
                }
            }

            #[cfg(not(feature = "gpu_wgpu"))]
            {
                let spectrum = utils::spectral::fft2_rgba(&centered, process_w, process_h)
                    .map_err(|_| Error::BadCallbackParameter)?;
                (spectrum.real, spectrum.imag)
            }
        };

        let selected = match output_mode {
            OutputMode::Real => &real,
            OutputMode::Imag => &imag,
        };

        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let raw_effective = raw_32 && out_is_f32;

        let progress_final = out_h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let sx = resize_sample_coord(x, out_w, process_w);
            let sy = resize_sample_coord(y, out_h, process_h);
            let i = (sy * process_w + sx) * 4;

            let mut out_px = PixelF32 {
                red: map_output_value(selected[i], offset, scale, raw_effective),
                green: map_output_value(selected[i + 1], offset, scale, raw_effective),
                blue: map_output_value(selected[i + 2], offset, scale, raw_effective),
                alpha: map_output_value(selected[i + 3], offset, scale, raw_effective),
            };

            if rgb_only {
                let in_x = resize_sample_coord(x, out_w, in_w);
                let in_y = resize_sample_coord(y, out_h, in_h);
                let src = read_pixel_f32(&in_layer, in_world_type, in_x, in_y);
                out_px.alpha = src.alpha;
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
}

fn output_mode_from_popup(value: i32) -> OutputMode {
    match value {
        2 => OutputMode::Imag,
        _ => OutputMode::Real,
    }
}

fn resolve_process_dim(param: i32, fallback: usize) -> usize {
    if param <= 0 { fallback } else { param as usize }
}

fn resize_sample_coord(coord: usize, out_len: usize, src_len: usize) -> usize {
    if src_len <= 1 || out_len <= 1 {
        0
    } else {
        ((coord * src_len) / out_len).min(src_len - 1)
    }
}

fn map_output_value(signal: f32, offset: f32, scale: f32, raw_32: bool) -> f32 {
    let base = if raw_32 { offset - 0.5 } else { offset };
    let mut v = base + signal * scale;
    if !v.is_finite() {
        v = 0.0;
    }
    if raw_32 { v } else { v.clamp(0.0, 1.0) }
}

fn sample_layer_rgba(
    layer: &Layer,
    world_type: ae::aegp::WorldType,
    out_w: usize,
    out_h: usize,
) -> Vec<f32> {
    let src_w = layer.width();
    let src_h = layer.height();
    let mut out = vec![0.0f32; out_w * out_h * 4];

    for y in 0..out_h {
        let sy = resize_sample_coord(y, out_h, src_h);
        for x in 0..out_w {
            let sx = resize_sample_coord(x, out_w, src_w);
            let p = read_pixel_f32(layer, world_type, sx, sy);
            let i = (y * out_w + x) * 4;
            out[i] = p.red;
            out[i + 1] = p.green;
            out[i + 2] = p.blue;
            out[i + 3] = p.alpha;
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
