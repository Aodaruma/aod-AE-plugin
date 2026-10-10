// SPDX-License-Identifier: MPL-2.0
use after_effects::PixelF32;
use utils::image::{SampleEdge, sample_bilinear};

pub type Rgba = [f32; 4];

pub fn times(now: i32, step: i32, gop: i32, independent: bool) -> Result<Vec<i32>, String> {
    if independent {
        return Ok(vec![now]);
    }
    let step = step
        .checked_abs()
        .filter(|s| *s > 0)
        .ok_or("Invalid frame duration")?;
    if !(1..=60).contains(&gop) {
        return Err("GOP length must be 1..60".into());
    }
    let frame = i64::from(now).div_euclid(i64::from(step));
    let phase = i64::from(now).rem_euclid(i64::from(step));
    let first = frame.div_euclid(i64::from(gop)) * i64::from(gop);
    (first..=frame)
        .map(|n| {
            i32::try_from(n * i64::from(step) + phase).map_err(|_| "Frame time overflow".into())
        })
        .collect()
}

pub fn finite_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
pub struct MapSettings {
    pub channel: usize,
    pub invert: bool,
    pub gamma: f32,
    pub black: f32,
    pub white: f32,
}

impl MapSettings {
    pub fn value(&self, pixel: PixelF32) -> f32 {
        let a = finite_unit(pixel.alpha);
        if a == 0.0 {
            return 0.0;
        }
        let mut value = match self.channel {
            1 => pixel.red / a,
            2 => pixel.green / a,
            3 => pixel.blue / a,
            4 => a,
            _ => (0.2126 * pixel.red + 0.7152 * pixel.green + 0.0722 * pixel.blue) / a,
        };
        value = finite_unit(value);
        if self.invert {
            value = 1.0 - value;
        }
        let gamma = if self.gamma.is_finite() {
            self.gamma.clamp(0.1, 10.0)
        } else {
            1.0
        };
        value = value.powf(1.0 / gamma);
        if self.channel != 4 {
            value *= a;
        }
        value
    }

    pub fn sample(
        &self,
        pixels: &[PixelF32],
        map_w: usize,
        map_h: usize,
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        if map_w == 0 || map_h == 0 {
            return vec![0.0; width * height];
        }
        (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let sx = (x as f32 + 0.5) * map_w as f32 / width as f32 - 0.5;
                    let sy = (y as f32 + 0.5) * map_h as f32 / height as f32 - 0.5;
                    self.value(sample_bilinear(
                        pixels,
                        map_w,
                        map_h,
                        sx,
                        sy,
                        SampleEdge::Clamp,
                    ))
                })
            })
            .collect()
    }

    pub fn blocks(&self, map: &[f32], width: usize, height: usize) -> Vec<f32> {
        let mut blocks = Vec::with_capacity(width.div_ceil(16) * height.div_ceil(16));
        for by in (0..height).step_by(16) {
            for bx in (0..width).step_by(16) {
                let mut sum = 0.0;
                let mut count = 0;
                for y in by..(by + 16).min(height) {
                    for x in bx..(bx + 16).min(width) {
                        sum += map[y * width + x];
                        count += 1;
                    }
                }
                blocks.push(
                    (self.black + (self.white - self.black) * sum / count as f32)
                        .clamp(-24.0, 24.0),
                );
            }
        }
        blocks
    }
}

pub fn padded_size(width: usize, height: usize) -> Result<(usize, usize, usize), String> {
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("CodecMap supports nonempty images up to 16384 pixels per side".into());
    }
    let w = (width + 1) & !1;
    let h = (height + 1) & !1;
    Ok((w, h, w * h * 3 / 2))
}

