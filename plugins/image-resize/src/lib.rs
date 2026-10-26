// These warnings originate in the upstream effect entry-point macro.
#![allow(clippy::drop_non_drop, clippy::question_mark)]

use ae::pf::*;
use after_effects as ae;
use utils::ToPixel;
use utils::image::{SampleEdge, SampleFilter, TRANSPARENT, read_pixel, sample_filtered};

mod geometry;
use geometry::{Bounds, Geometry, Settings};

const PLUGIN_DESCRIPTION: &str = "Crops or extends image edges or a fixed-size canvas without scaling, with boundary sampling and rotation.";

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    // Keep the original key: the wrapper hashes it for the saved parameter ID.
    Units,
    All,
    Top,
    Bottom,
    Left,
    Right,
    Extend,
    Angle,
    Interpolation,
    OutputWidth,
    OutputHeight,
    EdgesStart,
    EdgesEnd,
    OutputSizeStart,
    OutputSizeEnd,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}
ae::define_effect!(Plugin, (), Params);

#[derive(Clone, Copy)]
struct RenderData {
    geometry: Geometry,
    edge: SampleEdge,
    filter: SampleFilter,
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::Units,
            "Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Pixels", "Percent", "Output Size"]);
                d.set_default(2);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add(
            Params::All,
            "All %",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-30000.0);
                d.set_valid_max(30000.0);
                d.set_slider_min(-300.0);
                d.set_slider_max(300.0);
                d.set_default(100.0);
                d.set_precision(2);
            }),
        )?;
        params.add_group(
            Params::OutputSizeStart,
            Params::OutputSizeEnd,
            "Output Size",
            false,
            |params| {
                for (id, name, default) in [
                    (Params::OutputWidth, "Output Width (px)", 1920.0),
                    (Params::OutputHeight, "Output Height (px)", 1080.0),
                ] {
                    params.add_with_flags(
                        id,
                        name,
                        FloatSliderDef::setup(|d| {
                            d.set_valid_min(1.0);
                            d.set_valid_max(30000.0);
                            d.set_slider_min(1.0);
                            d.set_slider_max(4096.0);
                            d.set_default(default);
                            d.set_precision(0);
                        }),
                        ParamFlag::empty(),
                        ParamUIFlags::DISABLED,
                    )?;
                }
                Ok(())
            },
        )?;
        params.add_group(
            Params::EdgesStart,
            Params::EdgesEnd,
            "Edges",
            false,
            |params| {
                for (id, name) in [
                    (Params::Top, "Top (%)"),
                    (Params::Bottom, "Bottom (%)"),
                    (Params::Left, "Left (%)"),
                    (Params::Right, "Right (%)"),
                ] {
                    add_slider(params, id, name, -30000.0, 30000.0, 0.0)?;
                }
                Ok(())
            },
        )?;
        params.add(
            Params::Extend,
            "Extend",
            PopupDef::setup(|d| {
                d.set_options(&["None", "Mirror", "Tile", "Repeat"]);
                d.set_default(1);
            }),
        )?;
        params.add_with_flags(
            Params::Angle,
            "Angle",
            AngleDef::setup(|d| {
                d.set_default(0.0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add(
            Params::Interpolation,
            "Interpolation",
            PopupDef::setup(|d| {
                d.set_options(&["Nearest", "Bilinear"]);
                d.set_default(2);
            }),
        )?;
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
                "AOD_ImageResize {}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{} Aodaruma",
                env!("CARGO_PKG_VERSION"),
                env!("BUILD_YEAR"),
            )),
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::IExpandBuffer, true);
                out_data.set_out_flag(OutFlags::PixIndependent, false);
                out_data.set_out_flag(OutFlags::UseOutputExtent, false);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if !in_data.is_premiere() {
                    self.aegp_id = Some(
                        ae::aegp::suites::Utility::new()?.register_with_aegp("AOD_ImageResize")?,
                    );
                }
            }
            ae::Command::FrameSetup { .. } => {
                let data = read_settings(params, in_data, &mut out_data)?;
                let bounds = data.geometry.bounds;
                // An empty crop still needs a valid legacy world allocation.
                out_data.set_width(bounds.width().max(1) as u32);
                out_data.set_height(bounds.height().max(1) as u32);
                let source = in_data.pre_effect_source_origin();
                out_data.set_origin(ae::Point {
                    h: -source.h - bounds.left,
                    v: -source.v - bounds.top,
                });
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let data = read_settings(params, in_data, &mut out_data)?;
                render(in_data, Some(in_layer), out_layer, data, false)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let data = read_settings(params, in_data, &mut out_data)?;
                let mut request = extra.output_request();
                let requested = from_rect(request.rect.into());
                // Rotation and periodic extension can access any original pixel.
                // Request the full nominal canvas, including transparent margins.
                request.rect = to_rect(data.geometry.source_bounds()).into();
                extra.callbacks().checkout_layer(
                    0,
                    0,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                extra.set_max_result_rect(to_rect(data.geometry.bounds));
                extra.set_result_rect(to_rect(data.geometry.bounds.intersection(requested)));
                extra.set_pre_render_data(data);
            }
            ae::Command::SmartRender { extra } => {
                let data = *extra
                    .pre_render_data::<RenderData>()
                    .ok_or(Error::BadCallbackParameter)?;
                let callbacks = extra.callbacks();
                let input = callbacks.checkout_layer_pixels(0)?;
                let result: Result<(), Error> = (|| {
                    if let Some(output) = callbacks.checkout_output()? {
                        render(in_data, input, output, data, true)?;
                    }
                    Ok(())
                })();
                let checkin = callbacks.checkin_layer_pixels(0);
                result?;
                checkin?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if matches!(params.type_at(param_index), Params::Units | Params::Angle) {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params = params.cloned();
                let mode = params.get(Params::Units)?.as_popup()?.value();
                let enabled = mode == 3;
                self.set_visible(in_data, &mut params, Params::OutputSizeStart, enabled)?;
                for id in [Params::OutputWidth, Params::OutputHeight] {
                    self.set_visible(in_data, &mut params, id, enabled)?;
                    let disabled = params.get(id)?.ui_flags().contains(ParamUIFlags::DISABLED);
                    if disabled == enabled {
                        let mut param = params.get_mut(id)?;
                        param.set_ui_flag(ParamUIFlags::DISABLED, !enabled);
                        param.update_param_ui()?;
                    }
                }
                for (id, base) in [
                    (Params::Top, "Top"),
                    (Params::Bottom, "Bottom"),
                    (Params::Left, "Left"),
                    (Params::Right, "Right"),
                ] {
                    self.set_name(
                        &mut params,
                        id,
                        &format!("{base} ({})", if mode == 1 { "px" } else { "%" }),
                    )?;
                }
                self.set_name(
                    &mut params,
                    Params::EdgesStart,
                    if enabled {
                        "Center Adjustment"
                    } else {
                        "Edges"
                    },
                )?;
                let rotated = params
                    .get(Params::Angle)?
                    .as_angle()?
                    .float_value()?
                    .rem_euclid(360.0)
                    != 0.0;
                self.set_visible(in_data, &mut params, Params::Interpolation, rotated)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn set_name(
        &self,
        params: &mut Parameters<Params>,
        id: Params,
        name: &str,
    ) -> Result<(), Error> {
        // PF parameter copies can contain the setup name instead of the
        // current UI name, so reapply the label for the current mode.
        let mut param = params.get_mut(id)?;
        param.set_name(name)?;
        param.update_param_ui()?;
        Ok(())
    }

    fn set_visible(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if in_data.is_premiere() {
            if params.get(id)?.ui_flags().contains(ParamUIFlags::INVISIBLE) == visible {
                let mut param = params.get_mut(id)?;
                param.set_ui_flag(ParamUIFlags::INVISIBLE, !visible);
                param.update_param_ui()?;
            }
        } else {
            let plugin_id = self.aegp_id.ok_or(Error::BadCallbackParameter)?;
            let index = params.index(id).ok_or(Error::InvalidIndex)?;
            let effect = in_data.effect().aegp_effect(plugin_id)?;
            let stream = effect.new_stream_by_index(plugin_id, index as i32)?;
            // The wrapper's flag getter uses an enum and cannot represent
            // combined flags. Set only Hidden, preserving the other flags.
            stream.set_dynamic_stream_flag(
                ae::aegp::DynamicStreamFlags::Hidden,
                false,
                !visible,
            )?;
        }
        Ok(())
    }
}

fn add_slider(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    default: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(min.max(-100.0));
            d.set_slider_max(max.min(100.0));
            d.set_default(default);
            d.set_precision(2);
        }),
    )
}

