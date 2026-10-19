use crate::curve::Curve;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub width: f32,
    pub offset: f32,
    pub direction: i32,
    pub threshold: f32,
    pub silhouette_blur: f32,
    pub height_blur: f32,
    pub method: i32,
    pub smooth_radius: f32,
    pub iterations: usize,
    pub elevation: f32,
    pub smooth_curve: bool,
    pub scale: [f32; 2],
    pub curve: Curve,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 32.0,
            offset: 0.0,
            direction: 1,
            threshold: 0.5,
            silhouette_blur: 0.0,
            height_blur: 0.0,
            method: 1,
            smooth_radius: 8.0,
            iterations: 200,
            elevation: 16.0,
            smooth_curve: true,
            scale: [1.0; 2],
            curve: Curve::default(),
        }
    }
}

pub struct Surface {
    pub height: Vec<f32>,
    pub normals: Vec<[f32; 3]>,
    pub coverage: Vec<f32>,
}

// Felzenszwalb/Huttenlocher's lower envelope of parabolas, in physical pixel units.
fn edt_line(input: &[f32], step: f32, output: &mut [f32], sites: &mut [usize], breaks: &mut [f64]) {
    let a = f64::from(step).powi(2);
    let mut k = 0;
    sites[0] = 0;
    breaks[0] = f64::NEG_INFINITY;
    breaks[1] = f64::INFINITY;
    for q in 1..input.len() {
        let intersection = |p: usize| {
            ((input[q] as f64 - input[p] as f64) / a + (q * q) as f64 - (p * p) as f64)
                / (2.0 * (q - p) as f64)
        };
        let mut s = intersection(sites[k]);
        while k > 0 && s <= breaks[k] {
            k -= 1;
            s = intersection(sites[k]);
        }
        k += 1;
        sites[k] = q;
        breaks[k] = s;
        breaks[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in output.iter_mut().enumerate() {
        while breaks[k + 1] < q as f64 {
            k += 1;
        }
        *out = (a * (q as f64 - sites[k] as f64).powi(2) + input[sites[k]] as f64) as f32;
    }
}

fn squared_distance(mask: &[bool], w: usize, h: usize, target: bool, scale: [f32; 2]) -> Vec<f32> {
    let mut field: Vec<f32> = mask
        .iter()
        .map(|&v| if v == target { 0.0 } else { 1e20 })
        .collect();
    let n = w.max(h);
    let mut input = vec![0.0; n];
    let mut output = vec![0.0; n];
    let mut sites = vec![0; n];
    let mut breaks = vec![0.0; n + 1];
    for row in field.chunks_exact_mut(w) {
        edt_line(row, scale[0], &mut output[..w], &mut sites, &mut breaks);
        row.copy_from_slice(&output[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            input[y] = field[y * w + x];
        }
        edt_line(
            &input[..h],
            scale[1],
            &mut output[..h],
            &mut sites,
            &mut breaks,
        );
        for y in 0..h {
            field[y * w + x] = output[y];
        }
    }
    field
}

pub fn signed_distance(mask: &[bool], w: usize, h: usize, scale: [f32; 2]) -> Vec<f32> {
    if w == 0 || h == 0 {
        return vec![];
    }
    if !mask.iter().any(|&v| v) {
        return vec![-1e10; w * h];
    }
    // A transparent border gives opaque full-frame inputs a finite, closed silhouette.
    let pw = w + 2;
    let ph = h + 2;
    let mut padded = vec![false; pw * ph];
    for y in 0..h {
        padded[(y + 1) * pw + 1..(y + 1) * pw + 1 + w].copy_from_slice(&mask[y * w..(y + 1) * w]);
    }
    let to_inside = squared_distance(&padded, pw, ph, true, scale);
    let to_outside = squared_distance(&padded, pw, ph, false, scale);
    let half = 0.5 * scale[0].min(scale[1]);
    let mut result = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = (y + 1) * pw + x + 1;
            result[y * w + x] = if padded[i] {
                to_outside[i].sqrt() - half
            } else {
                half - to_inside[i].sqrt()
            };
        }
    }
    result
}

// Three separable fractional box passes approximate a Gaussian without radius-dependent cost.
pub fn blur(values: &mut Vec<f32>, w: usize, h: usize, sigma: f32, scale: [f32; 2]) {
    if sigma <= 0.0 || w == 0 || h == 0 {
        return;
    }
    let mut scratch = vec![0.0; values.len()];
    let mut prefix = vec![0.0_f64; w.max(h) + 1];
    for _ in 0..3 {
        for (axis, step) in scale.iter().enumerate() {
            let (length, lines) = if axis == 0 { (w, h) } else { (h, w) };
            let radius = (sigma / step).min(length as f32);
            let r = radius.floor() as usize;
            let frac = radius - r as f32;
            let index = |line: usize, p: usize| {
                if axis == 0 {
                    line * w + p
                } else {
                    p * w + line
                }
            };
            for line in 0..lines {
                prefix[0] = 0.0;
                for p in 0..length {
                    prefix[p + 1] = prefix[p] + values[index(line, p)] as f64;
                }
                for p in 0..length {
                    let a = p.saturating_sub(r);
                    let b = (p + r + 1).min(length);
                    let mut sum = prefix[b] - prefix[a];
                    if p > r {
                        sum += frac as f64 * values[index(line, p - r - 1)] as f64;
                    }
                    if p + r + 1 < length {
                        sum += frac as f64 * values[index(line, p + r + 1)] as f64;
                    }
                    scratch[index(line, p)] = (sum / (2.0 * radius as f64 + 1.0)) as f32;
                }
            }
            std::mem::swap(values, &mut scratch);
        }
    }
}

pub fn coordinate(distance: f32, s: Settings) -> f32 {
    let bias = match s.direction {
        2 => 1.0,
        3 => 0.5,
        _ => 0.0,
    };
    ((distance + s.offset) / s.width.max(0.01) + bias).clamp(0.0, 1.0)
}

fn smooth_profile<E>(
    rhs: &[f32],
    w: usize,
    h: usize,
    s: Settings,
    cancel: &impl Fn() -> Result<(), E>,
) -> Result<Vec<f32>, E> {
    let mut values = rhs.to_vec();
    let weights = [
        (s.smooth_radius / s.scale[0]).powi(2),
        (s.smooth_radius / s.scale[1]).powi(2),
    ];
    // (I - r^2 Laplacian) u = SDF profile. The identity term makes the Neumann system unique.
    for iteration in 0..s.iterations {
        if iteration % 8 == 0 {
            cancel()?;
        }
        let mut delta = 0.0_f32;
        for parity in 0..2 {
            for y in 0..h {
                for x in 0..w {
                    if (x + y) % 2 != parity {
                        continue;
                    }
                    let i = y * w + x;
                    if rhs[i] <= 0.0 {
                        continue;
                    }
                    let neighbors = [
                        x.checked_sub(1).map(|_| i - 1),
                        (x + 1 < w).then_some(i + 1),
                        y.checked_sub(1).map(|_| i - w),
                        (y + 1 < h).then_some(i + w),
                    ];
                    let mut sum = rhs[i];
                    let mut denom = 1.0;
                    for (j, neighbor) in neighbors.iter().enumerate() {
                        let weight = weights[j / 2];
                        if let Some(k) = *neighbor
                            && rhs[k] > 0.0
                        {
                            sum += weight * values[k];
                            denom += weight;
                        } else if s.method == 3 {
                            // Dirichlet uses zero-height ghost cells; Neumann omits their flux.
                            denom += weight;
                        }
                    }
                    let next = sum / denom;
                    delta = delta.max((next - values[i]).abs());
                    values[i] = next;
                }
            }
        }
        if delta < 1e-5 {
            break;
        }
    }
    Ok(values)
}

pub fn build<E>(
    silhouette: &[f32],
    w: usize,
    h: usize,
    s: Settings,
    cancel: impl Fn() -> Result<(), E>,
) -> Result<Surface, E> {
    cancel()?;
    let mut blurred = silhouette.to_vec();
    blur(&mut blurred, w, h, s.silhouette_blur, s.scale);
    let mask: Vec<bool> = blurred.iter().map(|&v| v > s.threshold).collect();
    let has_shape = mask.iter().any(|&v| v);
    let distance = signed_distance(&mask, w, h, s.scale);
    cancel()?;
    let profile: Vec<f32> = distance.iter().map(|&d| coordinate(d, s)).collect();
    let profile = if s.method != 1 && s.smooth_radius > 0.0 {
        smooth_profile(&profile, w, h, s, &cancel)?
    } else {
        profile
    };
    let mut height: Vec<f32> = profile
        .iter()
        .map(|&t| {
            if has_shape {
                s.curve.sample(t, s.smooth_curve)
            } else {
                0.0
            }
        })
        .collect();
    blur(&mut height, w, h, s.height_blur, s.scale);
    cancel()?;
    let mut normals = vec![[0.0, 0.0, 1.0]; w * h];
    let mut coverage = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let l = height[y * w + x.saturating_sub(1)];
            let r = height[y * w + (x + 1).min(w - 1)];
            let u = height[y.saturating_sub(1) * w + x];
            let d = height[(y + 1).min(h - 1) * w + x];
            let gx = (r - l) * 0.5;
            let gy = (d - u) * 0.5;
            normals[i] = normalize([
                -gx * s.elevation / s.scale[0],
                -gy * s.elevation / s.scale[1],
                1.0,
            ]);
            coverage[i] = if height[i] <= 1e-6 {
                0.0
            } else {
                (height[i] / (gx.abs() + gy.abs()).max(1e-6)).clamp(0.0, 1.0)
            };
        }
    }
    Ok(Surface {
        height,
        normals,
        coverage,
    })
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = v.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !length.is_finite() || length <= 1e-10 {
        [0.0, 0.0, 1.0]
    } else {
        v.map(|v| v / length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_distance_handles_holes_empty_full_and_anisotropic_pixels() {
        let mut mask = vec![false; 49];
        mask[24] = true;
        let d = signed_distance(&mask, 7, 7, [2.0, 1.0]);
        assert!((d[24] - 0.5).abs() < 1e-6);
        assert!((d[25] + 1.5).abs() < 1e-6);
        assert!((d[32] - (0.5 - 5.0_f32.sqrt())).abs() < 1e-6);
        assert!(
            signed_distance(&[false; 9], 3, 3, [1.0; 2])
                .iter()
                .all(|&v| v < 0.0 && v.is_finite())
        );
        assert_eq!(signed_distance(&[true; 9], 3, 3, [1.0; 2])[4], 1.5);
        let mut mask = [true; 49];
        mask[24] = false;
        assert!(signed_distance(&mask, 7, 7, [1.0; 2])[24] < 0.0);
    }

    #[test]
    fn exact_transform_matches_brute_force() {
        let mask: Vec<bool> = (0..143).map(|i| i % 17 == 3 || i % 29 == 9).collect();
        let d = squared_distance(&mask, 13, 11, true, [1.7, 0.8]);
        for (i, actual) in d.iter().enumerate() {
            let expected = (0..143)
                .filter(|&j| mask[j])
                .map(|j| {
                    ((i % 13) as f32 - (j % 13) as f32).powi(2) * 1.7_f32.powi(2)
                        + ((i / 13) as f32 - (j / 13) as f32).powi(2) * 0.8_f32.powi(2)
                })
                .fold(f32::INFINITY, f32::min);
            assert!((actual - expected).abs() < 1e-4);
        }
    }

    #[test]
    fn sides_and_offset_have_physical_meaning() {
        let mut s = Settings {
            width: 10.0,
            ..Settings::default()
        };
        assert_eq!(coordinate(5.0, s), 0.5);
        assert_eq!(coordinate(-5.0, s), 0.0);
        s.direction = 2;
        assert_eq!(coordinate(-5.0, s), 0.5);
        s.direction = 3;
        assert_eq!(coordinate(0.0, s), 0.5);
        s.offset = 5.0;
        assert_eq!(coordinate(-5.0, s), 0.5);
    }

    #[test]
    fn neumann_has_zero_flux_and_dirichlet_loses_edge_height() {
        let s = Settings {
            method: 2,
            ..Settings::default()
        };
        let neumann = smooth_profile(&[1.0; 25], 5, 5, s, &|| Ok::<_, ()>(())).unwrap();
        assert!(neumann.iter().all(|&v| (v - 1.0).abs() < 1e-5));
        let dirichlet = smooth_profile(&[1.0; 25], 5, 5, Settings { method: 3, ..s }, &|| {
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(dirichlet[0] < neumann[0]);
        assert!(dirichlet[12] > dirichlet[0]);
    }

    #[test]
    fn ring_profile_cuts_a_hole_and_empty_input_stays_empty() {
        let s = Settings {
            curve: Curve::preset(4),
            width: 5.0,
            ..Settings::default()
        };
        let surface = build(&[1.0; 225], 15, 15, s, || Ok::<_, ()>(())).unwrap();
        assert_eq!(surface.coverage[112], 0.0);
        assert!(surface.coverage.iter().any(|&v| v > 0.9));
        let empty = build(&[0.0; 9], 3, 3, s, || Ok::<_, ()>(())).unwrap();
        assert_eq!(empty.coverage, vec![0.0; 9]);
        assert!(empty.normals.iter().all(|&n| n == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn cancellation_is_propagated() {
        assert!(
            build(&[1.0; 9], 3, 3, Settings::default(), || Err::<(), _>(
                "cancel"
            ))
            .is_err()
        );
    }
}
