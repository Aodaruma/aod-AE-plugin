#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use palette::{FromColor, Hsl, Lab, LinSrgb, Oklab, Oklch, Srgb};
use std::env;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ColorSpace,      // Popup: OKLCH / OKLAB / LAB / HSL / CMYK / YUV / YCbCr / YIQ
    HueShift,        // deg
    ChromaScale,     // multiplier
    LightnessDelta,  // delta
    ClampToSRgb,     // bool
    FallbackPreview, // bool (将来プレビュー用のフック。現状は簡易オーバーレイ)
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "A plugin to modify the chroma and lightness of an image using various color space";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // param definitions here
        // Color Space (将来: LCh などを足す想定)
        params.add(
            Params::ColorSpace,
            "Color Space",
            ae::pf::PopupDef::setup(|d| {
                d.set_options(&[
                    "OKLCH", "OKLAB", "LAB", "HSL", "CMYK", "YUV", "YCbCr", "YIQ",
                ]);
                d.set_default(1); // 1-based
            }),
        )?;

        // Hue Shift (degrees)
        params.add(
            Params::HueShift,
            "Hue Shift (deg)",
            ae::pf::FloatSliderDef::setup(|d| {
                d.set_valid_min(-180.0);
                d.set_valid_max(180.0);
                d.set_slider_min(-180.0);
                d.set_slider_max(180.0);
                d.set_default(0.0);
                d.set_precision(1);
            }),
        )?;

        // Chroma Scale (multiplier)
        params.add(
            Params::ChromaScale,
            "Chroma Scale",
            ae::pf::FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(4.0);
                d.set_slider_min(0.0);
                d.set_slider_max(2.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        // Lightness Delta
        params.add(
            Params::LightnessDelta,
            "Lightness Delta",
            ae::pf::FloatSliderDef::setup(|d| {
                d.set_valid_min(-1.0);
                d.set_valid_max(1.0);
                d.set_slider_min(-0.5);
                d.set_slider_max(0.5);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        // Clamp to sRGB (0..1)
        params.add(
            Params::ClampToSRgb,
            "Clamp to sRGB",
            ae::pf::CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

        // Fallback preview (将来: “fallback 対象” を認知させる UI/表示へ拡張する想定)
        params.add(
            Params::FallbackPreview,
            "Fallback Preview",
            ae::pf::CheckBoxDef::setup(|d| {
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
                    "AOD_ColorAjust - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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

                if let (Some(in_layer), Some(out_layer)) = (in_layer_opt, out_layer_opt) {
                    self.do_render(in_data, in_layer, out_data, out_layer, params)?;
                }

                // self.do_render(in_data, in_layer_opt, out_data, out_layer_opt, params)?;

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
        in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let progress_final = out_layer.height() as i32;
        let color_space = params.get(Params::ColorSpace)?.as_popup()?.value(); // 1-based
        let hue_shift_deg = params.get(Params::HueShift)?.as_float_slider()?.value() as f32;
        let hue_shift_rad = hue_shift_deg.to_radians();
        let chroma_scale = params.get(Params::ChromaScale)?.as_float_slider()?.value() as f32;
        let lightness_delta = params
            .get(Params::LightnessDelta)?
            .as_float_slider()?
            .value() as f32;
        let clamp_to_srgb_param = params.get(Params::ClampToSRgb)?.as_checkbox()?.value();
        let fallback_preview = params.get(Params::FallbackPreview)?.as_checkbox()?.value();

        let _ = in_data.current_frame();
        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        // 32bpc ではクランプしない（Issue #2 対応）
        let clamp_to_srgb = clamp_to_srgb_param && !out_is_f32;

        #[inline]
        fn clamp01(x: f32) -> f32 {
            x.clamp(0.0, 1.0)
        }

        #[inline]
        fn clamp100(x: f32) -> f32 {
            x.clamp(0.0, 100.0)
        }

        let rotate_chroma = |x: f32, y: f32| -> (f32, f32) {
            let x = x * chroma_scale;
            let y = y * chroma_scale;
            let cs = hue_shift_rad.cos();
            let sn = hue_shift_rad.sin();
            (x * cs - y * sn, x * sn + y * cs)
        };

        // 将来ここは OCIO 等に差し替え可能
        let decode_input = |r: f32, g: f32, b: f32| Srgb::new(r, g, b);
        let encode_output = |srgb: Srgb<f32>| srgb;

        let finalize_rgb = |srgb_in: Srgb<f32>,
                            mut out: Srgb<f32>,
                            mut fallback_used: bool|
         -> (Srgb<f32>, bool) {
            // NaN/Inf guard
            if !out.red.is_finite() || !out.green.is_finite() || !out.blue.is_finite() {
                return (srgb_in, true);
            }

            // gamut out-of-range
            let out_of_range = out.red < 0.0
                || out.red > 1.0
                || out.green < 0.0
                || out.green > 1.0
                || out.blue < 0.0
                || out.blue > 1.0;

            if out_of_range {
                fallback_used = true;
                if clamp_to_srgb {
                    out.red = clamp01(out.red);
                    out.green = clamp01(out.green);
                    out.blue = clamp01(out.blue);
                }
            }

            (out, fallback_used)
        };

        // ---- Color Space conversions ----
        let rgb_to_cmyk = |r: f32, g: f32, b: f32| -> (f32, f32, f32, f32) {
            let k = 1.0 - r.max(g).max(b);
            if k >= 1.0 - 1.0e-8 {
                (0.0, 0.0, 0.0, 1.0)
            } else {
                let inv = 1.0 - k;
                (
                    (1.0 - r - k) / inv,
                    (1.0 - g - k) / inv,
                    (1.0 - b - k) / inv,
                    k,
                )
            }
        };
        let cmyk_to_rgb = |c: f32, m: f32, y: f32, k: f32| -> Srgb<f32> {
            Srgb::new(
                (1.0 - c) * (1.0 - k),
                (1.0 - m) * (1.0 - k),
                (1.0 - y) * (1.0 - k),
            )
        };

        let rgb_to_yuv = |r: f32, g: f32, b: f32| -> (f32, f32, f32) {
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let u = -0.14713 * r - 0.28886 * g + 0.436 * b;
            let v = 0.615 * r - 0.51499 * g - 0.10001 * b;
            (y, u, v)
        };
        let yuv_to_rgb = |y: f32, u: f32, v: f32| -> Srgb<f32> {
            Srgb::new(
                y + 1.13983 * v,
                y - 0.39465 * u - 0.58060 * v,
                y + 2.03211 * u,
            )
        };

        let rgb_to_ycbcr = |r: f32, g: f32, b: f32| -> (f32, f32, f32) {
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let cb = (b - y) * 0.564;
            let cr = (r - y) * 0.713;
            (y, cb, cr)
        };
        let ycbcr_to_rgb = |y: f32, cb: f32, cr: f32| -> Srgb<f32> {
            Srgb::new(y + 1.403 * cr, y - 0.344 * cb - 0.714 * cr, y + 1.773 * cb)
        };

        let rgb_to_yiq = |r: f32, g: f32, b: f32| -> (f32, f32, f32) {
            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            let i = 0.595716 * r - 0.274453 * g - 0.321263 * b;
            let q = 0.211456 * r - 0.522591 * g + 0.311135 * b;
            (y, i, q)
        };
        let yiq_to_rgb = |y: f32, i: f32, q: f32| -> Srgb<f32> {
            Srgb::new(
                y + 0.9563 * i + 0.6210 * q,
                y - 0.2721 * i - 0.6474 * q,
                y - 1.1070 * i + 1.7046 * q,
            )
        };

        let adjust_oklch = |srgb_in: Srgb<f32>| -> (Srgb<f32>, bool) {
            let mut fallback_used = false;

            let lin: LinSrgb<f32> = srgb_in.into_linear();
            let mut c: Oklch<f32> = Oklch::from_color(lin);
            c.hue += palette::hues::OklabHue::from_degrees(hue_shift_deg);
            c.chroma = (c.chroma * chroma_scale).max(0.0);

            let target_l = c.l + lightness_delta;
            c.l = if out_is_f32 {
                target_l
            } else {
                clamp01(target_l)
            };
            if !out_is_f32 && (c.l - target_l).abs() > 1.0e-6 {
                fallback_used = true;
            }

            let lin_out: LinSrgb<f32> = LinSrgb::from_color(c);
            let out: Srgb<f32> = Srgb::from_linear(lin_out);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        let adjust_oklab = |srgb_in: Srgb<f32>| -> (Srgb<f32>, bool) {
            let mut fallback_used = false;

            let lin: LinSrgb<f32> = srgb_in.into_linear();
            let mut c: Oklab<f32> = Oklab::from_color(lin);
            let (a, b) = rotate_chroma(c.a, c.b);
            c.a = a;
            c.b = b;

            let target_l = c.l + lightness_delta;
            c.l = if out_is_f32 {
                target_l
            } else {
                clamp01(target_l)
            };
            if !out_is_f32 && (c.l - target_l).abs() > 1.0e-6 {
                fallback_used = true;
            }

            let lin_out: LinSrgb<f32> = LinSrgb::from_color(c);
            let out: Srgb<f32> = Srgb::from_linear(lin_out);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        let adjust_lab = |srgb_in: Srgb<f32>| -> (Srgb<f32>, bool) {
            let mut fallback_used = false;

            let lin: LinSrgb<f32> = srgb_in.into_linear();
            let mut c: Lab = Lab::from_color(lin);
            let (a, b) = rotate_chroma(c.a, c.b);
            c.a = a;
            c.b = b;

            let target_l = c.l + lightness_delta * 100.0;
            c.l = if out_is_f32 {
                target_l
            } else {
                clamp100(target_l)
            };
            if !out_is_f32 && (c.l - target_l).abs() > 1.0e-6 {
                fallback_used = true;
            }

            let lin_out: LinSrgb<f32> = LinSrgb::from_color(c);
            let out: Srgb<f32> = Srgb::from_linear(lin_out);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        let adjust_hsl = |srgb_in: Srgb<f32>| -> (Srgb<f32>, bool) {
            let mut fallback_used = false;

            let mut c = Hsl::from_color(srgb_in);
            c.hue += palette::hues::RgbHue::from_degrees(hue_shift_deg);

            let target_s = (c.saturation * chroma_scale).max(0.0);
            c.saturation = if out_is_f32 {
                target_s
            } else {
                let s = clamp01(target_s);
                if (s - target_s).abs() > 1.0e-6 {
                    fallback_used = true;
                }
                s
            };

            let target_l = c.lightness + lightness_delta;
            c.lightness = if out_is_f32 {
                target_l
            } else {
                let l = clamp01(target_l);
                if (l - target_l).abs() > 1.0e-6 {
                    fallback_used = true;
                }
                l
            };

            let out: Srgb<f32> = Srgb::from_color(c);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        let adjust_cmyk = |srgb_in: Srgb<f32>| -> (Srgb<f32>, bool) {
            let mut fallback_used = false;
            let (mut c, mut m, mut y, mut k) =
                rgb_to_cmyk(srgb_in.red, srgb_in.green, srgb_in.blue);

            let mean = (c + m + y) / 3.0;
            let u = c - m;
            let v = (c + m - 2.0 * y) * 0.577_350_26; // 1 / sqrt(3)
            let (u2, v2) = rotate_chroma(u, v);
            c = mean + 0.5 * u2 + 0.288_675_13 * v2; // sqrt(3)/6
            m = mean - 0.5 * u2 + 0.288_675_13 * v2;
            y = mean - 0.577_350_26 * v2;

            let target_k = k - lightness_delta; // K は黒量なので反転方向
            k = if out_is_f32 {
                target_k
            } else {
                let kk = clamp01(target_k);
                if (kk - target_k).abs() > 1.0e-6 {
                    fallback_used = true;
                }
                kk
            };

            if !out_is_f32 {
                let c0 = c;
                let m0 = m;
                let y0 = y;
                c = clamp01(c);
                m = clamp01(m);
                y = clamp01(y);
                if (c - c0).abs() > 1.0e-6 || (m - m0).abs() > 1.0e-6 || (y - y0).abs() > 1.0e-6 {
                    fallback_used = true;
                }
            }

            let out = cmyk_to_rgb(c, m, y, k);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        let adjust_yuv_like = |srgb_in: Srgb<f32>,
                               rgb_to_yx: &dyn Fn(f32, f32, f32) -> (f32, f32, f32),
                               yx_to_rgb: &dyn Fn(f32, f32, f32) -> Srgb<f32>|
         -> (Srgb<f32>, bool) {
            let mut fallback_used = false;
            let (mut y, c1, c2) = rgb_to_yx(srgb_in.red, srgb_in.green, srgb_in.blue);
            let (c1, c2) = rotate_chroma(c1, c2);

            let target_y = y + lightness_delta;
            y = if out_is_f32 {
                target_y
            } else {
                let yy = clamp01(target_y);
                if (yy - target_y).abs() > 1.0e-6 {
                    fallback_used = true;
                }
                yy
            };

            let out = yx_to_rgb(y, c1, c2);
            finalize_rgb(srgb_in, out, fallback_used)
        };

        // ---- render ----
        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |x, y, in_px, mut out_px| {
                let p = match in_world_type {
                    ae::aegp::WorldType::U8 => in_px.as_u8().to_pixel32(),
                    ae::aegp::WorldType::U15 => in_px.as_u16().to_pixel32(),
                    ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => in_px.as_f32(),
                };

                let srgb_in = decode_input(p.red, p.green, p.blue);

                let (srgb_adj, fallback_used) = match color_space {
                    2 => adjust_oklab(srgb_in),
                    3 => adjust_lab(srgb_in),
                    4 => adjust_hsl(srgb_in),
                    5 => adjust_cmyk(srgb_in),
                    6 => adjust_yuv_like(srgb_in, &rgb_to_yuv, &yuv_to_rgb),
                    7 => adjust_yuv_like(srgb_in, &rgb_to_ycbcr, &ycbcr_to_rgb),
                    8 => adjust_yuv_like(srgb_in, &rgb_to_yiq, &yiq_to_rgb),
                    _ => adjust_oklch(srgb_in),
                };

                let mut srgb_out = encode_output(srgb_adj);
                if fallback_preview && fallback_used {
                    srgb_out.red = clamp01(srgb_out.red * 0.5 + 0.5);
                    srgb_out.green = clamp01(srgb_out.green * 0.5);
                    srgb_out.blue = clamp01(srgb_out.blue * 0.5 + 0.5);
                }

                let out_f32 = PixelF32 {
                    alpha: p.alpha,
                    red: srgb_out.red,
                    green: srgb_out.green,
                    blue: srgb_out.blue,
                };

                match out_world_type {
                    ae::aegp::WorldType::U8 => out_px.set_from_u8(out_f32.to_pixel8()),
                    ae::aegp::WorldType::U15 => out_px.set_from_u16(out_f32.to_pixel16()),
                    ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                        out_px.set_from_f32(out_f32);
                    }
                }

                let _ = (x, y);
                Ok(())
            },
        )?;

        Ok(())
    }
}
