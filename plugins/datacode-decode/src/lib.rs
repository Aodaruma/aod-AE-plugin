#![allow(
    clippy::drop_non_drop,
    clippy::question_mark,
    clippy::too_many_arguments
)]

use after_effects as ae;
use std::collections::HashSet;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use ae::pf::*;
use rxing::{BarcodeFormat, DecodeHints, RXingResult, helpers};
use utils::ToPixel;
use utils::datacode_custom as custom_code;

mod debug_log;
mod decode_core;
mod overlay;
mod settings;

use debug_log as dlog;
use decode_core::{
    bbox_from_dark_luma, bbox_from_points, build_luma8, bytes_to_hex_upper, clamp_rect, crop_luma,
    read_layer_pixel,
};
use overlay::{
    composite_with_overlay, draw_error_overlay, draw_rect_outline, draw_text_wrapped,
    transparent_pixel,
};
use settings::{possible_formats, read_detect_mode, read_settings};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    DetectMode,
    FormatHint,
    RegionOrigin,
    RegionWidth,
    RegionHeight,
    TryHarder,
    AlsoInverted,
    RenderDecoded,
    RenderAsHex,
    TrimZeroPadding,
    GenerateDecodeText,
    MaskDecodedRegion,
    RegionOnlyOutput,
    MaskOpacity,
    MaskColor,
    CustomCellPixelSize,
    CustomCellWidth,
    CustomCellHeight,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Decodes 1D and 2D data codes from layers with auto or region-based detection.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetectMode {
    Auto,
    Region,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatHint {
    Any,
    OneD,
    TwoD,
    Qr,
    DataMatrix,
    Pdf417,
    Rmqr,
    ColorCode,
    JustEmbedding,
}

#[derive(Clone, Debug)]
struct RenderSettings {
    detect_mode: DetectMode,
    format_hint: FormatHint,
    region_origin: (i32, i32),
    region_width: usize,
    region_height: usize,
    try_harder: bool,
    also_inverted: bool,
    render_decoded: bool,
    render_as_hex: bool,
    trim_zero_padding: bool,
    mask_decoded_region: bool,
    region_only_output: bool,
    mask_opacity: f32,
    mask_color: [f32; 3],
    custom_cell_pixel_size: usize,
    custom_cell_width: usize,
    custom_cell_height: usize,
}

#[derive(Clone, Debug)]
struct DecodedData {
    format: String,
    text: String,
    hex: String,
    bbox: (i32, i32, i32, i32),
}

struct DecodedResult {
    result: RXingResult,
    origin_offset: (i32, i32),
    content_bbox: Option<(i32, i32, i32, i32)>,
}

struct DecodeAttempt {
    luma: Vec<u8>,
    width: u32,
    height: u32,
    origin_offset: (i32, i32),
    pure: bool,
    name: String,
}

static DECODE_TEXT_LAYER_COUNTER: AtomicU64 = AtomicU64::new(1);
static LAST_DECODED_FOR_TEXT: LazyLock<Mutex<Option<DecodedData>>> =
    LazyLock::new(|| Mutex::new(None));

impl DecodeAttempt {
    fn new(
        luma: Vec<u8>,
        width: u32,
        height: u32,
        origin_offset: (i32, i32),
        pure: bool,
        name: &str,
    ) -> Self {
        Self {
            luma,
            width,
            height,
            origin_offset,
            pure,
            name: name.to_string(),
        }
    }

    fn padded(
        luma: &[u8],
        width: u32,
        height: u32,
        origin_offset: (i32, i32),
        border: u32,
        pure: bool,
    ) -> Self {
        let padded_w = width + border * 2;
        let padded_h = height + border * 2;
        let mut padded = vec![u8::MAX; (padded_w * padded_h) as usize];
        for y in 0..height {
            let src_row = (y * width) as usize;
            let dst_row = ((y + border) * padded_w + border) as usize;
            let count = width as usize;
            padded[dst_row..dst_row + count].copy_from_slice(&luma[src_row..src_row + count]);
        }
        let name = if pure {
            format!("padded{border}_pure")
        } else {
            format!("padded{border}")
        };
        Self {
            luma: padded,
            width: padded_w,
            height: padded_h,
            origin_offset: (
                origin_offset.0 - border as i32,
                origin_offset.1 - border as i32,
            ),
            pure,
            name,
        }
    }
}

fn qr_family_format(format_hint: FormatHint) -> bool {
    matches!(
        format_hint,
        FormatHint::Any | FormatHint::TwoD | FormatHint::Qr | FormatHint::Rmqr
    )
}

fn custom_format(format_hint: FormatHint) -> bool {
    matches!(
        format_hint,
        FormatHint::ColorCode | FormatHint::JustEmbedding
    )
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::DetectMode,
            "Detect Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Auto Detect", "Region"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::FormatHint,
            "Format Hint",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Any",
                    "1D",
                    "2D",
                    "QR",
                    "Data Matrix",
                    "PDF417",
                    "rMQR",
                    "Color Code",
                    "Just Embedding",
                ]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::RegionOrigin,
            "Region Origin",
            PointDef::setup(|p| {
                p.set_default((10.0, 10.0));
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::RegionWidth,
            "Region Width (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(8.0);
                d.set_valid_max(8192.0);
                d.set_slider_min(32.0);
                d.set_slider_max(2048.0);
                d.set_default(320.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::RegionHeight,
            "Region Height (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(8.0);
                d.set_valid_max(8192.0);
                d.set_slider_min(32.0);
                d.set_slider_max(2048.0);
                d.set_default(320.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add(
            Params::TryHarder,
            "Try Harder",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;
        params.add(
            Params::AlsoInverted,
            "Also Inverted",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;
        params.add(
            Params::RenderDecoded,
            "Render Decoded Text",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;
        params.add(
            Params::RenderAsHex,
            "Render As HEX",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;
        params.add(
            Params::TrimZeroPadding,
            "Trim Zero Padding",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;
        params.add_with_flags(
            Params::GenerateDecodeText,
            "Decode To Text Layer",
            ButtonDef::setup(|d| {
                d.set_label("Generate");
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::MaskDecodedRegion,
            "Mask Decoded Region",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::RegionOnlyOutput,
            "Region Only Output",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::MaskOpacity,
            "Mask Opacity",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(100.0);
                d.set_slider_min(0.0);
                d.set_slider_max(100.0);
                d.set_default(85.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::MaskColor,
            "Mask Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: u8::MAX,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::CustomCellPixelSize,
            "Custom Cell Pixel Size",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(128.0);
                d.set_slider_min(1.0);
                d.set_slider_max(32.0);
                d.set_default(8.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::CustomCellWidth,
            "Custom Cell Width",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(8.0);
                d.set_slider_max(256.0);
                d.set_default(96.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::CustomCellHeight,
            "Custom Cell Height",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(2048.0);
                d.set_slider_min(8.0);
                d.set_slider_max(256.0);
                d.set_default(96.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
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
                        "AOD_DatacodeDecode - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                dlog::session_start();
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_DatacodeDecode")
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
                if changed == Params::GenerateDecodeText {
                    self.create_decode_text_layer(in_data, &mut out_data, params)?;
                } else if matches!(
                    changed,
                    Params::DetectMode | Params::FormatHint | Params::MaskDecodedRegion
                ) {
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
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let detect_mode = read_detect_mode(params)?;
        let format_hint = settings::read_format_hint(params)?;
        let show_custom = custom_format(format_hint);
        let show_region = matches!(detect_mode, DetectMode::Region) || show_custom;
        let show_mask_controls = params
            .get(Params::MaskDecodedRegion)?
            .as_checkbox()?
            .value();
        self.set_param_visible(in_data, params, Params::RegionOrigin, show_region)?;
        self.set_param_visible(in_data, params, Params::RegionWidth, show_region)?;
        self.set_param_visible(in_data, params, Params::RegionHeight, show_region)?;
        self.set_param_visible(in_data, params, Params::MaskOpacity, show_mask_controls)?;
        self.set_param_visible(in_data, params, Params::MaskColor, show_mask_controls)?;
        self.set_param_visible(in_data, params, Params::CustomCellPixelSize, show_custom)?;
        self.set_param_visible(in_data, params, Params::CustomCellWidth, show_custom)?;
        self.set_param_visible(in_data, params, Params::CustomCellHeight, show_custom)?;
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

    fn set_param_ui_flag(
        params: &mut ae::Parameters<Params>,
        id: Params,
        flag: ParamUIFlags,
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

    fn decode_from_layer(layer: &Layer, settings: &RenderSettings) -> Result<DecodedData, String> {
        let width = layer.width();
        let height = layer.height();
        if width == 0 || height == 0 {
            return Err("EMPTY INPUT LAYER".to_string());
        }
        if custom_format(settings.format_hint) {
            return Self::decode_custom_from_layer(layer, settings);
        }

        let full_luma = build_luma8(layer);
        let (decode_luma, decode_w, decode_h, origin_offset) =
            if matches!(settings.detect_mode, DetectMode::Region) {
                let (rx, ry) = settings.region_origin;
                let rw = settings.region_width as i32;
                let rh = settings.region_height as i32;
                let region = clamp_rect(rx, ry, rw, rh, width as i32, height as i32);
                let crop = crop_luma(&full_luma, width, height, region);
                (crop, region.2 as u32, region.3 as u32, (region.0, region.1))
            } else {
                (full_luma, width as u32, height as u32, (0_i32, 0_i32))
            };

        let hints = DecodeHints {
            TryHarder: Some(settings.try_harder),
            AlsoInverted: Some(settings.also_inverted),
            PossibleFormats: possible_formats(settings.format_hint),
            ..Default::default()
        };

        let decoded_result = Self::decode_luma_with_fallbacks(
            decode_luma,
            decode_w,
            decode_h,
            origin_offset,
            &hints,
            settings.format_hint,
        )?;
        let result_origin_offset = decoded_result.origin_offset;
        let content_bbox = decoded_result.content_bbox;
        let result = decoded_result.result;
        let format_value = *result.getBarcodeFormat();

        let mut raw = result.getRawBytes().to_vec();
        if settings.trim_zero_padding {
            while raw.last().copied() == Some(0) {
                raw.pop();
            }
        }
        let format = format!("{:?}", result.getBarcodeFormat());
        let mut text = result.getText().to_string();
        if text.is_empty() && !raw.is_empty() {
            text = String::from_utf8_lossy(&raw).to_string();
        }
        let hex = bytes_to_hex_upper(&raw);
        let points_bbox = bbox_from_points(result.getPoints(), result_origin_offset);
        let bbox = Self::select_decode_bbox(format_value, points_bbox, content_bbox).unwrap_or((
            origin_offset.0,
            origin_offset.1,
            220,
            120,
        ));

        Ok(DecodedData {
            format,
            text,
            hex,
            bbox,
        })
    }

    fn decode_custom_from_layer(
        layer: &Layer,
        settings: &RenderSettings,
    ) -> Result<DecodedData, String> {
        let rect = Self::custom_decode_rect(layer, settings)?;
        let payload = match settings.format_hint {
            FormatHint::ColorCode => Self::decode_color_code_payload(layer, settings, rect)?,
            FormatHint::JustEmbedding => {
                Self::decode_just_embedding_payload(layer, settings, rect)?
            }
            _ => return Err("CUSTOM DECODER CALLED WITHOUT CUSTOM FORMAT HINT".into()),
        };
        let mut raw = payload.payload;
        if settings.trim_zero_padding {
            while raw.last().copied() == Some(0) {
                raw.pop();
            }
        }
        let text = String::from_utf8_lossy(&raw).to_string();
        let hex = bytes_to_hex_upper(&raw);
        let format = match payload.kind {
            custom_code::KIND_COLOR_CODE => "COLOR_CODE",
            custom_code::KIND_JUST_MONO => "JUST_EMBEDDING_MONO",
            custom_code::KIND_JUST_COLOR => "JUST_EMBEDDING_COLOR",
            _ => "CUSTOM_DATA_CODE",
        }
        .to_string();
        Ok(DecodedData {
            format,
            text,
            hex,
            bbox: rect,
        })
    }

    fn custom_decode_rect(
        layer: &Layer,
        settings: &RenderSettings,
    ) -> Result<(i32, i32, i32, i32), String> {
        let (x, y) = settings.region_origin;
        let w = (settings.custom_cell_width * settings.custom_cell_pixel_size) as i32;
        let h = (settings.custom_cell_height * settings.custom_cell_pixel_size) as i32;
        if w <= 0 || h <= 0 {
            return Err("CUSTOM GRID SIZE IS EMPTY".into());
        }
        if x < 0 || y < 0 || x + w > layer.width() as i32 || y + h > layer.height() as i32 {
            return Err("CUSTOM GRID REGION IS OUTSIDE THE LAYER".into());
        }
        Ok((x, y, w, h))
    }

    fn decode_color_code_payload(
        layer: &Layer,
        settings: &RenderSettings,
        rect: (i32, i32, i32, i32),
    ) -> Result<custom_code::ParsedPayload, String> {
        let positions = custom_code::jab_fill_positions(
            settings.custom_cell_width,
            settings.custom_cell_height,
        );
        let mut bits = Vec::with_capacity(positions.len() * 3);
        for (mx, my) in positions {
            let idx = custom_code::nearest_palette_index(Self::sample_custom_cell_rgb(
                layer,
                rect,
                settings.custom_cell_pixel_size,
                mx,
                my,
            ));
            bits.push((idx & 4) != 0);
            bits.push((idx & 2) != 0);
            bits.push((idx & 1) != 0);
        }
        let bytes = custom_code::bits_to_bytes(&bits);
        custom_code::parse_payload(&bytes, &[custom_code::KIND_COLOR_CODE])
    }

    fn decode_just_embedding_payload(
        layer: &Layer,
        settings: &RenderSettings,
        rect: (i32, i32, i32, i32),
    ) -> Result<custom_code::ParsedPayload, String> {
        let mut mono_bits =
            Vec::with_capacity(settings.custom_cell_width * settings.custom_cell_height);
        for my in 0..settings.custom_cell_height {
            for mx in 0..settings.custom_cell_width {
                let rgb = Self::sample_custom_cell_rgb(
                    layer,
                    rect,
                    settings.custom_cell_pixel_size,
                    mx,
                    my,
                );
                let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                mono_bits.push(luma < 0.5);
            }
        }
        let mono_bytes = custom_code::bits_to_bytes(&mono_bits);
        match custom_code::parse_payload(&mono_bytes, &[custom_code::KIND_JUST_MONO]) {
            Ok(payload) => Ok(payload),
            Err(mono_error) => {
                let mut color_bits = Vec::with_capacity(
                    settings.custom_cell_width * settings.custom_cell_height * 3,
                );
                for my in 0..settings.custom_cell_height {
                    for mx in 0..settings.custom_cell_width {
                        let idx = custom_code::nearest_palette_index(Self::sample_custom_cell_rgb(
                            layer,
                            rect,
                            settings.custom_cell_pixel_size,
                            mx,
                            my,
                        ));
                        color_bits.push((idx & 4) != 0);
                        color_bits.push((idx & 2) != 0);
                        color_bits.push((idx & 1) != 0);
                    }
                }
                let color_bytes = custom_code::bits_to_bytes(&color_bits);
                custom_code::parse_payload(&color_bytes, &[custom_code::KIND_JUST_COLOR])
                    .map_err(|color_error| format!("MONO={mono_error}; COLOR={color_error}"))
            }
        }
    }

    fn sample_custom_cell_rgb(
        layer: &Layer,
        rect: (i32, i32, i32, i32),
        cell_px: usize,
        mx: usize,
        my: usize,
    ) -> [f32; 3] {
        let cell_px = cell_px.max(1) as i32;
        let sx = (rect.0 + mx as i32 * cell_px + cell_px / 2)
            .clamp(0, layer.width().saturating_sub(1) as i32) as usize;
        let sy = (rect.1 + my as i32 * cell_px + cell_px / 2)
            .clamp(0, layer.height().saturating_sub(1) as i32) as usize;
        let px = read_layer_pixel(layer, layer.world_type(), sx, sy);
        if px.alpha > 1.0e-6 {
            [
                (px.red / px.alpha).clamp(0.0, 1.0),
                (px.green / px.alpha).clamp(0.0, 1.0),
                (px.blue / px.alpha).clamp(0.0, 1.0),
            ]
        } else {
            [1.0, 1.0, 1.0]
        }
    }

    fn select_decode_bbox(
        format: BarcodeFormat,
        points_bbox: Option<(i32, i32, i32, i32)>,
        content_bbox: Option<(i32, i32, i32, i32)>,
    ) -> Option<(i32, i32, i32, i32)> {
        if matches!(format, BarcodeFormat::RECTANGULAR_MICRO_QR_CODE) {
            return content_bbox.or(points_bbox);
        }
        match (points_bbox, content_bbox) {
            (Some(points), Some(content)) if Self::bbox_is_suspicious(points, content) => {
                Some(content)
            }
            (Some(points), _) => Some(points),
            (None, Some(content)) => Some(content),
            (None, None) => None,
        }
    }

    fn bbox_is_suspicious(points: (i32, i32, i32, i32), content: (i32, i32, i32, i32)) -> bool {
        let points_area = points.2.saturating_mul(points.3);
        let content_area = content.2.saturating_mul(content.3);
        points.0 <= content.0
            && points.1 <= content.1
            && content_area > 0
            && points_area > content_area.saturating_mul(2)
    }

    fn decode_luma_with_fallbacks(
        luma: Vec<u8>,
        width: u32,
        height: u32,
        origin_offset: (i32, i32),
        base_hints: &DecodeHints,
        format_hint: FormatHint,
    ) -> Result<DecodedResult, String> {
        let mut errors = Vec::new();
        let attempts = if qr_family_format(format_hint) {
            vec![
                DecodeAttempt::new(luma.clone(), width, height, origin_offset, false, "normal"),
                DecodeAttempt::new(luma.clone(), width, height, origin_offset, true, "pure"),
                DecodeAttempt::padded(&luma, width, height, origin_offset, 16, false),
                DecodeAttempt::padded(&luma, width, height, origin_offset, 16, true),
                DecodeAttempt::padded(&luma, width, height, origin_offset, 32, false),
                DecodeAttempt::padded(&luma, width, height, origin_offset, 32, true),
            ]
        } else {
            vec![
                DecodeAttempt::new(luma.clone(), width, height, origin_offset, false, "normal"),
                DecodeAttempt::new(luma, width, height, origin_offset, true, "pure"),
            ]
        };

        for attempt in attempts {
            let content_bbox = bbox_from_dark_luma(
                &attempt.luma,
                attempt.width,
                attempt.height,
                attempt.origin_offset,
            );
            let mut hints = base_hints.clone();
            if attempt.pure {
                hints.PureBarcode = Some(true);
            }
            match helpers::detect_in_luma_with_hints(
                attempt.luma,
                attempt.width,
                attempt.height,
                None,
                &mut hints,
            ) {
                Ok(result) => {
                    dlog::log(format!(
                        "decode succeeded via {} format={:?}",
                        attempt.name,
                        result.getBarcodeFormat()
                    ));
                    return Ok(DecodedResult {
                        result,
                        origin_offset: attempt.origin_offset,
                        content_bbox,
                    });
                }
                Err(e) => errors.push(format!("{}={e:?}", attempt.name)),
            }
        }

        Err(errors.join("; "))
    }

    fn js_string_literal(src: &str) -> String {
        let mut out = String::with_capacity(src.len() + 8);
        out.push('"');
        for ch in src.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\r' => out.push_str("\\r"),
                '\n' => out.push_str("\\r"),
                '\u{2028}' => out.push_str("\\u2028"),
                '\u{2029}' => out.push_str("\\u2029"),
                c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
                _ => out.push(ch),
            }
        }
        out.push('"');
        out
    }

    fn cache_decoded_for_text(decoded: &DecodedData) {
        if let Ok(mut slot) = LAST_DECODED_FOR_TEXT.lock() {
            *slot = Some(decoded.clone());
        }
    }

    fn load_cached_decoded_for_text() -> Option<DecodedData> {
        LAST_DECODED_FOR_TEXT
            .lock()
            .ok()
            .and_then(|slot| (*slot).clone())
    }

    fn create_decode_text_layer(
        &self,
        in_data: InData,
        _out_data: &mut OutData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        dlog::log("GenerateDecodeText begin");
        if !in_data.is_after_effects() {
            dlog::log("GenerateDecodeText skipped: not After Effects");
            return Ok(());
        }
        let Some(plugin_id) = self.aegp_id else {
            dlog::log("GenerateDecodeText error: AEGP plugin id is not initialized");
            return Ok(());
        };

        let checked_input = match ParamDef::checkout(
            in_data,
            0,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
            Some(ParamType::Layer),
        ) {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to checkout input layer: {e}"
                ));
                return Ok(());
            }
        };
        let input_layer_param = match checked_input.as_layer() {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to access input layer parameter: {e}"
                ));
                return Ok(());
            }
        };
        let Some(input_layer) = input_layer_param.value() else {
            dlog::log("GenerateDecodeText error: input layer is not available");
            return Ok(());
        };

        let settings = match read_settings(params) {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to read decode settings: {e}"
                ));
                return Ok(());
            }
        };
        let decoded = match Self::decode_from_layer(&input_layer, &settings) {
            Ok(v) => {
                Self::cache_decoded_for_text(&v);
                v
            }
            Err(msg) => {
                dlog::log(format!(
                    "GenerateDecodeText warn: live decode failed, fallback to last decoded data: {msg}"
                ));
                let Some(cached) = Self::load_cached_decoded_for_text() else {
                    dlog::log("GenerateDecodeText error: no cached decoded data is available");
                    return Ok(());
                };
                cached
            }
        };
        let draw_text = if settings.render_as_hex || decoded.text.is_empty() {
            decoded.hex.clone()
        } else {
            decoded.text.clone()
        };
        let payload = draw_text;

        let pf_interface = match ae::aegp::suites::PFInterface::new() {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to open PFInterface suite: {e}"
                ));
                return Ok(());
            }
        };
        let layer_suite = match ae::aegp::suites::Layer::new() {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to open Layer suite: {e}"
                ));
                return Ok(());
            }
        };

        let effect_layer = match pf_interface.effect_layer(in_data.effect_ref()) {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to access effect layer: {e}"
                ));
                return Ok(());
            }
        };
        let source_layer_id = match layer_suite.layer_id(effect_layer) {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to read source layer id: {e}"
                ));
                return Ok(());
            }
        };

        let utility = match ae::aegp::suites::Utility::new() {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to open Utility suite: {e}"
                ));
                return Ok(());
            }
        };
        match utility.is_scripting_available() {
            Ok(true) => {}
            Ok(false) => {
                dlog::log("GenerateDecodeText error: scripting is not available");
                return Ok(());
            }
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to inspect scripting availability: {e}"
                ));
                return Ok(());
            }
        }

        let unique_id = DECODE_TEXT_LAYER_COUNTER.fetch_add(1, Ordering::Relaxed);
        let layer_name = format!("AOD_DecodeText_{}_{}", source_layer_id, unique_id);
        let expression = Self::js_string_literal(&payload);
        let script = format!(
            r#"(function () {{
    var sourceLayer = null;
    if (app.project && app.project.layerByID) {{
        sourceLayer = app.project.layerByID({source_layer_id});
    }}
    var comp = (sourceLayer && sourceLayer.containingComp) ? sourceLayer.containingComp : (app.project && app.project.activeItem);
    if (!(comp instanceof CompItem)) {{
        throw new Error("No target composition.");
    }}
    app.beginUndoGroup("AOD Decode To Text Layer");
    try {{
        var layer = comp.layers.addText("");
        layer.name = {layer_name};
        var sourceText = layer.property("ADBE Text Properties").property("ADBE Text Document");
        sourceText.expression = {expression_code};
        sourceText.expressionEnabled = true;
        if (sourceLayer && sourceLayer.containingComp === comp) {{
            try {{ layer.moveAfter(sourceLayer); }} catch (_e) {{}}
        }}
        layer.locked = true;
        return layer.name;
    }} finally {{
        app.endUndoGroup();
    }}
}})()"#,
            source_layer_id = source_layer_id,
            layer_name = Self::js_string_literal(&layer_name),
            expression_code = Self::js_string_literal(&expression),
        );

        let (result, error_string) = match utility.execute_script(plugin_id, &script, false) {
            Ok(v) => v,
            Err(e) => {
                dlog::log(format!(
                    "GenerateDecodeText error: failed to execute script: {e}"
                ));
                return Ok(());
            }
        };
        if !error_string.trim().is_empty() {
            dlog::log(format!(
                "GenerateDecodeText script error: {}",
                error_string.trim()
            ));
            return Ok(());
        }

        dlog::log(format!("GenerateDecodeText created layer: {result}"));
        Ok(())
    }

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = out_layer.width();
        let height = out_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let settings = read_settings(params)?;
        let mut overlay = vec![transparent_pixel(); width * height];
        let mut region_only_rect: Option<(i32, i32, i32, i32)> = None;

        match Self::decode_from_layer(&in_layer, &settings) {
            Ok(decoded) => {
                Self::cache_decoded_for_text(&decoded);
                if settings.region_only_output {
                    region_only_rect = Some(decoded.bbox);
                }

                if settings.mask_decoded_region {
                    overlay::draw_filled_rect(
                        &mut overlay,
                        width,
                        height,
                        decoded.bbox,
                        settings.mask_color,
                        settings.mask_opacity,
                    );
                }

                if settings.render_decoded {
                    let draw_text = if settings.render_as_hex || decoded.text.is_empty() {
                        decoded.hex
                    } else {
                        decoded.text
                    };
                    let label = format!("{}: {}", decoded.format, draw_text);

                    draw_rect_outline(&mut overlay, width, height, decoded.bbox, [0.0, 1.0, 0.0]);
                    draw_text_wrapped(
                        &mut overlay,
                        width,
                        height,
                        decoded.bbox.0 + 4,
                        decoded.bbox.1 + 4,
                        decoded.bbox.2.max(16) as usize,
                        2,
                        &label.to_ascii_uppercase(),
                        [0.0, 1.0, 0.0],
                    );
                }
            }
            Err(msg) => {
                if settings.render_decoded {
                    let (x, y) = settings.region_origin;
                    draw_error_overlay(
                        &mut overlay,
                        width,
                        height,
                        (x, y),
                        settings.region_width as i32,
                        settings.region_height as i32,
                        &format!("DECODE ERROR: {msg}"),
                    );
                }
            }
        }

        composite_with_overlay(in_layer, &mut out_layer, &overlay, region_only_rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrqrpar::{Color as QrColor, EcLevel as QrEcLevel, QrCode, RmqrStrategy};

    #[test]
    fn decodes_rmqr_without_quiet_zone_using_fallback() {
        let payload = b"HELLO RMQR";
        let code = QrCode::rmqr_with_options(payload, QrEcLevel::M, RmqrStrategy::Area).unwrap();
        let cell_px = 6usize;
        let width = code.width() * cell_px;
        let height = code.height() * cell_px;
        let colors = code.to_colors();
        let mut luma = vec![u8::MAX; width * height];

        for y in 0..code.height() {
            for x in 0..code.width() {
                let value = if colors[y * code.width() + x] == QrColor::Dark {
                    0
                } else {
                    u8::MAX
                };
                for dy in 0..cell_px {
                    let row = (y * cell_px + dy) * width + x * cell_px;
                    for dx in 0..cell_px {
                        luma[row + dx] = value;
                    }
                }
            }
        }

        let mut hints = DecodeHints::default();
        hints.TryHarder = Some(true);
        hints.AlsoInverted = Some(true);
        hints.PossibleFormats = possible_formats(FormatHint::Rmqr);

        let decoded = Plugin::decode_luma_with_fallbacks(
            luma,
            width as u32,
            height as u32,
            (0, 0),
            &hints,
            FormatHint::Rmqr,
        )
        .unwrap();

        assert_eq!(
            *decoded.result.getBarcodeFormat(),
            BarcodeFormat::RECTANGULAR_MICRO_QR_CODE
        );
        assert_eq!(decoded.result.getText(), "HELLO RMQR");
    }
}
