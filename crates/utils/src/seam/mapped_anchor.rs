// SPDX-License-Identifier: MPL-2.0
use super::{CarveError, Channel, Image, Pixel, buffer, check_cancel, query_width};

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Black/white endpoint percentages, separately for X and Y.
    pub scales: [[f64; 2]; 2],
    /// Normalized brightness defining the fixed contour, independent of scale.
    pub anchor: f64,
    pub tolerance: f64,
    pub restore: bool,
    pub channel: Channel,
}

fn finite_clamp(value: f64, fallback: f64, lo: f64, hi: f64) -> f64 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        fallback
    }
}

pub fn scale_at(value: f64, endpoints: [f64; 2]) -> f64 {
    let value = finite_clamp(value, 0.0, 0.0, 1.0);
    let black = finite_clamp(endpoints[0], 100.0, -400.0, 400.0);
    let white = finite_clamp(endpoints[1], 100.0, -400.0, 400.0);
    black + (white - black) * value
}

fn sample_row(image: &Image, y: usize, position: f64) -> Pixel {
    let x0 = position.floor() as i64;
    let t = (position - x0 as f64) as f32;
    let read = |x: i64| {
        if x < 0 || x >= image.width as i64 {
            [0.0; 4]
        } else {
            image.pixels[y * image.width + x as usize]
        }
    };
    let a = read(x0);
    if t == 0.0 {
        return a;
    }
    let b = read(x0 + 1);
    std::array::from_fn(|c| a[c] * (1.0 - t) + b[c] * t)
}

/// Pin every sample in the anchor band and interpolated crossings between samples.
/// A single pin translates the row. Multiple pins linearly interpolate positional
/// offsets; no pin leaves the row's displacement unchanged.
fn pin_row(
    field: &mut [Pixel],
    map: &[f32],
    anchor: f64,
    tolerance: f64,
    pins: &mut Vec<(f64, f64)>,
) {
    pins.clear();
    let value = |i| finite_clamp(f64::from(map[i]), 0.0, 0.0, 1.0);
    for x in 0..map.len() {
        let m = value(x);
        if (m - anchor).abs() <= tolerance {
            pins.push((x as f64, f64::from(field[x][0]) - x as f64));
        }
        if x + 1 < map.len() {
            let next = value(x + 1);
            if (m - anchor).abs() > tolerance
                && (next - anchor).abs() > tolerance
                && ((m < anchor && next > anchor) || (m > anchor && next < anchor))
            {
                let t = (anchor - m) / (next - m);
                let position = x as f64 + t;
                let source = f64::from(field[x][0]) * (1.0 - t) + f64::from(field[x + 1][0]) * t;
                pins.push((position, source - position));
            }
        }
    }
    if pins.is_empty() {
        return;
    }
    let mut right = 0;
    for (x, sample) in field.iter_mut().enumerate() {
        let position = x as f64;
        while right < pins.len() && pins[right].0 < position {
            right += 1;
        }
        let offset = if right == 0 {
            pins[0].1
        } else if right == pins.len() {
            pins[right - 1].1
        } else {
            let (left_x, left_d) = pins[right - 1];
            let (right_x, right_d) = pins[right];
            left_d + (right_d - left_d) * ((position - left_x) / (right_x - left_x))
        };
        sample[0] = (f64::from(sample[0]) - offset) as f32;
    }
}

