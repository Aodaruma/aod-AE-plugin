use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    DistanceType,
    Direction,
    LpExponent,
    Width,
    Offset,
    Clamp32,
    UseOriginalAlpha,
    AlphaThreshold,
    LabelTolerance,
}

#[derive(Clone, Copy, Debug)]
enum DistanceType {
    L1,
    L2,
    Linf,
    Lp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Inner,
    Outer,
    Both,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "A plugin that can generate distance images from the contours of colored regions.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // param definitions here
        params.add(
            Params::DistanceType,
            "Distance Type",
            PopupDef::setup(|d| {
                d.set_options(&["L1 (Manhattan)", "L2 (Euclidean)", "Linf (Chebyshev)", "Lp"]);
                d.set_default(2); // L2
            }),
        )?;

        params.add(
            Params::Direction,
            "Direction",
            PopupDef::setup(|d| {
                d.set_options(&["Inner", "Outer", "Both (Signed)"]);
                d.set_default(3); // Both
            }),
        )?;

        params.add(
            Params::LpExponent,
            "Lp Exponent",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.1);
                d.set_valid_max(16.0);
                d.set_slider_min(0.5);
                d.set_slider_max(8.0);
                d.set_default(2.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Width,
            "Gradient Width (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(10000.0);
                d.set_slider_min(1.0);
                d.set_slider_max(256.0);
                d.set_default(32.0);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::Offset,
            "Offset",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-1000.0);
                d.set_valid_max(1000.0);
                d.set_slider_min(-1.0);
                d.set_slider_max(1.0);
                d.set_default(0.0);
                d.set_precision(4);
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
            Params::LabelTolerance,
            "Label Tolerance",
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
                out_data.set_return_msg(format!(
                    "AOD_DistanceGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                    version=env!("CARGO_PKG_VERSION"),
                    build_year=env!("BUILD_YEAR")
                ).as_str());
            }
            ae::Command::GlobalSetup => {
                // Declare that we do or do not support smart rendering
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

                if in_layer_opt.is_some() && out_layer_opt.is_some() {
                    self.do_render(
                        in_data,
                        in_layer_opt.unwrap(),
                        out_data,
                        out_layer_opt.unwrap(),
                        params,
                    )?;
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
        _in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let w = in_layer.width();
        let h = in_layer.height();
        let n = w * h;
        let progress_final = out_layer.height() as i32;
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        // --- read params ---
        let distance_type = match params.get(Params::DistanceType)?.as_popup()?.value() {
            1 => DistanceType::L1,
            2 => DistanceType::L2,
            3 => DistanceType::Linf,
            _ => DistanceType::Lp,
        };

        let direction = match params.get(Params::Direction)?.as_popup()?.value() {
            1 => Direction::Inner,
            2 => Direction::Outer,
            _ => Direction::Both,
        };

        let lp_exp = params.get(Params::LpExponent)?.as_float_slider()?.value() as f32;
        let lp_exp = lp_exp.max(0.1);

        let width = params.get(Params::Width)?.as_float_slider()?.value() as f32;
        let width = width.max(1.0e-6);
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
        let clamp_32 = params.get(Params::Clamp32)?.as_checkbox()?.value();
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();

        let alpha_thr = params
            .get(Params::AlphaThreshold)?
            .as_float_slider()?
            .value() as f32;
        let label_tol = params
            .get(Params::LabelTolerance)?
            .as_float_slider()?
            .value() as f32;

        // --- pass 1: build labels from input (color-coded regions) ---
        // label = 0 => background
        // label != 0 => packed RGB 0xRRGGBB (8-bit quantized)
        let in_world_type = in_layer.world_type();
        let mut label: Vec<u32> = vec![0; n];
        let mut alpha_map: Vec<f32> = vec![1.0; n];
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let px = read_pixel_f32(&in_layer, in_world_type, x, y);
                label[idx] = pack_label(px, alpha_thr, label_tol);
                alpha_map[idx] = px.alpha;
            }
        }

        // --- compute boundary mask ---
        let mut boundary: Vec<bool> = vec![false; n];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let lbl = label[i];
                if lbl == 0 {
                    let mut is_boundary = false;
                    if x > 0 && label[i - 1] != 0 {
                        is_boundary = true;
                    } else if x + 1 < w && label[i + 1] != 0 {
                        is_boundary = true;
                    } else if y > 0 && label[i - w] != 0 {
                        is_boundary = true;
                    } else if y + 1 < h && label[i + w] != 0 {
                        is_boundary = true;
                    }
                    boundary[i] = is_boundary;
                    continue;
                }

                if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                    boundary[i] = true;
                    continue;
                }

                let l = label[i - 1];
                let r = label[i + 1];
                let u = label[i - w];
                let d = label[i + w];
                boundary[i] = (l != lbl) || (r != lbl) || (u != lbl) || (d != lbl);
            }
        }

        // --- distance transform (chamfer / grid metric) ---
        let (use_diag, w_ortho, w_diag) = match distance_type {
            DistanceType::L1 => (false, 1.0, 2.0),
            DistanceType::L2 => (true, 1.0, 2.0_f32.sqrt()),
            DistanceType::Linf => (true, 1.0, 1.0),
            DistanceType::Lp => {
                let diag = 2.0_f32.powf(1.0 / lp_exp.max(0.1));
                (true, 1.0, diag)
            }
        };

        let inf = 1.0e20_f32;
        let mut dist: Vec<f32> = vec![inf; n];
        for i in 0..n {
            if boundary[i] {
                dist[i] = 0.0;
            }
        }

        // forward pass
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let lbl = label[i];
                let mut best = dist[i];

                if x > 0 {
                    let j = i - 1;
                    if label[j] == lbl {
                        best = best.min(dist[j] + w_ortho);
                    }
                }
                if y > 0 {
                    let j = i - w;
                    if label[j] == lbl {
                        best = best.min(dist[j] + w_ortho);
                    }
                    if use_diag {
                        if x > 0 {
                            let k = i - w - 1;
                            if label[k] == lbl {
                                best = best.min(dist[k] + w_diag);
                            }
                        }
                        if x + 1 < w {
                            let k = i - w + 1;
                            if label[k] == lbl {
                                best = best.min(dist[k] + w_diag);
                            }
                        }
                    }
                }

                dist[i] = best;
            }
        }

        // backward pass
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let i = y * w + x;
                let lbl = label[i];
                let mut best = dist[i];

                if x + 1 < w {
                    let j = i + 1;
                    if label[j] == lbl {
                        best = best.min(dist[j] + w_ortho);
                    }
                }
                if y + 1 < h {
                    let j = i + w;
                    if label[j] == lbl {
                        best = best.min(dist[j] + w_ortho);
                    }
                    if use_diag {
                        if x + 1 < w {
                            let k = i + w + 1;
                            if label[k] == lbl {
                                best = best.min(dist[k] + w_diag);
                            }
                        }
                        if x > 0 {
                            let k = i + w - 1;
                            if label[k] == lbl {
                                best = best.min(dist[k] + w_diag);
                            }
                        }
                    }
                }

                dist[i] = best;
            }
        }

        // --- write distance to output ---
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let i = y * w + x;

            let mut d = dist[i];
            if !d.is_finite() || d >= inf * 0.5 {
                d = 0.0;
            }

            let mut v = match direction {
                Direction::Inner => {
                    if label[i] == 0 {
                        0.0
                    } else {
                        d
                    }
                }
                Direction::Outer => {
                    if label[i] == 0 {
                        d
                    } else {
                        0.0
                    }
                }
                Direction::Both => {
                    if label[i] == 0 {
                        d
                    } else {
                        -d
                    }
                }
            };

            v = v / width + offset;
            if !v.is_finite() {
                v = 0.0;
            }

            if out_is_f32 {
                if clamp_32 {
                    v = v.clamp(0.0, 1.0);
                }
            } else {
                v = v.clamp(0.0, 1.0);
            }

            let mut out_alpha = 1.0;
            let mut out_v = v;
            if use_original_alpha {
                out_alpha = alpha_map[i];
                if !out_alpha.is_finite() {
                    out_alpha = 0.0;
                }
                out_alpha = out_alpha.clamp(0.0, 1.0);
                out_v *= out_alpha;
            }

            let out_px = PixelF32 {
                alpha: out_alpha,
                red: out_v,
                green: out_v,
                blue: out_v,
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

// --- pixel helpers ---
fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn pack_label(px: PixelF32, alpha_thr: f32, tol: f32) -> u32 {
    if px.alpha < alpha_thr {
        return 0;
    }
    let scale = ae::MAX_CHANNEL8 as f32;
    let tol = tol.clamp(0.0, 1.0);
    let step = (tol * scale).round().max(1.0) as i32;

    let quant = |v: f32| -> u32 {
        let raw = (v.clamp(0.0, 1.0) * scale + 0.5) as i32;
        if step <= 1 {
            return raw as u32;
        }
        let snapped = ((raw + step / 2) / step) * step;
        snapped.clamp(0, ae::MAX_CHANNEL8 as i32) as u32
    };

    let r = quant(px.red);
    let g = quant(px.green);
    let b = quant(px.blue);
    (r << 16) | (g << 8) | b
}
