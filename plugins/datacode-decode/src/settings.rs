use super::*;
use crate::overlay::point_value_f32;

pub(crate) fn read_detect_mode(params: &mut Parameters<Params>) -> Result<DetectMode, Error> {
    Ok(
        if params.get(Params::DetectMode)?.as_popup()?.value() == 2 {
            DetectMode::Region
        } else {
            DetectMode::Auto
        },
    )
}

pub(crate) fn read_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    let point_param = params.get(Params::RegionOrigin)?;
    let point = point_param.as_point()?;
    let (rx, ry) = point_value_f32(&point);
    Ok(RenderSettings {
        detect_mode: read_detect_mode(params)?,
        format_hint: read_format_hint(params)?,
        region_origin: (rx.round() as i32, ry.round() as i32),
        region_width: read_slider(params, Params::RegionWidth, 8.0, 8192.0)? as usize,
        region_height: read_slider(params, Params::RegionHeight, 8.0, 8192.0)? as usize,
        try_harder: params.get(Params::TryHarder)?.as_checkbox()?.value(),
        also_inverted: params.get(Params::AlsoInverted)?.as_checkbox()?.value(),
        render_decoded: params.get(Params::RenderDecoded)?.as_checkbox()?.value(),
        render_as_hex: params.get(Params::RenderAsHex)?.as_checkbox()?.value(),
        trim_zero_padding: params.get(Params::TrimZeroPadding)?.as_checkbox()?.value(),
    })
}

pub(crate) fn read_slider(
    params: &mut Parameters<Params>,
    key: Params,
    min: f64,
    max: f64,
) -> Result<f64, Error> {
    Ok(params
        .get(key)?
        .as_float_slider()?
        .value()
        .round()
        .clamp(min, max))
}

pub(crate) fn read_format_hint(params: &mut Parameters<Params>) -> Result<FormatHint, Error> {
    Ok(match params.get(Params::FormatHint)?.as_popup()?.value() {
        2 => FormatHint::OneD,
        3 => FormatHint::TwoD,
        4 => FormatHint::Qr,
        5 => FormatHint::DataMatrix,
        6 => FormatHint::Pdf417,
        7 => FormatHint::Rmqr,
        _ => FormatHint::Any,
    })
}

pub(crate) fn possible_formats(hint: FormatHint) -> Option<HashSet<BarcodeFormat>> {
    let set = match hint {
        FormatHint::Any => return None,
        FormatHint::OneD => vec![
            BarcodeFormat::EAN_13,
            BarcodeFormat::EAN_8,
            BarcodeFormat::CODE_39,
            BarcodeFormat::CODE_128,
            BarcodeFormat::UPC_A,
            BarcodeFormat::UPC_E,
            BarcodeFormat::ITF,
            BarcodeFormat::CODABAR,
        ],
        FormatHint::TwoD => vec![
            BarcodeFormat::QR_CODE,
            BarcodeFormat::DATA_MATRIX,
            BarcodeFormat::PDF_417,
            BarcodeFormat::MICRO_QR_CODE,
            BarcodeFormat::RECTANGULAR_MICRO_QR_CODE,
        ],
        FormatHint::Qr => vec![BarcodeFormat::QR_CODE, BarcodeFormat::MICRO_QR_CODE],
        FormatHint::DataMatrix => vec![BarcodeFormat::DATA_MATRIX],
        FormatHint::Pdf417 => vec![BarcodeFormat::PDF_417],
        FormatHint::Rmqr => vec![BarcodeFormat::RECTANGULAR_MICRO_QR_CODE],
    };
    Some(set.into_iter().collect())
}
