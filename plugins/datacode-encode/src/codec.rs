use super::*;

pub(crate) fn generate_code_matrix(settings: &RenderSettings) -> Result<CodeMatrix, String> {
    match settings.code_type {
        DataCodeType::JanEan => {
            let ean = normalize_ean13(&settings.payload_text)?;
            encode_via_rxing(settings, BarcodeFormat::EAN_13, &ean, false, false)
        }
        DataCodeType::Code39 => encode_via_rxing(
            settings,
            BarcodeFormat::CODE_39,
            &payload_for_1d(settings),
            false,
            false,
        ),
        DataCodeType::Code128 => encode_via_rxing(
            settings,
            BarcodeFormat::CODE_128,
            &payload_for_1d(settings),
            true,
            false,
        ),
        DataCodeType::Custom1d => encode_custom_1d(settings),
        DataCodeType::QrCode => encode_qr_like(settings, false),
        DataCodeType::DataMatrix => encode_via_rxing(
            settings,
            BarcodeFormat::DATA_MATRIX,
            &settings.payload_text,
            false,
            true,
        ),
        DataCodeType::Pdf417 => encode_via_rxing(
            settings,
            BarcodeFormat::PDF_417,
            &settings.payload_text,
            false,
            true,
        ),
        DataCodeType::Iqr => encode_iqr(settings),
        DataCodeType::Rmqr => encode_qr_like(settings, true),
        DataCodeType::ColorCode => encode_jab_like(settings),
        DataCodeType::JustEmbedding => encode_just_embedding(settings),
    }
}

pub(crate) fn payload_for_1d(settings: &RenderSettings) -> String {
    if matches!(settings.input_mode, InputMode::Utf8) {
        settings.payload_text.clone()
    } else {
        bytes_to_hex_upper(&settings.payload_bytes)
    }
}

pub(crate) fn encode_via_rxing(
    settings: &RenderSettings,
    format: BarcodeFormat,
    content: &str,
    code128: bool,
    trim_white_border: bool,
) -> Result<CodeMatrix, String> {
    let mut hints = EncodeHints {
        Margin: Some(settings.margin_modules.to_string()),
        ..Default::default()
    };
    if matches!(format, BarcodeFormat::QR_CODE | BarcodeFormat::DATA_MATRIX) {
        hints.ErrorCorrection = Some(ec_level_text(settings.error_correction).into());
    }
    if matches!(format, BarcodeFormat::PDF_417) {
        hints.ErrorCorrection = Some(settings.pdf417_level.to_string());
        hints.Pdf417Dimensions = Some(Pdf417Dimensions::new(
            settings.pdf417_cols,
            settings.pdf417_cols,
            settings.pdf417_rows,
            settings.pdf417_rows,
        ));
    }
    if code128 {
        hints.ForceCodeSet = match settings.code128_set {
            Code128Set::A => Some("A".into()),
            Code128Set::B => Some("B".into()),
            Code128Set::C => Some("C".into()),
            Code128Set::Auto => None,
        };
    }

    let matrix = MultiFormatWriter
        .encode_with_hints(
            content,
            &format,
            settings.cell_width as i32,
            settings.cell_height as i32,
            &hints,
        )
        .map_err(|e| format!("ENCODE ERROR: {e:?}"))?;

    let mut out = CodeMatrix::new(
        matrix.width() as usize,
        matrix.height() as usize,
        [1.0, 1.0, 1.0],
    );
    for y in 0..out.height {
        for x in 0..out.width {
            if matrix.get(x as u32, y as u32) {
                out.set(x, y, [0.0, 0.0, 0.0]);
            }
        }
    }
    if trim_white_border {
        Ok(trim_white_border_cells(&out))
    } else {
        Ok(out)
    }
}

pub(crate) fn encode_qr_like(settings: &RenderSettings, rmqr: bool) -> Result<CodeMatrix, String> {
    let ec = ec_level_qr(settings.error_correction);
    let code = if rmqr {
        QrCode::rmqr_with_options(
            &settings.payload_bytes,
            ec,
            map_rmqr_strategy(settings.rmqr_strategy),
        )
        .map_err(|e| format!("RMQR ERROR: {e}"))?
    } else {
        QrCode::with_error_correction_level(&settings.payload_bytes, ec)
            .map_err(|e| format!("QR ERROR: {e}"))?
    };
    let mut out = CodeMatrix::new(code.width(), code.height(), [1.0, 1.0, 1.0]);
    let colors = code.to_colors();
    for y in 0..out.height {
        for x in 0..out.width {
            if colors[y * out.width + x] == QrColor::Dark {
                out.set(x, y, [0.0, 0.0, 0.0]);
            }
        }
    }
    Ok(out)
}

pub(crate) fn encode_iqr(_settings: &RenderSettings) -> Result<CodeMatrix, String> {
    Err("IQR ENCODING IS NOT AVAILABLE IN THE CURRENT BACKEND".into())
}

