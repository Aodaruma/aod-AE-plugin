use std::f32::consts::TAU;

use palette::{
    Okhsl, Okhsv, Srgb, Xyz,
    cam16::{BakedParameters, Cam16Jmh, Cam16UcsJab, Cam16UcsJmh, Parameters, StaticWp},
    convert::{FromColorUnclamped, IntoColorUnclamped},
    white_point::D65,
};

const OKLCH_CHROMA_AT_100_PERCENT: f32 = 0.2;
const CIELAB_CHROMA_AT_100_PERCENT: f32 = 80.0;
const CIELUV_CHROMA_AT_100_PERCENT: f32 = 100.0;
const JZ_CHROMA_AT_100_PERCENT: f32 = 0.08;
const JZ_AT_100_PERCENT: f32 = 0.167_174;
const IPT_CHROMA_AT_100_PERCENT: f32 = 0.3;
const CAM16_UCS_COLORFULNESS_AT_100_PERCENT: f32 = 50.0;
// CAM16's incomplete chromatic adaptation can assign a small residual M' to
// display neutrals. Treat values below 2 as achromatic for hue interpolation.
const CAM16_NEUTRAL_COLORFULNESS: f32 = 2.0;

pub(crate) type Cam16ViewingConditions = BakedParameters<StaticWp<D65>, f32>;

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
    Okhsl,
    Okhsv,
    Cam16UcsJmh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TwoColorSpace {
    Oklab,
    Oklch,
    Cam16UcsJab,
    Cam16UcsJmh,
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
    /// Horizontal inverse shear, expressed as rise/run (`100% == 1.0`).
    pub skew: f32,
    pub exponent: f32,
    pub spiral_turns: f32,
    pub ray_count: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct PreparedTwoColor {
    start: [f32; 3],
    end: [f32; 3],
    start_rgb: [f32; 3],
    end_rgb: [f32; 3],
    color_space: TwoColorSpace,
}

#[derive(Clone, Copy)]
pub(crate) enum GradientColors {
    /// Cylindrical component order: `[unwrapped hue turns,
    /// saturation/chroma amount, value/lightness]`.
    Parametric {
        start: [f32; 3],
        end: [f32; 3],
        color_model: ColorModel,
    },
    TwoColor(PreparedTwoColor),
}

#[derive(Clone, Copy)]
pub(crate) struct Rainbow {
    pub colors: GradientColors,
    pub extend: ExtendMode,
    pub easing: Easing,
    pub bezier: [f32; 4],
    pub clamp_gamut: bool,
    pub cam16_parameters: Cam16ViewingConditions,
}

/// Fixed viewing conditions for display-referred CAM16-UCS rendering: D65,
/// 40 cd/m² adapting luminance, a 20% background, average surround, and
/// automatic illuminant discounting. Baking once per render avoids rebuilding
/// the CAM16 dependent parameters for each pixel.
pub(crate) fn cam16_viewing_conditions() -> Cam16ViewingConditions {
    Parameters::default_static_wp(40.0f32).bake()
}

pub(crate) fn geometry_value(point: [f32; 2], geometry: Geometry) -> f32 {
    let dx = geometry.end[0] - geometry.start[0];
    let dy = geometry.end[1] - geometry.start[1];
    let length_sq = dx.mul_add(dx, dy * dy);
    if length_sq <= 1.0e-12 {
        return 0.0;
    }

    let length = length_sq.sqrt();
    let ux = dx / length;
    let uy = dy / length;
    let px = point[0] - geometry.start[0];
    let py = point[1] - geometry.start[1];
    let local_x = px * ux + py * uy;
    let local_y = -px * uy + py * ux;
    let skew = if geometry.skew.is_finite() {
        geometry.skew.clamp(-10.0, 10.0)
    } else {
        0.0
    };
    // Coordinates are transformed back into the unskewed gradient domain. This
    // is the inverse of the forward local-space shear x' = x + skew * y.
    let local_x = local_x - skew * local_y;
    let aspect = if geometry.aspect.is_finite() {
        geometry.aspect.clamp(1.0e-4, 1.0e4)
    } else {
        1.0
    };
    let local_y = local_y * aspect;
    let normalized_x = local_x / length;
    let normalized_y = local_y / length;

    match geometry.shape {
        Shape::Linear => normalized_x,
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

pub(crate) fn sample_rainbow(raw_t: f32, rainbow: &Rainbow) -> [f32; 3] {
    let extended = extend_value(raw_t, rainbow.extend);
    let t = ease_value(extended, rainbow.easing, rainbow.bezier);
    match rainbow.colors {
        GradientColors::Parametric {
            start,
            end,
            color_model,
        } => components_to_rgb(
            lerp3(start, end, t),
            color_model,
            rainbow.cam16_parameters,
            rainbow.clamp_gamut,
        ),
        GradientColors::TwoColor(prepared) => interpolate_prepared_two_color(
            prepared,
            t,
            rainbow.cam16_parameters,
            rainbow.clamp_gamut,
        ),
    }
}

pub(crate) fn prepare_two_color(
    start_rgb: [f32; 3],
    end_rgb: [f32; 3],
    color_space: TwoColorSpace,
    cam16_parameters: Cam16ViewingConditions,
) -> GradientColors {
    let (start, end) = match color_space {
        TwoColorSpace::Oklab | TwoColorSpace::Oklch => {
            let start_oklab = linear_srgb_to_oklab(start_rgb.map(srgb_to_linear));
            let end_oklab = linear_srgb_to_oklab(end_rgb.map(srgb_to_linear));
            if color_space == TwoColorSpace::Oklab {
                (start_oklab, end_oklab)
            } else {
                prepare_polar_endpoints(
                    oklab_to_oklch(start_oklab),
                    oklab_to_oklch(end_oklab),
                    1.0e-7,
                    1.0,
                )
            }
        }
        TwoColorSpace::Cam16UcsJab => (
            srgb_to_cam16_ucs_jab(start_rgb, cam16_parameters),
            srgb_to_cam16_ucs_jab(end_rgb, cam16_parameters),
        ),
        TwoColorSpace::Cam16UcsJmh => prepare_polar_endpoints(
            srgb_to_cam16_ucs_jmh(start_rgb, cam16_parameters),
            srgb_to_cam16_ucs_jmh(end_rgb, cam16_parameters),
            CAM16_NEUTRAL_COLORFULNESS,
            360.0,
        ),
    };
    GradientColors::TwoColor(PreparedTwoColor {
        start,
        end,
        start_rgb,
        end_rgb,
        color_space,
    })
}

fn prepare_polar_endpoints(
    mut start: [f32; 3],
    mut end: [f32; 3],
    neutral_threshold: f32,
    hue_period: f32,
) -> ([f32; 3], [f32; 3]) {
    // The component order is [lightness, chroma/colorfulness, hue]. Hue is
    // undefined on the neutral axis, so borrow it from the chromatic endpoint.
    if start[1] <= neutral_threshold {
        start[2] = end[2];
    }
    if end[1] <= neutral_threshold {
        end[2] = start[2];
    }
    end[2] = start[2] + shortest_hue_delta(start[2], end[2], hue_period);
    (start, end)
}

fn interpolate_prepared_two_color(
    prepared: PreparedTwoColor,
    t: f32,
    cam16_parameters: Cam16ViewingConditions,
    clamp_gamut: bool,
) -> [f32; 3] {
    let PreparedTwoColor {
        start,
        end,
        start_rgb,
        end_rgb,
        color_space,
    } = prepared;
    if t <= 0.0 {
        let mut rgb = start_rgb;
        sanitize_rgb(&mut rgb, clamp_gamut);
        return rgb;
    }
    if t >= 1.0 {
        let mut rgb = end_rgb;
        sanitize_rgb(&mut rgb, clamp_gamut);
        return rgb;
    }
    let components = lerp3(start, end, t);
    let mut rgb = match color_space {
        TwoColorSpace::Oklab => linear_to_srgb3(oklab_to_linear_srgb(components)),
        TwoColorSpace::Oklch => linear_to_srgb3(oklab_to_linear_srgb(oklch_to_oklab(components))),
        TwoColorSpace::Cam16UcsJab => cam16_ucs_jab_to_srgb(components, cam16_parameters),
        TwoColorSpace::Cam16UcsJmh => cam16_ucs_jmh_to_srgb(components, cam16_parameters),
    };
    sanitize_rgb(&mut rgb, clamp_gamut);
    rgb
}

fn shortest_hue_delta(start: f32, end: f32, period: f32) -> f32 {
    (end - start + period * 0.5).rem_euclid(period) - period * 0.5
}

fn components_to_rgb(
    components: [f32; 3],
    color_model: ColorModel,
    cam16_parameters: Cam16ViewingConditions,
    clamp_gamut: bool,
) -> [f32; 3] {
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
        ColorModel::Okhsl => palette_srgb_to_array(Srgb::from_color_unclamped(Okhsl::new(
            hue * 360.0,
            second,
            third,
        ))),
        ColorModel::Okhsv => palette_srgb_to_array(Srgb::from_color_unclamped(Okhsv::new(
            hue * 360.0,
            second,
            third,
        ))),
        ColorModel::Cam16UcsJmh => cam16_ucs_jmh_to_srgb(
            [
                third * 100.0,
                second * CAM16_UCS_COLORFULNESS_AT_100_PERCENT,
                hue * 360.0,
            ],
            cam16_parameters,
        ),
    };
    sanitize_rgb(&mut rgb, clamp_gamut);
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

fn oklab_to_oklch(lab: [f32; 3]) -> [f32; 3] {
    [
        lab[0],
        lab[1].hypot(lab[2]),
        lab[2].atan2(lab[1]).rem_euclid(TAU) / TAU,
    ]
}

fn linear_srgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_46 * rgb[0] + 0.536_332_55 * rgb[1] + 0.051_445_995 * rgb[2];
    let m = 0.211_903_5 * rgb[0] + 0.680_699_5 * rgb[1] + 0.107_396_96 * rgb[2];
    let s = 0.088_302_46 * rgb[0] + 0.281_718_85 * rgb[1] + 0.629_978_7 * rgb[2];
    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn srgb_to_cam16_ucs_jmh(rgb: [f32; 3], parameters: Cam16ViewingConditions) -> [f32; 3] {
    let xyz: Xyz<D65, f32> = Srgb::from(rgb).into_color_unclamped();
    let ucs = Cam16UcsJmh::from_color_unclamped(Cam16Jmh::from_xyz(xyz, parameters));
    [
        ucs.lightness,
        ucs.colorfulness,
        ucs.hue.into_positive_degrees(),
    ]
}

fn srgb_to_cam16_ucs_jab(rgb: [f32; 3], parameters: Cam16ViewingConditions) -> [f32; 3] {
    let xyz: Xyz<D65, f32> = Srgb::from(rgb).into_color_unclamped();
    let ucs = Cam16UcsJab::from_color_unclamped(Cam16Jmh::from_xyz(xyz, parameters));
    [ucs.lightness, ucs.a, ucs.b]
}

fn cam16_ucs_jmh_to_srgb(components: [f32; 3], parameters: Cam16ViewingConditions) -> [f32; 3] {
    let ucs = Cam16UcsJmh::new(components[0], components[1], components[2]);
    let cam16 = Cam16Jmh::from_color_unclamped(ucs);
    palette_srgb_to_array(Srgb::from_color_unclamped(cam16.into_xyz(parameters)))
}

fn cam16_ucs_jab_to_srgb(components: [f32; 3], parameters: Cam16ViewingConditions) -> [f32; 3] {
    let ucs = Cam16UcsJab::new(components[0], components[1], components[2]);
    let cam16 = Cam16Jmh::from_color_unclamped(ucs);
    palette_srgb_to_array(Srgb::from_color_unclamped(cam16.into_xyz(parameters)))
}

fn palette_srgb_to_array(rgb: Srgb<f32>) -> [f32; 3] {
    let (red, green, blue) = rgb.into_components();
    [red, green, blue]
}

fn sanitize_rgb(rgb: &mut [f32; 3], clamp_gamut: bool) {
    for channel in rgb {
        if !channel.is_finite() {
            *channel = 0.0;
        } else if clamp_gamut {
            *channel = channel.clamp(0.0, 1.0);
        }
    }
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

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
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

    fn legacy_color_models() -> [ColorModel; 7] {
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

    fn all_color_models() -> [ColorModel; 10] {
        [
            ColorModel::Oklch,
            ColorModel::Hsv,
            ColorModel::Hsl,
            ColorModel::CielchAb,
            ColorModel::CielchUv,
            ColorModel::Jzczhz,
            ColorModel::IptIch,
            ColorModel::Okhsl,
            ColorModel::Okhsv,
            ColorModel::Cam16UcsJmh,
        ]
    }

    fn geometry(shape: Shape) -> Geometry {
        Geometry {
            shape,
            start: [0.0, 0.0],
            end: [10.0, 0.0],
            aspect: 1.0,
            skew: 0.0,
            exponent: 2.0,
            spiral_turns: 3.0,
            ray_count: 4.0,
        }
    }

    fn rainbow(color_model: ColorModel) -> Rainbow {
        Rainbow {
            colors: GradientColors::Parametric {
                start: [0.0, 1.0, 0.75],
                end: [1.0, 1.0, 0.75],
                color_model,
            },
            extend: ExtendMode::Clamp,
            easing: Easing::Linear,
            bezier: [0.25, 0.1, 0.25, 1.0],
            clamp_gamut: true,
            cam16_parameters: cam16_viewing_conditions(),
        }
    }

    fn two_color_rainbow(start: [f32; 3], end: [f32; 3], color_space: TwoColorSpace) -> Rainbow {
        let cam16_parameters = cam16_viewing_conditions();
        Rainbow {
            colors: prepare_two_color(start, end, color_space, cam16_parameters),
            extend: ExtendMode::Clamp,
            easing: Easing::Linear,
            bezier: [0.25, 0.1, 0.25, 1.0],
            clamp_gamut: true,
            cam16_parameters,
        }
    }

    #[test]
    fn linear_geometry_hits_both_anchors() {
        let geometry = geometry(Shape::Linear);
        close(geometry_value(geometry.start, geometry), 0.0);
        close(geometry_value(geometry.end, geometry), 1.0);
    }

    #[test]
    fn skew_is_an_inverse_horizontal_shear_for_every_shape() {
        for shape in [
            Shape::Linear,
            Shape::Radial,
            Shape::Diamond,
            Shape::Conic,
            Shape::Box,
            Shape::Minkowski,
            Shape::ReflectedLinear,
            Shape::Spiral,
            Shape::Starburst,
        ] {
            let mut skewed = geometry(shape);
            skewed.skew = 0.5;
            let reference = geometry_value([5.0, 4.0], geometry(shape));
            // Forward-shear the reference coordinate: x' = x + 0.5y.
            let transformed = geometry_value([7.0, 4.0], skewed);
            close(reference, transformed);
        }
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
        let start = sample_rainbow(0.0, &rainbow);
        let middle = sample_rainbow(0.5, &rainbow);
        let end = sample_rainbow(1.0, &rainbow);
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
                let rgb = sample_rainbow(t, &rainbow(model));
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
            ColorModel::Cam16UcsJmh,
        ] {
            let samples: Vec<_> = (0..8)
                .map(|index| sample_rainbow(index as f32 / 8.0, &rainbow(model)))
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
            let parameters = cam16_viewing_conditions();
            let start = components_to_rgb([0.137, 0.8, 0.65], model, parameters, true);
            let end = components_to_rgb([1.137, 0.8, 0.65], model, parameters, true);
            for index in 0..3 {
                close(start[index], end[index]);
            }
        }
    }

    #[test]
    fn legacy_color_models_retain_their_finite_periodic_mapping() {
        for model in legacy_color_models() {
            let parameters = cam16_viewing_conditions();
            let start = components_to_rgb([0.271, 0.63, 0.71], model, parameters, true);
            let end = components_to_rgb([1.271, 0.63, 0.71], model, parameters, true);
            assert!(start.into_iter().all(f32::is_finite));
            for (start, end) in start.into_iter().zip(end) {
                close(start, end);
            }
        }
    }

    #[test]
    fn ok_picker_spaces_keep_their_documented_black_and_white_endpoints() {
        let parameters = cam16_viewing_conditions();
        for model in [ColorModel::Okhsl, ColorModel::Okhsv] {
            let black = components_to_rgb([0.33, 1.0, 0.0], model, parameters, true);
            for channel in black {
                close_within(channel, 0.0, 1.0e-6);
            }
        }
        let white = components_to_rgb([0.71, 1.0, 1.0], ColorModel::Okhsl, parameters, true);
        for channel in white {
            close_within(channel, 1.0, 2.0e-5);
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
            ColorModel::Cam16UcsJmh,
        ] {
            let rgb = components_to_rgb([0.42, 0.0, 1.0], model, cam16_viewing_conditions(), true);
            let tolerance = if model == ColorModel::Cam16UcsJmh {
                1.0e-2
            } else {
                2.0e-3
            };
            for channel in rgb {
                close_within(channel, 1.0, tolerance);
            }
        }
    }

    #[test]
    fn unwrapped_hue_supports_multiple_cycles() {
        let mut rainbow = rainbow(ColorModel::Hsv);
        let GradientColors::Parametric { ref mut end, .. } = rainbow.colors else {
            unreachable!();
        };
        end[0] = 3.0;
        let at_one_third = sample_rainbow(1.0 / 3.0, &rainbow);
        let start = sample_rainbow(0.0, &rainbow);
        for index in 0..3 {
            close(start[index], at_one_third[index]);
        }
    }

    #[test]
    fn two_color_oklab_preserves_endpoints() {
        let rainbow = two_color_rainbow([1.0, 0.1, 0.0], [0.0, 0.2, 1.0], TwoColorSpace::Oklab);
        for (actual, expected) in sample_rainbow(0.0, &rainbow)
            .into_iter()
            .zip([1.0, 0.1, 0.0])
        {
            close_within(actual, expected, 3.0e-5);
        }
        for (actual, expected) in sample_rainbow(1.0, &rainbow)
            .into_iter()
            .zip([0.0, 0.2, 1.0])
        {
            close_within(actual, expected, 3.0e-5);
        }
    }

    #[test]
    fn two_color_oklch_uses_shortest_hue_path() {
        let start = linear_to_srgb3(oklab_to_linear_srgb(oklch_to_oklab([
            0.7,
            0.1,
            350.0 / 360.0,
        ])));
        let end = linear_to_srgb3(oklab_to_linear_srgb(oklch_to_oklab([
            0.7,
            0.1,
            10.0 / 360.0,
        ])));
        let parameters = cam16_viewing_conditions();
        let GradientColors::TwoColor(prepared) =
            prepare_two_color(start, end, TwoColorSpace::Oklch, parameters)
        else {
            unreachable!()
        };
        let middle = interpolate_prepared_two_color(prepared, 0.5, parameters, false);
        let middle_lch = oklab_to_oklch(linear_srgb_to_oklab(middle.map(srgb_to_linear)));
        assert!(
            middle_lch[2] < 0.01 || middle_lch[2] > 0.99,
            "{:?}",
            middle_lch
        );
    }

    #[test]
    fn every_two_color_space_preserves_endpoints_and_remains_finite() {
        let start = [0.82, 0.24, 0.11];
        let end = [0.08, 0.31, 0.88];
        for color_space in [
            TwoColorSpace::Oklab,
            TwoColorSpace::Oklch,
            TwoColorSpace::Cam16UcsJab,
            TwoColorSpace::Cam16UcsJmh,
        ] {
            let rainbow = two_color_rainbow(start, end, color_space);
            for (actual, expected) in sample_rainbow(0.0, &rainbow).into_iter().zip(start) {
                close_within(actual, expected, 8.0e-4);
            }
            for (actual, expected) in sample_rainbow(1.0, &rainbow).into_iter().zip(end) {
                close_within(actual, expected, 8.0e-4);
            }
            for step in 0..=8 {
                assert!(
                    sample_rainbow(step as f32 / 8.0, &rainbow)
                        .into_iter()
                        .all(f32::is_finite),
                    "{color_space:?}"
                );
            }
        }
    }

    #[test]
    fn cam16_ucs_jmh_uses_shortest_hue_and_borrows_neutral_hue() {
        let parameters = cam16_viewing_conditions();
        let start_rgb = cam16_ucs_jmh_to_srgb([60.0, 18.0, 350.0], parameters);
        let end_rgb = cam16_ucs_jmh_to_srgb([60.0, 18.0, 10.0], parameters);
        let GradientColors::TwoColor(PreparedTwoColor { start, end, .. }) =
            prepare_two_color(start_rgb, end_rgb, TwoColorSpace::Cam16UcsJmh, parameters)
        else {
            unreachable!()
        };
        assert!((end[2] - start[2]).abs() < 30.0, "{start:?} -> {end:?}");

        let GradientColors::TwoColor(PreparedTwoColor {
            start: neutral,
            end: chromatic,
            ..
        }) = prepare_two_color(
            [0.5, 0.5, 0.5],
            [0.8, 0.1, 0.05],
            TwoColorSpace::Cam16UcsJmh,
            parameters,
        )
        else {
            unreachable!()
        };
        close_within(neutral[2], chromatic[2], 1.0e-4);
    }
}
