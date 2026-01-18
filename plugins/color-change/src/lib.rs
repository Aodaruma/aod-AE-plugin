use after_effects as ae;
use seq_macro::seq;
use std::env;

use utils::ToPixel;

const MAX_PAIRS: usize = 32;
seq!(N in 1..=32 {
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Tolerrance,
    AddPairButton,
    RemovePairButton,
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
struct Plugin {}

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

        params.add(
            Params::AddPairButton,
            "Add Color",
            ae::pf::ButtonDef::setup(|d| {
                d.set_label("add");
            }),
        )?;
        params.add(
            Params::RemovePairButton,
            "Remove Color",
            ae::pf::ButtonDef::setup(|d| {
                d.set_label("remove");
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

            ae::Command::UserChangedParam { param_index } => match params.type_at(param_index) {
                Params::AddPairButton => {
                    let current_pairs = Self::count_color_pairs(params);
                    if current_pairs < MAX_PAIRS {
                        Self::set_color_pairs(params, current_pairs + 1)?;
                        out_data.set_out_flag(OutFlags::RefreshUi, true);
                    }
                }
                Params::RemovePairButton => {
                    let current_pairs = Self::count_color_pairs(params);
                    if current_pairs > 1 {
                        Self::set_color_pairs(params, current_pairs - 1)?;
                        out_data.set_out_flag(OutFlags::RefreshUi, true);
                    }
                }
                _ => {}
            },

            ae::Command::UpdateParamsUi => {
                let current_pairs = Self::count_color_pairs(params);
                Self::set_color_pairs(params, current_pairs)?;
            }

            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn count_color_pairs(params: &ae::Parameters<Params>) -> usize {
        // Determine the active pair count from the highest visible ColorFrom parameter.
        // (We use UI visibility as state; this is cheap and works well with UpdateParamsUi.)
        for idx in (0..MAX_PAIRS).rev() {
            if let Ok(p) = params.get(COLOR_FROM_PARAMS[idx]) {
                if !p.ui_flags().contains(ae::pf::ParamUIFlags::INVISIBLE) {
                    return idx + 1;
                }
            }
        }
        1
    }

    fn set_color_pairs(params: &mut ae::Parameters<Params>, pairs: usize) -> Result<(), Error> {
        let pairs = pairs.clamp(1, MAX_PAIRS);

        // Enable/disable the +/- buttons based on bounds.
        Self::set_param_enabled(params, Params::AddPairButton, pairs < MAX_PAIRS)?;
        Self::set_param_enabled(params, Params::RemovePairButton, pairs > 1)?;

        // Show/hide pairs.
        for idx in 0..MAX_PAIRS {
            let visible = idx < pairs;
            Self::set_param_visible(params, COLOR_FROM_PARAMS[idx], visible)?;
            Self::set_param_visible(params, COLOR_TO_PARAMS[idx], visible)?;
        }

        Ok(())
    }

    fn set_param_visible(
        params: &mut ae::Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
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
        // Avoid unnecessary PF_UpdateParamUI calls.
        // let current_status = params.get(id)?.ui_flags().contains(flag);
        // if current_status == status {
        //     return Ok(());
        // }

        // AE SDK guidance: don't mutate the original PF_ParamDef when calling PF_UpdateParamUI.
        // Clone, mutate the clone, then update.
        let mut p = params.get_mut(id)?;
        // let mut p = p.clone();
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
        let progress_final = out_layer.height() as i32;
        // let width = in_layer.width() as usize;
        // let height = in_layer.height() as usize;
        // let frame_num = in_data.current_frame() as usize;

        // Process here
        let tolerance = params.get(Params::Tolerrance)?.as_float_slider()?.value() as f32;
        let active_pairs = Self::count_color_pairs(params);

        for i in 0..active_pairs {
            let color_from = params
                .get(COLOR_FROM_PARAMS[i])?
                .as_color()?
                .value()
                .to_pixel32();
            let color_to = params.get(COLOR_TO_PARAMS[i])?.as_color()?.value();

            in_layer.iterate_with(&mut out_layer, 0, progress_final, None, |_x, _y, ip, op| {
                let ip = ip.as_f32();
                // let alpha = ip.alpha;

                let dr = ip.red - color_from.red;
                let dg = ip.green - color_from.green;
                let db = ip.blue - color_from.blue;
                let dist = (dr * dr + dg * dg + db * db).sqrt();
                if dist < tolerance {
                    match op {
                        GenericPixelMut::Pixel8(p) => {
                            let to_color = color_to.to_pixel8();
                            p.red = to_color.red;
                            p.green = to_color.green;
                            p.blue = to_color.blue;
                        }
                        GenericPixelMut::Pixel16(p) => {
                            let to_color = color_to.to_pixel16();
                            p.red = to_color.red;
                            p.green = to_color.green;
                            p.blue = to_color.blue;
                        }
                        GenericPixelMut::PixelF32(p) => {
                            let to_color = color_to.to_pixel32();
                            p.red = to_color.red;
                            p.green = to_color.green;
                            p.blue = to_color.blue;
                        }
                        GenericPixelMut::PixelF64(p) => {
                            let to_color = color_to.to_pixel32();
                            p.redF = to_color.red as _;
                            p.greenF = to_color.green as _;
                            p.blueF = to_color.blue as _;
                        }
                    }
                }

                Ok(())
            })?;
        }

        Ok(())
    }
}
