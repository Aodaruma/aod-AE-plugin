use after_effects as ae;
use color_art::{Color as ArtColor, ColorSpace as ArtColorSpace};
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, Lab, LinSrgb, Oklab, Oklch, Srgb};
use std::env;
use std::str::FromStr;

use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    FromSpace,   // Popup
    ToSpace,     // Popup
    ClampOutput, // bool
    FallbackPreview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ColorSpace {
    Rgb,
    Oklab,
    Oklch,
    Lab,
    Yiq,
    Yuv,
    YCbCr,
    Hsl,
    Hsv,
    Cmyk,
}

#[derive(Clone, Copy, Debug)]
struct EncodedColor {
    r: f32,
    g: f32,
    b: f32,
    a_override: Option<f32>,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "A plugin that can convert to and from various color spaces from RGB.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // param definitions here
        const OPTIONS: [&str; 10] = [
            "RGB", "OKLAB", "OKLCH", "LAB", "YIQ", "YUV", "YCbCr", "HSL", "HSV", "CMYK",
        ];

        params.add(
            Params::FromSpace,
            "From Color Space",
            ae::pf::PopupDef::setup(|d| {
                d.set_options(&OPTIONS);
                d.set_default(1); // RGB
            }),
        )?;

        params.add(
            Params::ToSpace,
            "To Color Space",
            ae::pf::PopupDef::setup(|d| {
                d.set_options(&OPTIONS);
                d.set_default(1); // RGB
            }),
        )?;

