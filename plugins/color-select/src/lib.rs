#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use seq_macro::seq;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const MAX_COLORS: usize = 32;
const MIN_COLORS: usize = 1;
const DEFAULT_COLORS: usize = 1;
const ALPHA_EPSILON: f32 = 1.0e-6;
const SQRT_3: f32 = 1.732_050_8;

seq!(N in 1..=32 {
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    KeepSelectedColors,
    Tolerance,
    ColorCount,
    AddColor,
    RemoveColor,
    #(
        Color~N,
    )*
}
});

seq!(N in 1..=32 {
const COLOR_PARAMS: [Params; 32] = [#(Params::Color~N,)*];
});

const DEFAULT_SWATCHES: [Pixel8; 8] = [
    Pixel8 {
        red: 255,
        green: 0,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 255,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 0,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 255,
        blue: 0,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 0,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 255,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    },
    Pixel8 {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    },
];

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Selects specified colors for keying or keeping with dynamic multi-color controls.";

#[derive(Debug)]
struct RenderSettings {
    keep_selected: bool,
    threshold_sq: f32,
    selected_colors: Vec<[f32; 3]>,
}

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
            Params::KeepSelectedColors,
            "Keep Selected Colors",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;

        params.add(
            Params::Tolerance,
            "Tolerance (%)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(25.0);
                d.set_default(2.0);
                d.set_precision(1);
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
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::AddColor,
            "Add Color",
            ButtonDef::setup(|d| {
                d.set_label("Add");
            }),
        )?;

        params.add(
            Params::RemoveColor,
            "Remove Color",
            ButtonDef::setup(|d| {
                d.set_label("Remove");
            }),
        )?;

        for (idx, color_param) in COLOR_PARAMS.iter().enumerate().take(MAX_COLORS) {
            params.add(
                *color_param,
                &format!("Color{}", idx + 1),
                ColorDef::setup(|d| {
                    d.set_default(default_color(idx));
                }),
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
                        "AOD_ColorSelect - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ColorSelect")
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
        if changed != Params::ColorCount
            && changed != Params::AddColor
            && changed != Params::RemoveColor
        {
            return Ok(());
        }

        let current = Self::color_count(params);
        let next = match changed {
            Params::AddColor => current.saturating_add(1),
            Params::RemoveColor => current.saturating_sub(1),
            _ => current,
        }
        .clamp(MIN_COLORS, MAX_COLORS);

        Self::set_color_count(params, next)?;
        out_data.set_out_flag(OutFlags::RefreshUi, true);
        Ok(())
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let count = Self::color_count(params);

        for (idx, color_param) in COLOR_PARAMS.iter().enumerate().take(MAX_COLORS) {
            let visible = idx < count;
            self.set_param_visible(in_data, params, *color_param, visible)?;
            Self::set_param_enabled(params, *color_param, visible)?;
        }

        Self::set_param_enabled(params, Params::AddColor, count < MAX_COLORS)?;
        Self::set_param_enabled(params, Params::RemoveColor, count > MIN_COLORS)?;

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

    fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
        let keep_selected = params
            .get(Params::KeepSelectedColors)?
            .as_checkbox()?
            .value();

        let tolerance_percent = params.get(Params::Tolerance)?.as_float_slider()?.value() as f32;
        let tolerance = (tolerance_percent / 100.0).clamp(0.0, 1.0);
        let threshold_sq = (tolerance * SQRT_3).powi(2);

        let active_colors = Self::color_count(params);
        let mut selected_colors = Vec::with_capacity(active_colors);
        for color_param in COLOR_PARAMS.iter().take(active_colors) {
            let color = params.get(*color_param)?.as_color()?.value().to_pixel32();
            selected_colors.push(color_to_straight_rgb(color));
        }

        Ok(RenderSettings {
            keep_selected,
            threshold_sq,
            selected_colors,
        })
    }

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        if out_layer.width() == 0 || out_layer.height() == 0 {
            return Ok(());
        }

        let settings = Self::read_settings(params)?;
        let progress_final = out_layer.height() as i32;

        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x, _y, src, mut dst| {
                let src_px = read_input_pixel(src);
                let src_rgb = pixel_to_straight_rgb(src_px);

                let mut min_dist_sq = f32::INFINITY;
                for target_rgb in &settings.selected_colors {
                    let dist_sq = distance_sq(src_rgb, *target_rgb);
                    if dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                    }
                }

                let matched = min_dist_sq <= settings.threshold_sq;
                let keep = if settings.keep_selected {
                    matched
                } else {
                    !matched
                };

                let mut out_px = src_px;
                if !keep {
                    out_px.alpha = 0.0;
                    out_px.red = 0.0;
                    out_px.green = 0.0;
                    out_px.blue = 0.0;
                }

                write_output_pixel(&mut dst, out_px);
                Ok(())
            },
        )?;

        Ok(())
    }
}

fn default_color(index: usize) -> Pixel8 {
    DEFAULT_SWATCHES[index % DEFAULT_SWATCHES.len()]
}

fn color_to_straight_rgb(color: PixelF32) -> [f32; 3] {
    pixel_to_straight_rgb(color)
}

fn pixel_to_straight_rgb(px: PixelF32) -> [f32; 3] {
    if px.alpha > ALPHA_EPSILON {
        [px.red / px.alpha, px.green / px.alpha, px.blue / px.alpha]
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dr = a[0] - b[0];
    let dg = a[1] - b[1];
    let db = a[2] - b[2];
    dr * dr + dg * dg + db * db
}

fn read_input_pixel(src: GenericPixel<'_>) -> PixelF32 {
    match src {
        GenericPixel::Pixel8(p) => p.to_pixel32(),
        GenericPixel::Pixel16(p) => p.to_pixel32(),
        GenericPixel::PixelF32(p) => *p,
        GenericPixel::PixelF64(p) => PixelF32 {
            alpha: p.alphaF as f32,
            red: p.redF as f32,
            green: p.greenF as f32,
            blue: p.blueF as f32,
        },
    }
}

fn write_output_pixel(dst: &mut GenericPixelMut<'_>, px: PixelF32) {
    match dst {
        GenericPixelMut::Pixel8(p) => {
            **p = px.to_pixel8();
        }
        GenericPixelMut::Pixel16(p) => {
            **p = px.to_pixel16();
        }
        GenericPixelMut::PixelF32(p) => {
            **p = px;
        }
        GenericPixelMut::PixelF64(p) => {
            p.alphaF = px.alpha as _;
            p.redF = px.red as _;
            p.greenF = px.green as _;
            p.blueF = px.blue as _;
        }
    }
}
