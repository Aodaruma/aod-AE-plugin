#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use seq_macro::seq;
use std::collections::VecDeque;
use std::env;

use ae::pf::*;
use utils::{RgbInterpretation, ToPixel, interpreted_rgb};

mod gap_protection;
#[cfg(feature = "profile-render")]
mod render_profile;

const MAX_POINTS: usize = 32;
const MIN_POINTS: usize = 1;
const DEFAULT_POINTS: usize = 1;
const SQRT_3: f32 = 1.732_050_8;
const ALPHA_EPSILON: f32 = 1.0e-6;
const INPUT_CHECKOUT_ID: i32 = 0;
const SOURCE_CHECKOUT_ID: i32 = 1;
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

seq!(N in 1..=32 {
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ColorThreshold,
    AlphaThreshold,
    MinAlpha,
    AlphaWorkflow,
    SelectionSource,
    SourceLayer,
    Connectivity,
    PointControlsGroupStart,
    PointCount,
    AddPoint,
    RemovePoint,
    PointControlsGroupEnd,
    #(
        PointGroupStart~N,
        Point~N,
        Opacity~N,
        PointGroupEnd~N,
    )*
    StabilizePreview,
    PreviewGapRadius,
}
});

seq!(N in 1..=32 {
const POINT_GROUP_START_PARAMS: [Params; 32] = [#(Params::PointGroupStart~N,)*];
const POINT_PARAMS: [Params; 32] = [#(Params::Point~N,)*];
const OPACITY_PARAMS: [Params; 32] = [#(Params::Opacity~N,)*];
const POINT_GROUP_END_PARAMS: [Params; 32] = [#(Params::PointGroupEnd~N,)*];
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Connectivity {
    Four,
    Eight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectionSource {
    Source,
    InputLayerData,
}

#[derive(Clone, Debug)]
struct SeedPoint {
    x: f32,
    y: f32,
    opacity_scale: f32,
}

#[derive(Clone, Debug)]
struct RenderSettings {
    seed_points: Vec<SeedPoint>,
    color_threshold_sq: f32,
    alpha_threshold: f32,
    min_alpha: f32,
    selection_source: SelectionSource,
    color_interpretation: RgbInterpretation,
    connectivity: Connectivity,
    stabilize_preview: bool,
    preview_gap_radius: usize,
}

#[derive(Default)]
struct Plugin {
    // Used only for dynamic controls on the UI thread, never for rendering.
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Adjusts opacity of contiguous regions around eyedropper points with per-point controls and selectable premultiplied or straight color matching.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let supervise_flags = || {
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP
        };
        params.add(
            Params::ColorThreshold,
            "Color Threshold (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(25.0);
                d.set_default(2.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::AlphaThreshold,
            "Alpha Threshold (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(25.0);
                d.set_default(5.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::MinAlpha,
            "Region Min Alpha (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(100.0);
                d.set_default(0.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::AlphaWorkflow,
            "Color Interpretation",
            PopupDef::setup(|d| {
                d.set_options(&["Premultiplied", "Straight"]);
                d.set_default(2);
            }),
        )?;

        params.add(
            Params::SelectionSource,
            "Selection Source",
            PopupDef::setup(|d| {
                d.set_options(&["Source", "Input Layer Data"]);
                d.set_default(2);
            }),
        )?;

        params.add_with_flags(
            Params::SourceLayer,
            "Source Layer (Internal)",
            LayerDef::setup(|d| {
                d.set_default_to_this_layer();
            }),
            ae::ParamFlag::CANNOT_TIME_VARY | ae::ParamFlag::CANNOT_INTERP,
            ae::ParamUIFlags::NO_ECW_UI | ae::ParamUIFlags::INVISIBLE,
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
            Params::StabilizePreview,
            "Stabilize Reduced Resolution",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;
        params.add(
            Params::PreviewGapRadius,
            "Preview Gap Radius (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(3.0);
                d.set_slider_min(0.0);
                d.set_slider_max(3.0);
                d.set_default(1.0);
                d.set_precision(0);
            }),
        )?;

        params.add_group(
            Params::PointControlsGroupStart,
            Params::PointControlsGroupEnd,
            "Point Controls",
            false,
            |params| {
                params.add_with_flags(
                    Params::PointCount,
                    "Number of Points",
                    FloatSliderDef::setup(|d| {
                        d.set_default(DEFAULT_POINTS as f64);
                        d.set_value(DEFAULT_POINTS as f64);
                        d.set_valid_min(MIN_POINTS as f32);
                        d.set_valid_max(MAX_POINTS as f32);
                        d.set_slider_min(MIN_POINTS as f32);
                        d.set_slider_max(MAX_POINTS as f32);
                        d.set_precision(0);
                    }),
                    supervise_flags(),
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::AddPoint,
                    "Add Point",
                    ButtonDef::setup(|d| {
                        d.set_label("Add");
                    }),
                )?;

                params.add(
                    Params::RemovePoint,
                    "Remove Point",
                    ButtonDef::setup(|d| {
                        d.set_label("Remove");
                    }),
                )?;

                Ok(())
            },
        )?;

        for idx in 0..MAX_POINTS {
            params.add_group(
                POINT_GROUP_START_PARAMS[idx],
                POINT_GROUP_END_PARAMS[idx],
                &format!("Point {}", idx + 1),
                idx != 0,
                |params| {
                    params.add(
                        POINT_PARAMS[idx],
                        "Position",
                        PointDef::setup(|d| {
                            d.set_default((50.0, 50.0));
                        }),
                    )?;

                    params.add(
                        OPACITY_PARAMS[idx],
                        "Opacity (%)",
                        FloatSliderDef::setup(|d| {
                            d.set_valid_min(0.0);
                            d.set_valid_max(100.0);
                            d.set_slider_min(0.0);
                            d.set_slider_max(100.0);
                            d.set_default(0.0);
                            d.set_precision(1);
                        }),
                    )?;

                    Ok(())
                },
            )?;
        }

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
                        "AOD_EyedropperMask - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_EyedropperMask")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let settings = read_settings(params)?;
                match settings.selection_source {
                    SelectionSource::InputLayerData => {
                        self.do_render(in_data, &in_layer, &in_layer, out_layer, &settings)?;
                    }
                    SelectionSource::Source => {
                        let source_checkout = params.checkout(Params::SourceLayer)?;
                        let source_layer = source_checkout.as_layer()?.value();
                        let selection_layer = source_layer.as_ref().unwrap_or(&in_layer);
                        self.do_render(in_data, &in_layer, selection_layer, out_layer, &settings)?;
                    }
                }
            }

            ae::Command::SmartPreRender { mut extra } => {
                let settings = read_settings(params)?;
                let mut req = extra.output_request();
                req.rect.left = 0;
                req.rect.top = 0;
                req.rect.right = in_data.width().max(0);
                req.rect.bottom = in_data.height().max(0);

                if let Ok(in_result) = extra.callbacks().checkout_layer(
                    0,
                    INPUT_CHECKOUT_ID,
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

                if settings.selection_source == SelectionSource::Source {
                    let source_index = params
                        .index(Params::SourceLayer)
                        .ok_or(Error::InvalidIndex)?;
                    extra.callbacks().checkout_layer(
                        source_index as i32,
                        SOURCE_CHECKOUT_ID,
                        &req,
                        in_data.current_time(),
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                }
            }

            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let in_layer_opt = cb.checkout_layer_pixels(INPUT_CHECKOUT_ID as u32)?;
                let Some(in_layer) = in_layer_opt else {
                    cb.checkin_layer_pixels(INPUT_CHECKOUT_ID as u32)?;
                    return Ok(());
                };
                let settings = match read_settings(params) {
                    Ok(settings) => settings,
                    Err(err) => {
                        let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID as u32);
                        return Err(err);
                    }
                };
                let source_layer_opt = if settings.selection_source == SelectionSource::Source {
                    match cb.checkout_layer_pixels(SOURCE_CHECKOUT_ID as u32) {
                        Ok(layer) => layer,
                        Err(err) => {
                            let _ = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID as u32);
                            return Err(err);
                        }
                    }
                } else {
                    None
                };

                let render_result: Result<(), Error> = (|| {
                    let out_layer_opt = cb.checkout_output()?;
                    if let Some(out_layer) = out_layer_opt {
                        let selection_layer = source_layer_opt.as_ref().unwrap_or(&in_layer);
                        self.do_render(in_data, &in_layer, selection_layer, out_layer, &settings)?;
                    }
                    Ok(())
                })();

                let source_checkin_result = if settings.selection_source == SelectionSource::Source
                {
                    cb.checkin_layer_pixels(SOURCE_CHECKOUT_ID as u32)
                } else {
                    Ok(())
                };
                let input_checkin_result = cb.checkin_layer_pixels(INPUT_CHECKOUT_ID as u32);

                render_result?;
                source_checkin_result?;
                input_checkin_result?;
            }

            ae::Command::UserChangedParam { param_index } => {
                self.handle_user_changed_param(param_index, params, &mut out_data)?;
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
    fn point_count(params: &ae::Parameters<Params>) -> usize {
        params
            .get(Params::PointCount)
            .ok()
            .and_then(|p| p.as_float_slider().ok().map(|s| s.value()))
            .map(|v| v.round() as usize)
            .unwrap_or(DEFAULT_POINTS)
            .clamp(MIN_POINTS, MAX_POINTS)
    }

    fn set_point_count(params: &mut ae::Parameters<Params>, count: usize) -> Result<(), Error> {
        let clamped = count.clamp(MIN_POINTS, MAX_POINTS);
        let mut count_param = params.get_mut(Params::PointCount)?;
        count_param.as_float_slider_mut()?.set_value(clamped as f64);
        count_param.update_param_ui()?;
        Ok(())
    }

    fn handle_user_changed_param(
        &self,
        param_index: usize,
        params: &mut ae::Parameters<Params>,
        out_data: &mut OutData,
    ) -> Result<(), Error> {
        let changed = params.type_at(param_index);
        if changed != Params::PointCount
            && changed != Params::AddPoint
            && changed != Params::RemovePoint
        {
            return Ok(());
        }

        let current = Self::point_count(params);
        let next = match changed {
            Params::AddPoint => current.saturating_add(1),
            Params::RemovePoint => current.saturating_sub(1),
            _ => current,
        }
        .clamp(MIN_POINTS, MAX_POINTS);

        Self::set_point_count(params, next)?;
        out_data.set_out_flag(OutFlags::RefreshUi, true);
        Ok(())
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let count = Self::point_count(params);

        for idx in 0..MAX_POINTS {
            let visible = idx < count;
            let group_start_param = POINT_GROUP_START_PARAMS[idx];
            let group_end_param = POINT_GROUP_END_PARAMS[idx];
            let point_param = POINT_PARAMS[idx];
            let opacity_param = OPACITY_PARAMS[idx];
            self.set_param_visible(in_data, params, group_start_param, visible)?;
            self.set_param_visible(in_data, params, point_param, visible)?;
            Self::set_param_enabled(params, point_param, visible)?;
            self.set_param_visible(in_data, params, opacity_param, visible)?;
            Self::set_param_enabled(params, opacity_param, visible)?;
            self.set_param_visible(in_data, params, group_end_param, visible)?;
        }

        Self::set_param_enabled(params, Params::AddPoint, count < MAX_POINTS)?;
        Self::set_param_enabled(params, Params::RemovePoint, count > MIN_POINTS)?;

        Ok(())
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

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::DISABLED, !enabled)
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
        in_data: InData,
        in_layer: &Layer,
        selection_layer: &Layer,
        mut out_layer: Layer,
        settings: &RenderSettings,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        #[cfg(feature = "profile-render")]
        let profile_start = std::time::Instant::now();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let n = width * height;
        let in_world_type = in_layer.world_type();
        let selection_world_type = selection_layer.world_type();
        let input_origin = in_layer.origin();
        let selection_origin = selection_layer.origin();

        let mut out = Vec::with_capacity(n);
        for y in 0..height {
            if y.is_multiple_of(32) {
                in_data.interact().abort()?;
            }
            for x in 0..width {
                out.push(read_pixel_f32(in_layer, in_world_type, x, y));
            }
        }
        // Input mode already decoded these exact pixels. Copy the packed buffer
        // instead of reading, mapping and converting the host world a second time.
        let selection = if std::ptr::eq(in_layer, selection_layer) {
            out.clone()
        } else {
            let mut pixels = Vec::with_capacity(n);
            for y in 0..height {
                if y.is_multiple_of(32) {
                    in_data.interact().abort()?;
                }
                for x in 0..width {
                    pixels.push(sample_selection_pixel(
                        selection_layer,
                        selection_world_type,
                        x,
                        y,
                        input_origin,
                        selection_origin,
                    ));
                }
            }
            pixels
        };

        #[cfg(feature = "profile-render")]
        let profile_read = profile_start.elapsed();
        mask_pixels(
            &mut out,
            &selection,
            width,
            height,
            input_origin,
            settings,
            gap_protection::preview_radius(
                settings,
                in_data.downsample_x(),
                in_data.downsample_y(),
            ),
            || in_data.interact().abort(),
        )?;

        #[cfg(feature = "profile-render")]
        let profile_mask = profile_start.elapsed();
        write_output(&mut out_layer, &out, width, height, input_origin)?;
        #[cfg(feature = "profile-render")]
        render_profile::record(
            width,
            height,
            in_world_type,
            profile_read,
            profile_mask,
            profile_start.elapsed(),
        );
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn mask_pixels(
    out: &mut [PixelF32],
    selection: &[PixelF32],
    width: usize,
    height: usize,
    input_origin: Point,
    settings: &RenderSettings,
    gap_radius: usize,
    mut abort: impl FnMut() -> Result<(), Error>,
) -> Result<(), Error> {
    let n = width * height;
    let neighbors: &[(isize, isize)] = match settings.connectivity {
        Connectivity::Four => &NEIGHBORS_4,
        Connectivity::Eight => &NEIGHBORS_8,
    };

    let mut affected_mask: Vec<bool> = vec![false; n];
    let mut opacity_scale_map: Vec<f32> = vec![1.0; n];
    let mut visited_stamp: Vec<u32> = if gap_radius == 0 {
        vec![0; n]
    } else {
        Vec::new()
    };
    let mut gap_state = Vec::new();
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut next_stamp: u32 = 1;

    for seed in &settings.seed_points {
        abort()?;
        let Some(seed_idx) = point_to_buffer_index(seed.x, seed.y, input_origin, width, height)
        else {
            continue;
        };
        let seed_px = selection[seed_idx];
        if seed_px.alpha < settings.min_alpha {
            continue;
        }
        let seed_rgb = interpreted_rgb(seed_px, settings.color_interpretation);

        if gap_radius > 0 {
            gap_protection::select_region(
                selection,
                width,
                height,
                seed_idx,
                settings,
                gap_radius,
                &mut gap_state,
                &mut queue,
                &mut abort,
            )?;
            for (idx, &state) in gap_state.iter().enumerate() {
                if gap_protection::is_selected(state) {
                    assign_opacity_scale(
                        &mut affected_mask,
                        &mut opacity_scale_map,
                        idx,
                        seed.opacity_scale,
                    );
                }
            }
            continue;
        }

        if next_stamp == u32::MAX {
            visited_stamp.fill(0);
            next_stamp = 1;
        }
        let stamp = next_stamp;
        next_stamp = next_stamp.saturating_add(1);

        queue.clear();
        visited_stamp[seed_idx] = stamp;
        assign_opacity_scale(
            &mut affected_mask,
            &mut opacity_scale_map,
            seed_idx,
            seed.opacity_scale,
        );
        queue.push_back(seed_idx);

        let mut visited_count = 0usize;
        while let Some(idx) = queue.pop_front() {
            visited_count += 1;
            if visited_count.is_multiple_of(16_384) {
                abort()?;
            }
            let x = idx % width;
            let y = idx / width;
            for (dx, dy) in neighbors.iter().copied() {
                let Some(next_idx) = offset_index(x, y, width, height, dx, dy) else {
                    continue;
                };
                if visited_stamp[next_idx] == stamp {
                    continue;
                }

                visited_stamp[next_idx] = stamp;
                let next_px = selection[next_idx];
                if !is_region_pixel(next_px, seed_rgb, seed_px.alpha, settings) {
                    continue;
                }

                assign_opacity_scale(
                    &mut affected_mask,
                    &mut opacity_scale_map,
                    next_idx,
                    seed.opacity_scale,
                );
                queue.push_back(next_idx);
            }
        }
    }

    for (idx, px) in out.iter_mut().enumerate() {
        if !affected_mask[idx] {
            continue;
        }
        apply_opacity_scale(px, opacity_scale_map[idx]);
    }

    Ok(())
}

fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    let color_threshold_percent = params
        .get(Params::ColorThreshold)?
        .as_float_slider()?
        .value() as f32;
    let color_threshold = (color_threshold_percent / 100.0).clamp(0.0, 1.0) * SQRT_3;
    let color_threshold_sq = color_threshold * color_threshold;

    let alpha_threshold_percent = params
        .get(Params::AlphaThreshold)?
        .as_float_slider()?
        .value() as f32;
    let alpha_threshold = (alpha_threshold_percent / 100.0).clamp(0.0, 1.0);

    let min_alpha_percent = params.get(Params::MinAlpha)?.as_float_slider()?.value() as f32;
    let min_alpha = (min_alpha_percent / 100.0).clamp(0.0, 1.0);

    let color_interpretation = match params.get(Params::AlphaWorkflow)?.as_popup()?.value() {
        2 => RgbInterpretation::Straight,
        _ => RgbInterpretation::Premultiplied,
    };
    let selection_source = match params.get(Params::SelectionSource)?.as_popup()?.value() {
        1 => SelectionSource::Source,
        _ => SelectionSource::InputLayerData,
    };

    let connectivity = match params.get(Params::Connectivity)?.as_popup()?.value() {
        1 => Connectivity::Four,
        _ => Connectivity::Eight,
    };

    let active_points = Plugin::point_count(params);
    let mut seed_points = Vec::with_capacity(active_points);
    for idx in 0..active_points {
        let point_param = POINT_PARAMS[idx];
        let point_param_def = params.get(point_param)?;
        let point = point_param_def.as_point()?;
        let (x, y) = point_value_f32(&point);

        let opacity_percent = params.get(OPACITY_PARAMS[idx])?.as_float_slider()?.value() as f32;
        let opacity_scale = (opacity_percent / 100.0).clamp(0.0, 1.0);

        seed_points.push(SeedPoint {
            x,
            y,
            opacity_scale,
        });
    }

    Ok(RenderSettings {
        seed_points,
        color_threshold_sq,
        alpha_threshold,
        min_alpha,
        selection_source,
        color_interpretation,
        connectivity,
        stabilize_preview: params.get(Params::StabilizePreview)?.as_checkbox()?.value(),
        preview_gap_radius: params
            .get(Params::PreviewGapRadius)?
            .as_float_slider()?
            .value()
            .round()
            .clamp(0.0, 3.0) as usize,
    })
}

fn point_value_f32(point: &PointDef<'_>) -> (f32, f32) {
    match point.float_value() {
        Ok(p) => (p.x as f32, p.y as f32),
        Err(_) => point.value(),
    }
}

fn point_to_buffer_index(
    x: f32,
    y: f32,
    buffer_origin: Point,
    width: usize,
    height: usize,
) -> Option<usize> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }

    let local_x = x.round() as i64 - i64::from(buffer_origin.h);
    let local_y = y.round() as i64 - i64::from(buffer_origin.v);
    if local_x < 0 || local_y < 0 || local_x >= width as i64 || local_y >= height as i64 {
        return None;
    }

    Some(local_y as usize * width + local_x as usize)
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

fn write_output(
    out_layer: &mut Layer,
    buffer: &[PixelF32],
    width: usize,
    height: usize,
    input_origin: Point,
) -> Result<(), Error> {
    let output_origin = out_layer.origin();
    let out_world_type = out_layer.world_type();
    let progress_final = out_layer.height() as i32;

    out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
        let source_x = i64::from(x) + i64::from(output_origin.h) - i64::from(input_origin.h);
        let source_y = i64::from(y) + i64::from(output_origin.v) - i64::from(input_origin.v);
        let px = if source_x >= 0
            && source_y >= 0
            && source_x < width as i64
            && source_y < height as i64
        {
            buffer[source_y as usize * width + source_x as usize]
        } else {
            PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            }
        };
        match out_world_type {
            ae::aegp::WorldType::U8 => dst.set_from_u8(px.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(px.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(px),
        }
        Ok(())
    })?;

    Ok(())
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dr = a[0] - b[0];
    let dg = a[1] - b[1];
    let db = a[2] - b[2];
    dr * dr + dg * dg + db * db
}

fn is_region_pixel(
    candidate: PixelF32,
    seed_rgb: [f32; 3],
    seed_alpha: f32,
    settings: &RenderSettings,
) -> bool {
    if candidate.alpha < settings.min_alpha {
        return false;
    }
    if (candidate.alpha - seed_alpha).abs() > settings.alpha_threshold {
        return false;
    }

    let candidate_rgb = interpreted_rgb(candidate, settings.color_interpretation);
    distance_sq(candidate_rgb, seed_rgb) <= settings.color_threshold_sq
}

fn assign_opacity_scale(
    affected_mask: &mut [bool],
    opacity_scale_map: &mut [f32],
    idx: usize,
    opacity_scale: f32,
) {
    if affected_mask[idx] {
        opacity_scale_map[idx] = opacity_scale_map[idx].min(opacity_scale);
    } else {
        affected_mask[idx] = true;
        opacity_scale_map[idx] = opacity_scale;
    }
}

fn apply_opacity_scale(px: &mut PixelF32, opacity_scale: f32) {
    if px.alpha <= ALPHA_EPSILON {
        px.alpha = 0.0;
        px.red = 0.0;
        px.green = 0.0;
        px.blue = 0.0;
        return;
    }

    let scale = opacity_scale.clamp(0.0, 1.0);
    if scale <= ALPHA_EPSILON {
        px.alpha = 0.0;
        px.red = 0.0;
        px.green = 0.0;
        px.blue = 0.0;
        return;
    }
    if scale >= 1.0 {
        return;
    }

    px.alpha *= scale;
    px.red *= scale;
    px.green *= scale;
    px.blue *= scale;
}

fn sample_selection_pixel(
    layer: &Layer,
    world_type: ae::aegp::WorldType,
    input_x: usize,
    input_y: usize,
    input_origin: Point,
    selection_origin: Point,
) -> PixelF32 {
    let x = input_x as i32 + input_origin.h - selection_origin.h;
    let y = input_y as i32 + input_origin.v - selection_origin.v;
    if x < 0 || y < 0 || x >= layer.width() as i32 || y >= layer.height() as i32 {
        return PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
    }

    read_pixel_f32(layer, world_type, x as usize, y as usize)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_scale_keeps_output_premultiplied() {
        let mut px = PixelF32 {
            alpha: 0.5,
            red: 0.4,
            green: 0.2,
            blue: 0.1,
        };

        apply_opacity_scale(&mut px, 0.5);

        assert_eq!(px.alpha, 0.25);
        assert_eq!([px.red, px.green, px.blue], [0.2, 0.1, 0.05]);
    }

    #[test]
    fn point_position_is_mapped_from_source_to_trimmed_buffer_coordinates() {
        let index = point_to_buffer_index(125.0, 75.0, Point { h: 100, v: 50 }, 40, 40);

        assert_eq!(index, Some(25 * 40 + 25));
    }

    #[test]
    fn point_outside_trimmed_buffer_is_not_clamped_to_an_edge() {
        let index = point_to_buffer_index(25.0, 25.0, Point { h: 100, v: 50 }, 40, 40);

        assert_eq!(index, None);
    }
}
