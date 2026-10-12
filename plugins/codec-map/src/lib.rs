// SPDX-License-Identifier: MPL-2.0
// The pinned after-effects entrypoint macro triggers these two upstream lints.
#![allow(clippy::drop_non_drop, clippy::question_mark)]
mod cache;
mod codec;
mod compute_ffi;
mod core;
mod render_error;
mod temporal;

use after_effects as ae;
use core::{MapSettings, Rgba};
use render_error::RenderError;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Crf,
    Mode,
    Gop,
    MapSource,
    MapLayer,
    Channel,
    Invert,
    Gamma,
    Black,
    White,
    Output,
    Mix,
    Preview,
    Reset,
    Generation,
    Codec,
    Vp9Quality,
    Mpeg4Quality,
    Future,
    DropI,
    DropB,
    ReplayMemory,
}

#[derive(Default)]
struct Plugin {
    cache_registered: bool,
    aegp_id: Option<ae::aegp::PluginId>,
}
ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies map-controlled video codec compression with deterministic temporal replay.";

struct Plan {
    times: Vec<i32>,
    maps: Vec<MapSettings>,
    map_enabled: bool,
    codec: codec::Codec,
    quality: f64,
    use_b: bool,
    target: usize,
    controls: Vec<codec::Controls>,
    decode: bool,
    replay_memory: i32,
    mix: f32,
    isolated: bool,
    preview: i32,
    generation: i32,
    state: ae::sys::PF_State,
    start: ae::Time,
    duration: ae::Time,
}

