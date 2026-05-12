use super::*;
use crate::liquid_source::{TemplateTransformContext, render_payload_from_template};
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
            8 => DataCodeType::Iqr,
            9 => DataCodeType::Rmqr,
            10 => DataCodeType::ColorCode,
            11 => DataCodeType::JustEmbedding,
            _ => DataCodeType::QrCode,
        },
    )
}

pub(crate) fn read_input_mode(params: &mut Parameters<Params>) -> Result<InputMode, Error> {
    Ok(if params.get(Params::InputMode)?.as_popup()?.value() == 2 {
        InputMode::Hex
    } else {
        InputMode::Utf8
    })
}

pub(crate) fn read_settings(
    in_data: InData,
    params: &mut Parameters<Params>,
    template_src: &str,
) -> Result<RenderSettings, String> {
    let code_type = read_code_type(params).map_err(|e| e.to_string())?;
    let input_mode = read_input_mode(params).map_err(|e| e.to_string())?;
    let point_param = params.get(Params::OriginPos).map_err(|e| e.to_string())?;
    let point = point_param.as_point().map_err(|e| e.to_string())?;
    let (ox, oy) = point_value_f32(&point);
    let origin_direction_popup = match params
        .get(Params::OriginDirection)
        .map_err(|e| e.to_string())?
        .as_popup()
        .map_err(|e| e.to_string())?
        .value()
    {
        2 => OriginDirection::LeftDown,
        3 => OriginDirection::RightUp,
        4 => OriginDirection::LeftUp,
        5 => OriginDirection::Center,
        _ => OriginDirection::RightDown,
    };
    let origin_direction_name = match origin_direction_popup {
        OriginDirection::RightDown => "right_down",
        OriginDirection::LeftDown => "left_down",
        OriginDirection::RightUp => "right_up",
        OriginDirection::LeftUp => "left_up",
        OriginDirection::Center => "center",
    };
    let cell_pixel_size = read_slider(params, Params::CellPixelSize, 1.0, 128.0)? as usize;
    let cell_width = read_slider(params, Params::CellWidth, 1.0, 2048.0)? as usize;
    let cell_height = read_slider(params, Params::CellHeight, 1.0, 2048.0)? as usize;
    let origin_px = (ox.round() as i32, oy.round() as i32);

    let template_transform = TemplateTransformContext {
        origin_x: origin_px.0,
        origin_y: origin_px.1,
        origin_direction: origin_direction_name,
        cell_pixel_size,
        cell_width,
        cell_height,
    };
    let (payload_bytes, payload_text) = render_payload_from_template(
        input_mode,
        template_src,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
        &template_transform,
    )?;

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
        input_mode, // used for debug/error reporting
        payload_bytes,
        payload_text,
        cell_pixel_size,
        cell_width,
        cell_height,
        origin_px,
        origin_direction: origin_direction_popup,
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
