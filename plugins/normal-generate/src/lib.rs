#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Method,
    NormalStrength,
    Invert,
    FlipY,
    UseOriginalAlpha,

    AlphaThreshold,
    LabelTolerance,
    BoundaryCondition,
    EdgeSoftness,

    // --- SDF ---
    SdfRadius,
    SdfExponent,

    // --- Poisson (Divergence) ---
    PoisIters,
    PoisDivergence,
    PoisScreened,
    PoisEdgeFeather,

    // Group markers (AE requires start/end ids for a group)
    AdvancedStart,
    GeneralStart,
    GeneralEnd,
    SdfGroupStart,
    SdfGroupEnd,
    PoissonGroupStart,
    PoissonGroupEnd,
    AdvancedEnd,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "A plugin that can generate normals from color-coded regions.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::Method,
            "Method",
            PopupDef::setup(|d| {
                d.set_options(&["SDF (fast)", "Divergence / Poisson (smooth)"]);
                d.set_default(2); // 1-based
            }),
        )?;

        params.add(
            Params::NormalStrength,
            "Normal Strength",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(50.0);
                d.set_slider_min(0.0);
                d.set_slider_max(10.0);
                d.set_default(10.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Invert,
            "Invert",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::FlipY,
            "Flip Y (DirectX)",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add_group(
            Params::AdvancedStart,
            Params::AdvancedEnd,
            "Advanced",
            true,
            |params| {
                params.add_group(
                    Params::GeneralStart,
                    Params::GeneralEnd,
                    "General",
                    false,
                    |params| {
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
                            Params::BoundaryCondition,
                            "Boundary Condition",
                            PopupDef::setup(|d| {
                                d.set_options(&[
                                    "Height = 0 (Dirichlet)",
                                    "Normal continuity (Neumann)",
                                ]);
                                d.set_default(2); // 1-based
                            }),
                        )?;

                        params.add(
                            Params::EdgeSoftness,
                            "Edge Softness (px)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(1024.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(128.0);
                                d.set_default(1.0);
                                d.set_precision(1);
                            }),
                        )?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::SdfGroupStart,
                    Params::SdfGroupEnd,
                    "SDF",
                    false,
                    |params| {
                        params.add(
                            Params::SdfRadius,
                            "SDF Radius (px)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(4096.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(256.0);
                                d.set_default(32.0);
                                d.set_precision(1);
                            }),
                        )?;

                        params.add(
                            Params::SdfExponent,
                            "SDF Exponent",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.05);
                                d.set_valid_max(16.0);
                                d.set_slider_min(0.1);
                                d.set_slider_max(8.0);
                                d.set_default(2.0);
                                d.set_precision(2);
                            }),
                        )?;

                        Ok(())
                    },
                )?;

                params.add_group(
                    Params::PoissonGroupStart,
                    Params::PoissonGroupEnd,
                    "Divergence / Poisson",
                    false,
                    |params| {
                        params.add(
                            Params::PoisIters,
                            "Poisson Iters",
                            SliderDef::setup(|d| {
                                d.set_valid_min(1);
                                d.set_valid_max(2000);
                                d.set_slider_min(1);
                                d.set_slider_max(300);
                                d.set_default(200);
                            }),
                        )?;

                        params.add(
                            Params::PoisDivergence,
                            "Divergence",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(50.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(10.0);
                                d.set_default(1.5);
                                d.set_precision(3);
                            }),
                        )?;

                        params.add(
                            Params::PoisScreened,
                            "Poisson Damping (Screened)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(4.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(2.0);
                                d.set_default(0.02);
                                d.set_precision(3);
                            }),
                        )?;

                        params.add(
                            Params::PoisEdgeFeather,
                            "Edge Feather (px)",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(1024.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(128.0);
                                d.set_default(1.0);
                                d.set_precision(1);
                            }),
                        )?;

                        Ok(())
                    },
                )?;

                Ok(())
            },
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
                        "AOD_NormalGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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

                // self.do_render(in_data, in_layer_opt, out_data, out_layer_opt, params)?;

                cb.checkin_layer_pixels(0)?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Method {
    Sdf,
    Poisson,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoundaryMode {
    Dirichlet,
    Neumann,
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

        let progress_final = h as i32;
        let out_world_type = out_layer.world_type();

        // --- read params (current time) ---
        let method_v = params.get(Params::Method)?.as_popup()?.value();
        let method = match method_v {
            2 => Method::Poisson,
            _ => Method::Sdf,
        };

        let normal_strength = params
            .get(Params::NormalStrength)?
            .as_float_slider()?
            .value() as f32;
        let invert = params.get(Params::Invert)?.as_checkbox()?.value();
        let flip_y = params.get(Params::FlipY)?.as_checkbox()?.value();
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();

        let alpha_thr = params
            .get(Params::AlphaThreshold)?
            .as_float_slider()?
            .value() as f32;
        let label_tol = params
            .get(Params::LabelTolerance)?
            .as_float_slider()?
            .value() as f32;
        let boundary_mode = match params.get(Params::BoundaryCondition)?.as_popup()?.value() {
            2 => BoundaryMode::Neumann,
            _ => BoundaryMode::Dirichlet,
        };
        let edge_softness = params.get(Params::EdgeSoftness)?.as_float_slider()?.value() as f32;

        let sdf_radius = params.get(Params::SdfRadius)?.as_float_slider()?.value() as f32;
        let sdf_exp = params.get(Params::SdfExponent)?.as_float_slider()?.value() as f32;

        let pois_iters = params
            .get(Params::PoisIters)?
            .as_slider()?
            .value()
            .clamp(1, 2000) as usize;
        let pois_div = params
            .get(Params::PoisDivergence)?
            .as_float_slider()?
            .value() as f32;
        let pois_screened = params.get(Params::PoisScreened)?.as_float_slider()?.value() as f32;
        let pois_feather = params
            .get(Params::PoisEdgeFeather)?
            .as_float_slider()?
            .value() as f32;

        let sign = if invert { -1.0 } else { 1.0 };

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
                    boundary[i] = false;
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

        // --- distance-to-boundary (chamfer) for SDF + Poisson edge-feathering ---
        // dist unit: chamfer(3-4). convert to pixels by /3
        let mut dist: Vec<i32> = vec![i32::MAX / 4; n];
        for i in 0..n {
            if label[i] != 0 && boundary[i] {
                dist[i] = 0;
            }
        }

        // forward pass
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let lbl = label[i];
                if lbl == 0 || dist[i] == 0 {
                    continue;
                }

                let mut best = dist[i];

                // left
                if x > 0 {
                    let j = i - 1;
                    if label[j] == lbl {
                        best = best.min(dist[j] + 3);
                    }
                }
                // up
                if y > 0 {
                    let j = i - w;
                    if label[j] == lbl {
                        best = best.min(dist[j] + 3);
                    }
                    // up-left
                    if x > 0 {
                        let k = i - w - 1;
                        if label[k] == lbl {
                            best = best.min(dist[k] + 4);
                        }
                    }
                    // up-right
                    if x + 1 < w {
                        let k = i - w + 1;
                        if label[k] == lbl {
                            best = best.min(dist[k] + 4);
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
                if lbl == 0 || dist[i] == 0 {
                    continue;
                }

                let mut best = dist[i];

                // right
                if x + 1 < w {
                    let j = i + 1;
                    if label[j] == lbl {
                        best = best.min(dist[j] + 3);
                    }
                }
                // down
                if y + 1 < h {
                    let j = i + w;
                    if label[j] == lbl {
                        best = best.min(dist[j] + 3);
                    }
                    // down-right
                    if x + 1 < w {
                        let k = i + w + 1;
                        if label[k] == lbl {
                            best = best.min(dist[k] + 4);
                        }
                    }
                    // down-left
                    if x > 0 {
                        let k = i + w - 1;
                        if label[k] == lbl {
                            best = best.min(dist[k] + 4);
                        }
                    }
                }

                dist[i] = best;
            }
        }

        // --- build height field ---
        let mut height: Vec<f32> = vec![0.0; n];

        match method {
            Method::Sdf => {
                let radius = sdf_radius.max(0.0001);
                let exp = sdf_exp.max(0.0001);

                for i in 0..n {
                    let lbl = label[i];
                    if lbl == 0 {
                        height[i] = 0.0;
                        continue;
                    }
                    // pixels
                    let dpx = (dist[i] as f32) / 3.0;
                    let t = (1.0 - (dpx / radius)).clamp(0.0, 1.0);
                    // emphasize center vs edge
                    let t = t.powf(exp);
                    height[i] = sign * t;
                }
            }

            Method::Poisson => {
                // Build b (rhs). For "bulge positive", use b negative (discrete Poisson sign convention).
                let mut b: Vec<f32> = vec![0.0; n];

                let feather = pois_feather.max(0.0);
                for i in 0..n {
                    if label[i] == 0 {
                        b[i] = 0.0;
                        continue;
                    }
                    if boundary_mode == BoundaryMode::Dirichlet && boundary[i] {
                        b[i] = 0.0;
                        continue;
                    }

                    // edge feather weight from distance-to-boundary
                    let wgt = if feather > 0.0 {
                        let dpx = (dist[i] as f32) / 3.0;
                        smoothstep(0.0, feather, dpx)
                    } else {
                        1.0
                    };

                    b[i] = -sign * pois_div * wgt;
                }

                // Red-Black Gauss-Seidel + SOR (faster convergence than Jacobi)
                let mut h0: Vec<f32> = vec![0.0; n];
                let omega = sor_omega(w, h);
                let lambda2 = pois_screened.max(0.0).powi(2);
                let eps = 1.0e-4;

                for _ in 0..pois_iters {
                    let mut max_delta = 0.0;

                    for pass in 0..2 {
                        for y in 0..h {
                            for x in 0..w {
                                if ((x ^ y) & 1) != pass {
                                    continue;
                                }

                                let i = y * w + x;
                                let lbl = label[i];

                                if lbl == 0 {
                                    continue;
                                }
                                if boundary_mode == BoundaryMode::Dirichlet && boundary[i] {
                                    continue;
                                }

                                let mut sum = 0.0;
                                let mut missing = 0;

                                // neighbor helper: add h0 if same label, else treat as Neumann if enabled
                                if x > 0 {
                                    let j = i - 1;
                                    if label[j] == lbl {
                                        sum += h0[j];
                                    } else if boundary_mode == BoundaryMode::Neumann {
                                        missing += 1;
                                    }
                                } else if boundary_mode == BoundaryMode::Neumann {
                                    missing += 1;
                                }
                                if x + 1 < w {
                                    let j = i + 1;
                                    if label[j] == lbl {
                                        sum += h0[j];
                                    } else if boundary_mode == BoundaryMode::Neumann {
                                        missing += 1;
                                    }
                                } else if boundary_mode == BoundaryMode::Neumann {
                                    missing += 1;
                                }
                                if y > 0 {
                                    let j = i - w;
                                    if label[j] == lbl {
                                        sum += h0[j];
                                    } else if boundary_mode == BoundaryMode::Neumann {
                                        missing += 1;
                                    }
                                } else if boundary_mode == BoundaryMode::Neumann {
                                    missing += 1;
                                }
                                if y + 1 < h {
                                    let j = i + w;
                                    if label[j] == lbl {
                                        sum += h0[j];
                                    } else if boundary_mode == BoundaryMode::Neumann {
                                        missing += 1;
                                    }
                                } else if boundary_mode == BoundaryMode::Neumann {
                                    missing += 1;
                                }

                                let denom = if boundary_mode == BoundaryMode::Neumann {
                                    let d = 4 - missing;
                                    if d <= 0 {
                                        continue;
                                    }
                                    d as f32
                                } else {
                                    4.0
                                };
                                let new_val = (sum - b[i]) / (denom + lambda2);
                                let old = h0[i];
                                let updated = old + omega * (new_val - old);
                                h0[i] = updated;
                                let delta = (updated - old).abs();
                                if delta > max_delta {
                                    max_delta = delta;
                                }
                            }
                        }
                    }

                    if max_delta < eps {
                        break;
                    }
                }

                height = h0;
            }
        }

        // --- pass 2: write normals to output ---
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let x = x as usize;
            let y = y as usize;
            let i = y * w + x;
            /* */
            let mut out_px = if label[i] == 0 {
                // flat normal
                PixelF32 {
                    alpha: 1.0,
                    red: 0.5,
                    green: 0.5,
                    blue: 1.0,
                }
            } else {
                let lbl = label[i];
                let h_c = height[i];
                let edge_wgt = if edge_softness > 0.0 {
                    let dpx = (dist[i] as f32) / 3.0;
                    smoothstep(0.0, edge_softness, dpx)
                } else {
                    1.0
                };
                let boundary_height = if boundary_mode == BoundaryMode::Neumann {
                    h_c
                } else {
                    0.0
                };

                // sample height with region boundary handling
                let h_l = if x > 0 {
                    let j = i - 1;
                    if label[j] == lbl {
                        height[j]
                    } else {
                        boundary_height
                    }
                } else {
                    boundary_height
                };

                let h_r = if x + 1 < w {
                    let j = i + 1;
                    if label[j] == lbl {
                        height[j]
                    } else {
                        boundary_height
                    }
                } else {
                    boundary_height
                };

                let h_u = if y > 0 {
                    let j = i - w;
                    if label[j] == lbl {
                        height[j]
                    } else {
                        boundary_height
                    }
                } else {
                    boundary_height
                };

                let h_d = if y + 1 < h {
                    let j = i + w;
                    if label[j] == lbl {
                        height[j]
                    } else {
                        boundary_height
                    }
                } else {
                    boundary_height
                };

                let dhdx = 0.5 * (h_r - h_l) * edge_wgt;
                let dhdy = 0.5 * (h_d - h_u) * edge_wgt;

                let nx = -dhdx * normal_strength;
                let mut ny = -dhdy * normal_strength;
                if flip_y {
                    ny = -ny;
                }
                let nz = 1.0;

                let (nx, ny, nz) = normalize3(nx, ny, nz);

                PixelF32 {
                    alpha: 1.0,
                    red: 0.5 * nx + 0.5,
                    green: 0.5 * ny + 0.5,
                    blue: 0.5 * nz + 0.5,
                }
            };

            if use_original_alpha {
                let mut out_alpha = alpha_map[i];
                if !out_alpha.is_finite() {
                    out_alpha = 0.0;
                }
                out_alpha = out_alpha.clamp(0.0, 1.0);
                out_px.red *= out_alpha;
                out_px.green *= out_alpha;
                out_px.blue *= out_alpha;
                out_px.alpha = out_alpha;
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

// --- math helpers ---
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn normalize3(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let len2 = x * x + y * y + z * z;
    if len2 <= 1.0e-20 {
        return (0.0, 0.0, 1.0);
    }
    let inv = len2.sqrt().recip();
    (x * inv, y * inv, z * inv)
}

fn sor_omega(w: usize, h: usize) -> f32 {
    let n = w.max(h) as f32;
    if n <= 1.0 {
        return 1.0;
    }
    let omega = 2.0 / (1.0 + (std::f32::consts::PI / n).sin());
    omega.clamp(1.0, 1.95)
}
