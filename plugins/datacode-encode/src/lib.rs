#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use qrqrpar::{Color as QrColor, EcLevel as QrEcLevel, QrCode, RmqrStrategy};
use rxing::pdf417::encoder::Dimensions as Pdf417Dimensions;
use rxing::{BarcodeFormat, EncodeHints, MultiFormatWriter, Writer};
use serde::{Deserialize, Serialize};
use utils::ToPixel;

mod codec;
mod overlay;
mod settings;

use codec::generate_code_matrix;
use overlay::{
    composite_with_overlay, draw_error_overlay, placement_rect, render_matrix_to_overlay,
    transparent_pixel,
};
use settings::{read_code_type, read_settings};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    DataCodeType,
    InputMode,
    Payload,
    CellPixelSize,
    CellWidth,
    CellHeight,
    OriginPos,
    OriginDirection,
    ForegroundColor,
    BackgroundColor,
    BackgroundTransparent,
    MarginModules,
    ErrorCorrection,
    Code128Set,
    Pdf417Cols,
    Pdf417Rows,
    Pdf417Level,
    RmqrStrategy,
    EmbedMode,
    EmbedRecovery,
    ZeroPadToFill,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Encodes 1D and 2D data codes with dynamic UI and custom payload parameters.";

#[derive(Serialize, Deserialize, Clone, PartialEq, PartialOrd)]
struct DataPayload {
    utf8: String,
    hex: String,
}

impl Default for DataPayload {
    fn default() -> Self {
        Self {
            utf8: "HELLO_AOD".into(),
            hex: "48656C6C6F5F414F44".into(),
        }
    }
}

