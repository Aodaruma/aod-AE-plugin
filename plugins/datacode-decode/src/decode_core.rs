use super::*;

pub(crate) fn build_luma8(layer: &Layer) -> Vec<u8> {
    let wt = layer.world_type();
    let w = layer.width();
    let h = layer.height();
    let mut out = vec![0_u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let px = read_layer_pixel(layer, wt, x, y);
            let (r, g, b) = if px.alpha > 1.0e-6 {
                (px.red / px.alpha, px.green / px.alpha, px.blue / px.alpha)
            } else {
                (1.0, 1.0, 1.0)
            };
            let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b).clamp(0.0, 1.0);
            out[y * w + x] = (luma * 255.0).round() as u8;
        }
    }
    out
}

pub(crate) fn bbox_from_dark_luma(
    luma: &[u8],
    width: u32,
    height: u32,
    offset: (i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0_u32;
    let mut max_y = 0_u32;
    let mut found = false;
    for y in 0..height {
        for x in 0..width {
            let v = luma[(y * width + x) as usize];
            if v < 96 {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if !found {
        return None;
    }
    Some((
        offset.0 + min_x as i32,
        offset.1 + min_y as i32,
        (max_x - min_x + 1).max(8) as i32,
        (max_y - min_y + 1).max(8) as i32,
    ))
}

pub(crate) fn read_layer_pixel(
    layer: &Layer,
    wt: ae::aegp::WorldType,
    x: usize,
    y: usize,
) -> PixelF32 {
    match wt {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

pub(crate) fn clamp_rect(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    frame_w: i32,
    frame_h: i32,
) -> (i32, i32, i32, i32) {
    let x0 = x.clamp(0, frame_w.saturating_sub(1));
    let y0 = y.clamp(0, frame_h.saturating_sub(1));
    let w = w.max(8).min(frame_w - x0).max(1);
    let h = h.max(8).min(frame_h - y0).max(1);
    (x0, y0, w, h)
}

pub(crate) fn crop_luma(
    src: &[u8],
    src_w: usize,
    _src_h: usize,
    rect: (i32, i32, i32, i32),
) -> Vec<u8> {
    let (x0, y0, rw, rh) = rect;
    let mut out = vec![255_u8; (rw * rh) as usize];
    for y in 0..rh as usize {
        for x in 0..rw as usize {
            let sx = x0 as usize + x;
            let sy = y0 as usize + y;
            out[y * rw as usize + x] = src[sy * src_w + sx];
        }
    }
    out
}

pub(crate) fn bbox_from_points(
    points: &[rxing::Point],
    offset: (i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    if points.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let x = min_x.floor() as i32 + offset.0;
    let y = min_y.floor() as i32 + offset.1;
    let w = (max_x.ceil() - min_x.floor()) as i32;
    let h = (max_y.ceil() - min_y.floor()) as i32;
    Some((x, y, w.max(8), h.max(8)))
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