        params.add(
            Params::ClampOutput,
            "Clamp Output 0..1",
            ae::pf::CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

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
                    "AOD_ColorConvert - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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

                // self.do_render(in_data, in_layer_opt, out_data, out_layer_opt, params)?;

                cb.checkin_layer_pixels(0)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn color_space_from_popup(value: i32) -> ColorSpace {
        match value {
            2 => ColorSpace::Oklab,
            3 => ColorSpace::Oklch,
            4 => ColorSpace::Lab,
            5 => ColorSpace::Yiq,
            6 => ColorSpace::Yuv,
            7 => ColorSpace::YCbCr,
            8 => ColorSpace::Hsl,
            9 => ColorSpace::Hsv,
            10 => ColorSpace::Cmyk,
            _ => ColorSpace::Rgb,
        }
    }

    #[inline]
    fn clamp01(x: f32) -> f32 {
        x.max(0.0).min(1.0)
    }

    #[inline]
    fn wrap01(x: f32) -> f32 {
        let mut v = x % 1.0;
        if v < 0.0 {
            v += 1.0;
        }
        v
    }

    #[inline]
    fn encode_signed(value: f32, max_abs: f32) -> f32 {
        (value / (2.0 * max_abs)) + 0.5
    }

    #[inline]
    fn decode_signed(channel: f32, max_abs: f32) -> f32 {
        (channel - 0.5) * (2.0 * max_abs)
    }

    #[inline]
    fn encode_pos(value: f32, max: f32) -> f32 {
        value / max
    }

    #[inline]
    fn decode_pos(channel: f32, max: f32) -> f32 {
        channel * max
    }

    fn decode_to_linear(space: ColorSpace, r: f32, g: f32, b: f32, a: f32) -> LinSrgb<f32> {
        const OKLAB_AB_MAX: f32 = 0.5;
        const OKLCH_CHROMA_MAX: f32 = 0.4;
        const LAB_L_MAX: f32 = 100.0;
        const LAB_AB_MAX: f32 = 128.0;
        const YIQ_I_MAX: f32 = 0.5957;
        const YIQ_Q_MAX: f32 = 0.5226;
        const YUV_U_MAX: f32 = 0.436;
        const YUV_V_MAX: f32 = 0.615;
        const YCBCR_MAX: f32 = 255.0;

        match space {
            ColorSpace::Rgb => Srgb::new(r, g, b).into_linear(),
            ColorSpace::Oklab => {
                let l = b;
                let a = Self::decode_signed(r, OKLAB_AB_MAX);
                let bb = Self::decode_signed(g, OKLAB_AB_MAX);
                LinSrgb::from_color(Oklab::new(l, a, bb))
            }
            ColorSpace::Oklch => {
                let l = b;
                let chroma = Self::decode_pos(g, OKLCH_CHROMA_MAX);
                let hue = Self::wrap01(r) * 360.0;
                LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)))
            }
            ColorSpace::Lab => {
                let l = b * LAB_L_MAX;
                let a = Self::decode_signed(r, LAB_AB_MAX);
                let bb = Self::decode_signed(g, LAB_AB_MAX);
                LinSrgb::from_color(Lab::new(l, a, bb))
            }
            ColorSpace::Yiq => {
                let y = b;
                let i = Self::decode_signed(r, YIQ_I_MAX);
                let q = Self::decode_signed(g, YIQ_Q_MAX);
                let spec = format!("yiq({:.6},{:.6},{:.6})", y, i, q);
                let color = ArtColor::from_str(&spec);
                if let Ok(color) = color {
                    let rgb = color.vec_of(ArtColorSpace::RGB);
                    Srgb::new(
                        (rgb[0] / 255.0) as f32,
                        (rgb[1] / 255.0) as f32,
                        (rgb[2] / 255.0) as f32,
                    )
                    .into_linear()
                } else {
                    Srgb::new(r, g, b).into_linear()
                }
            }
            ColorSpace::Yuv => {
                let y = b;
                let u = Self::decode_signed(r, YUV_U_MAX);
                let v = Self::decode_signed(g, YUV_V_MAX);
                let spec = format!("yuv({:.6},{:.6},{:.6})", y, u, v);
                let color = ArtColor::from_str(&spec);
                if let Ok(color) = color {
                    let rgb = color.vec_of(ArtColorSpace::RGB);
                    Srgb::new(
                        (rgb[0] / 255.0) as f32,
                        (rgb[1] / 255.0) as f32,
                        (rgb[2] / 255.0) as f32,
                    )
                    .into_linear()
                } else {
                    Srgb::new(r, g, b).into_linear()
                }
            }
            ColorSpace::YCbCr => {
                let y = Self::decode_pos(b, YCBCR_MAX);
                let cb = Self::decode_pos(r, YCBCR_MAX);
                let cr = Self::decode_pos(g, YCBCR_MAX);
                let spec = format!("ycbcr({:.3},{:.3},{:.3})", y, cb, cr);
                let color = ArtColor::from_str(&spec);
                if let Ok(color) = color {
                    let rgb = color.vec_of(ArtColorSpace::RGB);
                    Srgb::new(
                        (rgb[0] / 255.0) as f32,
                        (rgb[1] / 255.0) as f32,
                        (rgb[2] / 255.0) as f32,
                    )
                    .into_linear()
                } else {
                    Srgb::new(r, g, b).into_linear()
                }
            }
            ColorSpace::Hsl => {
                let hue = Self::wrap01(r) * 360.0;
                let saturation = g;
                let lightness = b;
                LinSrgb::from_color(Hsl::new(RgbHue::from_degrees(hue), saturation, lightness))
            }
            ColorSpace::Hsv => {
                let hue = Self::wrap01(r) * 360.0;
                let saturation = g;
                let value = b;
                LinSrgb::from_color(Hsv::new(RgbHue::from_degrees(hue), saturation, value))
            }
            ColorSpace::Cmyk => {
                let c = r as f64;
                let m = g as f64;
                let y = b as f64;
                let k = a as f64;
                match ArtColor::from_cmyk(c, m, y, k) {
                    Ok(color) => {
                        let rgb = color.vec_of(ArtColorSpace::RGB);
                        Srgb::new(
                            (rgb[0] / 255.0) as f32,
                            (rgb[1] / 255.0) as f32,
                            (rgb[2] / 255.0) as f32,
                        )
                        .into_linear()
                    }
                    Err(_) => Srgb::new(r, g, b).into_linear(),
                }
            }
        }
    }

    fn encode_from_linear(space: ColorSpace, lin: LinSrgb<f32>) -> EncodedColor {
        const OKLAB_AB_MAX: f32 = 0.5;
        const OKLCH_CHROMA_MAX: f32 = 0.4;
        const LAB_L_MAX: f32 = 100.0;
        const LAB_AB_MAX: f32 = 128.0;
        const YIQ_I_MAX: f32 = 0.5957;
        const YIQ_Q_MAX: f32 = 0.5226;
        const YUV_U_MAX: f32 = 0.436;
        const YUV_V_MAX: f32 = 0.615;
        const YCBCR_MAX: f32 = 255.0;

        match space {
            ColorSpace::Rgb => {
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                EncodedColor {
                    r: srgb.red,
                    g: srgb.green,
                    b: srgb.blue,
                    a_override: None,
                }
            }
            ColorSpace::Oklab => {
                let c: Oklab<f32> = Oklab::from_color(lin);
                let r = Self::encode_signed(c.a, OKLAB_AB_MAX);
                let g = Self::encode_signed(c.b, OKLAB_AB_MAX);
                let b = c.l;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Oklch => {
                let c: Oklch<f32> = Oklch::from_color(lin);
                let r = Self::wrap01(c.hue.into_degrees() / 360.0);
                let g = Self::encode_pos(c.chroma, OKLCH_CHROMA_MAX);
                let b = c.l;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Lab => {
                let c = Lab::from_color(lin);
                let r = Self::encode_signed(c.a, LAB_AB_MAX);
                let g = Self::encode_signed(c.b, LAB_AB_MAX);
                let b = c.l / LAB_L_MAX;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Yiq => {
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                let art = ArtColor::new(
                    (srgb.red as f64) * 255.0,
                    (srgb.green as f64) * 255.0,
                    (srgb.blue as f64) * 255.0,
                    1.0,
                );
                let yiq = art.vec_of(ArtColorSpace::YIQ);
                let r = Self::encode_signed(yiq[1] as f32, YIQ_I_MAX);
                let g = Self::encode_signed(yiq[2] as f32, YIQ_Q_MAX);
                let b = yiq[0] as f32;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Yuv => {
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                let art = ArtColor::new(
                    (srgb.red as f64) * 255.0,
                    (srgb.green as f64) * 255.0,
                    (srgb.blue as f64) * 255.0,
                    1.0,
                );
                let yuv = art.vec_of(ArtColorSpace::YUV);
                let r = Self::encode_signed(yuv[1] as f32, YUV_U_MAX);
                let g = Self::encode_signed(yuv[2] as f32, YUV_V_MAX);
                let b = yuv[0] as f32;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::YCbCr => {
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                let art = ArtColor::new(
                    (srgb.red as f64) * 255.0,
                    (srgb.green as f64) * 255.0,
                    (srgb.blue as f64) * 255.0,
                    1.0,
                );
                let ycbcr = art.vec_of(ArtColorSpace::YCbCr);
                let r = Self::encode_pos(ycbcr[1] as f32, YCBCR_MAX);
                let g = Self::encode_pos(ycbcr[2] as f32, YCBCR_MAX);
                let b = Self::encode_pos(ycbcr[0] as f32, YCBCR_MAX);
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Hsl => {
                let c = Hsl::from_color(lin);
                let r = Self::wrap01(c.hue.into_degrees() / 360.0);
                let g = c.saturation;
                let b = c.lightness;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Hsv => {
                let c = Hsv::from_color(lin);
                let r = Self::wrap01(c.hue.into_degrees() / 360.0);
                let g = c.saturation;
                let b = c.value;
                EncodedColor {
                    r,
                    g,
                    b,
                    a_override: None,
                }
            }
            ColorSpace::Cmyk => {
                let srgb: Srgb<f32> = Srgb::from_linear(lin);
                let art = ArtColor::new(
                    (srgb.red as f64) * 255.0,
                    (srgb.green as f64) * 255.0,
                    (srgb.blue as f64) * 255.0,
                    1.0,
                );
                let cmyk = art.vec_of(ArtColorSpace::CMYK);
                EncodedColor {
                    r: cmyk[0] as f32,
                    g: cmyk[1] as f32,
                    b: cmyk[2] as f32,
                    a_override: Some(cmyk[3] as f32),
                }
            }
        }
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let progress_final = out_layer.height() as i32;
        let width = in_layer.width() as usize;
        let height = in_layer.height() as usize;
        let frame_num = in_data.current_frame() as usize;
        let _ = (width, height, frame_num);

        let from_space =
            Self::color_space_from_popup(params.get(Params::FromSpace)?.as_popup()?.value() as i32);
        let to_space =
            Self::color_space_from_popup(params.get(Params::ToSpace)?.as_popup()?.value() as i32);
        let clamp_output = params.get(Params::ClampOutput)?.as_checkbox()?.value();
        let fallback_preview = params.get(Params::FallbackPreview)?.as_checkbox()?.value();

        let in_world_type = in_layer.world_type();
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x, _y, in_px, mut out_px| {
                let p = match in_world_type {
                    ae::aegp::WorldType::U8 => in_px.as_u8().to_pixel32(),
                    ae::aegp::WorldType::U15 => in_px.as_u16().to_pixel32(),
                    ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => in_px.as_f32(),
                };

                let lin = Self::decode_to_linear(from_space, p.red, p.green, p.blue, p.alpha);
                let encoded = Self::encode_from_linear(to_space, lin);

                let mut r = encoded.r;
                let mut g = encoded.g;
                let mut b = encoded.b;
                let mut out_alpha = encoded.a_override.unwrap_or(p.alpha);
                let mut fallback_used = false;

                if !out_is_f32 {
                    let non_finite = !r.is_finite()
                        || !g.is_finite()
                        || !b.is_finite()
                        || !out_alpha.is_finite();

                    if non_finite {
                        fallback_used = true;
                        r = p.red;
                        g = p.green;
                        b = p.blue;
                        out_alpha = p.alpha;
                    } else if clamp_output {
                        let out_of_range = r < 0.0
                            || r > 1.0
                            || g < 0.0
                            || g > 1.0
                            || b < 0.0
                            || b > 1.0
                            || out_alpha < 0.0
                            || out_alpha > 1.0;

                        if out_of_range {
                            fallback_used = true;
                            r = Self::clamp01(r);
                            g = Self::clamp01(g);
                            b = Self::clamp01(b);
                            out_alpha = Self::clamp01(out_alpha);
                        }
                    }

                    if fallback_preview && fallback_used {
                        r = Self::clamp01(r * 0.5 + 0.5);
                        g = Self::clamp01(g * 0.5);
                        b = Self::clamp01(b * 0.5 + 0.5);
                    }
                }

                let out_f32 = PixelF32 {
                    alpha: out_alpha,
                    red: r,
                    green: g,
                    blue: b,
                };

                match out_world_type {
                    ae::aegp::WorldType::U8 => out_px.set_from_u8(out_f32.to_pixel8()),
                    ae::aegp::WorldType::U15 => out_px.set_from_u16(out_f32.to_pixel16()),
                    ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                        out_px.set_from_f32(out_f32);
                    }
                }

                Ok(())
            },
        )?;

        Ok(())
    }
}
