use after_effects as ae;

use ae::PixelF32;
use ae::pf::Layer;

use crate::ToPixel;

pub const TRANSPARENT: PixelF32 = PixelF32 {
    alpha: 0.0,
    red: 0.0,
    green: 0.0,
    blue: 0.0,
};

// f32 no longer represents every integer beyond 2^52. Coordinates this large
// cannot describe a meaningful image sample and may overflow neighbour math.
const MAX_SAFE_SAMPLE_COORDINATE: f32 = 4_503_599_627_370_496.0;

// Separable filters currently top out at 8-lobe Lanczos. A non-integral
// coordinate can cover one more tap than the two inclusive radius endpoints.
const MAX_SEPARABLE_RADIUS: usize = 8;
const MAX_SEPARABLE_TAPS: usize = MAX_SEPARABLE_RADIUS * 2 + 2;

#[derive(Clone, Copy)]
struct SeparableTap {
    coordinate: i64,
    weight: f32,
}

const EMPTY_SEPARABLE_TAP: SeparableTap = SeparableTap {
    coordinate: 0,
    weight: 0.0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleEdge {
    Transparent,
    Clamp,
    Tile,
    Mirror,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampleFilter {
    Nearest,
    Bilinear,
    Bicubic,
    Mitchell { b: f32, c: f32 },
    Lanczos { lobes: f32 },
    CubicBSpline,
    EwaQuadratic { radius: f32 },
}

#[inline]
pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[inline]
pub fn unpremultiplied_rgb(pixel: PixelF32) -> [f32; 3] {
    let alpha = finite_or(pixel.alpha, 0.0);
    if alpha.abs() > 1.0e-6 {
        [
            finite_or(pixel.red / alpha, 0.0),
            finite_or(pixel.green / alpha, 0.0),
            finite_or(pixel.blue / alpha, 0.0),
        ]
    } else {
        [0.0; 3]
    }
}

#[inline]
pub fn luma(pixel: PixelF32) -> f32 {
    let rgb = unpremultiplied_rgb(pixel);
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

pub fn sanitize_pixel(mut pixel: PixelF32, clamp_rgb: bool) -> PixelF32 {
    pixel.alpha = finite_or(pixel.alpha, 0.0).clamp(0.0, 1.0);
    pixel.red = finite_or(pixel.red, 0.0);
    pixel.green = finite_or(pixel.green, 0.0);
    pixel.blue = finite_or(pixel.blue, 0.0);
    if clamp_rgb {
        pixel.red = pixel.red.clamp(0.0, 1.0);
        pixel.green = pixel.green.clamp(0.0, 1.0);
        pixel.blue = pixel.blue.clamp(0.0, 1.0);
    }
    pixel
}

pub fn read_pixel(layer: &Layer, x: usize, y: usize) -> PixelF32 {
    if x >= layer.width() || y >= layer.height() {
        return TRANSPARENT;
    }
    match layer.world_type() {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

pub fn read_layer(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let mut pixels = vec![TRANSPARENT; width.saturating_mul(height)];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = read_pixel(layer, x, y);
        }
    }
    pixels
}

pub fn sample_nearest(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
) -> PixelF32 {
    if !coordinates_are_safe(x, y) {
        return TRANSPARENT;
    }
    sample_integer(
        pixels,
        width,
        height,
        x.round() as i64,
        y.round() as i64,
        edge,
    )
}

pub fn sample_bilinear(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
) -> PixelF32 {
    if width == 0 || height == 0 || !coordinates_are_safe(x, y) {
        return TRANSPARENT;
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;
    let x0 = x0 as i64;
    let y0 = y0 as i64;
    let p00 = sample_integer(pixels, width, height, x0, y0, edge);
    let x1 = x0.saturating_add(1);
    let y1 = y0.saturating_add(1);
    let p10 = sample_integer(pixels, width, height, x1, y0, edge);
    let p01 = sample_integer(pixels, width, height, x0, y1, edge);
    let p11 = sample_integer(pixels, width, height, x1, y1, edge);
    lerp_pixel(lerp_pixel(p00, p10, tx), lerp_pixel(p01, p11, tx), ty)
}

pub fn sample_filtered(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
    filter: SampleFilter,
) -> PixelF32 {
    match filter {
        SampleFilter::Nearest => sample_nearest(pixels, width, height, x, y, edge),
        SampleFilter::Bilinear => sample_bilinear(pixels, width, height, x, y, edge),
        SampleFilter::Bicubic => sample_separable(pixels, width, height, x, y, edge, 2.0, |d| {
            cubic_keys_weight(d, -0.5)
        }),
        SampleFilter::Mitchell { b, c } => {
            sample_separable(pixels, width, height, x, y, edge, 2.0, |d| {
                mitchell_weight(d, b, c)
            })
        }
        SampleFilter::Lanczos { lobes } => {
            let lobes = finite_or(lobes, 3.0).clamp(1.0, MAX_SEPARABLE_RADIUS as f32);
            sample_separable(pixels, width, height, x, y, edge, lobes, |d| {
                lanczos_weight(d, lobes)
            })
        }
        SampleFilter::CubicBSpline => {
            sample_separable(pixels, width, height, x, y, edge, 2.0, cubic_bspline_weight)
        }
        SampleFilter::EwaQuadratic { radius } => sample_radial_quadratic(
            pixels,
            width,
            height,
            x,
            y,
            edge,
            finite_or(radius, 2.0).clamp(0.5, 8.0),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn sample_separable<F>(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
    radius: f32,
    weight: F,
) -> PixelF32
where
    F: Fn(f32) -> f32,
{
    if width == 0 || height == 0 || !coordinates_are_safe(x, y) {
        return TRANSPARENT;
    }
    let min_x = (x - radius).floor() as i64;
    let max_x = (x + radius).ceil() as i64;
    let min_y = (y - radius).floor() as i64;
    let max_y = (y + radius).ceil() as i64;
    let x_tap_count = max_x.saturating_sub(min_x).saturating_add(1);
    let Ok(x_tap_count) = usize::try_from(x_tap_count) else {
        return sample_separable_uncached(pixels, width, height, x, y, edge, radius, &weight);
    };
    if x_tap_count > MAX_SEPARABLE_TAPS {
        // At very large f32 coordinates, rounding can make x +/- radius span
        // more integer taps than the nominal kernel diameter. Preserve the
        // previous behavior without allocating or indexing past the stack.
        return sample_separable_uncached(pixels, width, height, x, y, edge, radius, &weight);
    }

    let mut x_taps = [EMPTY_SEPARABLE_TAP; MAX_SEPARABLE_TAPS];
    for (tap, sx) in x_taps.iter_mut().zip(min_x..=max_x) {
        *tap = SeparableTap {
            coordinate: sx,
            weight: weight(x - sx as f32),
        };
    }

    let mut sum = TRANSPARENT;
    let mut weight_sum = 0.0;
    for sy in min_y..=max_y {
        let wy = weight(y - sy as f32);
        if wy == 0.0 {
            continue;
        }
        for tap in &x_taps[..x_tap_count] {
            let sample_weight = wy * tap.weight;
            if sample_weight == 0.0 || !sample_weight.is_finite() {
                continue;
            }
            add_weighted(
                &mut sum,
                sample_integer(pixels, width, height, tap.coordinate, sy, edge),
                sample_weight,
            );
            weight_sum += sample_weight;
        }
    }
    normalized_pixel(sum, weight_sum)
}

#[allow(clippy::too_many_arguments)]
fn sample_separable_uncached<F>(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
    radius: f32,
    weight: &F,
) -> PixelF32
where
    F: Fn(f32) -> f32,
{
    let min_x = (x - radius).floor() as i64;
    let max_x = (x + radius).ceil() as i64;
    let min_y = (y - radius).floor() as i64;
    let max_y = (y + radius).ceil() as i64;
    let mut sum = TRANSPARENT;
    let mut weight_sum = 0.0;
    for sy in min_y..=max_y {
        let wy = weight(y - sy as f32);
        if wy == 0.0 {
            continue;
        }
        for sx in min_x..=max_x {
            let sample_weight = wy * weight(x - sx as f32);
            if sample_weight == 0.0 || !sample_weight.is_finite() {
                continue;
            }
            add_weighted(
                &mut sum,
                sample_integer(pixels, width, height, sx, sy, edge),
                sample_weight,
            );
            weight_sum += sample_weight;
        }
    }
    normalized_pixel(sum, weight_sum)
}

#[allow(clippy::too_many_arguments)]
fn sample_radial_quadratic(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    edge: SampleEdge,
    radius: f32,
) -> PixelF32 {
    if width == 0 || height == 0 || !coordinates_are_safe(x, y) {
        return TRANSPARENT;
    }
    let radius_squared = radius * radius;
    let min_x = (x - radius).floor() as i64;
    let max_x = (x + radius).ceil() as i64;
    let min_y = (y - radius).floor() as i64;
    let max_y = (y + radius).ceil() as i64;
    let mut sum = TRANSPARENT;
    let mut weight_sum = 0.0;
    for sy in min_y..=max_y {
        let dy = y - sy as f32;
        for sx in min_x..=max_x {
            let dx = x - sx as f32;
            let normalized_radius = (dx * dx + dy * dy) / radius_squared;
            if normalized_radius >= 1.0 {
                continue;
            }
            let sample_weight = (1.0 - normalized_radius).powi(2);
            add_weighted(
                &mut sum,
                sample_integer(pixels, width, height, sx, sy, edge),
                sample_weight,
            );
            weight_sum += sample_weight;
        }
    }
    normalized_pixel(sum, weight_sum)
}

fn add_weighted(sum: &mut PixelF32, pixel: PixelF32, weight: f32) {
    sum.alpha += pixel.alpha * weight;
    sum.red += pixel.red * weight;
    sum.green += pixel.green * weight;
    sum.blue += pixel.blue * weight;
}

fn normalized_pixel(pixel: PixelF32, weight_sum: f32) -> PixelF32 {
    if !weight_sum.is_finite() || weight_sum.abs() <= 1.0e-8 {
        return TRANSPARENT;
    }
    let inverse = weight_sum.recip();
    PixelF32 {
        alpha: pixel.alpha * inverse,
        red: pixel.red * inverse,
        green: pixel.green * inverse,
        blue: pixel.blue * inverse,
    }
}

fn cubic_keys_weight(distance: f32, a: f32) -> f32 {
    let x = distance.abs();
    if x <= 1.0 {
        (a + 2.0) * x.powi(3) - (a + 3.0) * x.powi(2) + 1.0
    } else if x < 2.0 {
        a * x.powi(3) - 5.0 * a * x.powi(2) + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn mitchell_weight(distance: f32, b: f32, c: f32) -> f32 {
    let x = distance.abs();
    let b = finite_or(b, 1.0 / 3.0).clamp(0.0, 1.0);
    let c = finite_or(c, 1.0 / 3.0).clamp(0.0, 1.0);
    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x.powi(3) + (-18.0 + 12.0 * b + 6.0 * c) * x.powi(2) + 6.0
            - 2.0 * b)
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x.powi(3)
            + (6.0 * b + 30.0 * c) * x.powi(2)
            + (-12.0 * b - 48.0 * c) * x
            + 8.0 * b
            + 24.0 * c)
            / 6.0
    } else {
        0.0
    }
}

fn cubic_bspline_weight(distance: f32) -> f32 {
    let x = distance.abs();
    if x < 1.0 {
        (4.0 - 6.0 * x.powi(2) + 3.0 * x.powi(3)) / 6.0
    } else if x < 2.0 {
        (2.0 - x).powi(3) / 6.0
    } else {
        0.0
    }
}

fn sinc(value: f32) -> f32 {
    if value.abs() < 1.0e-6 {
        1.0
    } else {
        let angle = std::f32::consts::PI * value;
        angle.sin() / angle
    }
}

fn lanczos_weight(distance: f32, lobes: f32) -> f32 {
    let x = distance.abs();
    if x >= lobes {
        0.0
    } else {
        sinc(x) * sinc(x / lobes)
    }
}

#[inline]
fn coordinates_are_safe(x: f32, y: f32) -> bool {
    x.is_finite()
        && y.is_finite()
        && x.abs() <= MAX_SAFE_SAMPLE_COORDINATE
        && y.abs() <= MAX_SAFE_SAMPLE_COORDINATE
}

#[inline]
pub fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: a.alpha + (b.alpha - a.alpha) * t,
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
    }
}

fn sample_integer(
    pixels: &[PixelF32],
    width: usize,
    height: usize,
    x: i64,
    y: i64,
    edge: SampleEdge,
) -> PixelF32 {
    let Some(x) = resolve_coordinate(x, width, edge) else {
        return TRANSPARENT;
    };
    let Some(y) = resolve_coordinate(y, height, edge) else {
        return TRANSPARENT;
    };
    pixels.get(y * width + x).copied().unwrap_or(TRANSPARENT)
}

fn resolve_coordinate(value: i64, length: usize, edge: SampleEdge) -> Option<usize> {
    if length == 0 {
        return None;
    }
    let length = length as i64;
    match edge {
        SampleEdge::Transparent => (0..length).contains(&value).then_some(value as usize),
        SampleEdge::Clamp => Some(value.clamp(0, length - 1) as usize),
        SampleEdge::Tile => Some(value.rem_euclid(length) as usize),
        SampleEdge::Mirror => {
            if length == 1 {
                return Some(0);
            }
            let period = length * 2 - 2;
            let folded = value.rem_euclid(period);
            Some(if folded < length {
                folded as usize
            } else {
                (period - folded) as usize
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn pixel(value: f32) -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red: value,
            green: value,
            blue: value,
        }
    }

    fn assert_pixel_bits_eq(actual: PixelF32, expected: PixelF32) {
        assert_eq!(actual.alpha.to_bits(), expected.alpha.to_bits(), "alpha");
        assert_eq!(actual.red.to_bits(), expected.red.to_bits(), "red");
        assert_eq!(actual.green.to_bits(), expected.green.to_bits(), "green");
        assert_eq!(actual.blue.to_bits(), expected.blue.to_bits(), "blue");
    }

    fn sample_filtered_uncached(
        pixels: &[PixelF32],
        width: usize,
        height: usize,
        x: f32,
        y: f32,
        edge: SampleEdge,
        filter: SampleFilter,
    ) -> PixelF32 {
        match filter {
            SampleFilter::Bicubic => {
                sample_separable_uncached(pixels, width, height, x, y, edge, 2.0, &|d| {
                    cubic_keys_weight(d, -0.5)
                })
            }
            SampleFilter::Mitchell { b, c } => {
                sample_separable_uncached(pixels, width, height, x, y, edge, 2.0, &|d| {
                    mitchell_weight(d, b, c)
                })
            }
            SampleFilter::Lanczos { lobes } => {
                let lobes = finite_or(lobes, 3.0).clamp(1.0, MAX_SEPARABLE_RADIUS as f32);
                sample_separable_uncached(pixels, width, height, x, y, edge, lobes, &|d| {
                    lanczos_weight(d, lobes)
                })
            }
            SampleFilter::CubicBSpline => sample_separable_uncached(
                pixels,
                width,
                height,
                x,
                y,
                edge,
                2.0,
                &cubic_bspline_weight,
            ),
            _ => panic!("test helper only supports separable filters"),
        }
    }

    #[test]
    fn edge_modes_resolve_outside_coordinates() {
        let pixels = vec![pixel(0.0), pixel(1.0), pixel(2.0)];
        assert_eq!(
            sample_nearest(&pixels, 3, 1, -1.0, 0.0, SampleEdge::Transparent).alpha,
            0.0
        );
        assert_eq!(
            sample_nearest(&pixels, 3, 1, -1.0, 0.0, SampleEdge::Clamp).red,
            0.0
        );
        assert_eq!(
            sample_nearest(&pixels, 3, 1, -1.0, 0.0, SampleEdge::Tile).red,
            2.0
        );
        assert_eq!(
            sample_nearest(&pixels, 3, 1, -1.0, 0.0, SampleEdge::Mirror).red,
            1.0
        );
    }

    #[test]
    fn bilinear_interpolation_blends_four_neighbors() {
        let pixels = vec![pixel(0.0), pixel(1.0), pixel(2.0), pixel(3.0)];
        let result = sample_bilinear(&pixels, 2, 2, 0.5, 0.5, SampleEdge::Clamp);
        assert!((result.red - 1.5).abs() < 1.0e-6);
    }

    #[test]
    fn extreme_coordinates_are_transparent_without_overflowing() {
        let pixels = vec![pixel(1.0)];
        for edge in [
            SampleEdge::Transparent,
            SampleEdge::Clamp,
            SampleEdge::Tile,
            SampleEdge::Mirror,
        ] {
            assert_eq!(
                sample_bilinear(&pixels, 1, 1, f32::MAX, f32::MIN, edge).alpha,
                0.0
            );
            assert_eq!(
                sample_nearest(&pixels, 1, 1, f32::INFINITY, 0.0, edge).alpha,
                0.0
            );
        }
    }

    #[test]
    fn reconstruction_filters_preserve_a_constant_field() {
        let pixels = vec![pixel(0.625); 25];
        for filter in [
            SampleFilter::Nearest,
            SampleFilter::Bilinear,
            SampleFilter::Bicubic,
            SampleFilter::Mitchell {
                b: 1.0 / 3.0,
                c: 1.0 / 3.0,
            },
            SampleFilter::Lanczos { lobes: 3.0 },
            SampleFilter::CubicBSpline,
            SampleFilter::EwaQuadratic { radius: 2.0 },
        ] {
            let result = sample_filtered(&pixels, 5, 5, 2.25, 1.75, SampleEdge::Clamp, filter);
            assert!((result.red - 0.625).abs() < 1.0e-5, "{filter:?}");
            assert!((result.alpha - 1.0).abs() < 1.0e-5, "{filter:?}");
        }
    }

    #[test]
    fn cached_separable_taps_are_bitwise_identical_to_previous_loop() {
        let pixels = (0..35)
            .map(|index| PixelF32 {
                alpha: 0.25 + index as f32 * 0.017,
                red: ((index * 17 + 3) % 29) as f32 / 19.0,
                green: ((index * 11 + 5) % 31) as f32 / 23.0,
                blue: ((index * 7 + 13) % 37) as f32 / 17.0,
            })
            .collect::<Vec<_>>();
        let filters = [
            SampleFilter::Bicubic,
            SampleFilter::Mitchell { b: 0.21, c: 0.47 },
            SampleFilter::Lanczos { lobes: 1.0 },
            SampleFilter::Lanczos { lobes: 3.0 },
            SampleFilter::Lanczos { lobes: 8.0 },
            SampleFilter::CubicBSpline,
        ];
        let coordinates = [
            (-2.25, -1.75),
            (-0.5, 0.0),
            (0.125, 0.875),
            (2.5, 3.25),
            (5.75, 4.5),
            (8.25, 7.125),
        ];

        for edge in [
            SampleEdge::Transparent,
            SampleEdge::Clamp,
            SampleEdge::Tile,
            SampleEdge::Mirror,
        ] {
            for filter in filters {
                for (x, y) in coordinates {
                    let actual = sample_filtered(&pixels, 7, 5, x, y, edge, filter);
                    let expected = sample_filtered_uncached(&pixels, 7, 5, x, y, edge, filter);
                    assert_pixel_bits_eq(actual, expected);
                }
            }
        }
    }

    #[test]
    fn separable_x_kernel_is_evaluated_once_per_tap() {
        let pixels = vec![pixel(0.5); 25];
        let evaluations = Cell::new(0);
        let _ = sample_separable(
            &pixels,
            5,
            5,
            2.25,
            1.75,
            SampleEdge::Clamp,
            2.0,
            |distance| {
                evaluations.set(evaluations.get() + 1);
                cubic_keys_weight(distance, -0.5)
            },
        );

        let x_taps = ((2.25_f32 + 2.0).ceil() - (2.25_f32 - 2.0).floor()) as usize + 1;
        let y_taps = ((1.75_f32 + 2.0).ceil() - (1.75_f32 - 2.0).floor()) as usize + 1;
        assert_eq!(evaluations.get(), x_taps + y_taps);
    }
}
