#![allow(clippy::drop_non_drop, clippy::question_mark)] // Generated SDK entrypoint.

use ae::pf::*;
use after_effects as ae;
use std::sync::{Arc, Mutex};
use utils::{
    ToPixel,
    image::{SampleEdge, TRANSPARENT, luma, read_layer, sample_bilinear, sanitize_pixel},
};

mod curve;
mod parameters;
mod shading;
mod surface;
mod ui;
use curve::Curve;
use parameters::{Params, Settings};

const PLUGIN_DESCRIPTION: &str =
    "Creates curve-shaped bevels from silhouettes with configurable materials and lighting.";

struct Cache {
    size: (usize, usize),
    silhouette: Vec<f32>,
    settings: surface::Settings,
    surface: Arc<surface::Surface>,
}

#[derive(Default)]
struct Plugin {
    cache: Mutex<Option<Cache>>,
}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        parameters::setup(params)?;
        in_data
            .interact()
            .register_ui(CustomUIInfo::new().events(ae::CustomEventFlags::EFFECT))?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => out_data.set_return_msg(&format!(
                "AOD_ImageBevel {}\r\r{PLUGIN_DESCRIPTION}",
                env!("CARGO_PKG_VERSION")
            )),
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::CustomUi, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
            }
            ae::Command::ArbitraryCallback { mut extra } => {
                extra.dispatch::<Curve, Params>(Params::Profile)?
            }
            ae::Command::Event { mut extra } => ui::event(params, &mut extra)?,
            ae::Command::UserChangedParam { param_index } => {
                if params.type_at(param_index) == Params::ApplyPreset {
                    let preset = params.get(Params::Preset)?.as_popup()?.value();
                    params
                        .get_mut(Params::Profile)?
                        .as_arbitrary_mut()?
                        .set_value(Curve::preset(preset))?;
                    out_data.set_out_flag(OutFlags::ForceRerender, true);
                }
                out_data.set_out_flag(OutFlags::RefreshUi, true);
            }
            ae::Command::UpdateParamsUi => {
                let sdf = params.get(Params::Method)?.as_popup()?.value() == 1;
                let mut cloned = params.cloned();
                for id in [Params::SmoothRadius, Params::Iterations] {
                    let mut p = cloned.get_mut(id)?;
                    if p.ui_flags().contains(ParamUIFlags::DISABLED) != sdf {
                        p.set_ui_flag(ParamUIFlags::DISABLED, sdf);
                        p.update_param_ui()?;
                    }
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let s = Settings::read(params, in_data)?;
                if s.material.transmission > 0.0 {
                    let checkout = params.checkout_at(Params::Background, None, None, None)?;
                    let background = checkout.as_layer()?.value();
                    self.render(in_data, in_layer, out_layer, background.as_ref(), s)?;
                } else {
                    self.render(in_data, in_layer, out_layer, None, s)?;
                }
            }
            ae::Command::SmartPreRender { mut extra } => {
                let request = extra.output_request();
                let cb = extra.callbacks();
                let result = checkout_full(cb, 0, 0, &request, in_data, true)?;
                let rect: ae::Rect = result.max_result_rect.into();
                let nominal = ae::Rect {
                    left: 0,
                    top: 0,
                    right: in_data.width(),
                    bottom: in_data.height(),
                };
                extra.union_result_rect(rect);
                extra.union_max_result_rect(rect);
                extra.union_result_rect(nominal);
                extra.union_max_result_rect(nominal);
                extra.set_returns_extra_pixels(true);
                if params.get(Params::Transmission)?.as_float_slider()?.value() > 0.0 {
                    let index = params
                        .index(Params::Background)
                        .ok_or(Error::InvalidParms)?;
                    checkout_full(cb, index as i32, 1, &request, in_data, false)?;
                }
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let s = Settings::read(params, in_data)?;
                let mut checked = Vec::new();
                let result = (|| -> Result<(), Error> {
                    let input = cb.checkout_layer_pixels(0)?;
                    checked.push(0);
                    let background = if s.material.transmission > 0.0 {
                        let layer = cb.checkout_layer_pixels(1)?;
                        checked.push(1);
                        layer
                    } else {
                        None
                    };
                    if let (Some(input), Some(output)) = (input, cb.checkout_output()?) {
                        self.render(in_data, input, output, background.as_ref(), s)?;
                    }
                    Ok(())
                })();
                let mut checkin = Ok(());
                for id in checked {
                    if let Err(e) = cb.checkin_layer_pixels(id) {
                        checkin = Err(e);
                    }
                }
                result?;
                checkin?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn surface(
        &self,
        silhouette: Vec<f32>,
        w: usize,
        h: usize,
        settings: surface::Settings,
        data: InData,
    ) -> Result<Arc<surface::Surface>, Error> {
        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| Error::InternalStructDamaged)?;
            if let Some(c) = cache.as_ref()
                && c.size == (w, h)
                && c.settings == settings
                && c.silhouette == silhouette
            {
                return Ok(Arc::clone(&c.surface));
            }
        }
        let surface = Arc::new(surface::build(&silhouette, w, h, settings, || {
            data.interact().abort()
        })?);
        *self
            .cache
            .lock()
            .map_err(|_| Error::InternalStructDamaged)? = Some(Cache {
            size: (w, h),
            silhouette,
            settings,
            surface: Arc::clone(&surface),
        });
        Ok(surface)
    }

    fn render(
        &self,
        data: InData,
        input: Layer,
        mut output: Layer,
        background: Option<&Layer>,
        s: Settings,
    ) -> Result<(), Error> {
        let (w, h) = (input.width(), input.height());
        if w == 0 || h == 0 {
            return Ok(());
        }
        let source: Vec<_> = read_layer(&input)
            .into_iter()
            .map(|p| sanitize_pixel(p, false))
            .collect();
        let silhouette = source
            .iter()
            .map(|p| {
                if s.silhouette == 2 {
                    (luma(*p) * p.alpha).clamp(0.0, 1.0)
                } else {
                    p.alpha
                }
            })
            .collect();
        let surface = self.surface(silhouette, w, h, s.surface, data)?;
        let source_origin = input.origin();
        let output_origin = output.origin();
        let backdrop = background.map(read_layer);
        let (background_pixels, bw, bh, background_origin) =
            if let (Some(pixels), Some(layer)) = (&backdrop, background) {
                (
                    pixels.as_slice(),
                    layer.width(),
                    layer.height(),
                    layer.origin(),
                )
            } else {
                (source.as_slice(), w, h, source_origin)
            };
        let world = output.world_type();
        let out_height = output.height();
        output.iterate(0, out_height as i32, None, |x, y, mut dst| {
            let sx = x + output_origin.h - source_origin.h;
            let sy = y + output_origin.v - source_origin.v;
            let inside = sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32;
            let (pixel, height, normal, coverage) = if inside {
                let i = sy as usize * w + sx as usize;
                (
                    source[i],
                    surface.height[i],
                    surface.normals[i],
                    surface.coverage[i],
                )
            } else {
                (TRANSPARENT, 0.0, [0.0, 0.0, 1.0], 0.0)
            };
            let px = (x + output_origin.h) as f32;
            let py = (y + output_origin.v) as f32;
            let light = surface::normalize([
                (s.light[0] - px) * s.surface.scale[0],
                (s.light[1] - py) * s.surface.scale[1],
                s.light[2] - height * s.surface.elevation,
            ]);
            let transmitted = if s.material.transmission > 0.0 {
                let offset =
                    shading::refraction_offset(normal, s.material.ior, s.material.thickness);
                let p = sample_bilinear(
                    background_pixels,
                    bw,
                    bh,
                    px - background_origin.h as f32 + offset[0] / s.surface.scale[0],
                    py - background_origin.v as f32 + offset[1] / s.surface.scale[1],
                    SampleEdge::Transparent,
                );
                [p.red, p.green, p.blue]
            } else {
                [0.0; 3]
            };
            let rgb = shading::shade(
                pixel,
                transmitted,
                normal,
                light,
                s.radiance,
                height,
                s.material,
                s.output,
            );
            let pixel = shading::pack(rgb, shading::alpha(s.alpha, pixel.alpha, coverage), s.clamp);
            match world {
                ae::aegp::WorldType::U8 => dst.set_from_u8(pixel.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(pixel.to_pixel16()),
                _ => dst.set_from_f32(pixel),
            }
            Ok(())
        })
    }
}

fn checkout_full(
    cb: PreRenderCallbacks,
    param: i32,
    id: i32,
    request: &ae::sys::PF_RenderRequest,
    data: InData,
    nominal: bool,
) -> Result<ae::sys::PF_CheckoutResult, Error> {
    let query = cb.checkout_layer(
        param,
        1000 + id,
        request,
        data.current_time(),
        data.time_step(),
        data.time_scale(),
    )?;
    let mut full = *request;
    full.rect = query.max_result_rect;
    if nominal {
        full.rect.left = full.rect.left.min(0);
        full.rect.top = full.rect.top.min(0);
        full.rect.right = full.rect.right.max(data.width());
        full.rect.bottom = full.rect.bottom.max(data.height());
    }
    cb.checkout_layer(
        param,
        id,
        &full,
        data.current_time(),
        data.time_step(),
        data.time_scale(),
    )
}
