#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::collections::VecDeque;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MainColor,
    Tolerance,
    AlphaThreshold,
    Connectivity,
    MaxIterations,
    EdgeErosionPx,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Connectivity {
    Four,
    Eight,
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    main_color: PixelF32,
    tolerance: f32,
    alpha_threshold: f32,
    connectivity: Connectivity,
    max_iterations: Option<u32>,
    edge_erosion_px: u32,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Repaints line-colored pixels by propagating neighboring colors.";
const NEIGHBORS_4: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const NEIGHBORS_8: [(isize, isize); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::MainColor,
            "Line Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: ae::MAX_CHANNEL8 as u8,
                });
            }),
        )?;

        params.add(
            Params::Tolerance,
            "Tolerance (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(20.0);
                d.set_default(0.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::AlphaThreshold,
            "Alpha Threshold",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(0.2);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Connectivity,
            "Connectivity",
            PopupDef::setup(|d| {
                d.set_options(&["4-neighbor", "8-neighbor"]);
                d.set_default(2);
            }),
        )?;

        params.add(
            Params::MaxIterations,
            "Max Iterations (0=Auto)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(512.0);
                d.set_default(0.0);
                d.set_precision(0);
            }),
        )?;

        params.add(
            Params::EdgeErosionPx,
            "Edge Erosion (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100000.0);
                d.set_slider_min(0.0);
                d.set_slider_max(64.0);
                d.set_default(0.0);
                d.set_precision(0);
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
                        "AOD_LineRepaint - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
        let width = in_layer.width();
        let height = in_layer.height();
        let n = width * height;
        if width == 0 || height == 0 {
            return Ok(());
        }

        let settings = read_settings(params)?;
        let neighbors: &[(isize, isize)] = match settings.connectivity {
            Connectivity::Four => &NEIGHBORS_4,
            Connectivity::Eight => &NEIGHBORS_8,
        };

        let in_world_type = in_layer.world_type();
        let mut src: Vec<PixelF32> = vec![
            PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            };
            n
        ];
        let mut line_mask: Vec<bool> = vec![false; n];
        let target_rgb = straight_rgb(settings.main_color);

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let px = read_pixel_f32(&in_layer, in_world_type, x, y);
                src[idx] = px;
                line_mask[idx] =
                    is_line_color(px, target_rgb, settings.tolerance, settings.alpha_threshold);
            }
        }

        if !line_mask.iter().any(|&v| v) {
            return write_output(&mut out_layer, &src);
        }

        let invalid = usize::MAX;
        let mut owner: Vec<usize> = vec![invalid; n];
        let mut dist: Vec<u32> = vec![u32::MAX; n];
        let mut queue: VecDeque<usize> = VecDeque::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if line_mask[idx] {
                    continue;
                }
                if src[idx].alpha <= settings.alpha_threshold {
                    continue;
                }
                if has_line_neighbor(x, y, width, height, &line_mask, neighbors) {
                    owner[idx] = idx;
                    dist[idx] = 0;
                    queue.push_back(idx);
                }
            }
        }

        while let Some(idx) = queue.pop_front() {
            let d = dist[idx];
            if let Some(limit) = settings.max_iterations
                && d >= limit
            {
                continue;
            }

            let x = idx % width;
            let y = idx / width;
            let next_d = d.saturating_add(1);

            for (dx, dy) in neighbors.iter().copied() {
                let Some(next_idx) = offset_index(x, y, width, height, dx, dy) else {
                    continue;
                };
                if !line_mask[next_idx] {
                    continue;
                }

                if next_d < dist[next_idx] {
                    dist[next_idx] = next_d;
                    owner[next_idx] = owner[idx];
                    queue.push_back(next_idx);
                } else if next_d == dist[next_idx] {
                    let cand_owner = owner[idx];
                    let cur_owner = owner[next_idx];
                    if cand_owner != invalid
                        && (cur_owner == invalid
                            || luminance(src[cand_owner]) < luminance(src[cur_owner]))
                    {
                        owner[next_idx] = cand_owner;
                    }
                }
            }
        }

        let mut out = src.clone();
        let mut repainted_mask: Vec<bool> = vec![false; n];
        for idx in 0..n {
            if !line_mask[idx] {
                continue;
            }

            let seed_idx = owner[idx];
            if seed_idx == invalid {
                continue;
            }

            out[idx].red = src[seed_idx].red;
            out[idx].green = src[seed_idx].green;
            out[idx].blue = src[seed_idx].blue;
            repainted_mask[idx] = true;
        }

        apply_edge_erosion(
            &mut out,
            &src,
            &repainted_mask,
            width,
            height,
            neighbors,
            settings.alpha_threshold,
            settings.edge_erosion_px,
        );

        write_output(&mut out_layer, &out)
    }
}