/// Exact DP produces a sampling field at each requested scale. Anchor constraints
/// then correct positions, independently of the black/white scale interpolation.
/// Each axis centers or restores its field to the original canvas before pinning.
pub fn apply(
    mut image: Image,
    map: &[f32],
    settings: Settings,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    if map.len() != image.pixels.len() {
        return Err(CarveError::InvalidSize);
    }
    let anchor = finite_clamp(settings.anchor, 0.0, 0.0, 1.0);
    let tolerance = finite_clamp(settings.tolerance, 0.0, 0.0, 0.5);
    check_cancel(cancel)?;
    if map
        .iter()
        .all(|&m| (finite_clamp(f64::from(m), 0.0, 0.0, 1.0) - anchor).abs() <= tolerance)
    {
        return Ok(image);
    }
    let original = [image.width, image.height];
    let mut targets = buffer(map.len(), 1)?;
    let mut row_map = buffer(original[0].max(original[1]), 0.0)?;
    let mut pins = Vec::new();
    pins.try_reserve_exact(row_map.len())
        .map_err(|_| CarveError::OutOfMemory)?;
    for (axis, &size) in original.iter().enumerate() {
        let mut identity = true;
        for y in 0..original[1] {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            for x in 0..original[0] {
                let scale = scale_at(f64::from(map[y * original[0] + x]), settings.scales[axis]);
                let magnitude = (size as f64 * scale.abs() / 100.0).round().max(1.0) as i32;
                let signed = if scale < 0.0 { -magnitude } else { magnitude };
                let index = if axis == 0 {
                    y * original[0] + x
                } else {
                    x * original[1] + y
                };
                targets[index] = signed;
                identity &= signed == size as i32;
            }
        }
        if identity {
            continue;
        }
        if axis == 1 {
            image = image.transpose(cancel)?;
        }
        let mut copy = buffer(image.pixels.len(), [0.0; 4])?;
        copy.copy_from_slice(&image.pixels);
        let input = Image::new(image.width, image.height, copy)?;
        let mut field = query_width(
            input,
            &targets,
            settings.restore,
            settings.channel,
            true,
            cancel,
        )?;
        let mut output = Image::new(
            image.width,
            image.height,
            buffer(image.pixels.len(), [0.0; 4])?,
        )?;
        for y in 0..image.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let row = &mut row_map[..image.width];
            for (x, value) in row.iter_mut().enumerate() {
                *value = if axis == 0 {
                    map[y * original[0] + x]
                } else {
                    map[x * original[0] + y]
                };
            }
            let fields = &mut field.pixels[y * image.width..(y + 1) * image.width];
            pin_row(fields, row, anchor, tolerance, &mut pins);
            for x in 0..image.width {
                let i = y * image.width + x;
                let m = finite_clamp(f64::from(row[x]), 0.0, 0.0, 1.0);
                output.pixels[i] = if (m - anchor).abs() <= tolerance {
                    image.pixels[i]
                } else {
                    sample_row(&image, y, f64::from(fields[x][0]))
                };
            }
        }
        image = output;
        if axis == 1 {
            image = image.transpose(cancel)?;
        }
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_interpolation_is_linear_and_independent_of_anchor() {
        for (value, expected) in [(0.0, 50.0), (0.3, 80.0), (0.5, 100.0), (1.0, 150.0)] {
            assert_eq!(scale_at(value, [50.0, 150.0]), expected);
        }
        assert_eq!(scale_at(0.25, [-100.0, 100.0]), -50.0);
    }

    #[test]
    fn restoration_keeps_anchor_plateaus_and_identity_is_exact() {
        let pixels: Vec<_> = (0..63).map(|i| [i as f32 / 63.0, 0.7, 0.2, 1.0]).collect();
        for restore in [false, true] {
            let settings = Settings {
                scales: [[50.0, 150.0], [200.0, 75.0]],
                anchor: 0.5,
                tolerance: 0.002,
                restore,
                channel: Channel::Rgba,
            };
            let result = apply(
                Image::new(9, 7, pixels.clone()).unwrap(),
                &[0.5; 63],
                settings,
                &mut || false,
            )
            .unwrap();
            assert_eq!(result.pixels, pixels);
            let map: Vec<_> = (0..63)
                .map(|i| {
                    if i % 3 == 0 {
                        0.5
                    } else {
                        (i % 7) as f32 / 6.0
                    }
                })
                .collect();
            let result = apply(
                Image::new(9, 7, pixels.clone()).unwrap(),
                &map,
                settings,
                &mut || false,
            )
            .unwrap();
            for i in 0..63 {
                if map[i] == 0.5 {
                    assert_eq!(result.pixels[i], pixels[i]);
                }
            }
            let identity = apply(
                Image::new(9, 7, pixels.clone()).unwrap(),
                &map,
                Settings {
                    scales: [[100.0; 2]; 2],
                    ..settings
                },
                &mut || false,
            )
            .unwrap();
            assert_eq!(identity.pixels, pixels);
        }
    }

    #[test]
    fn interpolated_anchor_crossing_and_absent_anchor_are_defined() {
        let original = vec![[2.0, 0.0, 1.0, 1.0], [4.0, 0.0, 1.0, 1.0]];
        let mut field = original.clone();
        let mut pins = Vec::with_capacity(2);
        pin_row(&mut field, &[0.0, 1.0], 0.25, 0.0, &mut pins);
        assert_eq!(pins[0].0, 0.25);
        assert_eq!(
            f64::from(field[0][0]) * 0.75 + f64::from(field[1][0]) * 0.25,
            0.25
        );
        field.copy_from_slice(&original);
        pin_row(&mut field, &[0.0, 0.1], 0.5, 0.0, &mut pins);
        assert_eq!(field, original);
    }

    #[test]
    fn pinning_translates_single_anchor_and_fixes_multiple_anchors() {
        let mut pins = Vec::with_capacity(5);
        let mut field = vec![
            [1.0, 0.0, 1.0, 1.0],
            [3.0, 0.0, 1.0, 1.0],
            [5.0, 0.0, 1.0, 1.0],
        ];
        pin_row(&mut field, &[0.0, 0.5, 1.0], 0.5, 0.0, &mut pins);
        assert_eq!(
            field.iter().map(|p| p[0]).collect::<Vec<_>>(),
            [-1.0, 1.0, 3.0]
        );
        pin_row(&mut field, &[0.5, 0.0, 0.5], 0.5, 0.0, &mut pins);
        assert_eq!(field[0][0], 0.0);
        assert_eq!(field[2][0], 2.0);
    }

    #[test]
    fn anchor_pixels_remain_bit_exact_with_both_axes_shrinking_expanding_and_flipping() {
        let (w, h) = (17, 13);
        let pixels: Vec<_> = (0..w * h)
            .map(|i| {
                [
                    (i % 17) as f32 / 16.0,
                    (i % 13) as f32 / 12.0,
                    (i % 7) as f32 / 6.0,
                    (i % 5) as f32 / 4.0,
                ]
            })
            .collect();
        for anchor in [0.0, 0.3, 0.5, 1.0] {
            let map: Vec<_> = (0..w * h)
                .map(|i| {
                    if i % 5 == 0 {
                        anchor as f32
                    } else {
                        (i % 11) as f32 / 10.0
                    }
                })
                .collect();
            for scales in [
                [[50.0, 150.0], [150.0, 50.0]],
                [[-200.0, 75.0], [75.0, -150.0]],
                [[50.0, 50.0], [200.0, 200.0]],
            ] {
                let result = apply(
                    Image::new(w, h, pixels.clone()).unwrap(),
                    &map,
                    Settings {
                        scales,
                        anchor,
                        tolerance: 0.002,
                        restore: false,
                        channel: Channel::Rgba,
                    },
                    &mut || false,
                )
                .unwrap();
                let mut changed = false;
                for i in 0..pixels.len() {
                    if (f64::from(map[i]) - anchor).abs() <= 0.002 {
                        assert_eq!(result.pixels[i], pixels[i], "anchor={anchor} pixel={i}");
                    } else {
                        changed |= result.pixels[i] != pixels[i];
                    }
                }
                assert!(changed);
            }
        }
    }
}
