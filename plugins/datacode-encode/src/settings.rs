use super::*;
use crate::codec::{bytes_to_hex_upper, parse_hex_binary};
use crate::overlay::point_value_f32;

pub(crate) fn read_code_type(params: &mut Parameters<Params>) -> Result<DataCodeType, Error> {
    Ok(
        match params.get(Params::DataCodeType)?.as_popup()?.value() {
            1 => DataCodeType::JanEan,
            2 => DataCodeType::Code39,
            3 => DataCodeType::Code128,
            4 => DataCodeType::Custom1d,
            5 => DataCodeType::QrCode,
            6 => DataCodeType::DataMatrix,
            7 => DataCodeType::Pdf417,
            8 => DataCodeType::IqrFallback,
            9 => DataCodeType::Rmqr,
            10 => DataCodeType::ColorCode,
            11 => DataCodeType::JustEmbedding,
            _ => DataCodeType::QrCode,
        },
    )
}

pub(crate) fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, String> {
    let code_type = read_code_type(params).map_err(|e| e.to_string())?;
    let input_mode = if params
        .get(Params::InputMode)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
        == 2
    {
        InputMode::Hex
    } else {
        InputMode::Utf8
    };

    let payload = params
        .get(Params::Payload)
        .map_err(|e| e.to_string())?
        .as_arbitrary()
        .map_err(|e| e.to_string())?
        .value::<DataPayload>()
        .map_err(|e| e.to_string())?;
    let payload = (*payload).clone();
    let payload_bytes = match input_mode {
        InputMode::Utf8 => payload.utf8.as_bytes().to_vec(),
        InputMode::Hex => parse_hex_binary(&payload.hex)?,
    };
    if payload_bytes.is_empty() {
        return Err("PAYLOAD IS EMPTY".into());
    }
    let payload_text = if matches!(input_mode, InputMode::Utf8) {
        payload.utf8
    } else {
        bytes_to_hex_upper(&payload_bytes)
    };

    let point_param = params.get(Params::OriginPos).map_err(|e| e.to_string())?;
    let point = point_param.as_point().map_err(|e| e.to_string())?;
    let (ox, oy) = point_value_f32(&point);
    let origin_direction = match params
        .get(Params::OriginDirection)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
    {
        2 => OriginDirection::LeftDown,
        3 => OriginDirection::RightUp,
        4 => OriginDirection::LeftUp,
        _ => OriginDirection::RightDown,
    };

    let fg = params
        .get(Params::ForegroundColor)
        .map_err(|e| e.to_string())?
        .as_color()
        .map_err(|e| e.to_string())?
        .value()
        .to_pixel32();
    let bg = params
        .get(Params::BackgroundColor)
        .map_err(|e| e.to_string())?
        .as_color()
        .map_err(|e| e.to_string())?
        .value()
        .to_pixel32();

    let error_correction = match params
        .get(Params::ErrorCorrection)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
    {
        1 => ErrorCorrection::L,
        3 => ErrorCorrection::Q,
        4 => ErrorCorrection::H,
        _ => ErrorCorrection::M,
    };
    let code128_set = match params
        .get(Params::Code128Set)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
    {
        2 => Code128Set::A,
        3 => Code128Set::B,
        4 => Code128Set::C,
        _ => Code128Set::Auto,
    };
    let rmqr_strategy = match params
        .get(Params::RmqrStrategy)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
    {
        2 => RmqrStrategyMode::Width,
        3 => RmqrStrategyMode::Height,
        _ => RmqrStrategyMode::Area,
    };
    let embed_mode = if params
        .get(Params::EmbedMode)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
        == 2
    {
        EmbedMode::Color
    } else {
        EmbedMode::Monochrome
    };

    Ok(RenderSettings {
        code_type,
        input_mode,
        payload_bytes,
        payload_text,
        cell_pixel_size: read_slider(params, Params::CellPixelSize, 1.0, 128.0)? as usize,
        cell_width: read_slider(params, Params::CellWidth, 1.0, 2048.0)? as usize,
        cell_height: read_slider(params, Params::CellHeight, 1.0, 2048.0)? as usize,
        origin_px: (ox.round() as i32, oy.round() as i32),
        origin_direction,
        fg_color: [fg.red, fg.green, fg.blue],
        bg_color: [bg.red, bg.green, bg.blue],
        background_transparent: params
            .get(Params::BackgroundTransparent)
            .map_err(|e| e.to_string())?
            .as_checkbox()
            .map_err(|e| e.to_string())?
            .value(),
        margin_modules: read_slider(params, Params::MarginModules, 0.0, 64.0)? as usize,
        error_correction,
        code128_set,
        pdf417_cols: read_slider(params, Params::Pdf417Cols, 1.0, 30.0)? as usize,
        pdf417_rows: read_slider(params, Params::Pdf417Rows, 3.0, 90.0)? as usize,
        pdf417_level: read_slider(params, Params::Pdf417Level, 0.0, 8.0)? as usize,
        rmqr_strategy,
        embed_mode,
        embed_recovery: params
            .get(Params::EmbedRecovery)
            .map_err(|e| e.to_string())?
            .as_checkbox()
            .map_err(|e| e.to_string())?
            .value(),
        zero_pad_to_fill: params
            .get(Params::ZeroPadToFill)
            .map_err(|e| e.to_string())?
            .as_checkbox()
            .map_err(|e| e.to_string())?
            .value(),
    })
}

pub(crate) fn read_slider(
    params: &mut Parameters<Params>,
    key: Params,
    min: f64,
    max: f64,
) -> Result<f64, String> {
    Ok(params
        .get(key)
        .map_err(|e| e.to_string())?
        .as_float_slider()
        .map_err(|e| e.to_string())?
        .value()
        .round()
        .clamp(min, max))
}
