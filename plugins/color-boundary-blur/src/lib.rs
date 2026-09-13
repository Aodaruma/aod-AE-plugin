#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use seq_macro::seq;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const MAX_COLORS: usize = 16;
const MIN_COLORS: usize = 2;
const DEFAULT_COLORS: usize = 2;
const RGB_DIAGONAL: f32 = 1.732_050_8;
const ALPHA_EPSILON: f32 = 1.0e-6;

seq!(N in 1..=16 {
    #[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
    enum Params {
        ColorTolerance,
        MinimumAlpha,
        Proximity,
        ColorCount,
        AddColor,
        RemoveColor,
        #(Color~N,)*
        BlurMode,
        BlurRadius,
        Samples,
        TangentAmount,
        PostSmooth,
        BoundaryWidth,
        MaskFeather,
        Mix,
        EdgeMode,
        PreserveAlpha,
    }
});

seq!(N in 1..=16 {
    const COLOR_PARAMS: [Params; 16] = [#(Params::Color~N,)*];
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlurMode {
    Box,
    Gaussian,
    BoundaryNormal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EdgeMode {
    Repeat,
    Mirror,
    Transparent,
}

#[derive(Clone, Copy)]
struct Direction {
    x: f32,
    y: f32,
    valid: bool,
}

struct RenderSettings {
    selected_colors: Vec<[f32; 3]>,
    color_tolerance_sq: f32,
    minimum_alpha: f32,
    proximity: usize,
    blur_mode: BlurMode,
    blur_radius: f32,
    samples: usize,
    tangent_amount: f32,
    post_smooth: f32,
    boundary_width: usize,
    mask_feather: f32,
    mix_amount: f32,
    edge_mode: EdgeMode,
    preserve_alpha: bool,
}

struct BoundaryField {
    mask: Vec<f32>,
    orientation_cos2: Vec<f32>,
    orientation_sin2: Vec<f32>,
    orientation_weight: Vec<f32>,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Blurs boundaries where selected colors meet using isotropic or boundary-normal kernels.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::ColorTolerance,
            "Color Tolerance (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(25.0);
                d.set_default(5.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::MinimumAlpha,
            "Minimum Alpha (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(25.0);
                d.set_default(1.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Proximity,
            "Color Proximity (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(64);
                d.set_slider_min(1);
                d.set_slider_max(16);
                d.set_default(2);
            }),
        )?;

        params.add_with_flags(
            Params::ColorCount,
            "Number of Colors",
            FloatSliderDef::setup(|d| {
                d.set_default(DEFAULT_COLORS as f64);
                d.set_value(DEFAULT_COLORS as f64);
                d.set_valid_min(MIN_COLORS as f32);
                d.set_valid_max(MAX_COLORS as f32);
                d.set_slider_min(MIN_COLORS as f32);
                d.set_slider_max(MAX_COLORS as f32);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE | ParamFlag::CANNOT_TIME_VARY | ParamFlag::CANNOT_INTERP,
            ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::AddColor,
            "Add Color",
            ButtonDef::setup(|d| {
                d.set_label("Add");
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::RemoveColor,
            "Remove Color",
            ButtonDef::setup(|d| {
                d.set_label("Remove");
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;

        for (idx, param) in COLOR_PARAMS.iter().copied().enumerate() {
            let default = default_color(idx);
            params.add(
                param,
                &format!("Color {}", idx + 1),
                ColorDef::setup(move |d| {
                    d.set_default(default);
                }),
            )?;
        }

        params.add_with_flags(
            Params::BlurMode,
            "Blur Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Box", "Gaussian", "Boundary Normal"]);
                d.set_default(2);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;

        params.add(
            Params::BlurRadius,
            "Blur Radius (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(512.0);
                d.set_slider_min(0.0);
                d.set_slider_max(64.0);
                d.set_default(8.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Samples,
            "Normal Samples",
            SliderDef::setup(|d| {
                d.set_valid_min(3);
                d.set_valid_max(129);
                d.set_slider_min(3);
                d.set_slider_max(65);
                d.set_default(17);
            }),
        )?;

        params.add(
            Params::TangentAmount,
            "Along-Boundary Radius (%)",
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
            Params::PostSmooth,
            "Post Smooth (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(128.0);
                d.set_slider_min(0.0);
                d.set_slider_max(16.0);
                d.set_default(1.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::BoundaryWidth,
            "Boundary Width (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(256.0);
                d.set_slider_min(0.0);
                d.set_slider_max(32.0);
                d.set_default(2.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::MaskFeather,
            "Mask Feather (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(256.0);
                d.set_slider_min(0.0);
                d.set_slider_max(32.0);
                d.set_default(2.0);
                d.set_precision(2);
            }),
        )?;

        params.add(
            Params::Mix,
            "Mix (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(100.0);
                d.set_default(100.0);
                d.set_precision(1);
            }),
        )?;

        params.add(
            Params::EdgeMode,
            "Edge Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Repeat", "Mirror", "Transparent"]);
                d.set_default(1);
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
                        "AOD_ColorBoundaryBlur - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ColorBoundaryBlur")
                {
                    self.aegp_id = Some(plugin_id);
                }
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
    fn color_count(params: &ae::Parameters<Params>) -> usize {
        params
            .get(Params::ColorCount)
            .ok()
            .and_then(|p| p.as_float_slider().ok().map(|s| s.value()))
            .map(|v| v.round() as usize)
            .unwrap_or(DEFAULT_COLORS)
            .clamp(MIN_COLORS, MAX_COLORS)
    }

    fn set_color_count(params: &mut ae::Parameters<Params>, count: usize) -> Result<(), Error> {
        let clamped = count.clamp(MIN_COLORS, MAX_COLORS);
        let mut count_param = params.get_mut(Params::ColorCount)?;
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
        if changed == Params::AddColor || changed == Params::RemoveColor {
            let current = Self::color_count(params);
            let next = match changed {
                Params::AddColor => current.saturating_add(1),
                Params::RemoveColor => current.saturating_sub(1),
                _ => current,
            };
            Self::set_color_count(params, next)?;
        } else if changed != Params::ColorCount && changed != Params::BlurMode {
            return Ok(());
        }

        out_data.set_out_flag(OutFlags::RefreshUi, true);
        Ok(())
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let color_count = Self::color_count(params);
        for (idx, param) in COLOR_PARAMS.iter().copied().enumerate() {
            self.set_param_visible(in_data, params, param, idx < color_count)?;
        }
        Self::set_param_enabled(params, Params::AddColor, color_count < MAX_COLORS)?;
        Self::set_param_enabled(params, Params::RemoveColor, color_count > MIN_COLORS)?;

        let blur_mode = blur_mode_from_popup(params.get(Params::BlurMode)?.as_popup()?.value());
        let directional = blur_mode == BlurMode::BoundaryNormal;
        Self::set_param_enabled(params, Params::Samples, directional)?;
        Self::set_param_enabled(params, Params::TangentAmount, directional)?;
        Self::set_param_enabled(params, Params::PostSmooth, directional)?;
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
            return Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible);
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

        Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ParamUIFlags::DISABLED, !enabled)
    }

    fn set_param_ui_flag(
        params: &mut ae::Parameters<Params>,
        id: Params,
        flag: ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let current_status = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current_status == status {
            return Ok(());
        }
        let mut param = params.get_mut(id)?;
        param.set_ui_flag(flag, status);
        param.update_param_ui()?;
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

        let settings = read_settings(params)?;
        let src = read_layer_rgba(&in_layer);
        if settings.blur_radius <= ALPHA_EPSILON || settings.mix_amount <= ALPHA_EPSILON {
            return write_output(&mut out_layer, &src);
        }

        let labels = classify_pixels(
            &src,
            &settings.selected_colors,
            settings.color_tolerance_sq,
            settings.minimum_alpha,
        );
        let field = detect_boundaries(&labels, width, height, settings.proximity);
        if !field.mask.iter().any(|value| *value > 0.0) {
            return write_output(&mut out_layer, &src);
        }

        let dilated_mask = dilate_binary_mask(&field.mask, width, height, settings.boundary_width);
        let mask = gaussian_blur_scalar(&dilated_mask, width, height, settings.mask_feather);

        let blur_source = if settings.preserve_alpha {
            src.iter()
                .map(|pixel| {
                    let rgb = straight_rgb(*pixel);
                    PixelF32 {
                        alpha: pixel.alpha,
                        red: rgb[0],
                        green: rgb[1],
                        blue: rgb[2],
                    }
                })
                .collect::<Vec<_>>()
        } else {
            src.clone()
        };

        let blurred = match settings.blur_mode {
            BlurMode::Box => box_blur_pixels(
                &blur_source,
                width,
                height,
                settings.blur_radius,
                settings.edge_mode,
            ),
            BlurMode::Gaussian => gaussian_blur_pixels(
                &blur_source,
                width,
                height,
                settings.blur_radius,
                settings.edge_mode,
            ),
            BlurMode::BoundaryNormal => {
                let directions = build_direction_field(
                    &src,
                    &field,
                    width,
                    height,
                    settings.boundary_width,
                    settings.mask_feather,
                    settings.edge_mode,
                );
                let mut directional = directional_blur_pass(
                    &blur_source,
                    &directions,
                    width,
                    height,
                    settings.blur_radius,
                    settings.samples,
                    false,
                    settings.edge_mode,
                );
                let tangent_radius = settings.blur_radius * settings.tangent_amount;
                if tangent_radius > ALPHA_EPSILON {
                    directional = directional_blur_pass(
                        &directional,
                        &directions,
                        width,
                        height,
                        tangent_radius,
                        settings.samples,
                        true,
                        settings.edge_mode,
                    );
                }
                if settings.post_smooth > ALPHA_EPSILON {
                    directional = gaussian_blur_pixels(
                        &directional,
                        width,
                        height,
                        settings.post_smooth,
                        settings.edge_mode,
                    );
                }
                directional
            }
        };

        let mut output = Vec::with_capacity(src.len());
        for idx in 0..src.len() {
            let amount = (mask[idx] * settings.mix_amount).clamp(0.0, 1.0);
            let mut blurred_pixel = blurred[idx];
            if settings.preserve_alpha {
                blurred_pixel.alpha = src[idx].alpha;
                blurred_pixel.red *= src[idx].alpha;
                blurred_pixel.green *= src[idx].alpha;
                blurred_pixel.blue *= src[idx].alpha;
            }
            let mut pixel = lerp_pixel(src[idx], blurred_pixel, amount);
            if settings.preserve_alpha {
                pixel.alpha = src[idx].alpha;
            }
            output.push(sanitize_pixel(pixel));
        }

        write_output(&mut out_layer, &output)
    }
}

fn default_color(index: usize) -> Pixel8 {
    const PALETTE: [[u8; 3]; MAX_COLORS] = [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 255],
        [0, 255, 0],
        [255, 0, 255],
        [0, 0, 255],
        [255, 255, 0],
        [128, 128, 128],
        [255, 128, 0],
        [128, 0, 255],
        [0, 128, 255],
        [128, 255, 0],
        [255, 0, 128],
        [0, 255, 128],
        [128, 64, 0],
    ];
    let rgb = PALETTE[index.min(MAX_COLORS - 1)];
    Pixel8 {
        alpha: 255,
        red: rgb[0],
        green: rgb[1],
        blue: rgb[2],
    }
}

fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    let tolerance_percent = params
        .get(Params::ColorTolerance)?
        .as_float_slider()?
        .value() as f32;
    let color_tolerance = (tolerance_percent / 100.0).clamp(0.0, 1.0) * RGB_DIAGONAL;

    let minimum_alpha_percent = params.get(Params::MinimumAlpha)?.as_float_slider()?.value() as f32;

    let color_count = Plugin::color_count(params);
    let mut selected_colors = Vec::with_capacity(color_count);
    for param in COLOR_PARAMS.iter().take(color_count).copied() {
        let color = params.get(param)?.as_color()?.value().to_pixel32();
        selected_colors.push([color.red, color.green, color.blue]);
    }

    let samples = params.get(Params::Samples)?.as_slider()?.value().max(3) as usize;
    let samples = if samples.is_multiple_of(2) {
        samples + 1
    } else {
        samples
    };

    Ok(RenderSettings {
        selected_colors,
        color_tolerance_sq: color_tolerance * color_tolerance,
        minimum_alpha: (minimum_alpha_percent / 100.0).clamp(0.0, 1.0),
        proximity: params.get(Params::Proximity)?.as_slider()?.value().max(1) as usize,
        blur_mode: blur_mode_from_popup(params.get(Params::BlurMode)?.as_popup()?.value()),
        blur_radius: params
            .get(Params::BlurRadius)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        samples,
        tangent_amount: (params
            .get(Params::TangentAmount)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        post_smooth: params
            .get(Params::PostSmooth)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        boundary_width: params
            .get(Params::BoundaryWidth)?
            .as_float_slider()?
            .value()
            .round()
            .max(0.0) as usize,
        mask_feather: params
            .get(Params::MaskFeather)?
            .as_float_slider()?
            .value()
            .max(0.0) as f32,
        mix_amount: (params.get(Params::Mix)?.as_float_slider()?.value() as f32 / 100.0)
            .clamp(0.0, 1.0),
        edge_mode: edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value()),
        preserve_alpha: params.get(Params::PreserveAlpha)?.as_checkbox()?.value(),
    })
}

fn blur_mode_from_popup(value: i32) -> BlurMode {
    match value {
        1 => BlurMode::Box,
        3 => BlurMode::BoundaryNormal,
        _ => BlurMode::Gaussian,
    }
}

fn edge_mode_from_popup(value: i32) -> EdgeMode {
    match value {
        2 => EdgeMode::Mirror,
        3 => EdgeMode::Transparent,
        _ => EdgeMode::Repeat,
    }
}

fn classify_pixels(
    src: &[PixelF32],
    selected_colors: &[[f32; 3]],
    tolerance_sq: f32,
    minimum_alpha: f32,
) -> Vec<i16> {
    src.iter()
        .map(|pixel| {
            if pixel.alpha < minimum_alpha || pixel.alpha <= ALPHA_EPSILON {
                return -1;
            }
            let rgb = straight_rgb(*pixel);
            let mut best_label = -1;
            let mut best_distance = f32::INFINITY;
            for (label, color) in selected_colors.iter().enumerate() {
                let distance = rgb_distance_sq(rgb, *color);
                if distance < best_distance {
                    best_distance = distance;
                    best_label = label as i16;
                }
            }
            if best_distance <= tolerance_sq {
                best_label
            } else {
                -1
            }
        })
        .collect()
}

fn detect_boundaries(
    labels: &[i16],
    width: usize,
    height: usize,
    proximity: usize,
) -> BoundaryField {
    let n = width * height;
    let mut field = BoundaryField {
        mask: vec![0.0; n],
        orientation_cos2: vec![0.0; n],
        orientation_sin2: vec![0.0; n],
        orientation_weight: vec![0.0; n],
    };
    let directions = [(1isize, 0isize), (0, 1), (1, 1), (-1, 1)];

    for y in 0..height {
        for x in 0..width {
            let label = labels[y * width + x];
            if label < 0 {
                continue;
            }
            for (dx, dy) in directions {
                for distance in 1..=proximity.max(1) {
                    let nx = x as isize + dx * distance as isize;
                    let ny = y as isize + dy * distance as isize;
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                        break;
                    }
                    let other_label = labels[ny as usize * width + nx as usize];
                    if other_label == label {
                        break;
                    }
                    if other_label < 0 {
                        continue;
                    }

                    mark_boundary_line(&mut field, width, height, x, y, dx, dy, distance);
                    break;
                }
            }
        }
    }
    field
}

#[allow(clippy::too_many_arguments)]
fn mark_boundary_line(
    field: &mut BoundaryField,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    dx: isize,
    dy: isize,
    distance: usize,
) {
    let length = ((dx * dx + dy * dy) as f32).sqrt().max(ALPHA_EPSILON);
    let normal_x = dx as f32 / length;
    let normal_y = dy as f32 / length;
    let cos2 = normal_x * normal_x - normal_y * normal_y;
    let sin2 = 2.0 * normal_x * normal_y;

    for step in 0..=distance {
        let px = x as isize + dx * step as isize;
        let py = y as isize + dy * step as isize;
        if px < 0 || py < 0 || px >= width as isize || py >= height as isize {
            continue;
        }
        let idx = py as usize * width + px as usize;
        field.mask[idx] = 1.0;
        field.orientation_cos2[idx] += cos2;
        field.orientation_sin2[idx] += sin2;
        field.orientation_weight[idx] += 1.0;
    }
}

fn build_direction_field(
    src: &[PixelF32],
    field: &BoundaryField,
    width: usize,
    height: usize,
    boundary_width: usize,
    mask_feather: f32,
    edge_mode: EdgeMode,
) -> Vec<Direction> {
    let propagation_radius = boundary_width
        .saturating_add(mask_feather.ceil() as usize)
        .saturating_add(2);
    let cos2 = box_blur_scalar(&field.orientation_cos2, width, height, propagation_radius);
    let sin2 = box_blur_scalar(&field.orientation_sin2, width, height, propagation_radius);
    let weight = box_blur_scalar(&field.orientation_weight, width, height, propagation_radius);

    (0..width * height)
        .map(|idx| {
            if weight[idx] <= ALPHA_EPSILON {
                return Direction {
                    x: 1.0,
                    y: 0.0,
                    valid: false,
                };
            }
            let magnitude_sq = cos2[idx] * cos2[idx] + sin2[idx] * sin2[idx];
            if magnitude_sq <= 1.0e-8 {
                let x = idx % width;
                let y = idx / width;
                return structure_tensor_normal(src, width, height, x, y, edge_mode);
            }
            let angle = 0.5 * sin2[idx].atan2(cos2[idx]);
            Direction {
                x: angle.cos(),
                y: angle.sin(),
                valid: true,
            }
        })
        .collect()
}

fn structure_tensor_normal(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    edge_mode: EdgeMode,
) -> Direction {
    let left = straight_rgb(sample_pixel_integer(
        src,
        width,
        height,
        x as i32 - 1,
        y as i32,
        edge_mode,
    ));
    let right = straight_rgb(sample_pixel_integer(
        src,
        width,
        height,
        x as i32 + 1,
        y as i32,
        edge_mode,
    ));
    let up = straight_rgb(sample_pixel_integer(
        src,
        width,
        height,
        x as i32,
        y as i32 - 1,
        edge_mode,
    ));
    let down = straight_rgb(sample_pixel_integer(
        src,
        width,
        height,
        x as i32,
        y as i32 + 1,
        edge_mode,
    ));

    let gx = [
        (right[0] - left[0]) * 0.5,
        (right[1] - left[1]) * 0.5,
        (right[2] - left[2]) * 0.5,
    ];
    let gy = [
        (down[0] - up[0]) * 0.5,
        (down[1] - up[1]) * 0.5,
        (down[2] - up[2]) * 0.5,
    ];
    let jxx = dot_rgb(gx, gx);
    let jyy = dot_rgb(gy, gy);
    let jxy = dot_rgb(gx, gy);
    if jxx + jyy <= 1.0e-8 {
        return Direction {
            x: 1.0,
            y: 0.0,
            valid: false,
        };
    }
    let angle = 0.5 * (2.0 * jxy).atan2(jxx - jyy);
    Direction {
        x: angle.cos(),
        y: angle.sin(),
        valid: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn directional_blur_pass(
    src: &[PixelF32],
    directions: &[Direction],
    width: usize,
    height: usize,
    radius: f32,
    samples: usize,
    tangent: bool,
    edge_mode: EdgeMode,
) -> Vec<PixelF32> {
    if radius <= ALPHA_EPSILON {
        return src.to_vec();
    }
    let sample_count = samples.max(3);
    let denominator = (sample_count - 1) as f32;
    let mut output = Vec::with_capacity(src.len());

    for idx in 0..src.len() {
        let direction = directions[idx];
        if !direction.valid {
            output.push(src[idx]);
            continue;
        }
        let (direction_x, direction_y) = if tangent {
            (-direction.y, direction.x)
        } else {
            (direction.x, direction.y)
        };
        let x = (idx % width) as f32;
        let y = (idx / width) as f32;
        let mut sum = zero_pixel();
        let mut weight_sum = 0.0f32;
        for sample in 0..sample_count {
            let t = sample as f32 / denominator * 2.0 - 1.0;
            let gaussian_position = t * 3.0;
            let weight = (-0.5 * gaussian_position * gaussian_position).exp();
            let pixel = sample_pixel_bilinear(
                src,
                width,
                height,
                x + direction_x * radius * t,
                y + direction_y * radius * t,
                edge_mode,
            );
            add_weighted_pixel(&mut sum, pixel, weight);
            weight_sum += weight;
        }
        output.push(scale_pixel(sum, weight_sum.recip()));
    }
    output
}

fn box_blur_pixels(
    src: &[PixelF32],
    width: usize,
    height: usize,
    radius: f32,
    edge_mode: EdgeMode,
) -> Vec<PixelF32> {
    let radius = radius.ceil().max(0.0) as i32;
    if radius == 0 {
        return src.to_vec();
    }
    let window = (radius * 2 + 1) as f32;
    let mut horizontal = vec![zero_pixel(); src.len()];
    for y in 0..height {
        let mut sum = zero_pixel();
        for offset in -radius..=radius {
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(src, width, height, offset, y as i32, edge_mode),
                1.0,
            );
        }
        horizontal[y * width] = scale_pixel(sum, window.recip());
        for x in 1..width {
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(src, width, height, x as i32 + radius, y as i32, edge_mode),
                1.0,
            );
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(
                    src,
                    width,
                    height,
                    x as i32 - radius - 1,
                    y as i32,
                    edge_mode,
                ),
                -1.0,
            );
            horizontal[y * width + x] = scale_pixel(sum, window.recip());
        }
    }

    let mut output = vec![zero_pixel(); src.len()];
    for x in 0..width {
        let mut sum = zero_pixel();
        for offset in -radius..=radius {
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(&horizontal, width, height, x as i32, offset, edge_mode),
                1.0,
            );
        }
        output[x] = scale_pixel(sum, window.recip());
        for y in 1..height {
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(
                    &horizontal,
                    width,
                    height,
                    x as i32,
                    y as i32 + radius,
                    edge_mode,
                ),
                1.0,
            );
            add_weighted_pixel(
                &mut sum,
                sample_pixel_integer(
                    &horizontal,
                    width,
                    height,
                    x as i32,
                    y as i32 - radius - 1,
                    edge_mode,
                ),
                -1.0,
            );
            output[y * width + x] = scale_pixel(sum, window.recip());
        }
    }
    output
}

