#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;
use std::f32::consts::TAU;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    VectorMode,
    GradientSource,
    MapLayer,
    Radius,
    Samples,
    Strength,
    EdgeMode,
    PreserveAlpha,
}

#[derive(Clone, Copy)]
enum VectorMode {
    GradientDirection,
    HueSaturation,
}

#[derive(Clone, Copy)]
enum GradientSource {
    Lightness,
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy)]
enum EdgeMode {
    None,
    Repeat,
    Tile,
    Mirror,
}

#[derive(Clone, Copy)]
struct FlowVector {
    dir_x: f32,
    dir_y: f32,
    strength: f32,
    valid: bool,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies directional blur along image gradients or hue-saturation vectors.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::VectorMode,
            "Vector Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Gradient Direction", "Hue / Saturation"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::GradientSource,
            "Gradient Source",
            PopupDef::setup(|d| {
                d.set_options(&["Lightness", "Red", "Green", "Blue", "Alpha"]);
                d.set_default(1);
            }),
        )?;

        params.add(Params::MapLayer, "Map Layer", LayerDef::new())?;

        params.add(
            Params::Radius,
            "Blur Radius (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(0.0);
                d.set_slider_max(128.0);
                d.set_default(12.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Samples,
            "Samples",
            SliderDef::setup(|d| {
                d.set_valid_min(2);
                d.set_valid_max(128);
                d.set_slider_min(2);
                d.set_slider_max(64);
                d.set_default(16);
            }),
        )?;

        params.add(
            Params::Strength,
            "Strength",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(32.0);
                d.set_slider_min(0.0);
                d.set_slider_max(8.0);
                d.set_default(1.0);
                d.set_precision(3);
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
            Params::PreserveAlpha,
            "Preserve Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
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
                        "AOD_GradientBlur - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
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
            ae::Command::UserChangedParam { param_index } => {
                if params.type_at(param_index) == Params::VectorMode {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params_copy = params.cloned();
                self.update_params_ui(&mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn update_params_ui(&self, params: &mut Parameters<Params>) -> Result<(), Error> {
        let vector_mode =
            vector_mode_from_popup(params.get(Params::VectorMode)?.as_popup()?.value());
        let show_gradient_source = matches!(vector_mode, VectorMode::GradientDirection);
        Self::set_param_enabled(params, Params::GradientSource, show_gradient_source)?;
        Ok(())
    }

    fn set_param_enabled(
        params: &mut Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        let flag = ae::pf::ParamUIFlags::DISABLED;
        let target_status = !enabled;
        let current_status = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current_status == target_status {
            return Ok(());
        }
        let mut p = params.get_mut(id)?;
        p.set_ui_flag(flag, target_status);
        p.update_param_ui()?;
        Ok(())
    }

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let vector_mode =
            vector_mode_from_popup(params.get(Params::VectorMode)?.as_popup()?.value());
        let gradient_source =
            gradient_source_from_popup(params.get(Params::GradientSource)?.as_popup()?.value());
        let radius = params.get(Params::Radius)?.as_float_slider()?.value() as f32;
        let samples = params.get(Params::Samples)?.as_slider()?.value().max(2) as usize;
        let strength = params.get(Params::Strength)?.as_float_slider()?.value() as f32;
        let edge_mode = edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value());
        let preserve_alpha = params.get(Params::PreserveAlpha)?.as_checkbox()?.value();

        let src = read_layer_rgba(&in_layer);
        let map_checkout = params.checkout_at(Params::MapLayer, None, None, None)?;
        let map_layer = map_checkout.as_layer()?.value();
        let mut map_src_owned: Option<Vec<PixelF32>> = None;
        let (map_w, map_h) = if let Some(layer) = map_layer.as_ref() {
            map_src_owned = Some(read_layer_rgba(layer));
            (layer.width(), layer.height())
        } else {
            (width, height)
        };
        let map_src: &[PixelF32] = match map_src_owned.as_ref() {
            Some(v) => v.as_slice(),
            None => src.as_slice(),
        };

        let out_world_type = out_layer.world_type();
        let w = width;
        let h = height;

        let progress_final = h as i32;
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let xi = x as usize;
            let yi = y as usize;
            let idx = yi * w + xi;
            let center = src[idx];
            let map_x = remap_coord_to_layer(xi, w, map_w);
            let map_y = remap_coord_to_layer(yi, h, map_h);

            let flow = match vector_mode {
                VectorMode::GradientDirection => gradient_flow(
                    map_src,
                    map_w,
                    map_h,
                    map_x,
                    map_y,
                    gradient_source,
                    edge_mode,
                ),
                VectorMode::HueSaturation => {
                    hue_sat_flow(sample_pixel(map_src, map_w, map_h, map_x, map_y, edge_mode))
                }
            };

            let out_px = if !flow.valid || radius <= 0.0 || strength <= 0.0 {
                center
            } else {
                let flow_strength = match vector_mode {
                    VectorMode::GradientDirection => 1.0,
                    VectorMode::HueSaturation => flow.strength,
                };
                let effective_radius = radius * strength * flow_strength;
                if effective_radius <= 1.0e-6 {
                    center
                } else {
                    directional_blur(
                        &src,
                        w,
                        h,
                        x as f32,
                        y as f32,
                        flow.dir_x,
                        flow.dir_y,
                        effective_radius,
                        samples,
                        edge_mode,
                    )
                }
            };

            let out_px = if preserve_alpha {
                PixelF32 {
                    red: out_px.red,
                    green: out_px.green,
                    blue: out_px.blue,
                    alpha: center.alpha,
                }
            } else {
                out_px
            };
            let out_px = sanitize_pixel(out_px);

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

fn vector_mode_from_popup(value: i32) -> VectorMode {
    match value {
        2 => VectorMode::HueSaturation,
        _ => VectorMode::GradientDirection,
    }
}

fn gradient_source_from_popup(value: i32) -> GradientSource {
    match value {
        2 => GradientSource::Red,
        3 => GradientSource::Green,
        4 => GradientSource::Blue,
        5 => GradientSource::Alpha,
        _ => GradientSource::Lightness,
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

fn directional_blur(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    dir_x: f32,
    dir_y: f32,
    radius: f32,
    samples: usize,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let mut sum = PixelF32 {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 0.0,
    };
    let mut sum_weight = 0.0f32;

    let denom = (samples.saturating_sub(1)).max(1) as f32;
    for i in 0..samples {
        let t = (i as f32 / denom) * 2.0 - 1.0;
        let weight = 1.0 - t.abs();
        let sx = x + dir_x * radius * t;
        let sy = y + dir_y * radius * t;
        let p = sample_bilinear(src, width, height, sx, sy, edge_mode);

        sum.red += p.red * weight;
        sum.green += p.green * weight;
        sum.blue += p.blue * weight;
        sum.alpha += p.alpha * weight;
        sum_weight += weight;
    }

    if sum_weight <= 1.0e-6 {
        return sample_bilinear(src, width, height, x, y, edge_mode);
    }

    PixelF32 {
        red: sum.red / sum_weight,
        green: sum.green / sum_weight,
        blue: sum.blue / sum_weight,
        alpha: sum.alpha / sum_weight,
    }
}

fn gradient_flow(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    source: GradientSource,
    edge_mode: EdgeMode,
) -> FlowVector {
    let left = sample_scalar(src, width, height, x - 1, y, source, edge_mode);
    let right = sample_scalar(src, width, height, x + 1, y, source, edge_mode);
    let up = sample_scalar(src, width, height, x, y - 1, source, edge_mode);
    let down = sample_scalar(src, width, height, x, y + 1, source, edge_mode);

    let dx = 0.5 * (right - left);
    let dy = 0.5 * (down - up);
    let mag = (dx * dx + dy * dy).sqrt();
    if !mag.is_finite() || mag <= 1.0e-6 {
        return FlowVector {
            dir_x: 0.0,
            dir_y: 0.0,
            strength: 0.0,
            valid: false,
        };
    }

    FlowVector {
        dir_x: dx / mag,
        dir_y: dy / mag,
        strength: 1.0,
        valid: true,
    }
}

fn hue_sat_flow(px: PixelF32) -> FlowVector {
    let (r, g, b) = to_unpremult_rgb(px);
    let (h, s) = rgb_to_hs(r, g, b);
    let angle = h * TAU;
    let dir_x = angle.cos();
    let dir_y = angle.sin();
    let sat = if s.is_finite() {
        s.clamp(0.0, 1.0)
    } else {
        0.0
    };

    FlowVector {
        dir_x,
        dir_y,
        strength: sat,
        valid: true,
    }
}

fn sample_scalar(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    source: GradientSource,
    edge_mode: EdgeMode,
) -> f32 {
    let px = sample_pixel(src, width, height, x, y, edge_mode);
    let (r, g, b) = to_unpremult_rgb(px);
    match source {
        GradientSource::Lightness => 0.2126 * r + 0.7152 * g + 0.0722 * b,
        GradientSource::Red => r,
        GradientSource::Green => g,
        GradientSource::Blue => b,
        GradientSource::Alpha => sanitize_non_finite(px.alpha),
    }
}

fn sample_bilinear(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
        return PixelF32 {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        };
    }

    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = sample_pixel(src, width, height, x0, y0, edge_mode);
    let p10 = sample_pixel(src, width, height, x1, y0, edge_mode);
    let p01 = sample_pixel(src, width, height, x0, y1, edge_mode);
    let p11 = sample_pixel(src, width, height, x1, y1, edge_mode);

    let top = lerp_pixel(p00, p10, tx);
    let bottom = lerp_pixel(p01, p11, tx);
    lerp_pixel(top, bottom, ty)
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
        alpha: a.alpha + (b.alpha - a.alpha) * t,
    }
}

fn sample_pixel(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let xi = resolve_coord(x, width, edge_mode);
    let yi = resolve_coord(y, height, edge_mode);
    if let (Some(xi), Some(yi)) = (xi, yi) {
        src[yi * width + xi]
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

fn remap_coord_to_layer(coord: usize, out_len: usize, map_len: usize) -> i32 {
    if out_len == 0 || map_len == 0 {
        return 0;
    }
    if out_len == map_len {
        return coord as i32;
    }
    let src = ((coord as f32 + 0.5) * map_len as f32 / out_len as f32) - 0.5;
    src.round() as i32
}

fn read_layer_rgba(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();

    let mut out = vec![
        PixelF32 {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        };
        width * height
    ];

    for y in 0..height {
        for x in 0..width {
            out[y * width + x] = read_pixel_f32(layer, world_type, x, y);
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

fn to_unpremult_rgb(px: PixelF32) -> (f32, f32, f32) {
    let a = px.alpha;
    if a > 1.0e-6 {
        (
            sanitize_non_finite(px.red / a),
            sanitize_non_finite(px.green / a),
            sanitize_non_finite(px.blue / a),
        )
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn rgb_to_hs(r: f32, g: f32, b: f32) -> (f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let sat = if max <= 1.0e-6 { 0.0 } else { delta / max };
    if delta <= 1.0e-6 {
        return (0.0, sat);
    }

    let hue_prime = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        ((b - r) / delta) + 2.0
    } else {
        ((r - g) / delta) + 4.0
    };
    let hue = (hue_prime / 6.0).rem_euclid(1.0);
    (hue, sat)
}

fn sanitize_pixel(px: PixelF32) -> PixelF32 {
    PixelF32 {
        red: sanitize_non_finite(px.red),
        green: sanitize_non_finite(px.green),
        blue: sanitize_non_finite(px.blue),
        alpha: sanitize_non_finite(px.alpha),
    }
}

#[inline]
fn sanitize_non_finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}
