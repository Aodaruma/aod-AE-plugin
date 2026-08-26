use std::f32::consts::TAU;

const OKLCH_CHROMA_AT_100_PERCENT: f32 = 0.2;
const CIELAB_CHROMA_AT_100_PERCENT: f32 = 80.0;
const CIELUV_CHROMA_AT_100_PERCENT: f32 = 100.0;
const JZ_CHROMA_AT_100_PERCENT: f32 = 0.08;
const JZ_AT_100_PERCENT: f32 = 0.167_174;
const IPT_CHROMA_AT_100_PERCENT: f32 = 0.3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape {
    Linear,
    Radial,
    Diamond,
    Conic,
    Box,
    Minkowski,
    ReflectedLinear,
    Spiral,
    Starburst,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorModel {
    Oklch,
    Hsv,
    Hsl,
    CielchAb,
    CielchUv,
    Jzczhz,
    IptIch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExtendMode {
    Clamp,
    Repeat,
    Mirror,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Smoothstep,
    Smootherstep,
    Custom,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub shape: Shape,
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub aspect: f32,
    pub exponent: f32,
    pub spiral_turns: f32,
    pub ray_count: f32,
}

/// Parametric rainbow endpoints in cylindrical component order:
/// `[unwrapped hue turns, saturation/chroma amount, value/lightness]`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rainbow {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub color_model: ColorModel,
    pub extend: ExtendMode,
    pub easing: Easing,
    pub bezier: [f32; 4],
    pub clamp_gamut: bool,
}

pub(crate) fn geometry_value(point: [f32; 2], geometry: Geometry) -> f32 {
    let dx = geometry.end[0] - geometry.start[0];
    let dy = geometry.end[1] - geometry.start[1];
    let length_sq = dx.mul_add(dx, dy * dy);
    if length_sq <= 1.0e-12 {
        return 0.0;
    }

    if geometry.shape == Shape::Linear {
        return ((point[0] - geometry.start[0]) * dx + (point[1] - geometry.start[1]) * dy)
            / length_sq;
    }

    let length = length_sq.sqrt();
    let ux = dx / length;
    let uy = dy / length;
    let px = point[0] - geometry.start[0];
    let py = point[1] - geometry.start[1];
    let local_x = px * ux + py * uy;
    let aspect = if geometry.aspect.is_finite() {
        geometry.aspect.clamp(1.0e-4, 1.0e4)
    } else {
        1.0
    };
    let local_y = (-px * uy + py * ux) * aspect;
    let normalized_x = local_x / length;
    let normalized_y = local_y / length;

    match geometry.shape {
        Shape::Linear => unreachable!(),
        Shape::Radial => normalized_x.hypot(normalized_y),
        Shape::Diamond => normalized_x.abs() + normalized_y.abs(),
        Shape::Box => normalized_x.abs().max(normalized_y.abs()),
        Shape::Minkowski => lp_norm(normalized_x, normalized_y, geometry.exponent),
        Shape::ReflectedLinear => normalized_x.abs(),
        Shape::Conic => local_y.atan2(local_x).rem_euclid(TAU) / TAU,
        Shape::Spiral => {
            let radius = normalized_x.hypot(normalized_y);
            let angle = local_y.atan2(local_x).rem_euclid(TAU) / TAU;
            let turns = if geometry.spiral_turns.is_finite() {
                geometry.spiral_turns.clamp(-128.0, 128.0)
            } else {
                0.0
            };
            (radius + angle * turns).rem_euclid(1.0)
        }
        Shape::Starburst => {
            let angle = local_y.atan2(local_x);
            let rays = if geometry.ray_count.is_finite() {
                geometry.ray_count.clamp(1.0, 512.0).round()
            } else {
                1.0
            };
            0.5 - 0.5 * (angle * rays).cos()
        }
    }
}

fn lp_norm(x: f32, y: f32, exponent: f32) -> f32 {
    let x = x.abs();
    let y = y.abs();
    let maximum = x.max(y);
    if maximum <= 1.0e-12 {
        return 0.0;
    }
    let exponent = if exponent.is_finite() {
        exponent.clamp(0.25, 64.0)
    } else {
        2.0
    };
    maximum * ((x / maximum).powf(exponent) + (y / maximum).powf(exponent)).powf(1.0 / exponent)
}

pub(crate) fn aspect_ratio_from_balance(balance: f32) -> f32 {
    let balance = if balance.is_finite() {
        balance.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    10.0_f32.powf(balance)
}

pub(crate) fn sample_rainbow(raw_t: f32, rainbow: Rainbow) -> [f32; 3] {
    let extended = extend_value(raw_t, rainbow.extend);
    let t = ease_value(extended, rainbow.easing, rainbow.bezier);
    let components = lerp3(rainbow.start, rainbow.end, t);
    components_to_rgb(components, rainbow.color_model, rainbow.clamp_gamut)
}

fn components_to_rgb(components: [f32; 3], color_model: ColorModel, clamp_gamut: bool) -> [f32; 3] {
    let hue = finite_or(components[0], 0.0).rem_euclid(1.0);
    let second = finite_or(components[1], 0.0).max(0.0);
    let third = finite_or(components[2], 0.0).max(0.0);
    let mut rgb = match color_model {
        ColorModel::Oklch => {
            let lch = [third, second * OKLCH_CHROMA_AT_100_PERCENT, hue];
            linear_to_srgb3(oklab_to_linear_srgb(oklch_to_oklab(lch)))
        }
        ColorModel::Hsv => hsv_to_srgb([hue, second, third]),
        ColorModel::Hsl => hsl_to_srgb([hue, second, third.clamp(0.0, 1.0)]),
        ColorModel::CielchAb => xyz_d65_to_srgb(cielch_ab_to_xyz_d65([
            third * 100.0,
            second * CIELAB_CHROMA_AT_100_PERCENT,
            hue,
        ])),
        ColorModel::CielchUv => xyz_d65_to_srgb(cielch_uv_to_xyz_d65([
            third * 100.0,
            second * CIELUV_CHROMA_AT_100_PERCENT,
            hue,
        ])),
        ColorModel::Jzczhz => xyz_d65_to_srgb(jzczhz_to_xyz_d65([
            third * JZ_AT_100_PERCENT,
            second * JZ_CHROMA_AT_100_PERCENT,
            hue,
        ])),
        ColorModel::IptIch => xyz_d65_to_srgb(ipt_ich_to_xyz_d65([
            third,
            second * IPT_CHROMA_AT_100_PERCENT,
            hue,
        ])),
    };
    for channel in &mut rgb {
        if !channel.is_finite() {
            *channel = 0.0;
        } else if clamp_gamut {
            *channel = channel.clamp(0.0, 1.0);
        }
    }
    rgb
}

pub(crate) fn extend_value(value: f32, mode: ExtendMode) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    match mode {
        ExtendMode::Clamp => value.clamp(0.0, 1.0),
        ExtendMode::Repeat => value.rem_euclid(1.0),
        ExtendMode::Mirror => {
            let phase = value.rem_euclid(2.0);
            if phase <= 1.0 { phase } else { 2.0 - phase }
        }
    }
}

pub(crate) fn ease_value(value: f32, easing: Easing, custom: [f32; 4]) -> f32 {
    let x = value.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => x,
        Easing::Smoothstep => x * x * (3.0 - 2.0 * x),
        Easing::Smootherstep => x * x * x * (x * (x * 6.0 - 15.0) + 10.0),
        Easing::EaseIn => cubic_bezier_y_for_x(x, [0.42, 0.0, 1.0, 1.0]),
        Easing::EaseOut => cubic_bezier_y_for_x(x, [0.0, 0.0, 0.58, 1.0]),
        Easing::EaseInOut => cubic_bezier_y_for_x(x, [0.42, 0.0, 0.58, 1.0]),
        Easing::Custom => cubic_bezier_y_for_x(x, custom),
    }
}