impl ae::ArbitraryData<DataPayload> for DataPayload {
    fn interpolate(&self, other: &Self, value: f64) -> Self {
        if value < 0.5 {
            self.clone()
        } else {
            other.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataCodeType {
    JanEan,
    Code39,
    Code128,
    Custom1d,
    QrCode,
    DataMatrix,
    Pdf417,
    IqrFallback,
    Rmqr,
    ColorCode,
    JustEmbedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Utf8,
    Hex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorCorrection {
    L,
    M,
    Q,
    H,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Code128Set {
    Auto,
    A,
    B,
    C,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RmqrStrategyMode {
    Area,
    Width,
    Height,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmbedMode {
    Monochrome,
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OriginDirection {
    RightDown,
    LeftDown,
    RightUp,
    LeftUp,
}

#[derive(Clone, Debug)]
struct RenderSettings {
    code_type: DataCodeType,
    input_mode: InputMode,
    payload_bytes: Vec<u8>,
    payload_text: String,
    cell_pixel_size: usize,
    cell_width: usize,
    cell_height: usize,
    origin_px: (i32, i32),
    origin_direction: OriginDirection,
    fg_color: [f32; 3],
    bg_color: [f32; 3],
    background_transparent: bool,
    margin_modules: usize,
    error_correction: ErrorCorrection,
    code128_set: Code128Set,
    pdf417_cols: usize,
    pdf417_rows: usize,
    pdf417_level: usize,
    rmqr_strategy: RmqrStrategyMode,
    embed_mode: EmbedMode,
    embed_recovery: bool,
    zero_pad_to_fill: bool,
}

#[derive(Clone)]
struct CodeMatrix {
    width: usize,
    height: usize,
    cells: Vec<[f32; 3]>,
}

impl CodeMatrix {
    fn new(width: usize, height: usize, bg: [f32; 3]) -> Self {
        Self {
            width,
            height,
            cells: vec![bg; width * height],
        }
    }
    fn set(&mut self, x: usize, y: usize, rgb: [f32; 3]) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = rgb;
        }
    }
    fn get(&self, x: usize, y: usize) -> [f32; 3] {
        self.cells[y * self.width + x]
    }
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::DataCodeType,
            "DataCode Type",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "JAN/EAN",
                    "CODE39",
                    "CODE128",
                    "Custom 1D",
                    "QR Code",
                    "Data Matrix",
                    "PDF417",
                    "iQR (Fallback)",
                    "rMQR",
                    "Color Code (JAB-like)",
                    "Just Embedding",
                ]);
                d.set_default(5);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::InputMode,
            "Payload Mode",
            PopupDef::setup(|d| {
                d.set_options(&["UTF-8 String", "HEX Binary"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::Payload,
            "Payload",
            ArbitraryDef::setup(|d| {
                d.set_default(DataPayload::default()).unwrap();
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::NO_ECW_UI,
        )?;

        params.add(
            Params::CellPixelSize,
            "Cell Pixel Size",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(128.0);
                d.set_slider_min(1.0);
                d.set_slider_max(32.0);
                d.set_default(8.0);
                d.set_precision(2);
            }),
        )?;
        params.add(
            Params::CellWidth,
            "Cell Width",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(1024.0);
                d.set_slider_min(8.0);
                d.set_slider_max(256.0);
                d.set_default(96.0);
                d.set_precision(0);
            }),
        )?;
        params.add(
            Params::CellHeight,
            "Cell Height",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(1024.0);
                d.set_slider_min(8.0);
                d.set_slider_max(256.0);
                d.set_default(96.0);
                d.set_precision(0);
            }),
        )?;
        params.add(
            Params::OriginPos,
            "Origin Pos",
            PointDef::setup(|p| {
                p.set_default((10.0, 10.0));
            }),
        )?;
        params.add_with_flags(
            Params::OriginDirection,
            "Origin Direction",
            PopupDef::setup(|d| {
                d.set_options(&["Right/Down", "Left/Down", "Right/Up", "Left/Up"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add(
            Params::ForegroundColor,
            "Foreground Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: u8::MAX,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
            }),
        )?;
        params.add(
            Params::BackgroundColor,
            "Background Color",
            ColorDef::setup(|d| {
                d.set_default(Pixel8 {
                    alpha: u8::MAX,
                    red: u8::MAX,
                    green: u8::MAX,
                    blue: u8::MAX,
                });
            }),
        )?;
        params.add(
            Params::BackgroundTransparent,
            "Background Transparent",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;
        params.add(
            Params::MarginModules,
            "Margin Modules",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(64.0);
                d.set_slider_min(0.0);
                d.set_slider_max(16.0);
                d.set_default(2.0);
                d.set_precision(0);
            }),
        )?;
        params.add_with_flags(
            Params::ErrorCorrection,
            "Error Correction",
            PopupDef::setup(|d| {
                d.set_options(&["L", "M", "Q", "H"]);
                d.set_default(2);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::empty(),
        )?;
        params.add_with_flags(
            Params::Code128Set,
            "CODE128 Set",
            PopupDef::setup(|d| {
                d.set_options(&["Auto", "A", "B", "C"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::Pdf417Cols,
            "PDF417 Columns",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.0);
                d.set_valid_max(30.0);
                d.set_slider_min(2.0);
                d.set_slider_max(12.0);
                d.set_default(6.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::Pdf417Rows,
            "PDF417 Rows",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(3.0);
                d.set_valid_max(90.0);
                d.set_slider_min(3.0);
                d.set_slider_max(40.0);
                d.set_default(18.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::Pdf417Level,
            "PDF417 EC Level",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(8.0);
                d.set_slider_min(0.0);
                d.set_slider_max(8.0);
                d.set_default(2.0);
                d.set_precision(0);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::RmqrStrategy,
            "rMQR Strategy",
            PopupDef::setup(|d| {
                d.set_options(&["Area", "Width", "Height"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::EmbedMode,
            "Embedding Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Monochrome", "Color"]);
                d.set_default(1);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::EmbedRecovery,
            "Embed Recovery",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
            ParamFlag::SUPERVISE,
            ParamUIFlags::INVISIBLE,
        )?;
        params.add_with_flags(
            Params::ZeroPadToFill,
            "Zero Pad To Fill",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
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
                        "AOD_DatacodeEncode - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_DatacodeEncode")
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
            ae::Command::ArbitraryCallback { mut extra } => {
                extra.dispatch::<DataPayload, Params>(Params::Payload)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if params.type_at(param_index) == Params::DataCodeType {
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
        let code_type = read_code_type(params)?;
        let show_code128 = matches!(code_type, DataCodeType::Code128);
        let show_pdf417 = matches!(code_type, DataCodeType::Pdf417);
        let show_rmqr = matches!(code_type, DataCodeType::Rmqr | DataCodeType::IqrFallback);
        let show_embed = matches!(
            code_type,
            DataCodeType::ColorCode | DataCodeType::JustEmbedding | DataCodeType::Custom1d
        );
        let show_zero_pad = show_embed;
        let show_margin = !matches!(
            code_type,
            DataCodeType::ColorCode | DataCodeType::JustEmbedding
        );
        let show_ec = matches!(
            code_type,
            DataCodeType::QrCode
                | DataCodeType::DataMatrix
                | DataCodeType::Pdf417
                | DataCodeType::IqrFallback
                | DataCodeType::Rmqr
        );

        self.set_param_visible(in_data, params, Params::Code128Set, show_code128)?;
        self.set_param_visible(in_data, params, Params::Pdf417Cols, show_pdf417)?;
        self.set_param_visible(in_data, params, Params::Pdf417Rows, show_pdf417)?;
        self.set_param_visible(in_data, params, Params::Pdf417Level, show_pdf417)?;
        self.set_param_visible(in_data, params, Params::RmqrStrategy, show_rmqr)?;
        self.set_param_visible(in_data, params, Params::EmbedMode, show_embed)?;
        self.set_param_visible(in_data, params, Params::EmbedRecovery, show_embed)?;
        self.set_param_visible(in_data, params, Params::ZeroPadToFill, show_zero_pad)?;
        self.set_param_visible(in_data, params, Params::MarginModules, show_margin)?;
        self.set_param_visible(in_data, params, Params::ErrorCorrection, show_ec)?;
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

        let mut overlay = vec![transparent_pixel(); width * height];
        let settings = match read_settings(params) {
            Ok(v) => v,
            Err(msg) => {
                draw_error_overlay(&mut overlay, width, height, (8, 8), 320, 120, &msg);
                return composite_with_overlay(in_layer, &mut out_layer, &overlay);
            }
        };

        let matrix = match generate_code_matrix(&settings) {
            Ok(v) => v,
            Err(msg) => {
                let rect = placement_rect(&settings, settings.cell_width, settings.cell_height);
                draw_error_overlay(
                    &mut overlay,
                    width,
                    height,
                    (rect.0, rect.1),
                    rect.2,
                    rect.3,
                    &msg,
                );
                return composite_with_overlay(in_layer, &mut out_layer, &overlay);
            }
        };

        if matrix.width > settings.cell_width || matrix.height > settings.cell_height {
            let rect = placement_rect(&settings, settings.cell_width, settings.cell_height);
            draw_error_overlay(
                &mut overlay,
                width,
                height,
                (rect.0, rect.1),
                rect.2,
                rect.3,
                "ENCODED DATA EXCEEDS CELL AREA",
            );
            return composite_with_overlay(in_layer, &mut out_layer, &overlay);
        }

        render_matrix_to_overlay(&settings, &matrix, width, height, &mut overlay);
        composite_with_overlay(in_layer, &mut out_layer, &overlay)
    }
}
