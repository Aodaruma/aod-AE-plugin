#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::collections::HashSet;
use std::env;

use ae::pf::*;
use rxing::{BarcodeFormat, DecodeHints, helpers};
use utils::ToPixel;

mod decode_core;
mod overlay;
mod settings;

use decode_core::{bbox_from_points, build_luma8, bytes_to_hex_upper, clamp_rect, crop_luma};
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
                d.set_options(&["Any", "1D", "2D", "QR", "Data Matrix", "PDF417", "rMQR"]);
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
            "Render Decoded",
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
                if params.type_at(param_index) == Params::DetectMode {
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
        let show_region = matches!(detect_mode, DetectMode::Region);
        self.set_param_visible(in_data, params, Params::RegionOrigin, show_region)?;
        self.set_param_visible(in_data, params, Params::RegionWidth, show_region)?;
        self.set_param_visible(in_data, params, Params::RegionHeight, show_region)?;
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
        let full_luma = build_luma8(&in_layer);
        let mut overlay = vec![transparent_pixel(); width * height];

        let (decode_luma, decode_w, decode_h, origin_offset) =
            if matches!(settings.detect_mode, DetectMode::Region) {
                let (rx, ry) = settings.region_origin;
                let rw = settings.region_width as i32;
                let rh = settings.region_height as i32;
                let region = clamp_rect(rx, ry, rw, rh, width as i32, height as i32);
                let crop = crop_luma(&full_luma, width, height, region);
                (crop, region.2 as u32, region.3 as u32, (region.0, region.1))
            } else {
                (
                    full_luma.clone(),
                    width as u32,
                    height as u32,
                    (0_i32, 0_i32),
                )
            };

        let mut hints = DecodeHints::default();
        hints.TryHarder = Some(settings.try_harder);
        hints.AlsoInverted = Some(settings.also_inverted);
        hints.PossibleFormats = possible_formats(settings.format_hint);

        let decode_result =
            helpers::detect_in_luma_with_hints(decode_luma, decode_w, decode_h, None, &mut hints)
                .map_err(|e| format!("{e:?}"));

        match decode_result {
            Ok(result) => {
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

                if settings.render_decoded {
                    let draw_text = if settings.render_as_hex || text.is_empty() {
                        hex
                    } else {
                        text
                    };
                    let label = format!("{}: {}", format, draw_text);

                    let bbox = bbox_from_points(result.getPoints(), origin_offset).unwrap_or((
                        origin_offset.0,
                        origin_offset.1,
                        220,
                        120,
                    ));
                    draw_rect_outline(&mut overlay, width, height, bbox, [0.0, 1.0, 0.0]);
                    draw_text_wrapped(
                        &mut overlay,
                        width,
                        height,
                        bbox.0 + 4,
                        bbox.1 + 4,
                        bbox.2.max(16) as usize,
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

        composite_with_overlay(in_layer, &mut out_layer, &overlay)
    }
}