pub fn to_yuv(pixels: &[Rgba], width: usize, height: usize) -> Result<Vec<u8>, String> {
    let (w, h, size) = padded_size(width, height)?;
    if pixels.len() != width * height {
        return Err("Invalid RGBA buffer length".into());
    }
    let rgb = |x: usize, y: usize| {
        let p = pixels[y.min(height - 1) * width + x.min(width - 1)];
        let a = finite_unit(p[3]);
        if a == 0.0 {
            [0.0; 3]
        } else {
            [
                finite_unit(p[0] / a),
                finite_unit(p[1] / a),
                finite_unit(p[2] / a),
            ]
        }
    };
    let mut out = vec![0; size];
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = rgb(x, y);
            out[y * w + x] = (16.0 + 219.0 * (0.2126 * r + 0.7152 * g + 0.0722 * b))
                .round()
                .clamp(16.0, 235.0) as u8;
        }
    }
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let mut u = 0.0;
            let mut v = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let [r, g, b] = rgb(x + dx, y + dy);
                    let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    u += (b - l) / 1.8556;
                    v += (r - l) / 1.5748;
                }
            }
            let i = (y / 2) * (w / 2) + x / 2;
            out[w * h + i] = (128.0 + 56.0 * u).round().clamp(16.0, 240.0) as u8;
            out[w * h * 5 / 4 + i] = (128.0 + 56.0 * v).round().clamp(16.0, 240.0) as u8;
        }
    }
    Ok(out)
}

pub fn decoded_rgb(yuv: &[u8], w: usize, h: usize, x: usize, y: usize) -> [f32; 3] {
    let yy = (yuv[y * w + x] as f32 - 16.0) / 219.0;
    let ci = (y / 2) * (w / 2) + x / 2;
    let u = (yuv[w * h + ci] as f32 - 128.0) / 224.0;
    let v = (yuv[w * h * 5 / 4 + ci] as f32 - 128.0) / 224.0;
    [
        finite_unit(yy + 1.5748 * v),
        finite_unit(yy - 0.187324 * u - 0.468124 * v),
        finite_unit(yy + 1.8556 * u),
    ]
}

pub fn composite(original: Rgba, rgb: [f32; 3], weight: f32) -> Rgba {
    if weight == 0.0 {
        return original;
    }
    let mut output = original;
    for c in 0..3 {
        output[c] = original[c] * (1.0 - weight) + rgb[c] * original[3] * weight;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporal_plan_has_no_future_and_preserves_phase() {
        assert_eq!(
            times(17 * 1001 + 123, 1001, 12, false).unwrap(),
            (12..=17).map(|n| n * 1001 + 123).collect::<Vec<_>>()
        );
        for now in -120..120 {
            let ts = times(now, -7, 12, false).unwrap();
            assert_eq!(ts.last(), Some(&now));
            assert!(ts.len() <= 12 && ts.iter().all(|t| *t <= now));
        }
        assert!(times(0, 0, 12, false).is_err());
        assert_eq!(times(24, 1, 12, false).unwrap(), vec![24]);
    }
    #[test]
    fn roi_preserves_partial_blocks_and_transparency() {
        let cfg = MapSettings {
            channel: 0,
            invert: false,
            gamma: 1.0,
            black: -6.0,
            white: 12.0,
        };
        let mut map = vec![0.0; 17 * 3];
        for y in 0..3 {
            map[y * 17 + 16] = 1.0;
        }
        assert_eq!(cfg.blocks(&map, 17, 3), vec![-6.0, 12.0]);
        assert_eq!(
            MapSettings {
                invert: true,
                ..cfg
            }
            .value(PixelF32 {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.0
            }),
            0.0
        );
    }
    #[test]
    fn yuv_roundtrip_and_padding() {
        for color in [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.4, 0.1, 0.2, 0.5],
        ] {
            let yuv = to_yuv(&vec![color; 17 * 9], 17, 9).unwrap();
            assert_eq!(yuv.len(), 18 * 10 * 3 / 2);
            let out = composite(color, decoded_rgb(&yuv, 18, 10, 16, 8), 1.0);
            for c in 0..3 {
                assert!((out[c] - color[c]).abs() < 0.008);
            }
            assert_eq!(out[3], color[3]);
        }
        let hdr = [-0.2, 3.4, 0.0000001, 0.73];
        assert_eq!(composite(hdr, [0.5; 3], 0.0), hdr);
    }
}
