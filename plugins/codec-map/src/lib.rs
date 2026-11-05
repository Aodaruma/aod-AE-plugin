// SPDX-License-Identifier: MPL-2.0
// The pinned after-effects entrypoint macro triggers these two upstream lints.
#![allow(clippy::drop_non_drop, clippy::question_mark)]
mod cache;
mod codec;
mod compute_ffi;
mod core;
mod render_error;
mod stream;
mod temporal;
mod ui;

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
    Style,
    Strength,
    QualityMode,
    AdvancedStart,
    AdvancedEnd,
    CodecStart,
    CodecEnd,
    MotionStart,
    MotionEnd,
    MapStart,
    MapEnd,
    DiagnosticsStart,
    DiagnosticsEnd,
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
    frame_keys: Vec<[u8; 16]>,
    current_map_settings: MapSettings,
    roi: bool,
    map_enabled: bool,
    current_needs_map: bool,
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
        ui::setup(params)
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
                output.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                self.cache_registered = cache::register(input)?;
                if let Ok(suite) = ae::aegp::suites::Utility::new() {
                    self.aegp_id = suite.register_with_aegp("AOD_CodecMap").ok();
                }
            }
            ae::Command::GlobalSetdown => {
                stream::clear();
                codec::unload_runtimes();
                if self.cache_registered {
                    cache::unregister(input);
                    self.cache_registered = false;
                }
            }
            ae::Command::UpdateParamsUi => update_ui(input, self.aegp_id, params)?,
            ae::Command::UserChangedParam { param_index } => {
                if Some(param_index) == params.index(Params::Reset) {
                    stream::clear();
                    let mut generation = params.get_mut(Params::Generation)?;
                    let value = generation.as_slider()?.value().wrapping_add(1) & i32::MAX;
                    generation.as_slider_mut()?.set_value(value);
                    generation.set_value_changed();
                    output.set_out_flag(OutFlags::RefreshUi, true);
                }
                ui::changed(param_index, params)?;
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
            (Params::MapLayer, choice(params, Params::MapSource)? != 2),
            (Params::DropI, independent),
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
        (Params::Strength, choice(params, Params::QualityMode)? == 2),
        (Params::Crf, choice(params, Params::QualityMode)? != 2),
        (
            Params::Vp9Quality,
            choice(params, Params::QualityMode)? != 2,
        ),
        (
            Params::Mpeg4Quality,
            choice(params, Params::QualityMode)? != 2,
        ),
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

fn map_settings(
    params: &Parameters<Params>,
    time: i32,
    input: InData,
) -> Result<MapSettings, Error> {
    let at = |id| {
        params.checkout_at(
            id,
            Some(time),
            Some(input.time_step()),
            Some(input.time_scale()),
        )
    };
    Ok(MapSettings {
        channel: (at(Params::Channel)?.as_popup()?.value() - 1) as usize,
        invert: at(Params::Invert)?.as_checkbox()?.value(),
        gamma: at(Params::Gamma)?.as_float_slider()?.value() as f32,
        black: at(Params::Black)?.as_float_slider()?.value() as f32,
        white: at(Params::White)?.as_float_slider()?.value() as f32,
    })
}
fn digest(h: &blake3::Hasher) -> [u8; 16] {
    h.finalize().as_bytes()[..16].try_into().unwrap()
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
    let simple = choice(params, Params::QualityMode)? == 1;
    let strength = float(params, Params::Strength)?;
    let mix = if simple && strength == 0.0 {
        0.0
    } else {
        (float(params, Params::Mix)? / 100.0) as f32
    };
    let preview = choice(params, Params::Preview)?;
    let decode = preview != 2 && preview != 3 && mix != 0.0;
    let mut replay = temporal::plan(
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
    // All packets after the retained seed are I pictures and will be discarded.
    if decode && !independent && gop == 1 {
        replay.times.truncate(1);
        replay.controls.truncate(1);
        replay.target = 0;
    }
    let map_enabled = choice(params, Params::MapSource)? == 2;
    let isolated = choice(params, Params::Output)? == 1;
    let roi =
        codec != codec::Codec::Mpeg4 && (codec != codec::Codec::Vp9 || (!independent && gop > 1));
    let current_map_settings = map_settings(params, input.current_time(), input)?;
    let current_needs_map = map_enabled
        && (preview == 2
            || preview == 3
            || (decode
                && (isolated
                    || (roi && current_map_settings.black != current_map_settings.white))));
    let map_index = params.index(Params::MapLayer).ok_or(Error::InvalidParms)? as i32;
    let mut request = extra.output_request();
    extra.set_returns_extra_pixels(true);
    request.channel_mask = ae::sys::PF_ChannelMask_ARGB as _;
    request.rect = ae::sys::PF_LRect {
        left: i32::MIN / 2,
        top: i32::MIN / 2,
        right: i32::MAX / 2,
        bottom: i32::MAX / 2,
    };
    request.preserve_rgb_of_zero_alpha = 1;
    let result = extra.callbacks().checkout_layer(
        0,
        1,
        &request,
        input.current_time(),
        input.time_step(),
        input.time_scale(),
    )?;
    extra.union_result_rect(result.result_rect.into());
    extra.union_max_result_rect(result.max_result_rect.into());
    if current_needs_map {
        extra.callbacks().checkout_layer(
            map_index,
            2,
            &request,
            input.current_time(),
            input.time_step(),
            input.time_scale(),
        )?;
    }
    let mut maps = Vec::with_capacity(replay.times.len());
    let mut frame_keys = Vec::with_capacity(replay.times.len());
    for (i, &time) in replay.times.iter().enumerate() {
        input.interact().abort()?;
        let settings = map_settings(params, time, input)?;
        let needs_map = map_enabled && roi && settings.black != settings.white;
        extra.callbacks().checkout_layer(
            0,
            (10 + i * 2) as i32,
            &request,
            time,
            input.time_step(),
            input.time_scale(),
        )?;
        if needs_map {
            extra.callbacks().checkout_layer(
                map_index,
                (11 + i * 2) as i32,
                &request,
                time,
                input.time_step(),
                input.time_scale(),
            )?;
        }
        let mut h = blake3::Hasher::new();
        h.update(b"codec-map-input-state-v4");
        h.update(&time.to_le_bytes());
        // The SDK supplies a 128-bit input-state hash, without rendering pixels.
        for index in [Some(0), needs_map.then_some(map_index)]
            .into_iter()
            .flatten()
        {
            let state = input.effect().current_param_state(
                index,
                Some(ae::Time {
                    value: time,
                    scale: input.time_scale(),
                }),
                Some(ae::Time {
                    value: 1,
                    scale: input.time_scale(),
                }),
            )?;
            for word in state.reservedAL {
                h.update(&word.to_le_bytes());
            }
        }
        h.update(&[roi as u8]);
        if roi {
            for v in [settings.black, settings.white] {
                h.update(&v.to_bits().to_le_bytes());
            }
            h.update(&[map_enabled as u8]);
            if needs_map {
                h.update(&settings.gamma.to_bits().to_le_bytes());
                h.update(&[settings.channel as u8, settings.invert as u8]);
            }
        }
        let c = replay.controls[i];
        h.update(&[c.force_i as u8, c.drop_i as u8, c.drop_b as u8]);
        maps.push(settings);
        frame_keys.push(digest(&h));
    }
    let start = ae::Time {
        value: replay.times[0].min(input.current_time()),
        scale: input.time_scale(),
    };
    let end = (*replay.times.last().unwrap()).max(input.current_time());
    let duration = ae::Time {
        value: i32::try_from(i64::from(end) - i64::from(start.value) + 1)
            .map_err(|_| Error::InvalidParms)?,
        scale: input.time_scale(),
    };
    let state = input.effect().current_param_state(
        ae::pf::PARAM_INDEX_CHECK_ALL,
        Some(start),
        Some(duration),
    )?;
    extra.set_pre_render_data(Plan {
        times: replay.times,
        maps,
        frame_keys,
        current_map_settings,
        roi,
        map_enabled,
        current_needs_map,
        codec,
        quality: if simple {
            ui::strength_quality(codec, strength)
        } else {
            float(
                params,
                match codec {
                    codec::Codec::H264 => Params::Crf,
                    codec::Codec::Vp9 => Params::Vp9Quality,
                    codec::Codec::Mpeg4 => Params::Mpeg4Quality,
                },
            )?
        },
        use_b,
        target: replay.target,
        controls: replay.controls,
        decode,
        replay_memory: params.get(Params::ReplayMemory)?.as_slider()?.value(),
        mix,
        isolated,
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
        let current = CheckedLayer::new(cb, 1)?;
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
        let canvas = layer_canvas(source);
        if plan.current_needs_map {
            core::check_buffer_memory(width * height * size_of::<f32>(), plan.replay_memory)?;
        }
        let current_map = if plan.current_needs_map {
            let checked = CheckedLayer::new(cb, 2)?;
            Some(sample_map(
                checked.layer.as_ref(),
                plan.current_map_settings,
                width,
                height,
            )?)
        } else {
            None
        };
        let current_offsets = offsets(
            plan.current_map_settings,
            current_map.as_deref(),
            plan.codec,
            pw,
            ph,
            width,
            height,
        );
        let black = plan.isolated
            && current_map
                .as_ref()
                .is_some_and(|m| m.iter().all(|v| *v == 0.0));
        if plan.decode && !black {
            core::check_buffer_memory(
                pw * ph * 3 / 2
                    + current_map
                        .as_ref()
                        .map_or(0, |m| m.len() * size_of::<f32>()),
                plan.replay_memory,
            )?;
        }
        let settings = codec::Settings {
            width: pw,
            height: ph,
            time_scale: i32::try_from(input.time_scale()).map_err(|_| "Time scale overflow")?,
            time_step: input
                .time_step()
                .checked_abs()
                .filter(|s| *s > 0)
                .ok_or("Invalid frame duration")?,
            codec: plan.codec,
            quality: plan.quality,
            use_b: plan.use_b,
        };
        let mut h = blake3::Hasher::new();
        h.update(b"codec-map-replay-v4");
        settings.hash(&mut h);
        h.update(&(width as u64).to_le_bytes());
        h.update(&(height as u64).to_le_bytes());
        h.update(&canvas.x.to_le_bytes());
        h.update(&canvas.y.to_le_bytes());
        h.update(&source.bit_depth().to_le_bytes());
        h.update(&plan.generation.to_le_bytes());
        let mut prefixes = Vec::with_capacity(plan.frame_keys.len());
        for key in &plan.frame_keys {
            h.update(key);
            prefixes.push(digest(&h));
        }
        let mut output_key = h.clone();
        output_key.update(&(plan.target as u64).to_le_bytes());
        let decoded = if !plan.decode || black {
            Vec::new()
        } else {
            cache::render(input, self.cache_registered, digest(&output_key), || {
                abort()?;
                core::check_working_memory(width, height, plan.replay_memory)?;
                let (image, _) = stream::render(
                    settings,
                    &prefixes,
                    &plan.controls,
                    plan.target,
                    |i| {
                        abort()?;
                        let mut key = blake3::Hasher::new();
                        key.update(b"codec-map-converted-v4");
                        key.update(&plan.frame_keys[i]);
                        for n in [
                            width as i64,
                            height as i64,
                            pw as i64,
                            ph as i64,
                            canvas.x as i64,
                            canvas.y as i64,
                            source.bit_depth() as i64,
                            plan.codec as i64,
                        ] {
                            key.update(&n.to_le_bytes());
                        }
                        let mut packed =
                            cache::render(input, self.cache_registered, digest(&key), || {
                                abort()?;
                                let mut yuv = if plan.times[i] == input.current_time() {
                                    layer_yuv(canvas, Some(source))?
                                } else {
                                    let checked = CheckedLayer::new(cb, (10 + i * 2) as u32)?;
                                    layer_yuv(canvas, checked.layer.as_ref())?
                                };
                                let map = if plan.map_enabled
                                    && plan.roi
                                    && plan.maps[i].black != plan.maps[i].white
                                {
                                    if plan.times[i] == input.current_time() {
                                        current_map.clone()
                                    } else {
                                        let checked = CheckedLayer::new(cb, (11 + i * 2) as u32)?;
                                        Some(sample_map(
                                            checked.layer.as_ref(),
                                            plan.maps[i],
                                            width,
                                            height,
                                        )?)
                                    }
                                } else {
                                    None
                                };
                                let values = offsets(
                                    plan.maps[i],
                                    map.as_deref(),
                                    plan.codec,
                                    pw,
                                    ph,
                                    width,
                                    height,
                                );
                                for v in values {
                                    yuv.extend_from_slice(&v.to_le_bytes());
                                }
                                validate_state(input, plan)?;
                                abort()?;
                                Ok(yuv)
                            })?;
                        let size = pw * ph * 3 / 2;
                        let values = packed[size..]
                            .chunks_exact(4)
                            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                            .collect();
                        packed.truncate(size);
                        Ok(codec::Frame {
                            yuv: packed,
                            offsets: values,
                            controls: plan.controls[i],
                        })
                    },
                    abort,
                )?;
                validate_state(input, plan)?;
                abort()?;
                Ok(image)
            })?
        };
        validate_state(input, plan)?;
        abort()?;
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
                        current_map.as_ref().map_or(1.0, |m| m[index])
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
                        let m = current_map.as_ref().map_or(1.0, |m| m[index]);
                        [m, m, m, 1.0]
                    }
                    3 => {
                        let block = (sy / 16) * pw.div_ceil(16) + sx / 16;
                        let m = (current_offsets[block] + 24.0) / 48.0;
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

fn validate_state(input: InData, plan: &Plan) -> Result<(), RenderError> {
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
        stream::clear();
        return Err("Inputs changed during render; render again.".into());
    }
    Ok(())
}
fn sample_map(
    layer: Option<&Layer>,
    settings: MapSettings,
    w: usize,
    h: usize,
) -> Result<Vec<f32>, RenderError> {
    if let Some(layer) = layer.filter(|s| s.width() > 0 && s.height() > 0) {
        core::padded_size(layer.width(), layer.height())?;
        Ok(
            settings.sample_from(layer.width(), layer.height(), w, h, |x, y| {
                pixel(layer, x, y)
            }),
        )
    } else {
        Ok(vec![0.0; w * h])
    }
}
fn offsets(
    settings: MapSettings,
    map: Option<&[f32]>,
    codec: codec::Codec,
    pw: usize,
    ph: usize,
    w: usize,
    h: usize,
) -> Vec<f32> {
    if codec == codec::Codec::Mpeg4 {
        vec![0.0; pw.div_ceil(16) * ph.div_ceil(16)]
    } else if let Some(map) = map {
        settings
            .blocks(map, w, h)
            .into_iter()
            .map(|v| codec.roi_offset(v))
            .collect()
    } else {
        (0..h)
            .step_by(16)
            .flat_map(|y| {
                (0..w).step_by(16).map(move |x| {
                    let count = ((h - y).min(16) * (w - x).min(16)) as f32;
                    codec.roi_offset(
                        (settings.black + (settings.white - settings.black) * count / count)
                            .clamp(-24.0, 24.0),
                    )
                })
            })
            .collect()
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
    let Some(layer) = layer.filter(|l| l.width() > 0 && l.height() > 0) else {
        return core::to_yuv_from(canvas.width, canvas.height, |_, _| [0.0; 4]);
    };
    let source = layer_canvas(layer);
    let read = |p: ae::PixelF32| [p.red, p.green, p.blue, p.alpha];
    match layer.bit_depth() {
        8 => convert_canvas(canvas, source, |x, y| {
            read(layer.as_pixel8(x, y).to_pixel32())
        }),
        16 => convert_canvas(canvas, source, |x, y| {
            read(layer.as_pixel16(x, y).to_pixel32())
        }),
        _ => convert_canvas(canvas, source, |x, y| read(*layer.as_pixel32(x, y))),
    }
}
fn convert_canvas(
    canvas: core::Canvas,
    source: core::Canvas,
    read: impl Fn(usize, usize) -> Rgba,
) -> Result<Vec<u8>, String> {
    if canvas == source {
        return core::to_yuv_from(canvas.width, canvas.height, read);
    }
    core::to_yuv_from(canvas.width, canvas.height, |x, y| {
        canvas
            .source_pixel(source, x, y)
            .map_or([0.0; 4], |(sx, sy)| read(sx, sy))
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