fn slider(value: f64, min: f32, max: f32) -> ae::FloatSliderDef<'static> {
    ae::FloatSliderDef::setup(|p| {
        p.set_default(value)
            .set_value(value)
            .set_valid_min(min)
            .set_valid_max(max)
            .set_slider_min(min)
            .set_slider_max(max);
    })
}
fn popup(options: &[&str]) -> ae::PopupDef<'static> {
    ae::PopupDef::setup(|p| {
        p.set_options(options);
        p.set_default(1).set_value(1);
    })
}
fn float(params: &Parameters<Params>, id: Params) -> Result<f64, Error> {
    Ok(params.get(id)?.as_float_slider()?.value())
}
fn choice(params: &Parameters<Params>, id: Params) -> Result<i32, Error> {
    Ok(params.get(id)?.as_popup()?.value())
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        use ae::{ParamFlag as F, ParamUIFlags as U};
        let fixed = || F::CANNOT_TIME_VARY | F::CANNOT_INTERP;
        params.add_with_flags(
            Params::Crf,
            "Base CRF",
            slider(28.0, 1.0, 51.0),
            fixed(),
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Mode,
            "Temporal Mode",
            popup(&["Past Only", "Independent"]),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Gop,
            "GOP Length",
            ae::SliderDef::setup(|p| {
                p.set_default(12)
                    .set_value(12)
                    .set_valid_min(1)
                    .set_valid_max(60)
                    .set_slider_min(1)
                    .set_slider_max(60);
            }),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add_with_flags(
            Params::MapSource,
            "Map Source",
            popup(&["Uniform", "Layer"]),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add(Params::MapLayer, "Compression Map", ae::LayerDef::new())?;
        params.add(
            Params::Channel,
            "Map Channel",
            popup(&["Luma", "Red", "Green", "Blue", "Alpha"]),
        )?;
        params.add(
            Params::Invert,
            "Invert Map",
            ae::CheckBoxDef::setup(|p| {
                p.set_default(false).set_value(false);
            }),
        )?;
        params.add(Params::Gamma, "Map Gamma", slider(1.0, 0.1, 10.0))?;
        params.add(Params::Black, "Black QP Offset", slider(0.0, -24.0, 24.0))?;
        params.add(Params::White, "White QP Offset", slider(12.0, -24.0, 24.0))?;
        params.add(
            Params::Output,
            "Output",
            popup(&["Map Isolated", "Codec Result"]),
        )?;
        params.add(Params::Mix, "Mix", slider(100.0, 0.0, 100.0))?;
        params.add(
            Params::Preview,
            "Preview",
            popup(&["Result", "Map", "Requested QP Offset", "Difference"]),
        )?;
        params.add_with_flags(
            Params::Reset,
            "Cache",
            ae::ButtonDef::setup(|p| {
                p.set_label("Reset Cache");
            }),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Generation,
            "Cache Generation",
            ae::SliderDef::setup(|p| {
                p.set_default(0)
                    .set_value(0)
                    .set_valid_min(0)
                    .set_valid_max(i32::MAX)
                    .set_slider_min(0)
                    .set_slider_max(i32::MAX);
            }),
            fixed(),
            U::INVISIBLE,
        )?;
        // Append IDs to preserve existing AE projects and expressions.
        params.add_with_flags(
            Params::Codec,
            "Codec",
            popup(&["H.264 (libx264)", "VP9 (libvpx)", "MPEG-4 Part 2"]),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Vp9Quality,
            "VP9 CQ",
            slider(35.0, 0.0, 63.0),
            fixed(),
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Mpeg4Quality,
            "MPEG-4 Quantizer",
            slider(8.0, 1.0, 31.0),
            fixed(),
            U::NONE,
        )?;
        params.add_with_flags(
            Params::Future,
            "Allow Future Frames",
            ae::CheckBoxDef::setup(|p| {
                p.set_default(false).set_value(false);
            }),
            fixed() | F::SUPERVISE,
            U::NONE,
        )?;
        params.add(
            Params::DropI,
            "Drop I-Frames",
            ae::CheckBoxDef::setup(|p| {
                p.set_default(false).set_value(false);
            }),
        )?;
        params.add(
            Params::DropB,
            "Drop B-Frames",
            ae::CheckBoxDef::setup(|p| {
                p.set_default(false).set_value(false);
            }),
        )?;
        params.add_with_flags(
            Params::ReplayMemory,
            "Replay Memory (MiB)",
            ae::SliderDef::setup(|p| {
                p.set_default(1024)
                    .set_value(1024)
                    .set_valid_min(64)
                    .set_valid_max(8192)
                    .set_slider_min(64)
                    .set_slider_max(8192);
            }),
            fixed(),
            U::NONE,
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        input: InData,
        mut output: OutData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => output.set_return_msg(&format!(
                "AOD_CodecMap {}\r{PLUGIN_DESCRIPTION}\rH.264 / VP9 / MPEG-4 Part 2 / 8-bit YUV 4:2:0",
                env!("CARGO_PKG_VERSION")
            )),
            ae::Command::GlobalSetup => {
                output.set_out_flag(OutFlags::NonParamVary, true);
                output.set_out_flag(OutFlags::WideTimeInput, true);
                output.set_out_flag(OutFlags::DeepColorAware, true);
                output.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                output.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                output.set_out_flag2(OutFlags2::FloatColorAware, true);
                output.set_out_flag2(OutFlags2::AutomaticWideTimeInput, true);
                self.cache_registered = cache::register(input)?;
                if let Ok(suite) = ae::aegp::suites::Utility::new() {
                    self.aegp_id = suite.register_with_aegp("AOD_CodecMap").ok();
                }
            }
            ae::Command::GlobalSetdown => {
                if self.cache_registered {
                    cache::unregister(input);
                    self.cache_registered = false;
                }
            }
            ae::Command::UpdateParamsUi => update_ui(input, self.aegp_id, params)?,
            ae::Command::UserChangedParam { param_index } => {
                if Some(param_index) == params.index(Params::Reset) {
                    let mut generation = params.get_mut(Params::Generation)?;
                    let value = generation.as_slider()?.value().wrapping_add(1) & i32::MAX;
                    generation.as_slider_mut()?.set_value(value);
                    generation.set_value_changed();
                    output.set_out_flag(OutFlags::RefreshUi, true);
                }
                update_ui(input, self.aegp_id, params)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                render_error::finish(
                    pre_render(input, params, &mut extra),
                    &mut output,
                )?;
            }
            ae::Command::SmartRender { extra } => {
                let plan = extra
                    .pre_render_data::<Plan>()
                    .ok_or(Error::InternalStructDamaged)?;
                render_error::finish(self.render(input, extra.callbacks(), plan), &mut output)?;
            }
            ae::Command::Render { .. } => {
                report_error(&mut output, "Requires After Effects Smart Render.");
                return Err(Error::BadCallbackParameter);
            }
            _ => {}
        }
        Ok(())
    }
}

fn report_error(output: &mut OutData, error: &str) {
    let mut message = format!("CodecMap: {error}");
    let mut end = message.len().min(255);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message.truncate(end);
    output.set_error_msg(&message);
}

fn update_ui(
    input: InData,
    plugin_id: Option<ae::aegp::PluginId>,
    params: &mut Parameters<Params>,
) -> Result<(), Error> {
    let codec = codec::Codec::from_choice(choice(params, Params::Codec)?)
        .map_err(|_| Error::InvalidParms)?;
    let independent = choice(params, Params::Mode)? == 2;
    let gop = params.get(Params::Gop)?.as_slider()?.value();
    let b_available = codec.supports_b() && !independent && gop >= 3;
    let use_b = b_available && params.get(Params::Future)?.as_checkbox()?.value();
    let roi =
        codec != codec::Codec::Mpeg4 && (codec != codec::Codec::Vp9 || (!independent && gop > 1));
    if !input.is_premiere()
        && let Some(plugin_id) = plugin_id
        && let Ok(effect) = input.effect().aegp_effect(plugin_id)
    {
        for (id, hidden) in [
            (Params::Generation, true),
            (Params::Crf, codec != codec::Codec::H264),
            (Params::Vp9Quality, codec != codec::Codec::Vp9),
            (Params::Mpeg4Quality, codec != codec::Codec::Mpeg4),
        ] {
            if let Some(index) = params.index(id)
                && let Ok(stream) = effect.new_stream_by_index(plugin_id, index as i32)
            {
                stream.set_dynamic_stream_flag(
                    ae::aegp::DynamicStreamFlags::Hidden,
                    false,
                    hidden,
                )?;
            }
        }
    }
    let map_enabled = choice(params, Params::MapSource)? == 2;
    for (id, disabled) in [
        (Params::Gop, independent),
        (Params::MapLayer, !map_enabled),
        (Params::Channel, !map_enabled),
        (Params::Invert, !map_enabled),
        (Params::Gamma, !map_enabled),
        (Params::Black, !roi),
        (Params::White, !roi),
        (Params::Future, !b_available),
        (Params::DropI, independent),
        (Params::DropB, !use_b),
    ] {
        let mut p = params.get_mut(id)?;
        if p.ui_flags().contains(ae::ParamUIFlags::DISABLED) != disabled {
            p.set_ui_flag(ae::ParamUIFlags::DISABLED, disabled);
            p.update_param_ui()?;
        }
    }
    Ok(())
}

fn pre_render(
    input: InData,
    params: &Parameters<Params>,
    extra: &mut ae::PreRenderExtra,
) -> Result<(), RenderError> {
    let codec = codec::Codec::from_choice(choice(params, Params::Codec)?)?;
    let independent = choice(params, Params::Mode)? == 2;
    let gop = params.get(Params::Gop)?.as_slider()?.value();
    let use_b = codec.supports_b()
        && !independent
        && gop >= 3
        && params.get(Params::Future)?.as_checkbox()?.value();
    let mix = (float(params, Params::Mix)? / 100.0) as f32;
    let preview = choice(params, Params::Preview)?;
    let decode = preview != 2 && preview != 3 && mix != 0.0;
    let replay = temporal::plan(
        input.current_time(),
        input.time_step(),
        gop,
        independent || !decode,
        use_b && decode,
        |time| {
            input.interact().abort()?;
            let at = |id| {
                params.checkout_at(
                    id,
                    Some(time),
                    Some(input.time_step()),
                    Some(input.time_scale()),
                )
            };
            Ok((
                at(Params::DropI)?.as_checkbox()?.value(),
                at(Params::DropB)?.as_checkbox()?.value(),
            ))
        },
    )?;
    let times = replay.times;
    let map_enabled = choice(params, Params::MapSource)? == 2;
    let mut request = extra.output_request();
    // A codec frame is a full image; AE may ask for a tile or a preview ROI.
    // Advertise the full result explicitly instead of claiming it fits that ROI.
    extra.set_returns_extra_pixels(true);
    request.channel_mask = ae::sys::PF_ChannelMask_ARGB as _;
    request.rect = ae::sys::PF_LRect {
        left: i32::MIN / 2,
        top: i32::MIN / 2,
        right: i32::MAX / 2,
        bottom: i32::MAX / 2,
    };
    request.preserve_rgb_of_zero_alpha = 1;
    let mut maps = Vec::with_capacity(times.len());
    for (i, &time) in times.iter().enumerate() {
        let result = extra.callbacks().checkout_layer(
            0,
            (i * 2) as i32,
            &request,
            time,
            input.time_step(),
            input.time_scale(),
        )?;
        if i == replay.target {
            extra.union_result_rect(result.result_rect.into());
            extra.union_max_result_rect(result.max_result_rect.into());
        }
        if map_enabled {
            extra.callbacks().checkout_layer(
                params.index(Params::MapLayer).ok_or(Error::InvalidParms)? as i32,
                (i * 2 + 1) as i32,
                &request,
                time,
                input.time_step(),
                input.time_scale(),
            )?;
        }
        let at = |id| {
            params.checkout_at(
                id,
                Some(time),
                Some(input.time_step()),
                Some(input.time_scale()),
            )
        };
        maps.push(MapSettings {
            channel: (at(Params::Channel)?.as_popup()?.value() - 1) as usize,
            invert: at(Params::Invert)?.as_checkbox()?.value(),
            gamma: at(Params::Gamma)?.as_float_slider()?.value() as f32,
            black: at(Params::Black)?.as_float_slider()?.value() as f32,
            white: at(Params::White)?.as_float_slider()?.value() as f32,
        });
    }
    let start = ae::Time {
        value: times[0],
        scale: input.time_scale(),
    };
    let duration = ae::Time {
        value: i32::try_from(i64::from(*times.last().unwrap()) - i64::from(times[0]) + 1)
            .map_err(|_| Error::InvalidParms)?,
        scale: input.time_scale(),
    };
    let state = input.effect().current_param_state(
        ae::pf::PARAM_INDEX_CHECK_ALL,
        Some(start),
        Some(duration),
    )?;
    extra.set_pre_render_data(Plan {
        times,
        maps,
        map_enabled,
        codec,
        quality: float(
            params,
            match codec {
                codec::Codec::H264 => Params::Crf,
                codec::Codec::Vp9 => Params::Vp9Quality,
                codec::Codec::Mpeg4 => Params::Mpeg4Quality,
            },
        )?,
        use_b,
        target: replay.target,
        controls: replay.controls,
        decode,
        replay_memory: params.get(Params::ReplayMemory)?.as_slider()?.value(),
        mix,
        isolated: choice(params, Params::Output)? == 1,
        preview,
        generation: params.get(Params::Generation)?.as_slider()?.value(),
        state,
        start,
        duration,
    });
    Ok(())
}

struct CheckedLayer {
    callbacks: ae::SmartRenderCallbacks,
    id: u32,
    layer: Option<Layer>,
}
impl CheckedLayer {
    fn new(callbacks: ae::SmartRenderCallbacks, id: u32) -> Result<Self, RenderError> {
        Ok(Self {
            layer: callbacks.checkout_layer_pixels(id)?,
            callbacks,
            id,
        })
    }
}
impl Drop for CheckedLayer {
    fn drop(&mut self) {
        self.layer.take();
        let _ = self.callbacks.checkin_layer_pixels(self.id);
    }
}

impl Plugin {
    fn render(
        &self,
        input: InData,
        cb: ae::SmartRenderCallbacks,
        plan: &Plan,
    ) -> Result<(), RenderError> {
        let abort = || input.interact().abort().map_err(RenderError::from);
        abort()?;
        let current = CheckedLayer::new(cb, (plan.target * 2) as u32)?;
        // SmartFX requires an input checkout before the output checkout.
        let Some(mut output) = cb.checkout_output()? else {
            return Ok(());
        };
        let Some(source) = current
            .layer
            .as_ref()
            .filter(|s| s.width() > 0 && s.height() > 0)
        else {
            for y in 0..output.height() {
                for x in 0..output.width() {
                    write_pixel(&mut output, x, y, [0.0; 4]);
                }
            }
            return Ok(());
        };
        let (width, height) = (source.width(), source.height());
        let (pw, ph, _) = core::padded_size(width, height)?;
        if plan.decode {
            core::check_replay_memory(width, height, plan.times.len(), plan.replay_memory)?;
        }
        let canvas = layer_canvas(source);
        let mut job = codec::Job {
            width: pw,
            height: ph,
            time_scale: i32::try_from(input.time_scale()).map_err(|_| "Time scale overflow")?,
            time_step: input
                .time_step()
                .checked_abs()
                .filter(|v| *v > 0)
                .ok_or("Invalid frame duration")?,
            codec: plan.codec,
            quality: plan.quality,
            use_b: plan.use_b,
            target: plan.target,
            frames: Vec::with_capacity(plan.times.len()),
        };
        let mut current_map = Vec::new();
        for (i, settings) in plan.maps.iter().enumerate() {
            abort()?;
            let map = if plan.map_enabled {
                let checked = CheckedLayer::new(cb, (i * 2 + 1) as u32)?;
                if let Some(layer) = checked
                    .layer
                    .as_ref()
                    .filter(|s| s.width() > 0 && s.height() > 0)
                {
                    core::padded_size(layer.width(), layer.height())?;
                    let p = (0..layer.height())
                        .flat_map(|y| (0..layer.width()).map(move |x| pixel(layer, x, y)))
                        .collect::<Vec<_>>();
                    settings.sample(&p, layer.width(), layer.height(), width, height)
                } else {
                    vec![0.0; width * height]
                }
            } else {
                vec![1.0; width * height]
            };
            let offsets = settings
                .blocks(&map, width, height)
                .into_iter()
                .map(|value| plan.codec.roi_offset(value))
                .collect();
            let yuv = if !plan.decode {
                Vec::new()
            } else if i == plan.target {
                layer_yuv(canvas, Some(source))?
            } else {
                let checked = CheckedLayer::new(cb, (i * 2) as u32)?;
                layer_yuv(canvas, checked.layer.as_ref())?
            };
            job.frames.push(codec::Frame {
                yuv,
                offsets,
                controls: plan.controls[i],
            });
            if i == plan.target {
                current_map = map;
            }
        }
        let state = input.effect().current_param_state(
            ae::pf::PARAM_INDEX_CHECK_ALL,
            Some(plan.start),
            Some(plan.duration),
        )?;
        if !ae::pf::suites::ParamUtils::new()?.are_states_identical(
            input.effect(),
            &plan.state,
            &state,
        )? {
            return Err("Inputs changed during render; render again.".into());
        }
        let mut salt = plan.generation.to_le_bytes().to_vec();
        for time in &plan.times {
            salt.extend_from_slice(&time.to_le_bytes());
        }
        let decoded = if !plan.decode {
            Vec::new()
        } else {
            cache::render(input, self.cache_registered, &job, &salt, abort)?
        };
        let output_canvas = layer_canvas(&output);
        for y in 0..output.height() {
            abort()?;
            for x in 0..output.width() {
                let Some((sx, sy)) = output_canvas.source_pixel(canvas, x, y) else {
                    write_pixel(&mut output, x, y, [0.0; 4]);
                    continue;
                };
                let index = sy * width + sx;
                let weight = core::finite_unit(plan.mix)
                    * if plan.isolated {
                        current_map[index]
                    } else {
                        1.0
                    };
                if plan.preview == 1 && weight == 0.0 {
                    copy_pixel(source, sx, sy, &mut output, x, y);
                    continue;
                }
                let rgb = if decoded.is_empty() {
                    [0.0; 3]
                } else {
                    core::decoded_rgb(&decoded, pw, ph, sx, sy)
                };
                let p = pixel(source, sx, sy);
                let original = [p.red, p.green, p.blue, p.alpha];
                let result = core::composite(original, rgb, weight);
                let result = match plan.preview {
                    2 => {
                        let m = current_map[index];
                        [m, m, m, 1.0]
                    }
                    3 => {
                        let block = (sy / 16) * pw.div_ceil(16) + sx / 16;
                        let m = (job.frames[plan.target].offsets[block] + 24.0) / 48.0;
                        [m, m, m, 1.0]
                    }
                    4 => [
                        (result[0] - original[0]).abs(),
                        (result[1] - original[1]).abs(),
                        (result[2] - original[2]).abs(),
                        1.0,
                    ],
                    _ => result,
                };
                write_pixel(&mut output, x, y, result);
            }
        }
        Ok(())
    }
}

fn pixel(layer: &Layer, x: usize, y: usize) -> ae::PixelF32 {
    match layer.bit_depth() {
        8 => layer.as_pixel8(x, y).to_pixel32(),
        16 => layer.as_pixel16(x, y).to_pixel32(),
        _ => *layer.as_pixel32(x, y),
    }
}
fn layer_canvas(layer: &Layer) -> core::Canvas {
    core::Canvas {
        width: layer.width(),
        height: layer.height(),
        x: layer.origin().h,
        y: layer.origin().v,
    }
}
fn layer_yuv(canvas: core::Canvas, layer: Option<&Layer>) -> Result<Vec<u8>, String> {
    // All historical frames use the current frame's canvas. Preserve layer
    // coordinates, clip excess pixels and fill uncovered regions with black.
    let source = layer.map(layer_canvas);
    core::to_yuv_from(canvas.width, canvas.height, |x, y| {
        if let Some((layer, (sx, sy))) =
            layer.zip(source.and_then(|s| canvas.source_pixel(s, x, y)))
        {
            let p = pixel(layer, sx, sy);
            [p.red, p.green, p.blue, p.alpha]
        } else {
            [0.0; 4]
        }
    })
}
fn write_pixel(layer: &mut Layer, x: usize, y: usize, p: Rgba) {
    let p = ae::PixelF32 {
        red: p[0],
        green: p[1],
        blue: p[2],
        alpha: p[3],
    };
    match layer.bit_depth() {
        8 => *layer.as_pixel8_mut(x, y) = p.to_pixel8(),
        16 => *layer.as_pixel16_mut(x, y) = p.to_pixel16(),
        _ => *layer.as_pixel32_mut(x, y) = p,
    }
}
fn copy_pixel(source: &Layer, sx: usize, sy: usize, output: &mut Layer, x: usize, y: usize) {
    match output.bit_depth() {
        8 => *output.as_pixel8_mut(x, y) = *source.as_pixel8(sx, sy),
        16 => *output.as_pixel16_mut(x, y) = *source.as_pixel16(sx, sy),
        _ => *output.as_pixel32_mut(x, y) = *source.as_pixel32(sx, sy),
    }
}