fn gaussian_blur_pixels(
    src: &[PixelF32],
    width: usize,
    height: usize,
    radius: f32,
    edge_mode: EdgeMode,
) -> Vec<PixelF32> {
    if radius > 8.0 {
        let mut output = src.to_vec();
        for box_radius in gaussian_box_radii(radius) {
            output = box_blur_pixels(&output, width, height, box_radius as f32, edge_mode);
        }
        return output;
    }

    let (kernel, kernel_radius) = gaussian_kernel(radius);
    if kernel_radius == 0 {
        return src.to_vec();
    }
    let mut horizontal = vec![zero_pixel(); src.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = zero_pixel();
            for (kernel_idx, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_idx as i32 - kernel_radius;
                let pixel = sample_pixel_integer(
                    src,
                    width,
                    height,
                    x as i32 + offset,
                    y as i32,
                    edge_mode,
                );
                add_weighted_pixel(&mut sum, pixel, weight);
            }
            horizontal[y * width + x] = sum;
        }
    }

    let mut output = vec![zero_pixel(); src.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = zero_pixel();
            for (kernel_idx, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_idx as i32 - kernel_radius;
                let pixel = sample_pixel_integer(
                    &horizontal,
                    width,
                    height,
                    x as i32,
                    y as i32 + offset,
                    edge_mode,
                );
                add_weighted_pixel(&mut sum, pixel, weight);
            }
            output[y * width + x] = sum;
        }
    }
    output
}

