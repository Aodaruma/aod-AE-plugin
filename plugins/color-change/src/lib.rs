#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use seq_macro::seq;
use std::env;

use utils::ToPixel;

const MAX_PAIRS: usize = 32;
const MIN_PAIRS: usize = 1;
const DEFAULT_PAIRS: usize = 1;
seq!(N in 1..=32 {
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Tolerrance,
    PairCount,
    AddColor,
    RemoveColor,
    #(
        ColorFrom~N,
        ColorTo~N,
    )*
}
});

seq!(N in 1..=32 {
    const COLOR_FROM_PARAMS: [Params; 32] = [#(Params::ColorFrom~N,)*];
    const COLOR_TO_PARAMS: [Params; 32] = [#(Params::ColorTo~N,)*];
});

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "A plugin to change some colors in a footage";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // param definitions here

        params.add(
            Params::Tolerrance,
            "Tolerance",
            ae::pf::FloatSliderDef::setup(|d| {
                d.set_default(0.001);
                d.set_valid_min(0.0);
                d.set_valid_max(1.0);
                d.set_slider_min(0.0);
                d.set_slider_max(1.0);
                d.set_precision(4);
            }),
        )?;

        params.add_with_flags(
            Params::PairCount,
            "Number of Colors",
            ae::pf::FloatSliderDef::setup(|d| {
                d.set_default(DEFAULT_PAIRS as f64);
                d.set_value(DEFAULT_PAIRS as f64);
                d.set_valid_min(1.0);
                d.set_valid_max(MAX_PAIRS as f32);
                d.set_slider_min(1.0);
                d.set_slider_max(MAX_PAIRS as f32);
                d.set_precision(0);
            }),
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::AddColor,
            "Add Color",
            ae::pf::ButtonDef::setup(|d| {
                d.set_label("Add");
            }),
        )?;

        params.add(
            Params::RemoveColor,
            "Remove Color",
            ae::pf::ButtonDef::setup(|d| {
                d.set_label("Remove");
            }),
        )?;

        seq!(N in 1..=32 {
            params.add(
                Params::ColorFrom~N,
                &format!("Color{} From", N),
                ae::pf::ColorDef::setup(|d| {
                    d.set_default(
                        Pixel8 {
                            red: 0,
                            green: 0,
                            blue: 0,
                            alpha: 1
                        }
                    );
                }),
            )?;

            params.add(
                Params::ColorTo~N,
                &format!("Color{} To", N),
                ae::pf::ColorDef::setup(|d| {
                    d.set_default(
                        Pixel8 {
                            red: 255u8,
                            green: 0,
                            blue: 0,
                            alpha: 1
                        }
                    );
                }),
            )?;
        });

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
                    "AOD_ColorChange - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                    version=env!("CARGO_PKG_VERSION"),
                    build_year=env!("BUILD_YEAR")
                ).as_str());
            }
            ae::Command::GlobalSetup => {
                // Declare that we do or do not support smart rendering
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ColorChange")
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
                // self.do_render(in_data, in_layer_opt, out_data, out_layer_opt, params)?;
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
    fn pair_count(params: &ae::Parameters<Params>) -> usize {
        params
            .get(Params::PairCount)
            .ok()
            .and_then(|p| p.as_float_slider().ok().map(|s| s.value()))
            .map(|v| v.round() as usize)
            .unwrap_or(DEFAULT_PAIRS)
            .clamp(MIN_PAIRS, MAX_PAIRS)
    }

    fn set_pair_count(params: &mut ae::Parameters<Params>, count: usize) -> Result<(), Error> {
        let clamped = count.clamp(MIN_PAIRS, MAX_PAIRS);
        let mut pair_count = params.get_mut(Params::PairCount)?;
        pair_count.as_float_slider_mut()?.set_value(clamped as f64);
        pair_count.update_param_ui()?;
        Ok(())
    }

    fn handle_user_changed_param(
        &self,
        param_index: usize,
        params: &mut ae::Parameters<Params>,
        out_data: &mut OutData,
    ) -> Result<(), Error> {
        let changed = params.type_at(param_index);
        if changed != Params::PairCount
            && changed != Params::AddColor
            && changed != Params::RemoveColor
        {
            return Ok(());
        }

        if changed == Params::AddColor || changed == Params::RemoveColor {
            let current = Self::pair_count(params);
            let next = match changed {
                Params::AddColor => current.saturating_add(1),
                Params::RemoveColor => current.saturating_sub(1),
                _ => current,
            }
            .clamp(MIN_PAIRS, MAX_PAIRS);
            Self::set_pair_count(params, next)?;
        }

        out_data.set_out_flag(OutFlags::RefreshUi, true);
        Ok(())
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let current_pairs = Self::pair_count(params);
        self.set_color_pairs(in_data, params, current_pairs)?;
        Self::set_param_enabled(params, Params::AddColor, current_pairs < MAX_PAIRS)?;
        Self::set_param_enabled(params, Params::RemoveColor, current_pairs > MIN_PAIRS)?;
        Ok(())
    }

    fn set_color_pairs(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
        pairs: usize,
    ) -> Result<(), Error> {
        let pairs = pairs.clamp(MIN_PAIRS, MAX_PAIRS);

        // Show/hide pairs.
        for idx in 0..MAX_PAIRS {
            let visible = idx < pairs;
            self.set_param_visible(in_data, params, COLOR_FROM_PARAMS[idx], visible)?;
            self.set_param_visible(in_data, params, COLOR_TO_PARAMS[idx], visible)?;
        }

        Ok(())
    }

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::DISABLED, !enabled)
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
        if out_layer.width() == 0 || out_layer.height() == 0 {
            return Ok(());
        }

        let progress_final = out_layer.height() as i32;
        let tolerance = params.get(Params::Tolerrance)?.as_float_slider()?.value() as f32;
        let tolerance_sq = tolerance * tolerance;
        let active_pairs = Self::pair_count(params);
        let mut color_pairs = Vec::with_capacity(active_pairs);
        for i in 0..active_pairs {
            let color_from = params
                .get(COLOR_FROM_PARAMS[i])?
                .as_color()?
                .value()
                .to_pixel32();
            let color_to = params
                .get(COLOR_TO_PARAMS[i])?
                .as_color()?
                .value()
                .to_pixel32();
            color_pairs.push((color_from, color_to));
        }

        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x, _y, ip, mut op| {
                let src = read_input_pixel(ip);
                let mut out = src;

                for (color_from, color_to) in &color_pairs {
                    let dr = src.red - color_from.red;
                    let dg = src.green - color_from.green;
                    let db = src.blue - color_from.blue;
                    let dist_sq = dr * dr + dg * dg + db * db;
                    if dist_sq < tolerance_sq {
                        out.red = color_to.red;
                        out.green = color_to.green;
                        out.blue = color_to.blue;
                    }
                }

                write_output_pixel(&mut op, out);

                Ok(())
            },
        )?;

        Ok(())
    }
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
