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
    RealLayer,
    ImagLayer,
    ProcessWidth,
    ProcessHeight,
    Offset,
    Scale,
    RgbOnly,
    Raw32,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Reconstructs an RGBA image from 2D FFT real and imaginary inputs.";

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
        params.add(Params::RealLayer, "Real Input", LayerDef::new())?;
        params.add(Params::ImagLayer, "Imaginary Input", LayerDef::new())?;

        params.add(
            Params::ProcessWidth,
            "Process Width (0=Input)",
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
            "Process Height (0=Input)",
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
                        "AOD_IFFT - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if out_w == 0 || out_h == 0 {
            return Ok(());
        }

        let real_checkout = params.checkout_at(Params::RealLayer, None, None, None)?;
        let imag_checkout = params.checkout_at(Params::ImagLayer, None, None, None)?;
        let real_layer_opt = real_checkout.as_layer()?.value();
        let imag_layer_opt = imag_checkout.as_layer()?.value();

        let Some(real_layer) = real_layer_opt else {
            return copy_layer(&in_layer, &mut out_layer);
        };
        let Some(imag_layer) = imag_layer_opt else {
            return copy_layer(&in_layer, &mut out_layer);
        };

        if real_layer.width() == 0
            || real_layer.height() == 0
            || real_layer.width() != imag_layer.width()
            || real_layer.height() != imag_layer.height()
        {
            return copy_layer(&in_layer, &mut out_layer);
        }

        let process_w = resolve_process_dim(
            params.get(Params::ProcessWidth)?.as_slider()?.value(),
            real_layer.width(),
        );
        let process_h = resolve_process_dim(
            params.get(Params::ProcessHeight)?.as_slider()?.value(),
            real_layer.height(),
        );
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
        let scale = params.get(Params::Scale)?.as_float_slider()?.value() as f32;
        let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value();
        let raw_32 = params.get(Params::Raw32)?.as_checkbox()?.value();

        let real_world = real_layer.world_type();
        let imag_world = imag_layer.world_type();
        let input_raw_effective = raw_32
            && matches!(
                real_world,
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
            )
            && matches!(
                imag_world,
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
            );

        let mut in_real = sample_layer_rgba(&real_layer, real_world, process_w, process_h);
        let mut in_imag = sample_layer_rgba(&imag_layer, imag_world, process_w, process_h);
        for i in 0..(process_w * process_h * 4) {
            in_real[i] = decode_input_value(in_real[i], offset, scale, input_raw_effective);
            in_imag[i] = decode_input_value(in_imag[i], offset, scale, input_raw_effective);
        }

        let reconstructed_centered = {
            #[cfg(feature = "gpu_wgpu")]
            {
                if let Some(ctx) = wgpu_context()
                    && let Ok(out) =
                        ctx.inverse_rgba(process_w as u32, process_h as u32, &in_real, &in_imag)
                {
                    out
                } else {
                    utils::spectral::ifft2_rgba(&in_real, &in_imag, process_w, process_h)
                        .map_err(|_| Error::BadCallbackParameter)?
                }
            }

            #[cfg(not(feature = "gpu_wgpu"))]
            {
                utils::spectral::ifft2_rgba(&in_real, &in_imag, process_w, process_h)
                    .map_err(|_| Error::BadCallbackParameter)?
            }
        };

        let out_world = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let out_raw_effective = raw_32 && out_is_f32;
        let in_world = in_layer.world_type();

        let in_w = in_layer.width();
        let in_h = in_layer.height();
        let progress_final = out_h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let sx = resize_sample_coord(x, out_w, process_w);
            let sy = resize_sample_coord(y, out_h, process_h);
            let i = (sy * process_w + sx) * 4;

            let mut out_px = PixelF32 {
                red: map_output_value(reconstructed_centered[i], offset, scale, out_raw_effective),
                green: map_output_value(
                    reconstructed_centered[i + 1],
                    offset,
                    scale,
                    out_raw_effective,
                ),
                blue: map_output_value(
                    reconstructed_centered[i + 2],
                    offset,
                    scale,
                    out_raw_effective,
                ),
                alpha: map_output_value(
                    reconstructed_centered[i + 3],
                    offset,
                    scale,
                    out_raw_effective,
                ),
            };

            if rgb_only {
                let in_x = resize_sample_coord(x, out_w, in_w);
                let in_y = resize_sample_coord(y, out_h, in_h);
                let src = read_pixel_f32(&in_layer, in_world, in_x, in_y);
                out_px.alpha = src.alpha;
            }

            match out_world {
                ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(out_px),
            }

            Ok(())
        })?;

        Ok(())
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

fn decode_input_value(v: f32, offset: f32, scale: f32, raw_32: bool) -> f32 {
    if !v.is_finite() || !scale.is_finite() || scale.abs() <= 1.0e-8 {
        return 0.0;
    }
    let base = if raw_32 { offset - 0.5 } else { offset };
    let mut out = (v - base) / scale;
    if !out.is_finite() {
        out = 0.0;
    }
    out
}

fn copy_layer(src_layer: &Layer, dst_layer: &mut Layer) -> Result<(), Error> {
    let src_world = src_layer.world_type();
    let dst_world = dst_layer.world_type();
    let out_w = dst_layer.width();
    let out_h = dst_layer.height();
    let src_w = src_layer.width();
    let src_h = src_layer.height();
    let progress_final = out_h as i32;

    dst_layer.iterate(0, progress_final, None, |x, y, mut dst| {
        let sx = resize_sample_coord(x as usize, out_w, src_w);
        let sy = resize_sample_coord(y as usize, out_h, src_h);
        let src = read_pixel_f32(src_layer, src_world, sx, sy);
        match dst_world {
            ae::aegp::WorldType::U8 => dst.set_from_u8(src.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(src.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(src),
        }
        Ok(())
    })?;

    Ok(())
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
