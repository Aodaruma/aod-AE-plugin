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

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Axis,
    Offset,
    Scale,
    OutOfRange,
    EdgeMode,
    RgbOnly,
    Raw32,
}

#[derive(Clone, Copy)]
enum AxisMode {
    X,
    Y,
    GradientLength,
}

#[derive(Clone, Copy)]
enum OutMode {
    Clamp,
    SoftClamp,
    Mirror,
    Wrap,
    PassThrough,
}

#[derive(Clone, Copy)]
enum EdgeMode {
    None,
    Repeat,
    Tile,
    Mirror,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Generates RGBA differential maps from image gradients.";

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
            Params::Axis,
            "Axis",
            PopupDef::setup(|d| {
                d.set_options(&["X", "Y", "Gradient Length"]);
                d.set_default(1);
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
                d.set_slider_min(-8.0);
                d.set_slider_max(8.0);
                d.set_default(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::OutOfRange,
            "Out Of Range",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Clamp",
                    "SoftClamp",
                    "Mirror",
                    "Wrap",
                    "Pass Through (32bpc, 8/16bpc Clamp)",
                ]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::EdgeMode,
            "Edge Mode",
            PopupDef::setup(|d| {
                d.set_options(&["None (Zero)", "Repeat", "Tile", "Mirror"]);
                d.set_default(2);
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
                        "AOD_DifferentialGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                #[cfg(feature = "gpu_wgpu")]
                {
                    let mut out_layer = out_layer;
                    if let Some(ctx) = wgpu_context()
                        && self
                            .do_render_wgpu(&in_layer, &mut out_layer, params, &ctx)
                            .is_ok()
                    {
                        return Ok(());
                    }

                    self.do_render(in_data, in_layer, out_data, out_layer, params)?;
                }

                #[cfg(not(feature = "gpu_wgpu"))]
                {
                    self.do_render(in_data, in_layer, out_data, out_layer, params)?;
                }
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
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
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

        let axis = axis_from_popup(params.get(Params::Axis)?.as_popup()?.value());
        let out_mode = out_mode_from_popup(params.get(Params::OutOfRange)?.as_popup()?.value());
        let edge_mode = edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value());
        let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value();
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
        let scale = params.get(Params::Scale)?.as_float_slider()?.value() as f32;
        let raw_32 = params.get(Params::Raw32)?.as_checkbox()?.value();

        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        let in_world_type = in_layer.world_type();
        let mut input = vec![0.0f32; out_w * out_h * 4];
        for y in 0..out_h {
            for x in 0..out_w {
                let i = (y * out_w + x) * 4;
                let px = read_pixel_f32(in_layer, in_world_type, x, y);
                input[i] = px.red;
                input[i + 1] = px.green;
                input[i + 2] = px.blue;
                input[i + 3] = px.alpha;
            }
        }

        let render_params = WgpuRenderParams {
            out_w: out_w as u32,
            out_h: out_h as u32,
            axis: match axis {
                AxisMode::X => 0,
                AxisMode::Y => 1,
                AxisMode::GradientLength => 2,
            },
            edge_mode: match edge_mode {
                EdgeMode::None => 0,
                EdgeMode::Repeat => 1,
                EdgeMode::Tile => 2,
                EdgeMode::Mirror => 3,
            },
            out_mode: match out_mode {
                OutMode::Clamp => 0,
                OutMode::SoftClamp => 1,
                OutMode::Mirror => 2,
                OutMode::Wrap => 3,
                OutMode::PassThrough => 4,
            },
            raw_32: raw_32 && out_is_f32,
            rgb_only,
            offset,
            scale,
        };

        let output = ctx.render(&render_params, &input)?;

        let progress_final = out_h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let idx = ((y as usize) * out_w + x as usize) * 4;
            let out_px = PixelF32 {
                red: output.data[idx],
                green: output.data[idx + 1],
                blue: output.data[idx + 2],
                alpha: output.data[idx + 3],
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

    fn do_render(
        &self,
        _in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let w = in_layer.width();
        let h = in_layer.height();
        if w == 0 || h == 0 {
            return Ok(());
        }

        let axis = axis_from_popup(params.get(Params::Axis)?.as_popup()?.value());
        let out_mode = out_mode_from_popup(params.get(Params::OutOfRange)?.as_popup()?.value());
        let edge_mode = edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value());
        let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value();
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
        let scale = params.get(Params::Scale)?.as_float_slider()?.value() as f32;
        let raw_32 = params.get(Params::Raw32)?.as_checkbox()?.value();

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        let raw_effective = raw_32 && out_is_f32;

        let progress_final = h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let center = sample_pixel_f32(&in_layer, in_world_type, x, y, w, h, edge_mode);
            let left = sample_pixel_f32(&in_layer, in_world_type, x - 1, y, w, h, edge_mode);
            let right = sample_pixel_f32(&in_layer, in_world_type, x + 1, y, w, h, edge_mode);
            let up = sample_pixel_f32(&in_layer, in_world_type, x, y - 1, w, h, edge_mode);
            let down = sample_pixel_f32(&in_layer, in_world_type, x, y + 1, w, h, edge_mode);

            let dx = diff_half(right, left);
            let dy = diff_half(down, up);

            let diff = match axis {
                AxisMode::X => dx,
                AxisMode::Y => dy,
                AxisMode::GradientLength => PixelF32 {
                    red: (dx.red * dx.red + dy.red * dy.red).sqrt(),
                    green: (dx.green * dx.green + dy.green * dy.green).sqrt(),
                    blue: (dx.blue * dx.blue + dy.blue * dy.blue).sqrt(),
                    alpha: (dx.alpha * dx.alpha + dy.alpha * dy.alpha).sqrt(),
                },
            };

            let out_px = PixelF32 {
                red: map_diff_value(diff.red, offset, scale, out_mode, raw_effective),
                green: map_diff_value(diff.green, offset, scale, out_mode, raw_effective),
                blue: map_diff_value(diff.blue, offset, scale, out_mode, raw_effective),
                alpha: if rgb_only {
                    center.alpha
                } else {
                    map_diff_value(diff.alpha, offset, scale, out_mode, raw_effective)
                },
            };

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

fn axis_from_popup(value: i32) -> AxisMode {
    match value {
        2 => AxisMode::Y,
        3 => AxisMode::GradientLength,
        _ => AxisMode::X,
    }
}

fn out_mode_from_popup(value: i32) -> OutMode {
    match value {
        2 => OutMode::SoftClamp,
        3 => OutMode::Mirror,
        4 => OutMode::Wrap,
        5 => OutMode::PassThrough,
        _ => OutMode::Clamp,
    }
}

fn edge_mode_from_popup(value: i32) -> EdgeMode {
    match value {
        1 => EdgeMode::None,
        3 => EdgeMode::Tile,
        4 => EdgeMode::Mirror,
        _ => EdgeMode::Repeat,
    }
}

fn map_diff_value(diff: f32, offset: f32, scale: f32, mode: OutMode, raw_32: bool) -> f32 {
    let base = if raw_32 { offset - 0.5 } else { offset };
    let mut v = base + diff * scale;
    if !v.is_finite() {
        v = 0.0;
    }

    match mode {
        OutMode::Clamp => v.clamp(0.0, 1.0),
        OutMode::SoftClamp => soft_clamp01(v),
        OutMode::Mirror => mirror01(v),
        OutMode::Wrap => wrap01(v),
        OutMode::PassThrough => v,
    }
}

fn soft_clamp01(v: f32) -> f32 {
    let centered = v - 0.5;
    0.5 + 0.5 * (centered / (1.0 + centered.abs()))
}

fn wrap01(v: f32) -> f32 {
    v.rem_euclid(1.0)
}

fn mirror01(v: f32) -> f32 {
    let t = v.rem_euclid(2.0);
    if t <= 1.0 { t } else { 2.0 - t }
}

fn diff_half(a: PixelF32, b: PixelF32) -> PixelF32 {
    PixelF32 {
        red: 0.5 * (a.red - b.red),
        green: 0.5 * (a.green - b.green),
        blue: 0.5 * (a.blue - b.blue),
        alpha: 0.5 * (a.alpha - b.alpha),
    }
}

fn sample_pixel_f32(
    layer: &Layer,
    world_type: ae::aegp::WorldType,
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let xx = resolve_coord(x, w, edge_mode);
    let yy = resolve_coord(y, h, edge_mode);
    if let (Some(xx), Some(yy)) = (xx, yy) {
        read_pixel_f32(layer, world_type, xx, yy)
    } else {
        PixelF32 {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        }
    }
}

fn resolve_coord(coord: i32, len: usize, edge_mode: EdgeMode) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let len_i = len as i32;
    match edge_mode {
        EdgeMode::None => {
            if coord < 0 || coord >= len_i {
                None
            } else {
                Some(coord as usize)
            }
        }
        EdgeMode::Repeat => Some(coord.clamp(0, len_i - 1) as usize),
        EdgeMode::Tile => Some(coord.rem_euclid(len_i) as usize),
        EdgeMode::Mirror => Some(mirror_index(coord, len_i) as usize),
    }
}

fn mirror_index(coord: i32, len: i32) -> i32 {
    if len <= 1 {
        return 0;
    }
    let period = 2 * len - 2;
    let t = coord.rem_euclid(period);
    if t < len { t } else { period - t }
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}
