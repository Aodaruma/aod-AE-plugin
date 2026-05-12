use super::*;

pub(crate) fn draw_rect_outline(
    overlay: &mut [PixelF32],
    frame_w: usize,
    frame_h: usize,
    rect: (i32, i32, i32, i32),
    rgb: [f32; 3],
) {
    let (x0, y0, w, h) = rect;
    let x1 = x0 + w - 1;
    let y1 = y0 + h - 1;
    for x in x0..=x1 {
        set_overlay_rgb(overlay, frame_w, frame_h, x, y0, rgb, 1.0);
        set_overlay_rgb(overlay, frame_w, frame_h, x, y1, rgb, 1.0);
    }
    for y in y0..=y1 {
        set_overlay_rgb(overlay, frame_w, frame_h, x0, y, rgb, 1.0);
        set_overlay_rgb(overlay, frame_w, frame_h, x1, y, rgb, 1.0);
    }
}

pub(crate) fn draw_filled_rect(
    overlay: &mut [PixelF32],
    frame_w: usize,
    frame_h: usize,
    rect: (i32, i32, i32, i32),
    rgb: [f32; 3],
    alpha: f32,
) {
    let (x0, y0, w, h) = rect;
    let x1 = x0 + w - 1;
    let y1 = y0 + h - 1;
    for y in y0..=y1 {
        for x in x0..=x1 {
            set_overlay_rgb(overlay, frame_w, frame_h, x, y, rgb, alpha);
        }
    }
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
    draw_rect_outline(overlay, frame_w, frame_h, (x0, y0, w, h), [1.0, 1.0, 1.0]);
    draw_text_wrapped(
        overlay,
        frame_w,
        frame_h,
        x0 + 4,
        y0 + 4,
        (w - 8).max(8) as usize,
        2,
        &message.to_ascii_uppercase(),
        [1.0, 0.0, 0.0],
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
    rgb: [f32; 3],
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
                rgb,
            );
            cx += step_x;
        }
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
    rgb: [f32; 3],
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
                        rgb,
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
    in_layer: Layer,
    out_layer: &mut Layer,
    overlay: &[PixelF32],
    region_only_rect: Option<(i32, i32, i32, i32)>,
) -> Result<(), Error> {
    let width = out_layer.width();
    let height = out_layer.height();
    in_layer.iterate_with(out_layer, 0, height as i32, None, |x, y, src, mut dst| {
        let mut out = read_input_pixel(src);

        if let Some(rect) = region_only_rect
            && !point_in_rect(x, y, rect)
        {
            out = transparent_pixel();
            write_output_pixel(&mut dst, out);
            return Ok(());
        }

        if x >= 0 && y >= 0 {
            let xu = x as usize;
            let yu = y as usize;
            if xu < width && yu < height {
                let ov = overlay[yu * width + xu];
                if ov.alpha > 0.0 {
                    out = blend_overlay_preserve_alpha(out, ov);
                }
            }
        }
        write_output_pixel(&mut dst, out);
        Ok(())
    })
}

fn point_in_rect(x: i32, y: i32, rect: (i32, i32, i32, i32)) -> bool {
    let (rx, ry, rw, rh) = rect;
    if rw <= 0 || rh <= 0 {
        return false;
    }
    x >= rx && y >= ry && x < rx + rw && y < ry + rh
}

fn blend_overlay_preserve_alpha(base: PixelF32, overlay: PixelF32) -> PixelF32 {
    let a = overlay.alpha.clamp(0.0, 1.0);
    PixelF32 {
        alpha: base.alpha,
        red: (base.red * (1.0 - a) + overlay.red * a).clamp(0.0, 1.0),
        green: (base.green * (1.0 - a) + overlay.green * a).clamp(0.0, 1.0),
        blue: (base.blue * (1.0 - a) + overlay.blue * a).clamp(0.0, 1.0),
    }
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