fn read_settings(params: &mut Parameters<Params>) -> Result<Settings, Error> {
    let main_color = params
        .get(Params::MainColor)?
        .as_color()?
        .value()
        .to_pixel32();

    let tolerance_percent = params.get(Params::Tolerance)?.as_float_slider()?.value() as f32;
    let tolerance = (tolerance_percent / 100.0).clamp(0.0, 1.0);

    let alpha_threshold = params
        .get(Params::AlphaThreshold)?
        .as_float_slider()?
        .value() as f32;
    let alpha_threshold = alpha_threshold.clamp(0.0, 1.0);

    let connectivity = match params.get(Params::Connectivity)?.as_popup()?.value() {
        1 => Connectivity::Four,
        _ => Connectivity::Eight,
    };

    let max_iterations_val = params
        .get(Params::MaxIterations)?
        .as_float_slider()?
        .value()
        .round();
    let max_iterations = if max_iterations_val <= 0.0 {
        None
    } else {
        Some(max_iterations_val.min(u32::MAX as f64) as u32)
    };

    let edge_erosion_px_val = params
        .get(Params::EdgeErosionPx)?
        .as_float_slider()?
        .value()
        .round();
    let edge_erosion_px = if edge_erosion_px_val <= 0.0 {
        0
    } else {
        edge_erosion_px_val.min(u32::MAX as f64) as u32
    };

    Ok(Settings {
        main_color,
        tolerance,
        alpha_threshold,
        connectivity,
        max_iterations,
        edge_erosion_px,
    })
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn write_output(out_layer: &mut Layer, buffer: &[PixelF32]) -> Result<(), Error> {
    let width = out_layer.width();
    let out_world_type = out_layer.world_type();
    let progress_final = out_layer.height() as i32;

    out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
        let idx = y as usize * width + x as usize;
        let px = buffer[idx];
        match out_world_type {
            ae::aegp::WorldType::U8 => dst.set_from_u8(px.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(px.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(px),
        }
        Ok(())
    })?;

    Ok(())
}

fn straight_rgb(px: PixelF32) -> (f32, f32, f32) {
    if px.alpha > 1.0e-6 {
        (px.red / px.alpha, px.green / px.alpha, px.blue / px.alpha)
    } else {
        (px.red, px.green, px.blue)
    }
}

fn is_line_color(
    px: PixelF32,
    target_rgb: (f32, f32, f32),
    tolerance: f32,
    alpha_threshold: f32,
) -> bool {
    if px.alpha <= alpha_threshold {
        return false;
    }
    let (r, g, b) = straight_rgb(px);
    (r - target_rgb.0).abs() <= tolerance
        && (g - target_rgb.1).abs() <= tolerance
        && (b - target_rgb.2).abs() <= tolerance
}

fn luminance(px: PixelF32) -> f32 {
    let (r, g, b) = straight_rgb(px);
    0.29891 * r + 0.58661 * g + 0.11448 * b
}

fn has_line_neighbor(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    line_mask: &[bool],
    neighbors: &[(isize, isize)],
) -> bool {
    neighbors.iter().copied().any(|(dx, dy)| {
        offset_index(x, y, width, height, dx, dy)
            .map(|idx| line_mask[idx])
            .unwrap_or(false)
    })
}

fn has_low_alpha_neighbor(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    src: &[PixelF32],
    alpha_threshold: f32,
    neighbors: &[(isize, isize)],
) -> bool {
    neighbors.iter().copied().any(|(dx, dy)| {
        offset_index(x, y, width, height, dx, dy)
            .map(|idx| src[idx].alpha <= alpha_threshold)
            .unwrap_or(false)
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_edge_erosion(
    out: &mut [PixelF32],
    src: &[PixelF32],
    repainted_mask: &[bool],
    width: usize,
    height: usize,
    neighbors: &[(isize, isize)],
    alpha_threshold: f32,
    erosion_px: u32,
) {
    if erosion_px == 0 || width == 0 || height == 0 {
        return;
    }

    let n = width * height;
    let mut dist: Vec<u32> = vec![u32::MAX; n];
    let mut queue: VecDeque<usize> = VecDeque::new();

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if !repainted_mask[idx] {
                continue;
            }
            if has_low_alpha_neighbor(x, y, width, height, src, alpha_threshold, neighbors) {
                dist[idx] = 0;
                queue.push_back(idx);
            }
        }
    }

    while let Some(idx) = queue.pop_front() {
        let d = dist[idx];
        let next_d = d.saturating_add(1);
        if next_d >= erosion_px {
            continue;
        }

        let x = idx % width;
        let y = idx / width;
        for (dx, dy) in neighbors.iter().copied() {
            let Some(next_idx) = offset_index(x, y, width, height, dx, dy) else {
                continue;
            };
            if !repainted_mask[next_idx] || next_d >= dist[next_idx] {
                continue;
            }
            dist[next_idx] = next_d;
            queue.push_back(next_idx);
        }
    }

    for idx in 0..n {
        if repainted_mask[idx] && dist[idx] < erosion_px {
            out[idx].red = 0.0;
            out[idx].green = 0.0;
            out[idx].blue = 0.0;
            out[idx].alpha = 0.0;
        }
    }
}

fn offset_index(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    dx: isize,
    dy: isize,
) -> Option<usize> {
    let nx = x as isize + dx;
    let ny = y as isize + dy;
    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
        return None;
    }
    Some(ny as usize * width + nx as usize)
}
