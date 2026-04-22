use super::*;

pub(crate) fn render_matrix_to_overlay(
    settings: &RenderSettings,
    matrix: &CodeMatrix,
    frame_w: usize,
    frame_h: usize,
    overlay: &mut [PixelF32],
) {
    let cell_px = settings.cell_pixel_size.max(1) as i32;
    let (ox, oy) = settings.origin_px;
    let (sx, sy) = direction_signs(settings.origin_direction);
    for my in 0..matrix.height {
        for mx in 0..matrix.width {
            let cell_rgb = matrix.get(mx, my);
            if settings.background_transparent && is_background_cell(cell_rgb) {
                continue;
            }
            let rgb = map_cell_color(settings, cell_rgb);
            let px0 = ox + sx * mx as i32 * cell_px;
            let py0 = oy + sy * my as i32 * cell_px;
            for dy in 0..cell_px {
                let py = if sy >= 0 { py0 + dy } else { py0 - dy };
                for dx in 0..cell_px {
                    let px = if sx >= 0 { px0 + dx } else { px0 - dx };
                    set_overlay_rgb(overlay, frame_w, frame_h, px, py, rgb, 1.0);
                }
            }
        }
    }
}

pub(crate) fn placement_rect(
    settings: &RenderSettings,
    modules_w: usize,
    modules_h: usize,
) -> (i32, i32, i32, i32) {
    let cell_px = settings.cell_pixel_size.max(1) as i32;
    let total_w = modules_w as i32 * cell_px;
    let total_h = modules_h as i32 * cell_px;
    let (sx, sy) = direction_signs(settings.origin_direction);
    let x0 = if sx >= 0 {
        settings.origin_px.0
    } else {
        settings.origin_px.0 - total_w + 1
    };
    let y0 = if sy >= 0 {
        settings.origin_px.1
    } else {
        settings.origin_px.1 - total_h + 1
    };
    (x0, y0, total_w, total_h)
}

pub(crate) fn draw_error_overlay(
    overlay: &mut [PixelF32],
    frame_w: usize,
    frame_h: usize,
    origin: (i32, i32),
    region_w: i32,
    region_h: i32,
    message: &str,
) {
    let x0 = origin.0.clamp(0, (frame_w.saturating_sub(1)) as i32);
    let y0 = origin.1.clamp(0, (frame_h.saturating_sub(1)) as i32);
    let w = region_w.max(120).min(frame_w as i32 - x0).max(8);
    let h = region_h.max(64).min(frame_h as i32 - y0).max(8);
    let x1 = x0 + w - 1;
    let y1 = y0 + h - 1;
    for x in x0..=x1 {
        set_overlay_rgb(overlay, frame_w, frame_h, x, y0, [1.0, 1.0, 1.0], 1.0);
        set_overlay_rgb(overlay, frame_w, frame_h, x, y1, [1.0, 1.0, 1.0], 1.0);
    }
    for y in y0..=y1 {
        set_overlay_rgb(overlay, frame_w, frame_h, x0, y, [1.0, 1.0, 1.0], 1.0);
        set_overlay_rgb(overlay, frame_w, frame_h, x1, y, [1.0, 1.0, 1.0], 1.0);
    }
    draw_text_wrapped(
        overlay,
        frame_w,
        frame_h,
        x0 + 4,
        y0 + 4,
        (w - 8).max(8) as usize,
        2,
        &message.to_ascii_uppercase(),
    );
}

pub(crate) fn draw_text_wrapped(
    overlay: &mut [PixelF32],
    frame_w: usize,
    frame_h: usize,
    x: i32,
    y: i32,
    max_width_px: usize,
    scale: i32,
    text: &str,
) {
    let glyph_w = 5 * scale;
    let glyph_h = 7 * scale;
    let step_x = glyph_w + scale;
    let step_y = glyph_h + scale * 2;
    let max_chars = (max_width_px as i32 / step_x).max(1) as usize;
    let chars: Vec<char> = text.chars().collect();
    for (line, chunk) in chars.chunks(max_chars).enumerate() {
        let mut cx = x;
        for ch in chunk {
            draw_glyph(
                overlay,
                frame_w,
                frame_h,
                cx,
                y + line as i32 * step_y,
                *ch,
                scale,
            );
            cx += step_x;
        }
    }
}