fn cubic_bezier_y_for_x(x: f32, control: [f32; 4]) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let [x1, y1, x2, y2] = control;
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);
    let mut t = x;
    for _ in 0..8 {
        let estimate = cubic_bezier(t, x1, x2) - x;
        let derivative = cubic_bezier_derivative(t, x1, x2);
        if derivative.abs() <= 1.0e-6 {
            break;
        }
        let candidate = t - estimate / derivative;
        if !(0.0..=1.0).contains(&candidate) {
            break;
        }
        t = candidate;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..12 {
        if cubic_bezier(t, x1, x2) < x {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) * 0.5;
    }
    cubic_bezier(t, y1, y2)
}

fn cubic_bezier(t: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - t;
    3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
}

fn cubic_bezier_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - t;
    3.0 * mt * mt * p1 + 6.0 * mt * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

fn oklch_to_oklab(lch: [f32; 3]) -> [f32; 3] {
    let angle = lch[2] * TAU;
    [lch[0], lch[1] * angle.cos(), lch[1] * angle.sin()]
}

fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    let l = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m = lab[0] - 0.105_561_346 * lab[1] - 0.063_854_17 * lab[2];
    let s = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];
    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn cielch_ab_to_xyz_d65(lch: [f32; 3]) -> [f32; 3] {
    let [a, b] = polar_components(lch[1], lch[2]);
    let fy = (lch[0] + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let xyz_d50 = [
        0.964_22 * cie_lab_inverse(fx),
        cie_lab_inverse(fy),
        0.825_21 * cie_lab_inverse(fz),
    ];

    // Bradford-adapted D50 -> D65 matrix from CSS Color 4.
    mul_matrix3(
        [
            [0.955_576_6, -0.023_039_3, 0.063_163_6],
            [-0.028_289_5, 1.009_941_6, 0.021_007_7],
            [0.012_298_2, -0.020_483, 1.329_909_8],
        ],
        xyz_d50,
    )
}

fn cie_lab_inverse(value: f32) -> f32 {
    const EPSILON: f32 = 216.0 / 24_389.0;
    const KAPPA: f32 = 24_389.0 / 27.0;
    let cube = value * value * value;
    if cube > EPSILON {
        cube
    } else {
        (116.0 * value - 16.0) / KAPPA
    }
}

fn cielch_uv_to_xyz_d65(lch: [f32; 3]) -> [f32; 3] {
    let lightness = lch[0].max(0.0);
    if lightness <= 1.0e-7 {
        return [0.0; 3];
    }

    const WHITE: [f32; 3] = [0.950_47, 1.0, 1.088_83];
    const KAPPA: f32 = 24_389.0 / 27.0;
    let white_denominator = WHITE[0] + 15.0 * WHITE[1] + 3.0 * WHITE[2];
    let white_u_prime = 4.0 * WHITE[0] / white_denominator;
    let white_v_prime = 9.0 * WHITE[1] / white_denominator;
    let [u, v] = polar_components(lch[1], lch[2]);
    let u_prime = u / (13.0 * lightness) + white_u_prime;
    let v_prime = v / (13.0 * lightness) + white_v_prime;
    let y = if lightness > 8.0 {
        ((lightness + 16.0) / 116.0).powi(3)
    } else {
        lightness / KAPPA
    };
    let denominator = nonzero_with_sign(4.0 * v_prime);
    [
        y * 9.0 * u_prime / denominator,
        y,
        y * (12.0 - 3.0 * u_prime - 20.0 * v_prime) / denominator,
    ]
}

fn jzczhz_to_xyz_d65(jch: [f32; 3]) -> [f32; 3] {
    const B: f32 = 1.15;
    const G: f32 = 0.66;
    const D: f32 = -0.56;
    const D0: f32 = 1.629_55e-11;
    let [az, bz] = polar_components(jch[1], jch[2]);
    let jz_plus_d0 = jch[0].max(0.0) + D0;
    let iz = jz_plus_d0 / nonzero_with_sign(1.0 + D - D * jz_plus_d0);
    let lms_prime = mul_matrix3(
        [
            [1.0, 0.138_605_04, 0.058_047_317],
            [1.0, -0.138_605_04, -0.058_047_317],
            [1.0, -0.096_019_24, -0.811_891_9],
        ],
        [iz, az, bz],
    );
    let lms = lms_prime.map(jz_pq_inverse);
    let [x_prime, y_prime, z_prime] = mul_matrix3(
        [
            [1.924_226_4, -1.004_792_3, 0.037_651_405],
            [0.350_316_76, 0.726_481_2, -0.065_384_425],
            [-0.090_982_81, -0.312_728_3, 1.522_766_6],
        ],
        lms,
    );
    let x = (x_prime + (B - 1.0) * z_prime) / B;
    let y = (y_prime + (G - 1.0) * x) / G;

    // JzAzBz is defined for absolute XYZ. The UI normalization anchors
    // 100% Jz to a 100 cd/m2 D65 white, so return relative XYZ here.
    [x / 100.0, y / 100.0, z_prime / 100.0]
}

fn jz_pq_inverse(value: f32) -> f32 {
    const C1: f32 = 3424.0 / 4096.0;
    const C2: f32 = 2413.0 / 128.0;
    const C3: f32 = 2392.0 / 128.0;
    const N: f32 = 2610.0 / 16_384.0;
    const P: f32 = 1.7 * 2523.0 / 32.0;
    let powered = value.max(0.0).powf(1.0 / P);
    let numerator = (powered - C1).max(0.0);
    let denominator = (C2 - C3 * powered).max(1.0e-7);
    10_000.0 * (numerator / denominator).powf(1.0 / N)
}

fn ipt_ich_to_xyz_d65(ich: [f32; 3]) -> [f32; 3] {
    let [p, t] = polar_components(ich[1], ich[2]);
    let lms_prime = mul_matrix3(
        [
            [1.0, 0.097_568_93, 0.205_226_44],
            [1.0, -0.113_876_484, 0.133_217_16],
            [1.0, 0.032_615_11, -0.676_887_16],
        ],
        [ich[0].max(0.0), p, t],
    );
    let lms = lms_prime.map(|value| signed_pow(value, 1.0 / 0.43));
    mul_matrix3(
        [
            [1.850_243, -1.138_301_6, 0.238_434_96],
            [0.366_830_77, 0.643_884_54, -0.010_673_444],
            [0.0, 0.0, 1.088_850_1],
        ],
        lms,
    )
}

fn polar_components(chroma: f32, hue: f32) -> [f32; 2] {
    let angle = hue * TAU;
    [chroma * angle.cos(), chroma * angle.sin()]
}

fn xyz_d65_to_srgb(xyz: [f32; 3]) -> [f32; 3] {
    linear_to_srgb3(mul_matrix3(
        [
            [3.240_454_2, -1.537_138_5, -0.498_531_4],
            [-0.969_266, 1.876_010_8, 0.041_556],
            [0.055_643_4, -0.204_025_9, 1.057_225_2],
        ],
        xyz,
    ))
}

fn mul_matrix3(matrix: [[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0].mul_add(vector[0], row[1].mul_add(vector[1], row[2] * vector[2])))
}

fn signed_pow(value: f32, exponent: f32) -> f32 {
    value.signum() * value.abs().powf(exponent)
}

fn nonzero_with_sign(value: f32) -> f32 {
    if value.abs() >= 1.0e-7 {
        value
    } else if value.is_sign_negative() {
        -1.0e-7
    } else {
        1.0e-7
    }
}

fn linear_to_srgb3(rgb: [f32; 3]) -> [f32; 3] {
    rgb.map(linear_to_srgb)
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn hsv_to_srgb(hsv: [f32; 3]) -> [f32; 3] {
    let hue = hsv[0].rem_euclid(1.0) * 6.0;
    let chroma = hsv[2] * hsv[1];
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let rgb = match hue.floor() as i32 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    let m = hsv[2] - chroma;
    [rgb[0] + m, rgb[1] + m, rgb[2] + m]
}

fn hsl_to_srgb(hsl: [f32; 3]) -> [f32; 3] {
    let hue = hsl[0].rem_euclid(1.0) * 6.0;
    let chroma = (1.0 - (2.0 * hsl[2] - 1.0).abs()) * hsl[1];
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let rgb = match hue.floor() as i32 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    let m = hsl[2] - chroma * 0.5;
    [rgb[0] + m, rgb[1] + m, rgb[2] + m]
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 2.0e-4, "{a} != {b}");
    }

    fn close_within(a: f32, b: f32, tolerance: f32) {
        assert!((a - b).abs() < tolerance, "{a} != {b} (tol {tolerance})");
    }

    fn all_color_models() -> [ColorModel; 7] {
        [
            ColorModel::Oklch,
            ColorModel::Hsv,
            ColorModel::Hsl,
            ColorModel::CielchAb,
            ColorModel::CielchUv,
            ColorModel::Jzczhz,
            ColorModel::IptIch,
        ]
    }

    fn geometry(shape: Shape) -> Geometry {
        Geometry {
            shape,
            start: [0.0, 0.0],
            end: [10.0, 0.0],
            aspect: 1.0,
            exponent: 2.0,
            spiral_turns: 3.0,
            ray_count: 4.0,
        }
    }

    fn rainbow(color_model: ColorModel) -> Rainbow {
        Rainbow {
            start: [0.0, 1.0, 0.75],
            end: [1.0, 1.0, 0.75],
            color_model,
            extend: ExtendMode::Clamp,
            easing: Easing::Linear,
            bezier: [0.25, 0.1, 0.25, 1.0],
            clamp_gamut: true,
        }
    }

    #[test]
    fn linear_geometry_hits_both_anchors() {
        let geometry = geometry(Shape::Linear);
        close(geometry_value(geometry.start, geometry), 0.0);
        close(geometry_value(geometry.end, geometry), 1.0);
    }

    #[test]
    fn radial_diamond_and_box_hit_radius() {
        for shape in [Shape::Radial, Shape::Diamond, Shape::Box] {
            close(geometry_value([10.0, 0.0], geometry(shape)), 1.0);
        }
    }

    #[test]
    fn minkowski_connects_diamond_radial_and_box_metrics() {
        close(lp_norm(0.5, 0.5, 1.0), 1.0);
        close(lp_norm(0.5, 0.5, 2.0), 0.5_f32.sqrt());
        close(lp_norm(0.5, 0.5, 64.0), 0.5 * 2.0_f32.powf(1.0 / 64.0));
    }

    #[test]
    fn reflected_and_starburst_are_parametric() {
        close(
            geometry_value([-5.0, 9.0], geometry(Shape::ReflectedLinear)),
            0.5,
        );
        close(
            geometry_value([10.0, 10.0], geometry(Shape::Starburst)),
            1.0,
        );
    }

    #[test]
    fn conic_uses_clockwise_full_turn() {
        let geometry = geometry(Shape::Conic);
        close(geometry_value([0.0, 1.0], geometry), 0.25);
        close(geometry_value([-1.0, 0.0], geometry), 0.5);
        close(geometry_value([0.0, -1.0], geometry), 0.75);
    }

    #[test]
    fn symmetric_aspect_is_reciprocal_around_zero() {
        close(aspect_ratio_from_balance(0.0), 1.0);
        close(aspect_ratio_from_balance(-1.0), 0.1);
        close(aspect_ratio_from_balance(1.0), 10.0);
        close(
            aspect_ratio_from_balance(-0.4) * aspect_ratio_from_balance(0.4),
            1.0,
        );
        close(aspect_ratio_from_balance(f32::NAN), 1.0);
    }

    #[test]
    fn extend_modes_handle_negative_values() {
        close(extend_value(-0.25, ExtendMode::Repeat), 0.75);
        close(extend_value(-0.25, ExtendMode::Mirror), 0.25);
        close(extend_value(-0.25, ExtendMode::Clamp), 0.0);
    }

    #[test]
    fn all_easing_curves_preserve_endpoints() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Smoothstep,
            Easing::Smootherstep,
            Easing::Custom,
        ] {
            close(ease_value(0.0, easing, [0.2, 0.8, 0.7, 0.1]), 0.0);
            close(ease_value(1.0, easing, [0.2, 0.8, 0.7, 0.1]), 1.0);
        }
    }

    #[test]
    fn hsv_default_range_generates_a_full_rainbow() {
        let rainbow = rainbow(ColorModel::Hsv);
        let start = sample_rainbow(0.0, rainbow);
        let middle = sample_rainbow(0.5, rainbow);
        let end = sample_rainbow(1.0, rainbow);
        close(start[0], 0.75);
        close(start[1], 0.0);
        close(start[2], 0.0);
        close(middle[0], 0.0);
        close(middle[1], 0.75);
        close(middle[2], 0.75);
        for index in 0..3 {
            close(start[index], end[index]);
        }
    }

    #[test]
    fn all_color_models_generate_finite_colors() {
        for model in all_color_models() {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let rgb = sample_rainbow(t, rainbow(model));
                assert!(rgb.iter().all(|channel| channel.is_finite()));
                assert!(rgb.iter().all(|channel| (0.0..=1.0).contains(channel)));
            }
        }
    }

    #[test]
    fn default_perceptual_rainbows_do_not_collapse_after_gamut_clamping() {
        for model in [
            ColorModel::Oklch,
            ColorModel::CielchAb,
            ColorModel::CielchUv,
            ColorModel::Jzczhz,
            ColorModel::IptIch,
        ] {
            let samples: Vec<_> = (0..8)
                .map(|index| sample_rainbow(index as f32 / 8.0, rainbow(model)))
                .collect();
            let mut distinct = 0;
            for (index, sample) in samples.iter().enumerate() {
                let repeats_previous = samples[..index].iter().any(|previous| {
                    sample
                        .iter()
                        .zip(previous)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f32>()
                        < 0.01
                });
                if !repeats_previous {
                    distinct += 1;
                }
            }
            assert!(
                distinct >= 6,
                "{model:?} produced only {distinct} distinct hues"
            );
        }
    }

    #[test]
    fn every_color_model_is_periodic_over_one_hue_turn() {
        for model in all_color_models() {
            let start = components_to_rgb([0.137, 0.8, 0.65], model, true);
            let end = components_to_rgb([1.137, 0.8, 0.65], model, true);
            for index in 0..3 {
                close(start[index], end[index]);
            }
        }
    }

    #[test]
    fn d65_xyz_matrix_reconstructs_the_srgb_red_primary() {
        let red = xyz_d65_to_srgb([0.412_456_4, 0.212_672_9, 0.019_333_9]);
        close_within(red[0], 1.0, 3.0e-5);
        close_within(red[1], 0.0, 3.0e-5);
        close_within(red[2], 0.0, 3.0e-5);
    }

    #[test]
    fn cielch_ab_matches_a_known_lab_conversion() {
        // CIELAB D50 [50, 40, 30], expressed cylindrically.
        let rgb = xyz_d65_to_srgb(cielch_ab_to_xyz_d65([50.0, 50.0, 0.102_416_38]));
        for (actual, expected) in rgb.into_iter().zip([0.734_241, 0.344_121, 0.276_336]) {
            close_within(actual, expected, 5.0e-4);
        }
    }

    #[test]
    fn cielch_uv_reconstructs_the_srgb_red_primary() {
        let rgb = xyz_d65_to_srgb(cielch_uv_to_xyz_d65([
            53.240_795,
            179.041_37,
            0.033_816_636,
        ]));
        for (actual, expected) in rgb.into_iter().zip([1.0, 0.0, 0.0]) {
            close_within(actual, expected, 5.0e-4);
        }
    }

    #[test]
    fn jzczhz_reconstructs_a_100_nit_srgb_red_primary() {
        let rgb = xyz_d65_to_srgb(jzczhz_to_xyz_d65([
            0.098_974_02,
            0.135_110_24,
            42.477_23 / 360.0,
        ]));
        for (actual, expected) in rgb.into_iter().zip([1.0, 0.0, 0.0]) {
            close_within(actual, expected, 4.0e-3);
        }
    }

    #[test]
    fn ipt_ich_reconstructs_the_srgb_red_primary() {
        let rgb = xyz_d65_to_srgb(ipt_ich_to_xyz_d65([
            0.456_192_67,
            0.762_700_7,
            35.494_137 / 360.0,
        ]));
        for (actual, expected) in rgb.into_iter().zip([1.0, 0.0, 0.0]) {
            close_within(actual, expected, 8.0e-4);
        }
    }

    #[test]
    fn neutral_100_percent_axes_are_d65_white() {
        for model in [
            ColorModel::CielchAb,
            ColorModel::CielchUv,
            ColorModel::Jzczhz,
            ColorModel::IptIch,
        ] {
            let rgb = components_to_rgb([0.42, 0.0, 1.0], model, true);
            for channel in rgb {
                close_within(channel, 1.0, 2.0e-3);
            }
        }
    }

    #[test]
    fn unwrapped_hue_supports_multiple_cycles() {
        let mut rainbow = rainbow(ColorModel::Hsv);
        rainbow.end[0] = 3.0;
        let at_one_third = sample_rainbow(1.0 / 3.0, rainbow);
        let start = sample_rainbow(0.0, rainbow);
        for index in 0..3 {
            close(start[index], at_one_third[index]);
        }
    }
}