fn slider(params: &Parameters<Params>, id: Params) -> Result<f64, Error> {
    Ok(params.get(id)?.as_float_slider()?.value())
}

fn ratio(value: ae::RationalScale) -> f64 {
    if value.num > 0 && value.den > 0 {
        f64::from(value.num) / f64::from(value.den)
    } else {
        1.0
    }
}

fn read_settings(
    params: &Parameters<Params>,
    in_data: InData,
    out_data: &mut OutData,
) -> Result<RenderData, Error> {
    let settings = Settings {
        percent: params.get(Params::Units)?.as_popup()?.value() != 1,
        all: slider(params, Params::All)?,
        sides: [
            slider(params, Params::Left)?,
            slider(params, Params::Top)?,
            slider(params, Params::Right)?,
            slider(params, Params::Bottom)?,
        ],
        angle: params.get(Params::Angle)?.as_angle()?.float_value()?,
        output_size: if params.get(Params::Units)?.as_popup()?.value() == 3 {
            Some([
                slider(params, Params::OutputWidth)?,
                slider(params, Params::OutputHeight)?,
            ])
        } else {
            None
        },
    };
    let geometry = Geometry::new(
        [f64::from(in_data.width()), f64::from(in_data.height())],
        [ratio(in_data.downsample_x()), ratio(in_data.downsample_y())],
        ratio(in_data.pixel_aspect_ratio()), settings,
    ).ok_or_else(|| {
        out_data.set_error_msg("AOD_ImageResize: output exceeds 30000 pixels per side or settings are invalid. Reduce the edge amounts or output size.");
        Error::BadCallbackParameter
    })?;
    Ok(RenderData {
        geometry,
        edge: match params.get(Params::Extend)?.as_popup()?.value() {
            2 => SampleEdge::Mirror,
            3 => SampleEdge::Tile,
            4 => SampleEdge::Clamp,
            _ => SampleEdge::Transparent,
        },
        filter: if params.get(Params::Interpolation)?.as_popup()?.value() == 1 {
            SampleFilter::Nearest
        } else {
            SampleFilter::Bilinear
        },
    })
}