fn direction_signs(dir: OriginDirection) -> (i32, i32) {
    match dir {
        OriginDirection::RightDown => (1, 1),
        OriginDirection::LeftDown => (-1, 1),
        OriginDirection::RightUp => (1, -1),
        OriginDirection::LeftUp => (-1, -1),
    }
}

fn is_background_cell(rgb: [f32; 3]) -> bool {
    const EPS: f32 = 1.0e-6;
    (rgb[0] - 1.0).abs() <= EPS && (rgb[1] - 1.0).abs() <= EPS && (rgb[2] - 1.0).abs() <= EPS
}

fn is_foreground_cell(rgb: [f32; 3]) -> bool {
    const EPS: f32 = 1.0e-6;
    rgb[0].abs() <= EPS && rgb[1].abs() <= EPS && rgb[2].abs() <= EPS
}

fn map_cell_color(settings: &RenderSettings, src: [f32; 3]) -> [f32; 3] {
    if is_foreground_cell(src) {
        settings.fg_color
    } else if is_background_cell(src) {
        settings.bg_color
    } else {
        src
    }
}

pub(crate) fn draw_glyph(
    overlay: &mut [PixelF32],
    frame_w: usize,
    frame_h: usize,
    x: i32,
    y: i32,
    ch: char,
    scale: i32,
) {
    let rows = glyph_rows(ch);
    for (ry, bits) in rows.iter().enumerate() {
        for rx in 0..5 {
            if ((bits >> (4 - rx)) & 1) == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    set_overlay_rgb(
                        overlay,
                        frame_w,
                        frame_h,
                        x + rx * scale + sx,
                        y + ry as i32 * scale + sy,
                        [1.0, 0.0, 0.0],
                        1.0,
                    );
                }
            }
        }
    }
}

pub(crate) fn glyph_rows(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b00110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '_' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
        ':' => [
            0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00100, 0b00100, 0b01000, 0b10000, 0b00000,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        ' ' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
        _ => [
            0b11111, 0b00001, 0b00110, 0b00100, 0b00000, 0b00100, 0b00100,
        ],
    }
}

pub(crate) fn composite_with_overlay(
    in_layer: &Layer,
    out_layer: &mut Layer,
    overlay: &[PixelF32],
) -> Result<(), Error> {
    let width = out_layer.width();
    let height = out_layer.height();
    in_layer.iterate_with(out_layer, 0, height as i32, None, |x, y, src, mut dst| {
        let mut out = read_input_pixel(src);
        if x >= 0 && y >= 0 {
            let xu = x as usize;
            let yu = y as usize;
            if xu < width && yu < height {
                let ov = overlay[yu * width + xu];
                if ov.alpha > 0.0 {
                    out = ov;
                }
            }
        }
        write_output_pixel(&mut dst, out);
        Ok(())
    })
}

pub(crate) fn set_overlay_rgb(
    overlay: &mut [PixelF32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    rgb: [f32; 3],
    alpha: f32,
) {
    if x < 0 || y < 0 {
        return;
    }
    let xu = x as usize;
    let yu = y as usize;
    if xu >= width || yu >= height {
        return;
    }
    overlay[yu * width + xu] = PixelF32 {
        alpha: alpha.clamp(0.0, 1.0),
        red: rgb[0].clamp(0.0, 1.0),
        green: rgb[1].clamp(0.0, 1.0),
        blue: rgb[2].clamp(0.0, 1.0),
    };
}

pub(crate) fn point_value_f32(point: &PointDef<'_>) -> (f32, f32) {
    match point.float_value() {
        Ok(p) => (p.x as f32, p.y as f32),
        Err(_) => point.value(),
    }
}

pub(crate) fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

pub(crate) fn read_input_pixel(src: GenericPixel<'_>) -> PixelF32 {
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

pub(crate) fn write_output_pixel(dst: &mut GenericPixelMut<'_>, px: PixelF32) {
    match dst {
        GenericPixelMut::Pixel8(p) => **p = px.to_pixel8(),
        GenericPixelMut::Pixel16(p) => **p = px.to_pixel16(),
        GenericPixelMut::PixelF32(p) => **p = px,
        GenericPixelMut::PixelF64(p) => {
            p.alphaF = px.alpha as _;
            p.redF = px.red as _;
            p.greenF = px.green as _;
            p.blueF = px.blue as _;
        }
    }
}