fn gaussian_kernel(radius: f32) -> (Vec<f32>, i32) {
    let kernel_radius = radius.ceil().max(0.0) as i32;
    if kernel_radius == 0 {
        return (vec![1.0], 0);
    }
    let sigma = (radius / 3.0).max(0.35);
    let mut kernel = Vec::with_capacity((kernel_radius * 2 + 1) as usize);
    let mut sum = 0.0f32;
    for offset in -kernel_radius..=kernel_radius {
        let x = offset as f32;
        let weight = (-0.5 * x * x / (sigma * sigma)).exp();
        kernel.push(weight);
        sum += weight;
    }
    for weight in &mut kernel {
        *weight /= sum;
    }
    (kernel, kernel_radius)
}

fn gaussian_blur_scalar(src: &[f32], width: usize, height: usize, radius: f32) -> Vec<f32> {
    if radius > 8.0 {
        let mut output = src.to_vec();
        for box_radius in gaussian_box_radii(radius) {
            output = box_blur_scalar(&output, width, height, box_radius);
        }
        return output;
    }

    let (kernel, kernel_radius) = gaussian_kernel(radius);
    if kernel_radius == 0 {
        return src.to_vec();
    }
    let mut horizontal = vec![0.0f32; src.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (kernel_idx, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_idx as i32 - kernel_radius;
                let sx = (x as i32 + offset).clamp(0, width as i32 - 1) as usize;
                sum += src[y * width + sx] * weight;
            }
            horizontal[y * width + x] = sum;
        }
    }
    let mut output = vec![0.0f32; src.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            for (kernel_idx, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_idx as i32 - kernel_radius;
                let sy = (y as i32 + offset).clamp(0, height as i32 - 1) as usize;
                sum += horizontal[sy * width + x] * weight;
            }
            output[y * width + x] = sum.clamp(0.0, 1.0);
        }
    }
    output
}

