#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use color_art::{Color as ArtColor, ColorSpace as ArtColorSpace};
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsl, Hsv, Lab, LinSrgb, Oklab, Oklch, Srgb};
use std::env;
use std::str::FromStr;

use ae::pf::*;
use utils::ToPixel;

const OUTPUT_CHANNEL_COUNT: usize = 4;
const OUTPUT_CHANNEL_LABELS: [&str; OUTPUT_CHANNEL_COUNT] = ["R", "G", "B", "A"];
const INPUT_CHANNEL_OPTIONS: [&str; 7] = ["R", "G", "B", "A", "Full", "Half", "None"];
const LAYER_CHANNEL_OPTIONS: [&str; 6] = ["R", "G", "B", "A", "Full", "None"];
const SOURCE_OPTIONS: [&str; 3] = ["Input", "Value", "Layer"];
const LAYER_COLOR_SPACE_OPTIONS: [&str; 11] = [
    "Linear RGB",
    "RGB",
    "OKLAB",
    "OKLCH",
    "LAB",
    "YIQ",
    "YUV",
    "YCbCr",
    "HSL",
    "HSV",
    "CMYK",
];

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    LayerColorSpace,

    OutRGroupStart,
    OutRSource,
    OutRInputChannel,
    OutRValue,
    OutRLayer,
    OutRLayerChannel,
    OutRGroupEnd,

    OutGGroupStart,
    OutGSource,
    OutGInputChannel,
    OutGValue,
    OutGLayer,
    OutGLayerChannel,
    OutGGroupEnd,

    OutBGroupStart,
    OutBSource,
    OutBInputChannel,
    OutBValue,
    OutBLayer,
    OutBLayerChannel,
    OutBGroupEnd,

    OutAGroupStart,
    OutASource,
    OutAInputChannel,
    OutAValue,
    OutALayer,
    OutALayerChannel,
    OutAGroupEnd,
}

const GROUP_START_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRGroupStart,
    Params::OutGGroupStart,
    Params::OutBGroupStart,
    Params::OutAGroupStart,
];

const GROUP_END_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRGroupEnd,
    Params::OutGGroupEnd,
    Params::OutBGroupEnd,
    Params::OutAGroupEnd,
];

const SOURCE_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRSource,
    Params::OutGSource,
    Params::OutBSource,
    Params::OutASource,
];

const INPUT_CHANNEL_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRInputChannel,
    Params::OutGInputChannel,
    Params::OutBInputChannel,
    Params::OutAInputChannel,
];

const VALUE_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRValue,
    Params::OutGValue,
    Params::OutBValue,
    Params::OutAValue,
];

const LAYER_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRLayer,
    Params::OutGLayer,
    Params::OutBLayer,
    Params::OutALayer,
];

