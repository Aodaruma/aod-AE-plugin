use after_effects as ae;
use std::env;

use ae::pf::*;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    // Mobius coefficients (complex): a,b,c,d
    ARe,
    AIm,
    BRe,
    BIm,
    CRe,
    CIm,
    DRe,
    DIm,

    // Mapping controls
    UseLayerCenter,
    Center,
    ScalePx,

    // Outside-destination behavior
    Edge,

    // Sampling interpolation mode
    Interpolation,

    // Minification anti-alias quality
    AntiAlias,

    // Minification strategy
    Minify,

    // Mipmap LOD bias
    MipmapBias,
}

#[derive(Default)]
struct MobiusPlugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(MobiusPlugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "A plugin for applying Mobius transformation to layers.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdgeMode {
    Expand,
    Repeat,
    Mirror,
    Tile,
    None,
}

impl EdgeMode {
    fn from_popup_value(v: i32) -> Self {
        match v {
            1 => EdgeMode::Expand,
            2 => EdgeMode::Repeat,
            3 => EdgeMode::Mirror,
            4 => EdgeMode::Tile,
            _ => EdgeMode::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InterpolationMode {
    Nearest,
    Bilinear,
    Bicubic,
    Mitchell,
    Lanczos3,
}

impl InterpolationMode {
    fn from_popup_value(v: i32) -> Self {
        match v {
            1 => InterpolationMode::Nearest,
            2 => InterpolationMode::Bilinear,
            3 => InterpolationMode::Bicubic,
            4 => InterpolationMode::Mitchell,
            5 => InterpolationMode::Lanczos3,
            _ => InterpolationMode::Bilinear,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AntiAliasMode {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl AntiAliasMode {
    fn from_popup_value(v: i32) -> Self {
        match v {
            1 => AntiAliasMode::Off,
            2 => AntiAliasMode::Low,
            3 => AntiAliasMode::Medium,
            4 => AntiAliasMode::High,
            5 => AntiAliasMode::Ultra,
            _ => AntiAliasMode::Medium,
        }
    }

    fn max_grid(self) -> i32 {
        match self {
            AntiAliasMode::Off => 1,
            AntiAliasMode::Low => 2,
            AntiAliasMode::Medium => 3,
            AntiAliasMode::High => 4,
            AntiAliasMode::Ultra => 5,
        }
    }

    fn scale_boost(self) -> f64 {
        match self {
            AntiAliasMode::Off => 1.0,
            AntiAliasMode::Low => 0.85,
            AntiAliasMode::Medium => 1.1,
            AntiAliasMode::High => 1.35,
            AntiAliasMode::Ultra => 1.7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MinifyMode {
    Supersample,
    Mipmap,
}

impl MinifyMode {
    fn from_popup_value(v: i32) -> Self {
        match v {
            2 => MinifyMode::Mipmap,
            _ => MinifyMode::Supersample,
        }
    }
}

#[derive(Clone, Debug)]
struct ImageF32 {
    width: usize,
    height: usize,
    pixels: Vec<PixelF32>,
}

#[derive(Clone, Copy, Debug)]
struct C64 {
    re: f64,
    im: f64,
}
impl C64 {
    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn norm2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    fn div(self, o: Self) -> Option<Self> {
        let d = o.norm2();
        if d < 1e-18 {
            return None;
        }
        Some(Self::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        ))
    }
}

impl AdobePluginGlobal for MobiusPlugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // a
        params.add(
            Params::ARe,
            "a.re",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(1.0);
                p.set_precision(4);
            }),
        )?;
        params.add(
            Params::AIm,
            "a.im",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;

        // b
        params.add(
            Params::BRe,
            "b.re",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;
        params.add(
            Params::BIm,
            "b.im",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;

        // c
        params.add(
            Params::CRe,
            "c.re",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;
        params.add(
            Params::CIm,
            "c.im",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;

        // d
        params.add(
            Params::DRe,
            "d.re",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(1.0);
                p.set_precision(4);
            }),
        )?;
        params.add(
            Params::DIm,
            "d.im",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-10.0);
                p.set_valid_max(10.0);
                p.set_slider_min(-10.0);
                p.set_slider_max(10.0);
                p.set_default(0.0);
                p.set_precision(4);
            }),
        )?;

        params.add(
            Params::UseLayerCenter,
            "Use Layer Center",
            CheckBoxDef::setup(|c| {
                c.set_default(true);
            }),
        )?;

        params.add(
            Params::Center,
            "Center",
            PointDef::setup(|p| {
                p.set_default((50.0, 50.0));
                p.set_restrict_bounds(true);
            }),
        )?;

        params.add(
            Params::ScalePx,
            "Scale (px, 0=auto)",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(0.0);
                p.set_valid_max(20000.0);
                p.set_slider_min(0.0);
                p.set_slider_max(20000.0);
                p.set_default(0.0);
                p.set_precision(2);
            }),
        )?;

        params.add(
            Params::Edge,
            "Edge",
            PopupDef::setup(|d| {
                d.set_options(&["Expand", "Repeat", "Mirror", "Tile", "None"]);
                d.set_default(5);
            }),
        )?;

        params.add(
            Params::Interpolation,
            "Interpolation",
            PopupDef::setup(|d| {
                d.set_options(&["Nearest", "Bilinear", "Bicubic", "Mitchell", "Lanczos3"]);
                d.set_default(2);
            }),
        )?;

        params.add(
            Params::AntiAlias,
            "Anti-alias",
            PopupDef::setup(|d| {
                d.set_options(&["Off", "Low", "Medium", "High", "Ultra"]);
                d.set_default(3);
            }),
        )?;

        params.add_with_flags(
            Params::Minify,
            "Minify",
            PopupDef::setup(|d| {
                d.set_options(&["Supersample", "Mipmap"]);
                d.set_default(2);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::MipmapBias,
            "Mipmap Bias",
            FloatSliderDef::setup(|p| {
                p.set_valid_min(-2.0);
                p.set_valid_max(4.0);
                p.set_slider_min(-1.0);
                p.set_slider_max(2.0);
                p.set_default(0.0);
                p.set_precision(2);
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
                    "AOD_MobiusTransform - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                    version=env!("CARGO_PKG_VERSION"),
                    build_year=env!("BUILD_YEAR")
                ).as_str());
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_MobiusTransform")
                {
                    self.aegp_id = Some(plugin_id);
                }
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
                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let t = params.type_at(param_index);
                if t == Params::Minify {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params_copy = params.cloned();
                self.update_params_ui(in_data, &mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl MobiusPlugin {
    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let minify_mode =
            MinifyMode::from_popup_value(params.get(Params::Minify)?.as_popup()?.value());
        let show_mipmap_bias = matches!(minify_mode, MinifyMode::Mipmap);
        self.set_param_visible(in_data, params, Params::MipmapBias, show_mipmap_bias)
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if in_data.is_premiere() {
            return Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible);
        }

        if let Some(plugin_id) = self.aegp_id {
            let effect = in_data.effect();
            if let Some(index) = params.index(id)
                && let Ok(effect_ref) = effect.aegp_effect(plugin_id)
                && let Ok(stream) = effect_ref.new_stream_by_index(plugin_id, index as i32)
            {
                return stream.set_dynamic_stream_flag(
                    ae::aegp::DynamicStreamFlags::Hidden,
                    false,
                    !visible,
                );
            }
        }

        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_ui_flag(
        params: &mut ae::Parameters<Params>,
        id: Params,
        flag: ae::pf::ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let flag_bits = flag.bits();
        let current_status = (params.get(id)?.ui_flags().bits() & flag_bits) != 0;
        if current_status == status {
            return Ok(());
        }

        let mut p = params.get_mut(id)?;
        p.set_ui_flag(flag, status);
        p.update_param_ui()?;
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
        let width = in_layer.width();
        let height = in_layer.height();
        let progress_final = height as i32;

        let a = C64::new(
            params.get(Params::ARe)?.as_float_slider()?.value() as f64,
            params.get(Params::AIm)?.as_float_slider()?.value() as f64,
        );
        let b = C64::new(
            params.get(Params::BRe)?.as_float_slider()?.value() as f64,
            params.get(Params::BIm)?.as_float_slider()?.value() as f64,
        );
        let c = C64::new(
            params.get(Params::CRe)?.as_float_slider()?.value() as f64,
            params.get(Params::CIm)?.as_float_slider()?.value() as f64,
        );
        let d = C64::new(
            params.get(Params::DRe)?.as_float_slider()?.value() as f64,
            params.get(Params::DIm)?.as_float_slider()?.value() as f64,
        );

        let use_layer_center = params.get(Params::UseLayerCenter)?.as_checkbox()?.value();
        let (pcx, pcy) = params.get(Params::Center)?.as_point()?.value();
        let mut scale_px = params.get(Params::ScalePx)?.as_float_slider()?.value() as f64;
        let edge_mode =
            EdgeMode::from_popup_value(params.get(Params::Edge)?.as_popup()?.value() as i32);
        let interpolation = InterpolationMode::from_popup_value(
            params.get(Params::Interpolation)?.as_popup()?.value() as i32,
        );
        let aa_mode = AntiAliasMode::from_popup_value(
            params.get(Params::AntiAlias)?.as_popup()?.value() as i32,
        );
        let minify_mode =
            MinifyMode::from_popup_value(params.get(Params::Minify)?.as_popup()?.value() as i32);
        let mipmap_bias = params.get(Params::MipmapBias)?.as_float_slider()?.value() as f64;

        if scale_px <= 1e-9 {
            scale_px = (width.min(height) as f64 - 1.0) * 0.5;
        }

        let (cx, cy) = if use_layer_center {
            ((width as f64 - 1.0) * 0.5, (height as f64 - 1.0) * 0.5)
        } else {
            (pcx as f64, pcy as f64)
        };

        let out_depth = out_layer.bit_depth();
        let det = a.mul(d).sub(b.mul(c));
        let mipmaps = if matches!(minify_mode, MinifyMode::Mipmap) {
            Some(Self::build_mipmaps(&Self::capture_source(&in_layer)))
        } else {
            None
        };

        // f(z) = (a z + b) / (c z + d)
        // inverse: z = (d w - b) / (-c w + a)
        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |x, y, _in_px, mut out_px| {
                let wx = (x as f64 - cx) / scale_px;
                let wy = (y as f64 - cy) / scale_px;
                let w = C64::new(wx, wy);

                let num = d.mul(w).sub(b);
                let den = C64::new(-c.re, -c.im).mul(w).add(a);

                let Some(z) = num.div(den) else {
                    return Self::write_transparent(&mut out_px, out_depth);
                };

                let sx = z.re * scale_px + cx;
                let sy = z.im * scale_px + cy;

                let local_scale = det
                    .div(den.mul(den))
                    .map(|v| v.norm2().sqrt())
                    .filter(|v| v.is_finite())
                    .unwrap_or(1.0);

                let sampled = if let Some(mips) = mipmaps.as_ref() {
                    Self::sample_mipmap_edge_filtered_f32(
                        mips,
                        sx,
                        sy,
                        edge_mode,
                        interpolation,
                        local_scale,
                        aa_mode,
                        mipmap_bias,
                    )
                } else {
                    Self::sample_edge_filtered_f32(
                        &in_layer,
                        sx,
                        sy,
                        edge_mode,
                        interpolation,
                        local_scale,
                        aa_mode,
                    )
                };

                if let Some(p) = sampled {
                    Self::write_f32(&mut out_px, out_depth, p)?;
                } else {
                    Self::write_transparent(&mut out_px, out_depth)?;
                }
                Ok(())
            },
        )?;

        Ok(())
    }

    fn write_transparent(out_px: &mut GenericPixelMut<'_>, depth: i16) -> Result<(), Error> {
        Self::write_f32(
            out_px,
            depth,
            PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            },
        )
    }

    fn write_f32(out_px: &mut GenericPixelMut<'_>, depth: i16, p: PixelF32) -> Result<(), Error> {
        fn clamp01(v: f32) -> f32 {
            v.max(0.0).min(1.0)
        }
        match depth {
            8 => {
                let to_u8 = |v: f32| (clamp01(v) * 255.0 + 0.5) as u8;
                out_px.set_from_u8(Pixel8 {
                    alpha: to_u8(p.alpha),
                    red: to_u8(p.red),
                    green: to_u8(p.green),
                    blue: to_u8(p.blue),
                });
                Ok(())
            }
            16 => {
                let to_u16 = |v: f32| (clamp01(v) * 65535.0 + 0.5) as u16;
                out_px.set_from_u16(Pixel16 {
                    alpha: to_u16(p.alpha),
                    red: to_u16(p.red),
                    green: to_u16(p.green),
                    blue: to_u16(p.blue),
                });
                Ok(())
            }
            _ => {
                out_px.set_from_f32(p);
                Ok(())
            }
        }
    }

    fn read_f32(layer: &Layer, x: usize, y: usize) -> PixelF32 {
        match layer.bit_depth() {
            8 => {
                let p = layer.as_pixel8(x, y);
                PixelF32 {
                    alpha: p.alpha as f32 / 255.0,
                    red: p.red as f32 / 255.0,
                    green: p.green as f32 / 255.0,
                    blue: p.blue as f32 / 255.0,
                }
            }
            16 => {
                let p = layer.as_pixel16(x, y);
                PixelF32 {
                    alpha: p.alpha as f32 / 65535.0,
                    red: p.red as f32 / 65535.0,
                    green: p.green as f32 / 65535.0,
                    blue: p.blue as f32 / 65535.0,
                }
            }
            _ => *layer.as_pixel32(x, y),
        }
    }

    fn capture_source(layer: &Layer) -> ImageF32 {
        let width = layer.width();
        let height = layer.height();
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                pixels.push(Self::read_f32(layer, x, y));
            }
        }
        ImageF32 {
            width,
            height,
            pixels,
        }
    }

    fn build_mipmaps(base: &ImageF32) -> Vec<ImageF32> {
        let mut levels = vec![base.clone()];
        while let Some(prev) = levels.last() {
            if prev.width <= 1 && prev.height <= 1 {
                break;
            }

            let next_w = (prev.width + 1) / 2;
            let next_h = (prev.height + 1) / 2;
            let mut next_pixels = vec![
                PixelF32 {
                    alpha: 0.0,
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                };
                next_w * next_h
            ];

            for y in 0..next_h {
                for x in 0..next_w {
                    let sx0 = (x * 2).min(prev.width - 1);
                    let sx1 = (sx0 + 1).min(prev.width - 1);
                    let sy0 = (y * 2).min(prev.height - 1);
                    let sy1 = (sy0 + 1).min(prev.height - 1);

                    let p00 = prev.pixels[sy0 * prev.width + sx0];
                    let p10 = prev.pixels[sy0 * prev.width + sx1];
                    let p01 = prev.pixels[sy1 * prev.width + sx0];
                    let p11 = prev.pixels[sy1 * prev.width + sx1];

                    next_pixels[y * next_w + x] = PixelF32 {
                        alpha: (p00.alpha + p10.alpha + p01.alpha + p11.alpha) * 0.25,
                        red: (p00.red + p10.red + p01.red + p11.red) * 0.25,
                        green: (p00.green + p10.green + p01.green + p11.green) * 0.25,
                        blue: (p00.blue + p10.blue + p01.blue + p11.blue) * 0.25,
                    };
                }
            }

            levels.push(ImageF32 {
                width: next_w,
                height: next_h,
                pixels: next_pixels,
            });
        }
        levels
    }

    fn read_img_f32(img: &ImageF32, x: usize, y: usize) -> PixelF32 {
        img.pixels[y * img.width + x]
    }

    fn read_img_f32_clamped(img: &ImageF32, x: i32, y: i32) -> PixelF32 {
        let max_x = img.width.saturating_sub(1) as i32;
        let max_y = img.height.saturating_sub(1) as i32;
        let cx = x.clamp(0, max_x) as usize;
        let cy = y.clamp(0, max_y) as usize;
        Self::read_img_f32(img, cx, cy)
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    fn lerp_px(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
        PixelF32 {
            alpha: Self::lerp(a.alpha, b.alpha, t),
            red: Self::lerp(a.red, b.red, t),
            green: Self::lerp(a.green, b.green, t),
            blue: Self::lerp(a.blue, b.blue, t),
        }
    }

    fn sample_bilinear_f32(layer: &Layer, x: f64, y: f64) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x0 + 1).min(w - 1);
        let y1 = (y0 + 1).min(h - 1);

        let tx = (x - x0 as f64) as f32;
        let ty = (y - y0 as f64) as f32;

        let p00 = Self::read_f32(layer, x0 as usize, y0 as usize);
        let p10 = Self::read_f32(layer, x1 as usize, y0 as usize);
        let p01 = Self::read_f32(layer, x0 as usize, y1 as usize);
        let p11 = Self::read_f32(layer, x1 as usize, y1 as usize);

        let a = Self::lerp_px(p00, p10, tx);
        let b = Self::lerp_px(p01, p11, tx);
        Some(Self::lerp_px(a, b, ty))
    }

    fn sample_nearest_f32(layer: &Layer, x: f64, y: f64) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let ix = x.round() as i32;
        let iy = y.round() as i32;
        Some(Self::read_f32(layer, ix as usize, iy as usize))
    }

    fn cubic_weight(d: f64) -> f64 {
        let a = -0.5;
        let x = d.abs();
        if x <= 1.0 {
            (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
        } else if x < 2.0 {
            a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
        } else {
            0.0
        }
    }

    fn mitchell_weight(d: f64) -> f64 {
        let x = d.abs();
        let b = 1.0 / 3.0;
        let c = 1.0 / 3.0;
        if x < 1.0 {
            ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
                + (-18.0 + 12.0 * b + 6.0 * c) * x * x
                + (6.0 - 2.0 * b))
                / 6.0
        } else if x < 2.0 {
            ((-b - 6.0 * c) * x * x * x
                + (6.0 * b + 30.0 * c) * x * x
                + (-12.0 * b - 48.0 * c) * x
                + (8.0 * b + 24.0 * c))
                / 6.0
        } else {
            0.0
        }
    }

    fn sinc(x: f64) -> f64 {
        if x.abs() < 1e-12 { 1.0 } else { x.sin() / x }
    }

    fn lanczos_weight(d: f64, a: f64) -> f64 {
        let x = d.abs();
        if x == 0.0 {
            1.0
        } else if x < a {
            let pix = std::f64::consts::PI * x;
            Self::sinc(pix) * Self::sinc(pix / a)
        } else {
            0.0
        }
    }

    fn read_f32_clamped(layer: &Layer, x: i32, y: i32) -> PixelF32 {
        let max_x = layer.width().saturating_sub(1) as i32;
        let max_y = layer.height().saturating_sub(1) as i32;
        let cx = x.clamp(0, max_x) as usize;
        let cy = y.clamp(0, max_y) as usize;
        Self::read_f32(layer, cx, cy)
    }

    fn sample_bicubic_f32(layer: &Layer, x: f64, y: f64) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;

        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;

        for my in -1..=2 {
            let sy = y_base + my;
            let wy = Self::cubic_weight(y - sy as f64);
            if wy == 0.0 {
                continue;
            }

            for mx in -1..=2 {
                let sx = x_base + mx;
                let wx = Self::cubic_weight(x - sx as f64);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_f32_clamped(layer, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
            }
        }

        Some(PixelF32 {
            alpha: acc_a.clamp(0.0, 1.0) as f32,
            red: acc_r.clamp(0.0, 1.0) as f32,
            green: acc_g.clamp(0.0, 1.0) as f32,
            blue: acc_b.clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_mitchell_f32(layer: &Layer, x: f64, y: f64) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;

        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;
        let mut wsum = 0.0_f64;

        for my in -1..=2 {
            let sy = y_base + my;
            let wy = Self::mitchell_weight(y - sy as f64);
            if wy == 0.0 {
                continue;
            }

            for mx in -1..=2 {
                let sx = x_base + mx;
                let wx = Self::mitchell_weight(x - sx as f64);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_f32_clamped(layer, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
                wsum += wgt;
            }
        }

        if wsum.abs() <= 1e-12 {
            return Some(Self::read_f32_clamped(layer, x_base, y_base));
        }
        let inv = 1.0 / wsum;

        Some(PixelF32 {
            alpha: (acc_a * inv).clamp(0.0, 1.0) as f32,
            red: (acc_r * inv).clamp(0.0, 1.0) as f32,
            green: (acc_g * inv).clamp(0.0, 1.0) as f32,
            blue: (acc_b * inv).clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_lanczos3_f32(layer: &Layer, x: f64, y: f64) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;

        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;
        let mut wsum = 0.0_f64;

        for my in -2..=3 {
            let sy = y_base + my;
            let wy = Self::lanczos_weight(y - sy as f64, 3.0);
            if wy == 0.0 {
                continue;
            }

            for mx in -2..=3 {
                let sx = x_base + mx;
                let wx = Self::lanczos_weight(x - sx as f64, 3.0);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_f32_clamped(layer, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
                wsum += wgt;
            }
        }

        if wsum.abs() <= 1e-12 {
            return Some(Self::read_f32_clamped(layer, x_base, y_base));
        }
        let inv = 1.0 / wsum;

        Some(PixelF32 {
            alpha: (acc_a * inv).clamp(0.0, 1.0) as f32,
            red: (acc_r * inv).clamp(0.0, 1.0) as f32,
            green: (acc_g * inv).clamp(0.0, 1.0) as f32,
            blue: (acc_b * inv).clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_nearest_img_f32(img: &ImageF32, x: f64, y: f64) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let ix = x.round() as i32;
        let iy = y.round() as i32;
        Some(Self::read_img_f32(img, ix as usize, iy as usize))
    }

    fn sample_bilinear_img_f32(img: &ImageF32, x: f64, y: f64) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let x1 = (x0 + 1).min(w - 1);
        let y1 = (y0 + 1).min(h - 1);

        let tx = (x - x0 as f64) as f32;
        let ty = (y - y0 as f64) as f32;

        let p00 = Self::read_img_f32(img, x0 as usize, y0 as usize);
        let p10 = Self::read_img_f32(img, x1 as usize, y0 as usize);
        let p01 = Self::read_img_f32(img, x0 as usize, y1 as usize);
        let p11 = Self::read_img_f32(img, x1 as usize, y1 as usize);

        let a = Self::lerp_px(p00, p10, tx);
        let b = Self::lerp_px(p01, p11, tx);
        Some(Self::lerp_px(a, b, ty))
    }

    fn sample_bicubic_img_f32(img: &ImageF32, x: f64, y: f64) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;
        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;

        for my in -1..=2 {
            let sy = y_base + my;
            let wy = Self::cubic_weight(y - sy as f64);
            if wy == 0.0 {
                continue;
            }
            for mx in -1..=2 {
                let sx = x_base + mx;
                let wx = Self::cubic_weight(x - sx as f64);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_img_f32_clamped(img, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
            }
        }

        Some(PixelF32 {
            alpha: acc_a.clamp(0.0, 1.0) as f32,
            red: acc_r.clamp(0.0, 1.0) as f32,
            green: acc_g.clamp(0.0, 1.0) as f32,
            blue: acc_b.clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_mitchell_img_f32(img: &ImageF32, x: f64, y: f64) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;
        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;
        let mut wsum = 0.0_f64;

        for my in -1..=2 {
            let sy = y_base + my;
            let wy = Self::mitchell_weight(y - sy as f64);
            if wy == 0.0 {
                continue;
            }
            for mx in -1..=2 {
                let sx = x_base + mx;
                let wx = Self::mitchell_weight(x - sx as f64);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_img_f32_clamped(img, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
                wsum += wgt;
            }
        }

        if wsum.abs() <= 1e-12 {
            return Some(Self::read_img_f32_clamped(img, x_base, y_base));
        }
        let inv = 1.0 / wsum;

        Some(PixelF32 {
            alpha: (acc_a * inv).clamp(0.0, 1.0) as f32,
            red: (acc_r * inv).clamp(0.0, 1.0) as f32,
            green: (acc_g * inv).clamp(0.0, 1.0) as f32,
            blue: (acc_b * inv).clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_lanczos3_img_f32(img: &ImageF32, x: f64, y: f64) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }
        if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
            return None;
        }

        let x_base = x.floor() as i32;
        let y_base = y.floor() as i32;
        let mut acc_a = 0.0_f64;
        let mut acc_r = 0.0_f64;
        let mut acc_g = 0.0_f64;
        let mut acc_b = 0.0_f64;
        let mut wsum = 0.0_f64;

        for my in -2..=3 {
            let sy = y_base + my;
            let wy = Self::lanczos_weight(y - sy as f64, 3.0);
            if wy == 0.0 {
                continue;
            }
            for mx in -2..=3 {
                let sx = x_base + mx;
                let wx = Self::lanczos_weight(x - sx as f64, 3.0);
                if wx == 0.0 {
                    continue;
                }

                let wgt = wx * wy;
                let p = Self::read_img_f32_clamped(img, sx, sy);
                acc_a += p.alpha as f64 * wgt;
                acc_r += p.red as f64 * wgt;
                acc_g += p.green as f64 * wgt;
                acc_b += p.blue as f64 * wgt;
                wsum += wgt;
            }
        }

        if wsum.abs() <= 1e-12 {
            return Some(Self::read_img_f32_clamped(img, x_base, y_base));
        }
        let inv = 1.0 / wsum;

        Some(PixelF32 {
            alpha: (acc_a * inv).clamp(0.0, 1.0) as f32,
            red: (acc_r * inv).clamp(0.0, 1.0) as f32,
            green: (acc_g * inv).clamp(0.0, 1.0) as f32,
            blue: (acc_b * inv).clamp(0.0, 1.0) as f32,
        })
    }

    fn sample_interpolated_img_f32(
        img: &ImageF32,
        x: f64,
        y: f64,
        interpolation: InterpolationMode,
    ) -> Option<PixelF32> {
        match interpolation {
            InterpolationMode::Nearest => Self::sample_nearest_img_f32(img, x, y),
            InterpolationMode::Bilinear => Self::sample_bilinear_img_f32(img, x, y),
            InterpolationMode::Bicubic => Self::sample_bicubic_img_f32(img, x, y),
            InterpolationMode::Mitchell => Self::sample_mitchell_img_f32(img, x, y),
            InterpolationMode::Lanczos3 => Self::sample_lanczos3_img_f32(img, x, y),
        }
    }

    fn sample_edge_img_f32(
        img: &ImageF32,
        x: f64,
        y: f64,
        edge: EdgeMode,
        interpolation: InterpolationMode,
    ) -> Option<PixelF32> {
        let w = img.width as i32;
        let h = img.height as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        let max_x = (w - 1) as f64;
        let max_y = (h - 1) as f64;
        let in_bounds = x >= 0.0 && y >= 0.0 && x <= max_x && y <= max_y;
        if in_bounds {
            return Self::sample_interpolated_img_f32(img, x, y, interpolation);
        }

        match edge {
            EdgeMode::None => None,
            EdgeMode::Expand => {
                let cx = x.clamp(0.0, max_x);
                let cy = y.clamp(0.0, max_y);
                Self::sample_interpolated_img_f32(img, cx, cy, interpolation)
            }
            EdgeMode::Repeat | EdgeMode::Tile => {
                let cx = Self::wrap_coord(x, w);
                let cy = Self::wrap_coord(y, h);
                Self::sample_interpolated_img_f32(img, cx, cy, interpolation)
            }
            EdgeMode::Mirror => {
                let cx = Self::mirror_coord(x, w);
                let cy = Self::mirror_coord(y, h);
                Self::sample_interpolated_img_f32(img, cx, cy, interpolation)
            }
        }
    }

    fn sample_mipmap_edge_filtered_f32(
        mipmaps: &[ImageF32],
        x: f64,
        y: f64,
        edge: EdgeMode,
        interpolation: InterpolationMode,
        local_scale: f64,
        aa_mode: AntiAliasMode,
        mipmap_bias: f64,
    ) -> Option<PixelF32> {
        if mipmaps.is_empty() {
            return None;
        }

        let effective_scale = (local_scale * aa_mode.scale_boost()).max(1.0);
        let max_level = (mipmaps.len() - 1) as f64;
        let lod = (effective_scale.log2() + mipmap_bias).clamp(0.0, max_level);
        let l0 = lod.floor() as usize;
        let l1 = (l0 + 1).min(mipmaps.len() - 1);
        let t = (lod - l0 as f64) as f32;

        let scale0 = 2.0_f64.powi(l0 as i32);
        let scale1 = 2.0_f64.powi(l1 as i32);
        let x0 = (x + 0.5) / scale0 - 0.5;
        let y0 = (y + 0.5) / scale0 - 0.5;
        let x1 = (x + 0.5) / scale1 - 0.5;
        let y1 = (y + 0.5) / scale1 - 0.5;

        let p0 = Self::sample_edge_img_f32(&mipmaps[l0], x0, y0, edge, interpolation);
        let p1 = Self::sample_edge_img_f32(&mipmaps[l1], x1, y1, edge, interpolation);

        match (p0, p1) {
            (Some(a), Some(b)) => Some(Self::lerp_px(a, b, t)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn sample_interpolated_f32(
        layer: &Layer,
        x: f64,
        y: f64,
        interpolation: InterpolationMode,
    ) -> Option<PixelF32> {
        match interpolation {
            InterpolationMode::Nearest => Self::sample_nearest_f32(layer, x, y),
            InterpolationMode::Bilinear => Self::sample_bilinear_f32(layer, x, y),
            InterpolationMode::Bicubic => Self::sample_bicubic_f32(layer, x, y),
            InterpolationMode::Mitchell => Self::sample_mitchell_f32(layer, x, y),
            InterpolationMode::Lanczos3 => Self::sample_lanczos3_f32(layer, x, y),
        }
    }

    fn sample_edge_f32(
        layer: &Layer,
        x: f64,
        y: f64,
        edge: EdgeMode,
        interpolation: InterpolationMode,
    ) -> Option<PixelF32> {
        let w = layer.width() as i32;
        let h = layer.height() as i32;
        if w <= 0 || h <= 0 {
            return None;
        }

        let max_x = (w - 1) as f64;
        let max_y = (h - 1) as f64;
        let in_bounds = x >= 0.0 && y >= 0.0 && x <= max_x && y <= max_y;
        if in_bounds {
            return Self::sample_interpolated_f32(layer, x, y, interpolation);
        }

        match edge {
            EdgeMode::None => None,
            EdgeMode::Expand => {
                let cx = x.clamp(0.0, max_x);
                let cy = y.clamp(0.0, max_y);
                Self::sample_interpolated_f32(layer, cx, cy, interpolation)
            }
            EdgeMode::Repeat | EdgeMode::Tile => {
                let cx = Self::wrap_coord(x, w);
                let cy = Self::wrap_coord(y, h);
                Self::sample_interpolated_f32(layer, cx, cy, interpolation)
            }
            EdgeMode::Mirror => {
                let cx = Self::mirror_coord(x, w);
                let cy = Self::mirror_coord(y, h);
                Self::sample_interpolated_f32(layer, cx, cy, interpolation)
            }
        }
    }

    fn sample_edge_filtered_f32(
        layer: &Layer,
        x: f64,
        y: f64,
        edge: EdgeMode,
        interpolation: InterpolationMode,
        local_scale: f64,
        aa_mode: AntiAliasMode,
    ) -> Option<PixelF32> {
        if matches!(aa_mode, AntiAliasMode::Off) {
            return Self::sample_edge_f32(layer, x, y, edge, interpolation);
        }

        let effective_scale = (local_scale * aa_mode.scale_boost()).max(1.0);
        let mut sample_grid = if effective_scale > 4.0 {
            5
        } else if effective_scale > 3.0 {
            4
        } else if effective_scale > 2.0 {
            3
        } else if effective_scale > 1.25 {
            2
        } else {
            1
        };
        sample_grid = sample_grid.min(aa_mode.max_grid());

        if sample_grid == 1 {
            return Self::sample_edge_f32(layer, x, y, edge, interpolation);
        }

        let footprint = effective_scale.clamp(1.0, sample_grid as f64);
        let mut acc = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };

        let inv_count = 1.0_f32 / (sample_grid * sample_grid) as f32;
        for gy in 0..sample_grid {
            let oy = ((gy as f64 + 0.5) / sample_grid as f64 - 0.5) * footprint;
            for gx in 0..sample_grid {
                let ox = ((gx as f64 + 0.5) / sample_grid as f64 - 0.5) * footprint;
                let p = Self::sample_edge_f32(layer, x + ox, y + oy, edge, interpolation)
                    .unwrap_or(PixelF32 {
                        alpha: 0.0,
                        red: 0.0,
                        green: 0.0,
                        blue: 0.0,
                    });
                acc.alpha += p.alpha * inv_count;
                acc.red += p.red * inv_count;
                acc.green += p.green * inv_count;
                acc.blue += p.blue * inv_count;
            }
        }

        Some(acc)
    }

    fn wrap_coord(v: f64, size: i32) -> f64 {
        if size <= 0 {
            return 0.0;
        }
        let size_f = size as f64;
        let mut t = v % size_f;
        if t < 0.0 {
            t += size_f;
        }
        t
    }

    fn mirror_coord(v: f64, size: i32) -> f64 {
        if size <= 1 {
            return 0.0;
        }
        let max = (size - 1) as f64;
        let period = 2.0 * max;
        let mut t = v % period;
        if t < 0.0 {
            t += period;
        }
        if t > max {
            t = period - t;
        }
        t
    }
}