fn gaussian_box_radii(radius: f32) -> [usize; 3] {
    const PASSES: usize = 3;
    let sigma = (radius / 3.0).max(0.01);
    let ideal_width = (12.0 * sigma * sigma / PASSES as f32 + 1.0).sqrt();
    let mut lower_width = ideal_width.floor() as i32;
    if lower_width % 2 == 0 {
        lower_width -= 1;
    }
    lower_width = lower_width.max(1);
    let upper_width = lower_width + 2;
    let lower = lower_width as f32;
    let pass_count = PASSES as f32;
    let lower_passes = ((12.0 * sigma * sigma
        - pass_count * lower * lower
        - 4.0 * pass_count * lower
        - 3.0 * pass_count)
        / (-4.0 * lower - 4.0))
        .round()
        .clamp(0.0, pass_count) as usize;

    std::array::from_fn(|idx| {
        let width = if idx < lower_passes {
            lower_width
        } else {
            upper_width
        };
        ((width - 1) / 2) as usize
    })
}

fn box_blur_scalar(src: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return src.to_vec();
    }
    let mut horizontal = vec![0.0f32; src.len()];
    let window = radius * 2 + 1;
    for y in 0..height {
        let mut sum = 0.0;
        for offset in 0..window {
            let x = (offset as isize - radius as isize).clamp(0, width as isize - 1) as usize;
            sum += src[y * width + x];
        }
        horizontal[y * width] = sum / window as f32;
        for x in 1..width {
            let add_x = (x + radius).min(width - 1);
            let remove_x = x.saturating_sub(radius + 1);
            sum += src[y * width + add_x] - src[y * width + remove_x];
            horizontal[y * width + x] = sum / window as f32;
        }
    }

    let mut output = vec![0.0f32; src.len()];
    for x in 0..width {
        let mut sum = 0.0;
        for offset in 0..window {
            let y = (offset as isize - radius as isize).clamp(0, height as isize - 1) as usize;
            sum += horizontal[y * width + x];
        }
        output[x] = sum / window as f32;
        for y in 1..height {
            let add_y = (y + radius).min(height - 1);
            let remove_y = y.saturating_sub(radius + 1);
            sum += horizontal[add_y * width + x] - horizontal[remove_y * width + x];
            output[y * width + x] = sum / window as f32;
        }
    }
    output
}