const LAYER_CHANNEL_PARAMS: [Params; OUTPUT_CHANNEL_COUNT] = [
    Params::OutRLayerChannel,
    Params::OutGLayerChannel,
    Params::OutBLayerChannel,
    Params::OutALayerChannel,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKind {
    Input,
    Value,
    Layer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChannelSelector {
    R,
    G,
    B,
    A,
    Full,
    Half,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LayerColorSpace {
    LinearRgb,
    Rgb,
    Oklab,
    Oklch,
    Lab,
    Yiq,
    Yuv,
    YCbCr,
    Hsl,
    Hsv,
    Cmyk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CalcColorSpace {
    Rgb,
    Oklab,
    Oklch,
    Lab,
    Yiq,
    Yuv,
    YCbCr,
    Hsl,
    Hsv,
    Cmyk,
}

#[derive(Clone, Copy, Debug)]
struct EncodedColor {
    r: f32,
    g: f32,
    b: f32,
    a_override: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct OutputMapping {
    source_kind: SourceKind,
    input_selector: ChannelSelector,
    value: f32,
    layer_selector: ChannelSelector,
    layer_slot: usize,
}

struct RenderSettings {
    layer_color_space: LayerColorSpace,
    mappings: [OutputMapping; OUTPUT_CHANNEL_COUNT],
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Remaps RGBA channels from source channels, constants, and layers with selectable color spaces.";

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
            Params::LayerColorSpace,
            "Layer Color Space",
            PopupDef::setup(|d| {
                d.set_options(&LAYER_COLOR_SPACE_OPTIONS);
                d.set_default(1);
            }),
        )?;

        for idx in 0..OUTPUT_CHANNEL_COUNT {
            let channel_name = OUTPUT_CHANNEL_LABELS[idx];
            let default_channel = (idx + 1) as i32;

            params.add_group(
                GROUP_START_PARAMS[idx],
                GROUP_END_PARAMS[idx],
                &format!("Output {}", channel_name),
                false,
                |params| {
                    params.add_with_flags(
                        SOURCE_PARAMS[idx],
                        "Source",
                        PopupDef::setup(|d| {
                            d.set_options(&SOURCE_OPTIONS);
                            d.set_default(1);
                        }),
                        supervise_flags(),
                        ae::ParamUIFlags::empty(),
                    )?;

                    params.add(
                        INPUT_CHANNEL_PARAMS[idx],
                        "Input Channel",
                        PopupDef::setup(|d| {
                            d.set_options(&INPUT_CHANNEL_OPTIONS);
                            d.set_default(default_channel);
                        }),
                    )?;

                    params.add(
                        VALUE_PARAMS[idx],
                        "Value",
                        FloatSliderDef::setup(|d| {
                            d.set_valid_min(-100000.0);
                            d.set_valid_max(100000.0);
                            d.set_slider_min(0.0);
                            d.set_slider_max(1.0);
                            d.set_default(if idx == 3 { 1.0 } else { 0.0 });
                            d.set_precision(4);
                        }),
                    )?;

                    params.add(LAYER_PARAMS[idx], "Layer", LayerDef::new())?;

                    params.add(
                        LAYER_CHANNEL_PARAMS[idx],
                        "Layer Channel",
                        PopupDef::setup(|d| {
                            d.set_options(&LAYER_CHANNEL_OPTIONS);
                            d.set_default(default_channel);
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
                        "AOD_ChannelRemap - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ChannelRemap")
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
                let changed = params.type_at(param_index);
                if SOURCE_PARAMS.contains(&changed) {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
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
    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let mut any_layer_source = false;

        for idx in 0..OUTPUT_CHANNEL_COUNT {
            let source_kind =
                source_kind_from_popup(params.get(SOURCE_PARAMS[idx])?.as_popup()?.value());
            let show_input = matches!(source_kind, SourceKind::Input);
            let show_value = matches!(source_kind, SourceKind::Value);
            let show_layer = matches!(source_kind, SourceKind::Layer);
            any_layer_source |= show_layer;

            Self::set_param_enabled(params, SOURCE_PARAMS[idx], true)?;
            self.set_param_visible(in_data, params, INPUT_CHANNEL_PARAMS[idx], show_input)?;
            self.set_param_visible(in_data, params, VALUE_PARAMS[idx], show_value)?;
            self.set_param_visible(in_data, params, LAYER_PARAMS[idx], show_layer)?;
            self.set_param_visible(in_data, params, LAYER_CHANNEL_PARAMS[idx], show_layer)?;

            Self::set_param_enabled(params, INPUT_CHANNEL_PARAMS[idx], show_input)?;
            Self::set_param_enabled(params, VALUE_PARAMS[idx], show_value)?;
            Self::set_param_enabled(params, LAYER_PARAMS[idx], show_layer)?;
            Self::set_param_enabled(params, LAYER_CHANNEL_PARAMS[idx], show_layer)?;
        }

        self.set_param_visible(in_data, params, Params::LayerColorSpace, any_layer_source)?;
        Self::set_param_enabled(params, Params::LayerColorSpace, any_layer_source)?;

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
        let layer_color_space =
            layer_color_space_from_popup(params.get(Params::LayerColorSpace)?.as_popup()?.value());

        let mut mappings = [OutputMapping {
            source_kind: SourceKind::Input,
            input_selector: ChannelSelector::R,
            value: 0.0,
            layer_selector: ChannelSelector::R,
            layer_slot: 0,
        }; OUTPUT_CHANNEL_COUNT];

        for idx in 0..OUTPUT_CHANNEL_COUNT {
            mappings[idx] = OutputMapping {
                source_kind: source_kind_from_popup(
                    params.get(SOURCE_PARAMS[idx])?.as_popup()?.value(),
                ),
                input_selector: input_channel_selector_from_popup(
                    params.get(INPUT_CHANNEL_PARAMS[idx])?.as_popup()?.value(),
                ),
                value: params.get(VALUE_PARAMS[idx])?.as_float_slider()?.value() as f32,
                layer_selector: layer_channel_selector_from_popup(
                    params.get(LAYER_CHANNEL_PARAMS[idx])?.as_popup()?.value(),
                ),
                layer_slot: idx,
            };
        }

        Ok(RenderSettings {
            layer_color_space,
            mappings,
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

        let mut layer_checkouts = Vec::with_capacity(OUTPUT_CHANNEL_COUNT);
        let mut layer_sources = Vec::with_capacity(OUTPUT_CHANNEL_COUNT);
        let mut layer_world_types = Vec::with_capacity(OUTPUT_CHANNEL_COUNT);
        for layer_param in LAYER_PARAMS.iter().take(OUTPUT_CHANNEL_COUNT) {
            let checkout = params.checkout_at(*layer_param, None, None, None)?;
            let layer = checkout.as_layer()?.value();
            let world_type = layer.as_ref().map(|l| l.world_type());
            layer_sources.push(layer);
            layer_world_types.push(world_type);
            layer_checkouts.push(checkout);
        }

        let progress_final = out_layer.height() as i32;
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );

        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |x, y, src, mut dst| {
                let x = x as usize;
                let y = y as usize;
                let src_px = read_input_pixel(src);

                let red = resolve_output_value(
                    &settings.mappings[0],
                    src_px,
                    x,
                    y,
                    &layer_sources,
                    &layer_world_types,
                    settings.layer_color_space,
                );
                let green = resolve_output_value(
                    &settings.mappings[1],
                    src_px,
                    x,
                    y,
                    &layer_sources,
                    &layer_world_types,
                    settings.layer_color_space,
                );
                let blue = resolve_output_value(
                    &settings.mappings[2],
                    src_px,
                    x,
                    y,
                    &layer_sources,
                    &layer_world_types,
                    settings.layer_color_space,
                );
                let alpha = resolve_output_value(
                    &settings.mappings[3],
                    src_px,
                    x,
                    y,
                    &layer_sources,
                    &layer_world_types,
                    settings.layer_color_space,
                );

                let clamp_01 = !out_is_f32;
                let out_px = PixelF32 {
                    red: sanitize_output(red, clamp_01),
                    green: sanitize_output(green, clamp_01),
                    blue: sanitize_output(blue, clamp_01),
                    alpha: sanitize_output(alpha, clamp_01),
                };
                write_output_pixel(&mut dst, out_px);

                Ok(())
            },
        )?;

        let _ = layer_checkouts;
        Ok(())
    }
}

fn source_kind_from_popup(value: i32) -> SourceKind {
    match value {
        2 => SourceKind::Value,
        3 => SourceKind::Layer,
        _ => SourceKind::Input,
    }
}

fn input_channel_selector_from_popup(value: i32) -> ChannelSelector {
    match value {
        2 => ChannelSelector::G,
        3 => ChannelSelector::B,
        4 => ChannelSelector::A,
        5 => ChannelSelector::Full,
        6 => ChannelSelector::Half,
        7 => ChannelSelector::None,
        _ => ChannelSelector::R,
    }
}

fn layer_channel_selector_from_popup(value: i32) -> ChannelSelector {
    match value {
        2 => ChannelSelector::G,
        3 => ChannelSelector::B,
        4 => ChannelSelector::A,
        5 => ChannelSelector::Full,
        6 => ChannelSelector::None,
        _ => ChannelSelector::R,
    }
}

fn layer_color_space_from_popup(value: i32) -> LayerColorSpace {
    match value {
        2 => LayerColorSpace::Rgb,
        3 => LayerColorSpace::Oklab,
        4 => LayerColorSpace::Oklch,
        5 => LayerColorSpace::Lab,
        6 => LayerColorSpace::Yiq,
        7 => LayerColorSpace::Yuv,
        8 => LayerColorSpace::YCbCr,
        9 => LayerColorSpace::Hsl,
        10 => LayerColorSpace::Hsv,
        11 => LayerColorSpace::Cmyk,
        _ => LayerColorSpace::LinearRgb,
    }
}

fn resolve_output_value(
    mapping: &OutputMapping,
    src_px: PixelF32,
    x: usize,
    y: usize,
    layer_sources: &[Option<Layer>],
    layer_world_types: &[Option<ae::aegp::WorldType>],
    layer_color_space: LayerColorSpace,
) -> f32 {
    match mapping.source_kind {
        SourceKind::Input => select_channel(
            [src_px.red, src_px.green, src_px.blue, src_px.alpha],
            mapping.input_selector,
        ),
        SourceKind::Value => sanitize_non_finite(mapping.value),
        SourceKind::Layer => sample_layer_channel(
            mapping.layer_slot,
            x,
            y,
            layer_sources,
            layer_world_types,
            mapping.layer_selector,
            layer_color_space,
        ),
    }
}

fn sample_layer_channel(
    slot: usize,
    x: usize,
    y: usize,
    layer_sources: &[Option<Layer>],
    layer_world_types: &[Option<ae::aegp::WorldType>],
    selector: ChannelSelector,
    layer_color_space: LayerColorSpace,
) -> f32 {
    let Some(layer) = layer_sources.get(slot).and_then(|l| l.as_ref()) else {
        return 0.0;
    };
    let Some(world_type) = layer_world_types.get(slot).copied().flatten() else {
        return 0.0;
    };

    if layer.width() == 0 || layer.height() == 0 {
        return 0.0;
    }

    let sx = x.min(layer.width().saturating_sub(1));
    let sy = y.min(layer.height().saturating_sub(1));
    let px = read_layer_pixel_f32(layer, world_type, sx, sy);
    let converted = layer_channels_in_space(px, layer_color_space);
    select_channel(converted, selector)
}

fn select_channel(channels: [f32; 4], selector: ChannelSelector) -> f32 {
    match selector {
        ChannelSelector::R => channels[0],
        ChannelSelector::G => channels[1],
        ChannelSelector::B => channels[2],
        ChannelSelector::A => channels[3],
        ChannelSelector::Full => 1.0,
        ChannelSelector::Half => 0.5,
        ChannelSelector::None => 0.0,
    }
}

fn layer_channels_in_space(px: PixelF32, color_space: LayerColorSpace) -> [f32; 4] {
    match color_space {
        LayerColorSpace::LinearRgb => {
            let lin = decode_to_linear(CalcColorSpace::Rgb, px.red, px.green, px.blue, px.alpha);
            [lin.red, lin.green, lin.blue, px.alpha]
        }
        LayerColorSpace::Rgb => [px.red, px.green, px.blue, px.alpha],
        LayerColorSpace::Oklab => encode_layer_channels(px, CalcColorSpace::Oklab),
        LayerColorSpace::Oklch => encode_layer_channels(px, CalcColorSpace::Oklch),
        LayerColorSpace::Lab => encode_layer_channels(px, CalcColorSpace::Lab),
        LayerColorSpace::Yiq => encode_layer_channels(px, CalcColorSpace::Yiq),
        LayerColorSpace::Yuv => encode_layer_channels(px, CalcColorSpace::Yuv),
        LayerColorSpace::YCbCr => encode_layer_channels(px, CalcColorSpace::YCbCr),
        LayerColorSpace::Hsl => encode_layer_channels(px, CalcColorSpace::Hsl),
        LayerColorSpace::Hsv => encode_layer_channels(px, CalcColorSpace::Hsv),
        LayerColorSpace::Cmyk => encode_layer_channels(px, CalcColorSpace::Cmyk),
    }
}

fn encode_layer_channels(px: PixelF32, space: CalcColorSpace) -> [f32; 4] {
    let lin = decode_to_linear(CalcColorSpace::Rgb, px.red, px.green, px.blue, px.alpha);
    let encoded = encode_from_linear(space, lin);
    let _k_channel = encoded.a_override.unwrap_or(px.alpha);
    [encoded.r, encoded.g, encoded.b, px.alpha]
}

fn read_layer_pixel_f32(
    layer: &Layer,
    world_type: ae::aegp::WorldType,
    x: usize,
    y: usize,
) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
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

fn sanitize_output(mut v: f32, clamp_01: bool) -> f32 {
    if !v.is_finite() {
        v = 0.0;
    }
    if clamp_01 {
        v = v.clamp(0.0, 1.0);
    }
    v
}

fn sanitize_non_finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[inline]
fn wrap01(x: f32) -> f32 {
    let mut v = x % 1.0;
    if v < 0.0 {
        v += 1.0;
    }
    v
}

#[inline]
fn encode_signed(value: f32, max_abs: f32) -> f32 {
    (value / (2.0 * max_abs)) + 0.5
}

#[inline]
fn decode_signed(channel: f32, max_abs: f32) -> f32 {
    (channel - 0.5) * (2.0 * max_abs)
}

#[inline]
fn encode_pos(value: f32, max: f32) -> f32 {
    value / max
}

#[inline]
fn decode_pos(channel: f32, max: f32) -> f32 {
    channel * max
}

fn decode_to_linear(space: CalcColorSpace, r: f32, g: f32, b: f32, a: f32) -> LinSrgb<f32> {
    const OKLAB_AB_MAX: f32 = 0.5;
    const OKLCH_CHROMA_MAX: f32 = 0.4;
    const LAB_L_MAX: f32 = 100.0;
    const LAB_AB_MAX: f32 = 128.0;
    const YIQ_I_MAX: f32 = 0.5957;
    const YIQ_Q_MAX: f32 = 0.5226;
    const YUV_U_MAX: f32 = 0.436;
    const YUV_V_MAX: f32 = 0.615;
    const YCBCR_MAX: f32 = 255.0;

    match space {
        CalcColorSpace::Rgb => Srgb::new(r, g, b).into_linear(),
        CalcColorSpace::Oklab => {
            let l = b;
            let a = decode_signed(r, OKLAB_AB_MAX);
            let bb = decode_signed(g, OKLAB_AB_MAX);
            LinSrgb::from_color(Oklab::new(l, a, bb))
        }
        CalcColorSpace::Oklch => {
            let l = b;
            let chroma = decode_pos(g, OKLCH_CHROMA_MAX);
            let hue = wrap01(r) * 360.0;
            LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)))
        }
        CalcColorSpace::Lab => {
            let l = b * LAB_L_MAX;
            let a = decode_signed(r, LAB_AB_MAX);
            let bb = decode_signed(g, LAB_AB_MAX);
            LinSrgb::from_color(Lab::new(l, a, bb))
        }
        CalcColorSpace::Yiq => {
            let y = b;
            let i = decode_signed(r, YIQ_I_MAX);
            let q = decode_signed(g, YIQ_Q_MAX);
            let spec = format!("yiq({:.6},{:.6},{:.6})", y, i, q);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::Yuv => {
            let y = b;
            let u = decode_signed(r, YUV_U_MAX);
            let v = decode_signed(g, YUV_V_MAX);
            let spec = format!("yuv({:.6},{:.6},{:.6})", y, u, v);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::YCbCr => {
            let y = decode_pos(b, YCBCR_MAX);
            let cb = decode_pos(r, YCBCR_MAX);
            let cr = decode_pos(g, YCBCR_MAX);
            let spec = format!("ycbcr({:.3},{:.3},{:.3})", y, cb, cr);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                Srgb::new(
                    (rgb[0] / 255.0) as f32,
                    (rgb[1] / 255.0) as f32,
                    (rgb[2] / 255.0) as f32,
                )
                .into_linear()
            } else {
                Srgb::new(r, g, b).into_linear()
            }
        }
        CalcColorSpace::Hsl => {
            let hue = wrap01(r) * 360.0;
            let saturation = g;
            let lightness = b;
            LinSrgb::from_color(Hsl::new(RgbHue::from_degrees(hue), saturation, lightness))
        }
        CalcColorSpace::Hsv => {
            let hue = wrap01(r) * 360.0;
            let saturation = g;
            let value = b;
            LinSrgb::from_color(Hsv::new(RgbHue::from_degrees(hue), saturation, value))
        }
        CalcColorSpace::Cmyk => {
            let c = r as f64;
            let m = g as f64;
            let y = b as f64;
            let k = a as f64;
            match ArtColor::from_cmyk(c, m, y, k) {
                Ok(color) => {
                    let rgb = color.vec_of(ArtColorSpace::RGB);
                    Srgb::new(
                        (rgb[0] / 255.0) as f32,
                        (rgb[1] / 255.0) as f32,
                        (rgb[2] / 255.0) as f32,
                    )
                    .into_linear()
                }
                Err(_) => Srgb::new(r, g, b).into_linear(),
            }
        }
    }
}

fn encode_from_linear(space: CalcColorSpace, lin: LinSrgb<f32>) -> EncodedColor {
    const OKLAB_AB_MAX: f32 = 0.5;
    const OKLCH_CHROMA_MAX: f32 = 0.4;
    const LAB_L_MAX: f32 = 100.0;
    const LAB_AB_MAX: f32 = 128.0;
    const YIQ_I_MAX: f32 = 0.5957;
    const YIQ_Q_MAX: f32 = 0.5226;
    const YUV_U_MAX: f32 = 0.436;
    const YUV_V_MAX: f32 = 0.615;
    const YCBCR_MAX: f32 = 255.0;

    match space {
        CalcColorSpace::Rgb => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            EncodedColor {
                r: srgb.red,
                g: srgb.green,
                b: srgb.blue,
                a_override: None,
            }
        }
        CalcColorSpace::Oklab => {
            let c: Oklab<f32> = Oklab::from_color(lin);
            EncodedColor {
                r: encode_signed(c.a, OKLAB_AB_MAX),
                g: encode_signed(c.b, OKLAB_AB_MAX),
                b: c.l,
                a_override: None,
            }
        }
        CalcColorSpace::Oklch => {
            let c: Oklch<f32> = Oklch::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: encode_pos(c.chroma, OKLCH_CHROMA_MAX),
                b: c.l,
                a_override: None,
            }
        }
        CalcColorSpace::Lab => {
            let c = Lab::from_color(lin);
            EncodedColor {
                r: encode_signed(c.a, LAB_AB_MAX),
                g: encode_signed(c.b, LAB_AB_MAX),
                b: c.l / LAB_L_MAX,
                a_override: None,
            }
        }
        CalcColorSpace::Yiq => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let yiq = art.vec_of(ArtColorSpace::YIQ);
            EncodedColor {
                r: encode_signed(yiq[1] as f32, YIQ_I_MAX),
                g: encode_signed(yiq[2] as f32, YIQ_Q_MAX),
                b: yiq[0] as f32,
                a_override: None,
            }
        }
        CalcColorSpace::Yuv => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let yuv = art.vec_of(ArtColorSpace::YUV);
            EncodedColor {
                r: encode_signed(yuv[1] as f32, YUV_U_MAX),
                g: encode_signed(yuv[2] as f32, YUV_V_MAX),
                b: yuv[0] as f32,
                a_override: None,
            }
        }
        CalcColorSpace::YCbCr => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let ycbcr = art.vec_of(ArtColorSpace::YCbCr);
            EncodedColor {
                r: encode_pos(ycbcr[1] as f32, YCBCR_MAX),
                g: encode_pos(ycbcr[2] as f32, YCBCR_MAX),
                b: encode_pos(ycbcr[0] as f32, YCBCR_MAX),
                a_override: None,
            }
        }
        CalcColorSpace::Hsl => {
            let c = Hsl::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: c.saturation,
                b: c.lightness,
                a_override: None,
            }
        }
        CalcColorSpace::Hsv => {
            let c = Hsv::from_color(lin);
            EncodedColor {
                r: wrap01(c.hue.into_degrees() / 360.0),
                g: c.saturation,
                b: c.value,
                a_override: None,
            }
        }
        CalcColorSpace::Cmyk => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let cmyk = art.vec_of(ArtColorSpace::CMYK);
            EncodedColor {
                r: cmyk[0] as f32,
                g: cmyk[1] as f32,
                b: cmyk[2] as f32,
                a_override: Some(cmyk[3] as f32),
            }
        }
    }
}
