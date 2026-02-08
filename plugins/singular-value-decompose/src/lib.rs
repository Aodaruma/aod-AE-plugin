#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use nalgebra::DMatrix;
use std::env;

#[cfg(feature = "gpu_wgpu")]
use std::sync::{Arc, OnceLock};

use ae::pf::*;
use utils::ToPixel;

#[cfg(feature = "gpu_wgpu")]
mod gpu;
#[cfg(feature = "gpu_wgpu")]
use crate::gpu::wgpu::{WgpuContext, WgpuRenderParams};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Rank,
    ProcessWidth,
    ProcessHeight,
    RgbOnly,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Performs 2D singular value decomposition and low-rank approximation on RGBA channels.";

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
        params.add(
            Params::Rank,
            "Low Rank (k)",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(1024);
                d.set_slider_min(1);
                d.set_slider_max(256);
                d.set_default(16);
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
            Params::RgbOnly,
            "RGB Only (Keep Alpha)",
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
                        "AOD_SingularValueDecompose - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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

        let rank_requested = params.get(Params::Rank)?.as_slider()?.value().max(1) as usize;
        let process_w =
            resolve_process_dim(params.get(Params::ProcessWidth)?.as_slider()?.value(), in_w);
        let process_h = resolve_process_dim(
            params.get(Params::ProcessHeight)?.as_slider()?.value(),
            in_h,
        );
        let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value();

        let rank = rank_requested.min(process_w.min(process_h)).max(1);

        let in_world_type = in_layer.world_type();
        let sampled = sample_layer_rgba(&in_layer, in_world_type, process_w, process_h);

        let (u_data, v_data) = build_svd_factors(&sampled, process_w, process_h, rank)
            .map_err(|_| Error::BadCallbackParameter)?;

        let reconstructed = {
            #[cfg(feature = "gpu_wgpu")]
            {
                if let Some(ctx) = wgpu_context()
                    && let Ok(out) = ctx.render(
                        &WgpuRenderParams {
                            width: process_w as u32,
                            height: process_h as u32,
                            rank: rank as u32,
                        },
                        &u_data,
                        &v_data,
                    )
                {
                    out.data
                } else {
                    reconstruct_cpu(process_w, process_h, rank, &u_data, &v_data)
                }
            }

            #[cfg(not(feature = "gpu_wgpu"))]
            {
                reconstruct_cpu(process_w, process_h, rank, &u_data, &v_data)
            }
        };

        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        let progress_final = out_h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let sx = resize_sample_coord(x, out_w, process_w);
            let sy = resize_sample_coord(y, out_h, process_h);
            let i = (sy * process_w + sx) * 4;

            let mut out_px = PixelF32 {
                red: sanitize_output(reconstructed[i], !out_is_f32),
                green: sanitize_output(reconstructed[i + 1], !out_is_f32),
                blue: sanitize_output(reconstructed[i + 2], !out_is_f32),
                alpha: sanitize_output(reconstructed[i + 3], !out_is_f32),
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

fn build_svd_factors(
    sampled_rgba: &[f32],
    width: usize,
    height: usize,
    rank: usize,
) -> Result<(Vec<f32>, Vec<f32>), &'static str> {
    let expected = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(4))
        .ok_or("image size overflow")?;
    if sampled_rgba.len() != expected {
        return Err("invalid input length");
    }

    let mut u_data = vec![0.0f32; 4 * height * rank];
    let mut v_data = vec![0.0f32; 4 * rank * width];

    for ch in 0..4 {
        let mut channel = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                channel[y * width + x] = sampled_rgba[(y * width + x) * 4 + ch];
            }
        }

        let matrix = DMatrix::<f32>::from_row_slice(height, width, &channel);
        let svd = matrix.svd(true, true);
        let Some(u) = svd.u else {
            return Err("svd U unavailable");
        };
        let Some(v_t) = svd.v_t else {
            return Err("svd Vt unavailable");
        };

        for i in 0..rank {
            let sigma = svd.singular_values[i];
            for y in 0..height {
                let u_idx = ch * (height * rank) + y * rank + i;
                u_data[u_idx] = u[(y, i)];
            }
            for x in 0..width {
                let v_idx = ch * (rank * width) + i * width + x;
                v_data[v_idx] = sigma * v_t[(i, x)];
            }
        }
    }

    Ok((u_data, v_data))
}

fn reconstruct_cpu(
    width: usize,
    height: usize,
    rank: usize,
    u_data: &[f32],
    v_data: &[f32],
) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let pix = (y * width + x) * 4;
            for ch in 0..4 {
                let mut acc = 0.0f32;
                for i in 0..rank {
                    let u_idx = ch * (height * rank) + y * rank + i;
                    let v_idx = ch * (rank * width) + i * width + x;
                    acc += u_data[u_idx] * v_data[v_idx];
                }
                out[pix + ch] = acc;
            }
        }
    }

    out
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

fn sanitize_output(mut v: f32, clamp_01: bool) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    if clamp_01 {
        v = v.clamp(0.0, 1.0);
    }
    v
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