fn dilate_binary_mask(src: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return src.to_vec();
    }
    let mut horizontal = vec![0u8; src.len()];
    let mut prefix = vec![0usize; width + 1];
    for y in 0..height {
        prefix[0] = 0;
        for x in 0..width {
            prefix[x + 1] = prefix[x] + usize::from(src[y * width + x] > 0.0);
        }
        for x in 0..width {
            let left = x.saturating_sub(radius);
            let right = x.saturating_add(radius).min(width - 1);
            horizontal[y * width + x] = u8::from(prefix[right + 1] > prefix[left]);
        }
    }

    let mut output = vec![0.0f32; src.len()];
    prefix.resize(height + 1, 0);
    for x in 0..width {
        prefix[0] = 0;
        for y in 0..height {
            prefix[y + 1] = prefix[y] + horizontal[y * width + x] as usize;
        }
        for y in 0..height {
            let top = y.saturating_sub(radius);
            let bottom = y.saturating_add(radius).min(height - 1);
            output[y * width + x] = f32::from(prefix[bottom + 1] > prefix[top]);
        }
    }
    output
}

fn sample_pixel_bilinear(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = sample_pixel_integer(src, width, height, x0, y0, edge_mode);
    let p10 = sample_pixel_integer(src, width, height, x0 + 1, y0, edge_mode);
    let p01 = sample_pixel_integer(src, width, height, x0, y0 + 1, edge_mode);
    let p11 = sample_pixel_integer(src, width, height, x0 + 1, y0 + 1, edge_mode);
    lerp_pixel(lerp_pixel(p00, p10, tx), lerp_pixel(p01, p11, tx), ty)
}