fn to_rect(b: Bounds) -> ae::Rect {
    ae::Rect {
        left: b.left,
        top: b.top,
        right: b.right,
        bottom: b.bottom,
    }
}
fn from_rect(b: ae::Rect) -> Bounds {
    Bounds {
        left: b.left,
        top: b.top,
        right: b.right,
        bottom: b.bottom,
    }
}

fn render(
    in_data: InData,
    input: Option<Layer>,
    mut output: Layer,
    data: RenderData,
    smart: bool,
) -> Result<(), Error> {
    let g = data.geometry;
    let source_bounds = g.source_bounds();
    let width = source_bounds.width() as usize;
    let height = source_bounds.height() as usize;
    let count = width.checked_mul(height).ok_or(Error::OutOfMemory)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| Error::OutOfMemory)?;
    pixels.resize(count, TRANSPARENT);
    let pre_origin = in_data.pre_effect_source_origin();
    let source_origin = input
        .as_ref()
        .map(|layer| {
            if smart {
                layer.origin()
            } else {
                ae::Point {
                    h: -pre_origin.h,
                    v: -pre_origin.v,
                }
            }
        })
        .unwrap_or(ae::Point { h: 0, v: 0 });
    if let Some(input) = input {
        // Restore trimmed worlds to the nominal canvas so transparent margins
        // cannot change the repeat period used by Mirror and Tile.
        for y in 0..height {
            if y % 64 == 0 {
                in_data.interact().abort()?;
            }
            let iy = y as i64 - i64::from(source_origin.v);
            if iy < 0 || iy >= input.height() as i64 {
                continue;
            }
            for x in 0..width {
                let ix = x as i64 - i64::from(source_origin.h);
                if ix >= 0 && ix < input.width() as i64 {
                    pixels[y * width + x] = read_pixel(&input, ix as usize, iy as usize);
                }
            }
        }
    }
    let origin = if smart {
        output.origin()
    } else {
        let shift = in_data.output_origin();
        ae::Point {
            h: source_origin.h - shift.h,
            v: source_origin.v - shift.v,
        }
    };
    let world_type = output.world_type();
    output.iterate(0, output.height() as i32, None, |x, y, mut dest| {
        let point = [f64::from(x + origin.h) + 0.5, f64::from(y + origin.v) + 0.5];
        let coverage = g.coverage(point);
        let pixel = if coverage == 0.0 {
            TRANSPARENT
        } else {
            let local = g.inverse(point);
            let mut p = sample_filtered(
                &pixels,
                width,
                height,
                (local[0] * g.scale[0] - 0.5) as f32,
                (local[1] * g.scale[1] - 0.5) as f32,
                data.edge,
                data.filter,
            );
            let coverage = coverage as f32;
            p.alpha *= coverage;
            p.red *= coverage;
            p.green *= coverage;
            p.blue *= coverage;
            p
        };
        match world_type {
            ae::aegp::WorldType::U8 => dest.set_from_u8(pixel.to_pixel8()),
            ae::aegp::WorldType::U15 => dest.set_from_u16(pixel.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dest.set_from_f32(pixel),
        }
        Ok(())
    })
}
