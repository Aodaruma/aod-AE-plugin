#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;
use std::time::{Duration, Instant};

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    RegionSource,
    Tolerance,
    Mode,
    Seed,
    UseOriginalAlpha,
    GradientPoint,
    ModeRandomness,
    PositionCenterX,
    PositionCenterY,
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    RandomColor,
    PositionColor,
    IndexMaskSequential,
    IndexMaskRandom,
    IndexMaskPointDistance,
}

#[derive(Clone, Copy, Debug)]
enum RegionSource {
    Opacity,
    Color,
}

#[derive(Default, Clone, Copy)]
struct RegionInfo {
    count: u32,
    sum_x: u64,
    sum_y: u64,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Colors connected regions with random, positional, or index-based schemes.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::RegionSource,
            "Region Source",
            PopupDef::setup(|d| {
                d.set_options(&["Opacity", "Color"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::Tolerance,
            "Tolerance",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.01);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::Mode,
            "Mode",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Random Color",
                    "Position Color",
                    "Index Gradient (Sequential)",
                    "Index Gradient (Random)",
                    "Index Gradient (Point Distance)",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::Seed,
            "Seed",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(100000);
                d.set_slider_min(0);
                d.set_slider_max(1000);
                d.set_default(0);
            }),
        )?;

        params.add(
            Params::UseOriginalAlpha,
            "Use Original Alpha",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add_with_flags(
            Params::GradientPoint,
            "Gradient Point",
            PointDef::setup(|p| {
                p.set_default((0.0, 0.0));
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add(
            Params::ModeRandomness,
            "Randomness (Position/Index)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add_with_flags(
            Params::PositionCenterX,
            "Position Center X",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.5);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;

        params.add_with_flags(
            Params::PositionCenterY,
            "Position Center Y",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_default(0.5);
                d.set_precision(3);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
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
                        "AOD_RegionColorize - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_RegionColorize")
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
                if params.type_at(param_index) == Params::Mode {
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

impl Plugin {
    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let mode = mode_from_popup(params.get(Params::Mode)?.as_popup()?.value());
        let show_gradient_point = matches!(mode, Mode::IndexMaskPointDistance);
        let show_position_center = matches!(mode, Mode::PositionColor);
        let show_mode_randomness = !matches!(mode, Mode::RandomColor);

        self.set_param_visible(in_data, params, Params::GradientPoint, show_gradient_point)?;
        self.set_param_visible(
            in_data,
            params,
            Params::ModeRandomness,
            show_mode_randomness,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::PositionCenterX,
            show_position_center,
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::PositionCenterY,
            show_position_center,
        )?;
        Ok(())
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
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
        params: &mut Parameters<Params>,
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
        let profile_enabled = profiling_enabled();
        let profile_total_start = Instant::now();

        let w = in_layer.width();
        let h = in_layer.height();
        let n = w * h;
        let progress_final = out_layer.height() as i32;
        let out_world_type = out_layer.world_type();

        let mode = mode_from_popup(params.get(Params::Mode)?.as_popup()?.value());

        let region_source = match params.get(Params::RegionSource)?.as_popup()?.value() {
            2 => RegionSource::Color,
            _ => RegionSource::Opacity,
        };

        let mode_randomness = params
            .get(Params::ModeRandomness)?
            .as_float_slider()?
            .value() as f32;
        let mode_randomness = mode_randomness.clamp(0.0, 1.0);

        let gradient_point = if matches!(mode, Mode::IndexMaskPointDistance) {
            let point_param = params.get(Params::GradientPoint)?;
            let point = point_param.as_point()?;
            point_value_f32(&point)
        } else {
            (0.0, 0.0)
        };

        let position_center = if matches!(mode, Mode::PositionColor) {
            let x = params
                .get(Params::PositionCenterX)?
                .as_float_slider()?
                .value() as f32;
            let y = params
                .get(Params::PositionCenterY)?
                .as_float_slider()?
                .value() as f32;
            (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
        } else {
            (0.5, 0.5)
        };

        let seed = params.get(Params::Seed)?.as_slider()?.value().max(0) as u32;

        let threshold = params.get(Params::Tolerance)?.as_float_slider()?.value() as f32;
        let alpha_thr = threshold;
        let label_tol = threshold;
        let use_original_alpha = params.get(Params::UseOriginalAlpha)?.as_checkbox()?.value();

        let in_world_type = in_layer.world_type();
        let mut base_label: Vec<u32> = vec![0; n];
        let mut alpha_map: Option<Vec<f32>> = if use_original_alpha {
            Some(vec![1.0; n])
        } else {
            None
        };

        let read_start = Instant::now();
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let px = read_pixel_f32(&in_layer, in_world_type, x, y);
                if let Some(alpha_map) = alpha_map.as_mut() {
                    alpha_map[idx] = px.alpha;
                }
                if px.alpha < alpha_thr {
                    base_label[idx] = 0;
                    continue;
                }
                base_label[idx] = match region_source {
                    RegionSource::Opacity => 1,
                    RegionSource::Color => pack_label(px, alpha_thr, label_tol),
                };
            }
        }
        let read_elapsed = read_start.elapsed();

        let mut region_id: Vec<u32> = vec![0; n];
        let mut regions: Vec<RegionInfo> = Vec::with_capacity((n / 64).max(2));
        regions.push(RegionInfo::default());
        let mut queue: Vec<usize> = Vec::with_capacity((w.max(h)).max(64));

        let ccl_start = Instant::now();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let lbl = base_label[i];
                if lbl == 0 || region_id[i] != 0 {
                    continue;
                }

                let new_id = regions.len() as u32;
                regions.push(RegionInfo::default());
                region_id[i] = new_id;
                queue.clear();
                queue.push(i);
                let mut head = 0usize;

                while head < queue.len() {
                    let idx = queue[head];
                    head += 1;
                    let px = idx % w;
                    let py = idx / w;

                    let info = &mut regions[new_id as usize];
                    info.count = info.count.saturating_add(1);
                    info.sum_x = info.sum_x.saturating_add(px as u64);
                    info.sum_y = info.sum_y.saturating_add(py as u64);

                    if px > 0 {
                        let j = idx - 1;
                        if region_id[j] == 0 && base_label[j] == lbl {
                            region_id[j] = new_id;
                            queue.push(j);
                        }
                    }
                    if px + 1 < w {
                        let j = idx + 1;
                        if region_id[j] == 0 && base_label[j] == lbl {
                            region_id[j] = new_id;
                            queue.push(j);
                        }
                    }
                    if py > 0 {
                        let j = idx - w;
                        if region_id[j] == 0 && base_label[j] == lbl {
                            region_id[j] = new_id;
                            queue.push(j);
                        }
                    }
                    if py + 1 < h {
                        let j = idx + w;
                        if region_id[j] == 0 && base_label[j] == lbl {
                            region_id[j] = new_id;
                            queue.push(j);
                        }
                    }
                }
            }
        }
        let ccl_elapsed = ccl_start.elapsed();

        let region_count = regions.len().saturating_sub(1);
        let mut region_color: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; regions.len()];
        let color_start = Instant::now();

        match mode {
            Mode::RandomColor => {
                for (id, color) in region_color.iter_mut().enumerate().skip(1) {
                    *color = random_color(id as u32, seed);
                }
            }
            Mode::PositionColor => {
                for (id, color) in region_color.iter_mut().enumerate().skip(1) {
                    let info = regions[id];
                    if info.count == 0 {
                        continue;
                    }
                    let cx = info.sum_x as f32 / info.count as f32;
                    let cy = info.sum_y as f32 / info.count as f32;
                    let base = position_color(cx, cy, w, h, position_center);
                    let random = random_color(id as u32, seed ^ 0xD1B5_4A32);
                    *color = [
                        lerp(base[0], random[0], mode_randomness),
                        lerp(base[1], random[1], mode_randomness),
                        lerp(base[2], random[2], mode_randomness),
                    ];
                }
            }
            Mode::IndexMaskSequential | Mode::IndexMaskRandom | Mode::IndexMaskPointDistance => {
                if region_count == 0 {
                    // no regions
                } else {
                    let mut base_rank: Vec<usize> = vec![0; regions.len()];
                    match mode {
                        Mode::IndexMaskSequential => {
                            for (id, rank) in base_rank.iter_mut().enumerate().skip(1) {
                                *rank = id - 1;
                            }
                        }
                        Mode::IndexMaskRandom => {
                            let mut order: Vec<(u32, usize)> = Vec::with_capacity(region_count);
                            for id in 1..=region_count {
                                let key = hash_u32(id as u32 ^ seed ^ 0x9E37_79B9);
                                order.push((key, id));
                            }
                            order.sort_by_key(|(key, _)| *key);
                            for (rank, (_, id)) in order.iter().enumerate() {
                                base_rank[*id] = rank;
                            }
                        }
                        Mode::IndexMaskPointDistance => {
                            let mut order: Vec<(f32, usize)> = Vec::with_capacity(region_count);
                            for (id, info) in
                                regions.iter().enumerate().take(region_count + 1).skip(1)
                            {
                                if info.count == 0 {
                                    continue;
                                }
                                let cx = info.sum_x as f32 / info.count as f32 + 0.5;
                                let cy = info.sum_y as f32 / info.count as f32 + 0.5;
                                let dx = cx - gradient_point.0;
                                let dy = cy - gradient_point.1;
                                let dist2 = dx * dx + dy * dy;
                                order.push((dist2, id));
                            }

                            order.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

                            for (rank, (_, id)) in order.iter().enumerate() {
                                base_rank[*id] = rank;
                            }
                        }
                        Mode::RandomColor | Mode::PositionColor => {}
                    }

                    for id in 1..=region_count {
                        let base_t = grayscale_for_rank(base_rank[id], region_count);
                        let random_t = random01(hash_u32(id as u32 ^ seed ^ 0xA361_95A7));
                        let t = lerp(base_t, random_t, mode_randomness);
                        region_color[id] = [t, t, t];
                    }
                }
            }
        }
        let color_elapsed = color_start.elapsed();

        let write_start = Instant::now();
        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let idx = y as usize * w + x as usize;
            let id = region_id[idx] as usize;
            let mut out_px = PixelF32 {
                alpha: 1.0,
                red: region_color[id][0],
                green: region_color[id][1],
                blue: region_color[id][2],
            };

            if let Some(alpha_map) = alpha_map.as_ref() {
                let mut out_alpha = alpha_map[idx];
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
        let write_elapsed = write_start.elapsed();

        if profile_enabled {
            let total_elapsed = profile_total_start.elapsed();
            debugview_log(&format!(
                "[AOD_RegionColorize][PROFILE] {}x{} mode={:?} source={:?} regions={} read={:.3}ms ccl={:.3}ms color={:.3}ms write={:.3}ms total={:.3}ms",
                w,
                h,
                mode,
                region_source,
                region_count,
                duration_ms(read_elapsed),
                duration_ms(ccl_elapsed),
                duration_ms(color_elapsed),
                duration_ms(write_elapsed),
                duration_ms(total_elapsed),
            ));
        }

        Ok(())
    }
}

fn mode_from_popup(value: i32) -> Mode {
    match value {
        2 => Mode::PositionColor,
        3 => Mode::IndexMaskSequential,
        4 => Mode::IndexMaskRandom,
        5 => Mode::IndexMaskPointDistance,
        _ => Mode::RandomColor,
    }
}

fn point_value_f32(point: &PointDef<'_>) -> (f32, f32) {
    match point.float_value() {
        Ok(p) => (p.x as f32, p.y as f32),
        Err(_) => point.value(),
    }
}

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

fn random_color(id: u32, seed: u32) -> [f32; 3] {
    let h = hash_u32(id ^ seed);
    let r = 0.2 + 0.8 * (((h & 0xff) as f32) / 255.0);
    let g = 0.2 + 0.8 * ((((h >> 8) & 0xff) as f32) / 255.0);
    let b = 0.2 + 0.8 * ((((h >> 16) & 0xff) as f32) / 255.0);
    [r, g, b]
}

fn position_color(x: f32, y: f32, w: usize, h: usize, center: (f32, f32)) -> [f32; 3] {
    let nx = if w > 1 { (x + 0.5) / w as f32 } else { 0.0 };
    let ny = if h > 1 { (y + 0.5) / h as f32 } else { 0.0 };
    let sx = (nx - center.0 + 0.5).clamp(0.0, 1.0);
    let sy = (ny - center.1 + 0.5).clamp(0.0, 1.0);
    [sx, sy, 0.0]
}

fn grayscale_for_rank(rank: usize, count: usize) -> f32 {
    if count <= 1 {
        1.0
    } else {
        (rank as f32) / (count.saturating_sub(1) as f32)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn random01(h: u32) -> f32 {
    h as f32 / u32::MAX as f32
}

fn duration_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn profiling_enabled() -> bool {
    cfg!(debug_assertions)
}

#[cfg(target_os = "windows")]
fn debugview_log(message: &str) {
    use std::ffi::CString;
    use std::os::raw::c_char;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn OutputDebugStringA(lp_output_string: *const c_char);
    }

    let mut line = String::with_capacity(message.len() + 1);
    line.push_str(message);
    line.push('\n');
    if let Ok(c) = CString::new(line) {
        // SAFETY: CString guarantees a valid, null-terminated pointer.
        unsafe {
            OutputDebugStringA(c.as_ptr());
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn debugview_log(_message: &str) {}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}
