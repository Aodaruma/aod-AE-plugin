use after_effects as ae;
use std::env;

use ae::pf::*;

use crate::glass::{HeightSource, Shape};
use crate::params::Params;
use crate::render::{
    OwnedMaps, RenderLayout, read_layer_buffer, read_layer_buffer_with_rect, read_settings, render,
    shape,
};

const SMART_INPUT_ID: u32 = 0;
const SMART_MAP_ID: u32 = 1;
const SMART_QUERY_ID_BASE: i32 = 100;

#[derive(Clone, Copy)]
struct SmartLayout {
    input_rect: ae::Rect,
    output_rect: ae::Rect,
    map_rect: Option<ae::Rect>,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies map-driven glass refraction with parametric fractures and spectral dispersion.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        crate::params::setup(params)
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
                        "AOD_GlassDisplace - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_GlassDisplace")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let settings = read_settings(params, in_data)?;
                let custom = if settings.height_source == HeightSource::CustomMap {
                    let param = params.get(Params::MapLayer)?;
                    let layer = param.as_layer()?.value();
                    layer.as_ref().map(read_layer_buffer)
                } else {
                    None
                };
                render(
                    in_data,
                    in_layer,
                    out_layer,
                    settings,
                    OwnedMaps { custom },
                    RenderLayout::default(),
                )?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let request = extra.output_request();
                let settings = read_settings(params, in_data)?;
                let callbacks = extra.callbacks();
                let input = checkout_full_smart_layer(
                    callbacks,
                    0,
                    SMART_QUERY_ID_BASE + SMART_INPUT_ID as i32,
                    SMART_INPUT_ID,
                    &request,
                    in_data,
                )?;
                let full_rect: ae::Rect = input.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);

                let mut map_rect = None;
                if settings.height_source == HeightSource::CustomMap {
                    let index = params
                        .index(Params::MapLayer)
                        .ok_or(Error::BadCallbackParameter)?;
                    let map = checkout_full_smart_layer(
                        callbacks,
                        index as i32,
                        SMART_QUERY_ID_BASE + SMART_MAP_ID as i32,
                        SMART_MAP_ID,
                        &request,
                        in_data,
                    )?;
                    map_rect = Some(map.max_result_rect.into());
                }
                extra.set_pre_render_data(SmartLayout {
                    input_rect: full_rect,
                    output_rect: full_rect,
                    map_rect,
                });
            }
            ae::Command::SmartRender { extra } => {
                let callbacks = extra.callbacks();
                let settings = read_settings(params, in_data)?;
                let layout = extra.pre_render_data::<SmartLayout>().copied();
                let custom = if settings.height_source == HeightSource::CustomMap {
                    let layer = callbacks.checkout_layer_pixels(SMART_MAP_ID)?;
                    let buffer = layer.as_ref().map(|layer| {
                        read_layer_buffer_with_rect(layer, layout.and_then(|value| value.map_rect))
                    });
                    callbacks.checkin_layer_pixels(SMART_MAP_ID)?;
                    buffer
                } else {
                    None
                };

                let input = callbacks.checkout_layer_pixels(SMART_INPUT_ID)?;
                let render_result: Result<(), Error> = (|| {
                    let output = callbacks.checkout_output()?;
                    if let (Some(input), Some(output)) = (input, output) {
                        render(
                            in_data,
                            input,
                            output,
                            settings,
                            OwnedMaps { custom },
                            RenderLayout {
                                input_rect: layout.map(|value| value.input_rect),
                                output_rect: layout.map(|value| value.output_rect),
                            },
                        )?;
                    }
                    Ok(())
                })();
                let checkin_result = callbacks.checkin_layer_pixels(SMART_INPUT_ID);
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index }
                if matches!(
                    params.type_at(param_index),
                    Params::HeightSource
                        | Params::Shape
                        | Params::Dispersion
                        | Params::AutoSpectralSteps
                ) =>
            {
                out_data.set_out_flag(OutFlags::RefreshUi, true);
            }
            ae::Command::UpdateParamsUi => {
                let mut copy = params.cloned();
                self.update_params_ui(in_data, &mut copy)?;
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
        let source_value = params.get(Params::HeightSource)?.as_popup()?.value();
        let source = match source_value {
            2 => HeightSource::CustomMap,
            3 => HeightSource::InputLuma,
            4 => HeightSource::InputAlpha,
            _ => HeightSource::Procedural,
        };
        let shape = shape(params.get(Params::Shape)?.as_popup()?.value());
        let procedural = source == HeightSource::Procedural;
        let custom_map = source == HeightSource::CustomMap;
        let mapped = source != HeightSource::Procedural;

        self.toggle(in_data, params, Params::Shape, procedural)?;
        for id in [Params::MapLayer, Params::MapChannel] {
            self.toggle(in_data, params, id, custom_map)?;
        }
        for id in [Params::InvertMap, Params::MapBlack, Params::MapWhite] {
            self.toggle(in_data, params, id, mapped)?;
        }
        for id in [Params::Center, Params::Rotation] {
            self.toggle(in_data, params, id, procedural)?;
        }
        let bounded = procedural && !matches!(shape, Shape::Facets | Shape::Shards);
        for id in [Params::Size, Params::Aspect] {
            self.toggle(in_data, params, id, bounded)?;
        }
        self.toggle(
            in_data,
            params,
            Params::Roundness,
            procedural && shape == Shape::RoundedRectangle,
        )?;
        self.toggle(
            in_data,
            params,
            Params::RingWidth,
            procedural && shape == Shape::Ring,
        )?;
        let facets =
            procedural && matches!(shape, Shape::Facets | Shape::Shards | Shape::ImpactGlass);
        for id in [
            Params::FacetSize,
            Params::FacetAmount,
            Params::FacetJitter,
            Params::Seed,
        ] {
            self.toggle(in_data, params, id, facets)?;
        }
        self.toggle(
            in_data,
            params,
            Params::CrackWidth,
            procedural && matches!(shape, Shape::Shards | Shape::ImpactGlass),
        )?;
        let cracked = procedural && matches!(shape, Shape::Shards | Shape::ImpactGlass);
        self.toggle(in_data, params, Params::CrackDepth, cracked)?;
        let impact = procedural && shape == Shape::ImpactGlass;
        for id in [
            Params::RadialCracks,
            Params::CrackBranching,
            Params::CrackJitter,
            Params::StressRings,
            Params::RingJitter,
            Params::ImpactFalloff,
        ] {
            self.toggle(in_data, params, id, impact)?;
        }
        let dispersion = params.get(Params::Dispersion)?.as_float_slider()?.value();
        let spectral_active = dispersion.abs() > 1.0e-6;
        let automatic_steps = params
            .get(Params::AutoSpectralSteps)?
            .as_checkbox()?
            .value();
        self.toggle(in_data, params, Params::AutoSpectralSteps, spectral_active)?;
        // Keep the manual value visible while Auto Spectral Steps is active,
        // but gray it out so the active source of the step count is explicit.
        // This is separate from `toggle`: repeatedly enabling and disabling
        // the same control in one UI update causes unnecessary host redraws.
        self.set_param_visible(in_data, params, Params::DispersionSteps, spectral_active)?;
        Self::set_param_ui_flag(
            params,
            Params::DispersionSteps,
            ParamUIFlags::DISABLED,
            !spectral_active || automatic_steps,
        )?;
        Ok(())
    }

    fn toggle(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        self.set_param_visible(in_data, params, id, visible)?;
        Self::set_param_ui_flag(params, id, ParamUIFlags::DISABLED, !visible)
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
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

    fn set_param_ui_flag(
        params: &mut Parameters<Params>,
        id: Params,
        flag: ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let current = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current == status {
            return Ok(());
        }
        let mut param = params.get_mut(id)?;
        param.set_ui_flag(flag, status);
        param.update_param_ui()?;
        Ok(())
    }
}

fn checkout_full_smart_layer(
    callbacks: PreRenderCallbacks,
    param_index: i32,
    query_id: i32,
    checkout_id: u32,
    request: &ae::sys::PF_RenderRequest,
    in_data: InData,
) -> Result<ae::sys::PF_CheckoutResult, Error> {
    let query = callbacks.checkout_layer(
        param_index,
        query_id,
        request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;
    let mut full_request = *request;
    full_request.rect = query.max_result_rect;
    callbacks.checkout_layer(
        param_index,
        checkout_id as i32,
        &full_request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )
}
