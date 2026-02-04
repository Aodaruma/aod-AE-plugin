use after_effects as ae;
use std::env;

use ae::pf::*;
use imageproc::distance_transform::Norm;
use imageproc::image::GrayImage;
use imageproc::{edges, filter, morphology};
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    LowThreshold,
    HighThreshold,
    BlurSigma,
    LineWidth,
    ThinLines,
    LineColor,
    UseAlpha,
    Invert,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "A plugin for extracting contour lines from a layer using the Canny method.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // param definitions here
        params.add(
            Params::LowThreshold,
            "Low Threshold",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.1);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::HighThreshold,
            "High Threshold",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.3);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::BlurSigma,
            "Pre Blur Sigma",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(10.0);
                d.set_slider_min(0.0);
                d.set_slider_max(5.0);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::LineWidth,
            "Line Width (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(128.0);
                d.set_slider_min(1.0);
                d.set_slider_max(32.0);
                d.set_default(1.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::ThinLines,
            "Thin Lines (1px)",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

        params.add(
            Params::LineColor,
            "Line Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                });
            }),
        )?;

        params.add(
            Params::UseAlpha,
            "Use Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

        params.add(
            Params::Invert,
            "Invert",
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
                    "AOD_ContourGenerate - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
        let progress_final = out_layer.height() as i32;
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        // --- read params ---
        let mut t_low = params.get(Params::LowThreshold)?.as_float_slider()?.value() as f64;
        let mut t_high = params
            .get(Params::HighThreshold)?
            .as_float_slider()?
            .value() as f64;
        t_low = t_low.clamp(0.0, 1.0);
        t_high = t_high.clamp(0.0, 1.0);
        if t_low > t_high {
            std::mem::swap(&mut t_low, &mut t_high);
        }

        let blur_sigma = params.get(Params::BlurSigma)?.as_float_slider()?.value() as f64;
        let mut line_width = params.get(Params::LineWidth)?.as_float_slider()?.value();
        let thin_lines = params.get(Params::ThinLines)?.as_checkbox()?.value();
        let line_color = params.get(Params::LineColor)?.as_color()?.float_value()?;
        let use_alpha = params.get(Params::UseAlpha)?.as_checkbox()?.value();
        let invert = params.get(Params::Invert)?.as_checkbox()?.value();

        if !line_width.is_finite() {
            line_width = 1.0;
        }
        let line_width = line_width.max(1.0).round() as i32;
        let kernel_size = if line_width % 2 == 0 {
            line_width + 1
        } else {
            line_width
        };
        let dilate_k = (kernel_size / 2).min(255) as u8;

        // --- build grayscale buffer (u8) ---
        let w = width as usize;
        let h = height as usize;
        let mut gray: Vec<u8> = vec![0; w * h];
        let mut alpha_map: Vec<f32> = vec![1.0; w * h];
        let in_world_type = in_layer.world_type();

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let p = read_pixel_f32(&in_layer, in_world_type, x, y);
                let a = p.alpha;
                alpha_map[idx] = a;
                let (r, g, b) = if a > 1.0e-6 {
                    (p.red / a, p.green / a, p.blue / a)
                } else {
                    (0.0, 0.0, 0.0)
                };
                let mut luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                if use_alpha {
                    luma *= a;
                }
                if !luma.is_finite() {
                    luma = 0.0;
                }
                let v = (luma.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                gray[idx] = v;
            }
        }

        // --- imageproc Canny ---
        let base = {
            let gray_img = imageproc::image::GrayImage::from_vec(width as u32, height as u32, gray)
                .ok_or(Error::BadCallbackParameter)?;
            if blur_sigma > 0.0 {
                filter::gaussian_blur_f32(&gray_img, blur_sigma as f32)
            } else {
                gray_img
            }
        };

        const CANNY_SCALE: f32 = 255.0;
        let low = (t_low as f32) * CANNY_SCALE;
        let high = (t_high as f32) * CANNY_SCALE;
        let mut edges_img = edges::canny(&base, low, high);

        if thin_lines {
            thin_edges_zhang_suen(&mut edges_img);
        }

        if dilate_k > 0 {
            edges_img = morphology::dilate(&edges_img, Norm::L2, dilate_k);
        }

        let out_world_type = out_layer.world_type();
        let edges_data = edges_img.as_raw();

        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let idx = y as usize * w + x as usize;
            let mut v = if idx < edges_data.len() {
                edges_data[idx] as f32 / 255.0
            } else {
                0.0
            };
            if invert {
                v = 1.0 - v;
            }

            let vis = if use_alpha { v * alpha_map[idx] } else { v };

            let out_px = PixelF32 {
                alpha: if use_alpha { vis } else { 1.0 },
                red: line_color.red * vis,
                green: line_color.green * vis,
                blue: line_color.blue * vis,
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

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn thin_edges_zhang_suen(img: &mut GrayImage) {
    let w = img.width() as usize;
    let h = img.height() as usize;
    if w < 3 || h < 3 {
        return;
    }

    let mut data: Vec<u8> = img
        .as_raw()
        .iter()
        .map(|&v| if v > 0 { 1 } else { 0 })
        .collect();

    let mut changed = true;
    let mut iter = 0;
    while changed && iter < 64 {
        iter += 1;
        changed = false;

        for pass in 0..2 {
            let mut to_remove: Vec<usize> = Vec::new();

            for y in 1..(h - 1) {
                for x in 1..(w - 1) {
                    let idx = y * w + x;
                    if data[idx] == 0 {
                        continue;
                    }

                    let p2 = data[(y - 1) * w + x];
                    let p3 = data[(y - 1) * w + x + 1];
                    let p4 = data[y * w + x + 1];
                    let p5 = data[(y + 1) * w + x + 1];
                    let p6 = data[(y + 1) * w + x];
                    let p7 = data[(y + 1) * w + x - 1];
                    let p8 = data[y * w + x - 1];
                    let p9 = data[(y - 1) * w + x - 1];

                    let bp = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9;
                    if bp < 2 || bp > 6 {
                        continue;
                    }

                    let ap = (p2 == 0 && p3 == 1) as u8
                        + (p3 == 0 && p4 == 1) as u8
                        + (p4 == 0 && p5 == 1) as u8
                        + (p5 == 0 && p6 == 1) as u8
                        + (p6 == 0 && p7 == 1) as u8
                        + (p7 == 0 && p8 == 1) as u8
                        + (p8 == 0 && p9 == 1) as u8
                        + (p9 == 0 && p2 == 1) as u8;

                    if ap != 1 {
                        continue;
                    }

                    let m1 = if pass == 0 {
                        p2 * p4 * p6
                    } else {
                        p2 * p4 * p8
                    };
                    let m2 = if pass == 0 {
                        p4 * p6 * p8
                    } else {
                        p2 * p6 * p8
                    };

                    if m1 != 0 || m2 != 0 {
                        continue;
                    }

                    to_remove.push(idx);
                }
            }

            if !to_remove.is_empty() {
                changed = true;
                for idx in to_remove {
                    data[idx] = 0;
                }
            }
        }
    }

    for (dst, &src) in img.as_mut().iter_mut().zip(data.iter()) {
        *dst = if src != 0 { 255 } else { 0 };
    }
}