fn sample_pixel_integer(
    src: &[PixelF32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    let x = resolve_coord(x, width, edge_mode);
    let y = resolve_coord(y, height, edge_mode);
    if let (Some(x), Some(y)) = (x, y) {
        src[y * width + x]
    } else {
        zero_pixel()
    }
}

fn resolve_coord(coord: i32, length: usize, edge_mode: EdgeMode) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let length = length as i32;
    match edge_mode {
        EdgeMode::Transparent => {
            if coord < 0 || coord >= length {
                None
            } else {
                Some(coord as usize)
            }
        }
        EdgeMode::Repeat => Some(coord.clamp(0, length - 1) as usize),
        EdgeMode::Mirror => Some(mirror_index(coord, length) as usize),
    }
}

fn mirror_index(coord: i32, length: i32) -> i32 {
    if length <= 1 {
        return 0;
    }
    let period = length * 2 - 2;
    let wrapped = coord.rem_euclid(period);
    if wrapped < length {
        wrapped
    } else {
        period - wrapped
    }
}

fn read_layer_rgba(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut output = vec![zero_pixel(); width * height];
    for y in 0..height {
        for x in 0..width {
            output[y * width + x] = match world_type {
                ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
                ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
            };
        }
    }
    output
}

fn write_output(out_layer: &mut Layer, pixels: &[PixelF32]) -> Result<(), Error> {
    let width = out_layer.width();
    let world_type = out_layer.world_type();
    let progress_final = out_layer.height() as i32;
    out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
        let pixel = pixels[y as usize * width + x as usize];
        match world_type {
            ae::aegp::WorldType::U8 => dst.set_from_u8(pixel.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(pixel.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(pixel),
        }
        Ok(())
    })?;
    Ok(())
}