pub(crate) fn encode_custom_1d(settings: &RenderSettings) -> Result<CodeMatrix, String> {
    let mut bits = vec![true, false, true, false];
    let mut data = settings.payload_bytes.clone();
    if settings.embed_recovery {
        data = append_recovery(data);
    }
    for b in data {
        for shift in (0..8).rev() {
            bits.push(((b >> shift) & 1) == 1);
        }
    }
    bits.extend_from_slice(&[false, true, true, false, true]);
    if bits.len() > settings.cell_width {
        return Err("CUSTOM 1D DATA EXCEEDS CELL WIDTH".into());
    }
    if settings.zero_pad_to_fill {
        bits.resize(settings.cell_width, false);
    }
    let mut out = CodeMatrix::new(
        settings.cell_width,
        settings.cell_height.max(1),
        [1.0, 1.0, 1.0],
    );
    let sx = if settings.zero_pad_to_fill {
        0
    } else {
        (settings.cell_width - bits.len()) / 2
    };
    for (i, bit) in bits.iter().enumerate() {
        if *bit {
            for y in 0..out.height {
                out.set(sx + i, y, [0.0, 0.0, 0.0]);
            }
        }
    }
    Ok(out)
}

pub(crate) fn encode_jab_like(settings: &RenderSettings) -> Result<CodeMatrix, String> {
    if settings.cell_width < 12 || settings.cell_height < 12 {
        return Err("JAB-LIKE MODE REQUIRES CELL WIDTH/HEIGHT >= 12".into());
    }
    let data = custom_code::pack_payload(
        &settings.payload_bytes,
        custom_code::KIND_COLOR_CODE,
        settings.embed_recovery,
    );
    let bits = custom_code::bytes_to_bits(&data);
    let mut out = CodeMatrix::new(settings.cell_width, settings.cell_height, [1.0, 1.0, 1.0]);
    draw_finder(&mut out, 0, 0);
    let out_w = out.width as i32;
    let out_h = out.height as i32;
    draw_finder(&mut out, out_w - 5, 0);
    draw_finder(&mut out, 0, out_h - 5);
    let positions = custom_code::jab_fill_positions(out.width, out.height);
    let capacity = positions.len() * 3;
    if bits.len() > capacity {
        return Err("JAB-LIKE PAYLOAD EXCEEDS CAPACITY".into());
    }
    let bits = if settings.zero_pad_to_fill {
        let mut padded = bits;
        padded.resize(capacity, false);
        padded
    } else {
        bits
    };
    for (i, (x, y)) in positions.iter().enumerate() {
        let b0 = bits.get(i * 3).copied().unwrap_or(false);
        let b1 = bits.get(i * 3 + 1).copied().unwrap_or(false);
        let b2 = bits.get(i * 3 + 2).copied().unwrap_or(false);
        out.set(
            *x,
            *y,
            custom_code::palette(((b0 as usize) << 2) | ((b1 as usize) << 1) | b2 as usize),
        );
    }
    Ok(out)
}

pub(crate) fn encode_just_embedding(settings: &RenderSettings) -> Result<CodeMatrix, String> {
    let kind = match settings.embed_mode {
        EmbedMode::Monochrome => custom_code::KIND_JUST_MONO,
        EmbedMode::Color => custom_code::KIND_JUST_COLOR,
    };
    let data = custom_code::pack_payload(&settings.payload_bytes, kind, settings.embed_recovery);
    let bits = custom_code::bytes_to_bits(&data);
    let mut out = CodeMatrix::new(settings.cell_width, settings.cell_height, [1.0, 1.0, 1.0]);
    match settings.embed_mode {
        EmbedMode::Monochrome => {
            let capacity = out.width * out.height;
            if bits.len() > capacity {
                return Err("MONO EMBEDDING PAYLOAD EXCEEDS CAPACITY".into());
            }
            let bits = if settings.zero_pad_to_fill {
                let mut padded = bits;
                padded.resize(capacity, false);
                padded
            } else {
                bits
            };
            for (i, bit) in bits.iter().enumerate() {
                if *bit {
                    out.set(i % out.width, i / out.width, [0.0, 0.0, 0.0]);
                }
            }
        }
        EmbedMode::Color => {
            let capacity = out.width * out.height * 3;
            if bits.len() > capacity {
                return Err("COLOR EMBEDDING PAYLOAD EXCEEDS CAPACITY".into());
            }
            let bits = if settings.zero_pad_to_fill {
                let mut padded = bits;
                padded.resize(capacity, false);
                padded
            } else {
                bits
            };
            for i in 0..(out.width * out.height) {
                let b0 = bits.get(i * 3).copied().unwrap_or(false);
                let b1 = bits.get(i * 3 + 1).copied().unwrap_or(false);
                let b2 = bits.get(i * 3 + 2).copied().unwrap_or(false);
                out.set(
                    i % out.width,
                    i / out.width,
                    custom_code::palette(((b0 as usize) << 2) | ((b1 as usize) << 1) | b2 as usize),
                );
            }
        }
    }
    Ok(out)
}

