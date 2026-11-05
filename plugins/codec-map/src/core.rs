// SPDX-License-Identifier: MPL-2.0
use after_effects::PixelF32;

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
        if gamma != 1.0 {
            value = value.powf(1.0 / gamma);
        }
        if self.channel != 4 {
            value *= a;
        }
        value
    }

    #[cfg(test)]
    pub fn sample(
        &self,
        pixels: &[PixelF32],
        map_w: usize,
        map_h: usize,
        width: usize,
        height: usize,
    ) -> Vec<f32> {
        self.sample_from(map_w, map_h, width, height, |x, y| pixels[y * map_w + x])
    }

    pub fn sample_from(
        &self,
        map_w: usize,
        map_h: usize,
        width: usize,
        height: usize,
        pixel: impl Fn(usize, usize) -> PixelF32,
    ) -> Vec<f32> {
        if map_w == 0 || map_h == 0 {
            return vec![0.0; width * height];
        }
        (0..height)
            .flat_map(|y| {
                let pixel = &pixel;
                (0..width).map(move |x| {
                    let sx = (x as f32 + 0.5) * map_w as f32 / width as f32 - 0.5;
                    let sy = (y as f32 + 0.5) * map_h as f32 / height as f32 - 0.5;
                    let x0 = sx.floor();
                    let y0 = sy.floor();
                    let tx = sx - x0;
                    let ty = sy - y0;
                    let at = |x: i64, y: i64| {
                        pixel(
                            x.clamp(0, map_w as i64 - 1) as usize,
                            y.clamp(0, map_h as i64 - 1) as usize,
                        )
                    };
                    let x0 = x0 as i64;
                    let y0 = y0 as i64;
                    self.value(utils::image::lerp_pixel(
                        utils::image::lerp_pixel(at(x0, y0), at(x0 + 1, y0), tx),
                        utils::image::lerp_pixel(at(x0, y0 + 1), at(x0 + 1, y0 + 1), tx),
                        ty,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub x: i32,
    pub y: i32,
}

impl Canvas {
    /// Map a canvas pixel into another buffer in the same layer coordinates.
    pub fn source_pixel(self, source: Self, x: usize, y: usize) -> Option<(usize, usize)> {
        let sx = x as i64 + i64::from(self.x) - i64::from(source.x);
        let sy = y as i64 + i64::from(self.y) - i64::from(source.y);
        if sx < 0 || sy < 0 || sx >= source.width as i64 || sy >= source.height as i64 {
            None
        } else {
            Some((sx as usize, sy as usize))
        }
    }
}

pub fn check_buffer_memory(bytes: usize, limit_mib: i32) -> Result<(), String> {
    if !(64..=8192).contains(&limit_mib) {
        return Err("Working Memory must be 64..8192 MiB".into());
    }
    if bytes > limit_mib as usize * 1024 * 1024 {
        return Err(format!(
            "Working buffers need {} MiB. Increase Working Memory or lower preview resolution.",
            bytes.div_ceil(1024 * 1024)
        ));
    }
    Ok(())
}
pub fn check_working_memory(width: usize, height: usize, limit_mib: i32) -> Result<(), String> {
    padded_size(width, height)?;
    if !(64..=8192).contains(&limit_mib) {
        return Err("Working Memory must be 64..8192 MiB".into());
    }
    let bytes = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(32))
        .ok_or("Working size overflow")?;
    if bytes > limit_mib as usize * 1024 * 1024 {
        return Err(format!(
            "Working buffers need {} MiB. Increase Working Memory or lower preview resolution.",
            bytes.div_ceil(1024 * 1024)
        ));
    }
    Ok(())
}

#[cfg(test)]
pub fn check_replay_memory(
    width: usize,
    height: usize,
    frames: usize,
    limit_mib: i32,
) -> Result<(), String> {
    let (w, h, size) = padded_size(width, height)?;
    let per_frame = size + w.div_ceil(16) * h.div_ceil(16) * size_of::<f32>();
    let bytes = per_frame
        .checked_mul(frames)
        .ok_or("Replay size overflow")?;
    if !(64..=8192).contains(&limit_mib) {
        return Err("Replay Memory must be 64..8192 MiB".into());
    }
    if bytes > limit_mib as usize * 1024 * 1024 {
        return Err(format!(
            "Replay needs {} MiB; Replay Memory is {} MiB. Increase Replay Memory, lower preview resolution, or keep a recent I-frame.",
            bytes.div_ceil(1024 * 1024),
            limit_mib
        ));
    }
    Ok(())
}

#[cfg(test)]
pub fn to_yuv(pixels: &[Rgba], width: usize, height: usize) -> Result<Vec<u8>, String> {
    if pixels.len() != width * height {
        return Err("Invalid RGBA buffer length".into());
    }
    to_yuv_from(width, height, |x, y| pixels[y * width + x])
}

/// Convert directly from host pixels without retaining a float RGBA copy.
pub fn to_yuv_from(
    width: usize,
    height: usize,
    pixel: impl Fn(usize, usize) -> Rgba,
) -> Result<Vec<u8>, String> {
    let (w, h, size) = padded_size(width, height)?;
    let rgb = |x: usize, y: usize| {
        let p = pixel(x.min(width - 1), y.min(height - 1));
        let a = finite_unit(p[3]);
        if a == 0.0 {
            [0.0; 3]
        } else if a == 1.0 {
            [finite_unit(p[0]), finite_unit(p[1]), finite_unit(p[2])]
        } else {
            [
                finite_unit(p[0] / a),
                finite_unit(p[1] / a),
                finite_unit(p[2] / a),
            ]
        }
    };
    let mut out = Vec::new();
    out.try_reserve_exact(size)
        .map_err(|_| "Not enough available memory for a codec frame. Lower preview resolution or Replay Memory.")?;
    out.resize(size, 0);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let mut u = 0.0;
            let mut v = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let [r, g, b] = rgb(x + dx, y + dy);
                    let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    out[(y + dy) * w + x + dx] =
                        (16.0 + 219.0 * l).round().clamp(16.0, 235.0) as u8;
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
    fn direct_map_sampling_matches_shared_bilinear() {
        let p = (0..35)
            .map(|i| PixelF32 {
                red: i as f32 / 41.0,
                green: 0.2,
                blue: 0.7,
                alpha: if i % 3 == 0 { 0.5 } else { 1.0 },
            })
            .collect::<Vec<_>>();
        let cfg = MapSettings {
            channel: 0,
            invert: true,
            gamma: 1.8,
            black: -6.0,
            white: 12.0,
        };
        let actual = cfg.sample(&p, 7, 5, 13, 9);
        for y in 0..9 {
            for x in 0..13 {
                let sx = (x as f32 + 0.5) * 7.0 / 13.0 - 0.5;
                let sy = (y as f32 + 0.5) * 5.0 / 9.0 - 0.5;
                let expected = cfg.value(utils::image::sample_bilinear(
                    &p,
                    7,
                    5,
                    sx,
                    sy,
                    utils::image::SampleEdge::Clamp,
                ));
                assert_eq!(actual[y * 13 + x], expected);
            }
        }
    }
    #[test]
    fn working_memory_is_independent_of_history_length() {
        assert!(check_working_memory(1920, 1080, 64).is_ok());
        assert!(check_working_memory(3840, 2160, 64).is_err());
        assert!(check_working_memory(3840, 2160, 256).is_ok());
    }
    #[test]
    fn replay_budget_includes_roi_and_can_exceed_old_limit() {
        // Ninety full-HD frames require 270 MiB including ROI offsets.
        let error = check_replay_memory(1920, 1080, 90, 256).unwrap_err();
        assert!(error.contains("270 MiB"));
        assert!(check_replay_memory(1920, 1080, 90, 1024).is_ok());
        assert!(check_replay_memory(3840, 2160, 90, 1024).is_err());
        assert!(check_replay_memory(3840, 2160, 90, 2048).is_ok());
        assert!(check_replay_memory(1920, 1080, usize::MAX, 8192).is_err());
        assert!(check_replay_memory(1920, 1080, 1, 0).is_err());
    }

    #[test]
    fn varying_bounds_align_without_scaling_and_pad_black() {
        let canvas = Canvas {
            width: 5,
            height: 3,
            x: -2,
            y: 4,
        };
        let source = Canvas {
            width: 3,
            height: 2,
            x: 0,
            y: 5,
        };
        let pixels = [
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0, 1.0],
        ];
        let aligned = to_yuv_from(5, 3, |x, y| {
            canvas
                .source_pixel(source, x, y)
                .map_or([0.0; 4], |(sx, sy)| pixels[sy * 3 + sx])
        })
        .unwrap();
        let mut expected = vec![[0.0; 4]; 15];
        expected[7..10].copy_from_slice(&pixels[..3]);
        expected[12..15].copy_from_slice(&pixels[3..]);
        assert_eq!(aligned, to_yuv(&expected, 5, 3).unwrap());
        // A larger preceding frame is cropped; empty/disjoint inputs are black.
        assert_eq!(source.source_pixel(canvas, 0, 0), Some((2, 1)));
        assert_eq!(
            canvas.source_pixel(Canvas { width: 0, ..source }, 2, 1),
            None
        );
        assert_eq!(
            canvas.source_pixel(
                Canvas {
                    x: i32::MAX,
                    ..source
                },
                2,
                1
            ),
            None
        );
        assert_eq!(
            Canvas {
                x: i32::MAX,
                ..canvas
            }
            .source_pixel(
                Canvas {
                    x: i32::MIN,
                    ..source
                },
                0,
                0
            ),
            None
        );
    }

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
            assert_eq!(times(now, -7, 1, false).unwrap(), vec![now]);
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