fn straight_rgb(pixel: PixelF32) -> [f32; 3] {
    if pixel.alpha > ALPHA_EPSILON {
        [
            pixel.red / pixel.alpha,
            pixel.green / pixel.alpha,
            pixel.blue / pixel.alpha,
        ]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn rgb_distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let difference = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot_rgb(difference, difference)
}

fn dot_rgb(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn zero_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn add_weighted_pixel(sum: &mut PixelF32, pixel: PixelF32, weight: f32) {
    sum.alpha += pixel.alpha * weight;
    sum.red += pixel.red * weight;
    sum.green += pixel.green * weight;
    sum.blue += pixel.blue * weight;
}

fn scale_pixel(pixel: PixelF32, scale: f32) -> PixelF32 {
    PixelF32 {
        alpha: pixel.alpha * scale,
        red: pixel.red * scale,
        green: pixel.green * scale,
        blue: pixel.blue * scale,
    }
}

fn lerp_pixel(a: PixelF32, b: PixelF32, amount: f32) -> PixelF32 {
    PixelF32 {
        alpha: a.alpha + (b.alpha - a.alpha) * amount,
        red: a.red + (b.red - a.red) * amount,
        green: a.green + (b.green - a.green) * amount,
        blue: a.blue + (b.blue - a.blue) * amount,
    }
}

fn sanitize_pixel(pixel: PixelF32) -> PixelF32 {
    PixelF32 {
        alpha: sanitize_channel(pixel.alpha),
        red: sanitize_channel(pixel.red),
        green: sanitize_channel(pixel.green),
        blue: sanitize_channel(pixel.blue),
    }
}

fn sanitize_channel(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(red: f32, green: f32, blue: f32) -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red,
            green,
            blue,
        }
    }

    #[test]
    fn classifies_only_pixels_within_selected_color_tolerance() {
        let source = [
            opaque(0.0, 0.0, 0.0),
            opaque(1.0, 1.0, 1.0),
            opaque(1.0, 0.0, 0.0),
        ];
        let colors = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let labels = classify_pixels(&source, &colors, 0.01, 0.0);
        assert_eq!(labels, vec![0, 1, -1]);
    }

    #[test]
    fn detects_a_boundary_between_two_selected_colors() {
        let labels = [0, 0, 1, 1];
        let field = detect_boundaries(&labels, 4, 1, 1);
        assert_eq!(field.mask, vec![0.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn bridges_a_small_unmatched_gap() {
        let labels = [0, -1, 1];
        let field = detect_boundaries(&labels, 3, 1, 2);
        assert_eq!(field.mask, vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn box_blur_preserves_a_constant_image() {
        let source = vec![opaque(0.25, 0.5, 0.75); 9];
        let output = box_blur_pixels(&source, 3, 3, 2.0, EdgeMode::Repeat);
        for pixel in output {
            assert!((pixel.red - 0.25).abs() < 1.0e-6);
            assert!((pixel.green - 0.5).abs() < 1.0e-6);
            assert!((pixel.blue - 0.75).abs() < 1.0e-6);
            assert!((pixel.alpha - 1.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn large_gaussian_uses_nonzero_box_radii() {
        let radii = gaussian_box_radii(64.0);
        assert!(radii.iter().all(|radius| *radius > 0));
        assert!((radii.iter().sum::<usize>() as i32 - 64).abs() <= 2);
    }

    #[test]
    fn boundary_normal_blur_samples_across_a_vertical_color_edge() {
        let source = vec![
            opaque(1.0, 0.0, 0.0),
            opaque(1.0, 0.0, 0.0),
            opaque(0.0, 0.0, 1.0),
            opaque(0.0, 0.0, 1.0),
        ];
        let labels = [0, 0, 1, 1];
        let field = detect_boundaries(&labels, 4, 1, 1);
        let directions = build_direction_field(&source, &field, 4, 1, 1, 0.0, EdgeMode::Repeat);
        assert!(directions[1].x.abs() > 0.99);
        assert!(directions[1].y.abs() < 0.01);

        let output =
            directional_blur_pass(&source, &directions, 4, 1, 1.5, 9, false, EdgeMode::Repeat);
        assert!(output[1].blue > 0.0);
        assert!(output[2].red > 0.0);
    }
}