pub(crate) fn normalize_ean13(input: &str) -> Result<String, String> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 12 {
        let mut out = digits;
        out.push((b'0' + ean13_check(out.as_bytes())?) as char);
        return Ok(out);
    }
    if digits.len() == 13 {
        let expected = ean13_check(&digits.as_bytes()[0..12])?;
        let got = digits.as_bytes()[12] - b'0';
        if expected != got {
            return Err("EAN-13 CHECKSUM MISMATCH".into());
        }
        return Ok(digits);
    }
    Err("JAN/EAN REQUIRES 12 OR 13 DIGITS".into())
}

pub(crate) fn ean13_check(input12: &[u8]) -> Result<u8, String> {
    if input12.len() != 12 || input12.iter().any(|v| !v.is_ascii_digit()) {
        return Err("INVALID EAN SOURCE".into());
    }
    let mut sum = 0_u32;
    for (i, c) in input12.iter().enumerate() {
        let d = (c - b'0') as u32;
        if i % 2 == 0 {
            sum += d;
        } else {
            sum += d * 3;
        }
    }
    Ok(((10 - (sum % 10)) % 10) as u8)
}

pub(crate) fn parse_hex_binary(input: &str) -> Result<Vec<u8>, String> {
    let normalized: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .collect();
    if normalized.is_empty() {
        return Ok(Vec::new());
    }
    if !normalized.len().is_multiple_of(2) {
        return Err("HEX PAYLOAD MUST HAVE EVEN LENGTH".into());
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let b = normalized.as_bytes();
    for i in (0..b.len()).step_by(2) {
        let hi = hex_nibble(b[i])?;
        let lo = hex_nibble(b[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

pub(crate) fn hex_nibble(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err("INVALID HEX DIGIT".into()),
    }
}

pub(crate) fn bytes_to_hex_upper(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push(hex_char((b >> 4) & 0x0F));
        out.push(hex_char(b & 0x0F));
    }
    out
}

pub(crate) fn hex_char(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'A' + (v - 10)) as char,
    }
}

pub(crate) fn ec_level_text(ec: ErrorCorrection) -> &'static str {
    match ec {
        ErrorCorrection::L => "L",
        ErrorCorrection::M => "M",
        ErrorCorrection::Q => "Q",
        ErrorCorrection::H => "H",
    }
}
pub(crate) fn ec_level_qr(ec: ErrorCorrection) -> QrEcLevel {
    match ec {
        ErrorCorrection::L => QrEcLevel::L,
        ErrorCorrection::M => QrEcLevel::M,
        ErrorCorrection::Q => QrEcLevel::Q,
        ErrorCorrection::H => QrEcLevel::H,
    }
}
pub(crate) fn map_rmqr_strategy(v: RmqrStrategyMode) -> RmqrStrategy {
    match v {
        RmqrStrategyMode::Area => RmqrStrategy::Area,
        RmqrStrategyMode::Width => RmqrStrategy::Width,
        RmqrStrategyMode::Height => RmqrStrategy::Height,
    }
}
pub(crate) fn append_recovery(mut v: Vec<u8>) -> Vec<u8> {
    let p = v.iter().fold(0_u8, |acc, b| acc ^ *b);
    let l = (v.len() & 0xFF) as u8;
    v.push(p);
    v.push(l);
    v
}
pub(crate) fn draw_finder(matrix: &mut CodeMatrix, x0: i32, y0: i32) {
    for y in 0..5_i32 {
        for x in 0..5_i32 {
            let gx = x0 + x;
            let gy = y0 + y;
            if gx < 0 || gy < 0 {
                continue;
            }
            let (gx, gy) = (gx as usize, gy as usize);
            if gx >= matrix.width || gy >= matrix.height {
                continue;
            }
            let outer = x == 0 || x == 4 || y == 0 || y == 4;
            let center = x == 2 && y == 2;
            matrix.set(
                gx,
                gy,
                if outer || center {
                    [0.0, 0.0, 0.0]
                } else {
                    [1.0, 1.0, 1.0]
                },
            );
        }
    }
}

fn trim_white_border_cells(matrix: &CodeMatrix) -> CodeMatrix {
    let mut min_x = matrix.width;
    let mut min_y = matrix.height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut has_fg = false;

    for y in 0..matrix.height {
        for x in 0..matrix.width {
            if is_foreground_cell(matrix.get(x, y)) {
                has_fg = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !has_fg {
        return matrix.clone();
    }

    let out_w = max_x - min_x + 1;
    let out_h = max_y - min_y + 1;
    let mut out = CodeMatrix::new(out_w, out_h, [1.0, 1.0, 1.0]);
    for y in 0..out_h {
        for x in 0..out_w {
            out.set(x, y, matrix.get(min_x + x, min_y + y));
        }
    }
    out
}

fn is_foreground_cell(rgb: [f32; 3]) -> bool {
    const EPS: f32 = 1.0e-6;
    rgb[0].abs() <= EPS && rgb[1].abs() <= EPS && rgb[2].abs() <= EPS
}
