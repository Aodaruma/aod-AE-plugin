use crate::surface::normalize;
use after_effects::PixelF32;
use utils::image::unpremultiplied_rgb;

#[derive(Clone, Copy)]
pub struct Material {
    pub base: [f32; 3],
    pub source_color: bool,
    pub diffuse: f32,
    pub roughness: f32,
    pub specular: f32,
    pub metallic: f32,
    pub ior: f32,
    pub transmission: f32,
    pub thickness: f32,
    pub emission: [f32; 3],
    pub emission_strength: f32,
    pub ambient: f32,
    pub exposure: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base: [0.65, 0.72, 0.82],
            source_color: false,
            diffuse: 1.0,
            roughness: 0.3,
            specular: 0.5,
            metallic: 0.0,
            ior: 1.5,
            transmission: 0.0,
            thickness: 20.0,
            emission: [1.0; 3],
            emission_strength: 0.0,
            ambient: 0.15,
            exposure: 0.0,
        }
    }
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

pub fn dielectric_f0(ior: f32, specular: f32) -> f32 {
    (((ior - 1.0) / (ior + 1.0)).powi(2) * 2.0 * specular).clamp(0.0, 1.0)
}

fn smith(cosine: f32, a2: f32) -> f32 {
    2.0 * cosine / (cosine + (a2 + (1.0 - a2) * cosine * cosine).sqrt()).max(1e-8)
}

// Camera looks along -Z. Snell refraction intersects a backing plane in physical pixels.
pub fn refraction_offset(normal: [f32; 3], ior: f32, thickness: f32) -> [f32; 2] {
    let eta = 1.0 / ior.max(1.0);
    let c = normal[2].clamp(0.0, 1.0);
    let k = (1.0 - eta * eta * (1.0 - c * c)).max(0.0).sqrt();
    let a = eta * c - k;
    let z = (-eta + a * normal[2]).abs().max(1e-4);
    [a * normal[0] * thickness / z, a * normal[1] * thickness / z]
}

pub fn alpha(mode: i32, original: f32, coverage: f32) -> f32 {
    let a = match mode {
        2 => coverage,
        3 => 1.0,
        _ => original,
    };
    if a.is_finite() {
        a.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn pack(rgb: [f32; 3], alpha: f32, clamp: bool) -> PixelF32 {
    let rgb = rgb.map(|v| {
        let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
        (if clamp { v.min(1.0) } else { v }) * alpha
    });
    PixelF32 {
        alpha,
        red: rgb[0],
        green: rgb[1],
        blue: rgb[2],
    }
}

#[allow(clippy::too_many_arguments)]
pub fn shade(
    source: PixelF32,
    transmitted: [f32; 3],
    normal: [f32; 3],
    light: [f32; 3],
    radiance: [f32; 3],
    height: f32,
    m: Material,
    output: i32,
) -> [f32; 3] {
    if output == 2 {
        return [height; 3];
    }
    if output == 3 {
        return [
            0.5 + 0.5 * normal[0],
            0.5 - 0.5 * normal[1],
            0.5 + 0.5 * normal[2],
        ];
    }
    let source_rgb = if m.source_color && source.alpha > 1e-6 {
        unpremultiplied_rgb(source)
    } else {
        [1.0; 3]
    };
    let base = std::array::from_fn::<_, 3, _>(|c| m.base[c] * source_rgb[c]);
    let no_l = dot(normal, light).max(0.0);
    let no_v = normal[2].max(1e-4);
    let halfway = normalize([light[0], light[1], light[2] + 1.0]);
    let no_h = dot(normal, halfway).max(0.0);
    let vo_h = halfway[2].clamp(0.0, 1.0);
    let a2 = m.roughness.max(0.04).powi(4);
    let denominator = no_h * no_h * (a2 - 1.0) + 1.0;
    let distribution = a2 / (std::f32::consts::PI * denominator * denominator).max(1e-10);
    let geometry = smith(no_l, a2) * smith(no_v, a2);
    let f0 = dielectric_f0(m.ior, m.specular);
    std::array::from_fn(|c| {
        let f0 = f0 * (1.0 - m.metallic) + base[c].clamp(0.0, 1.0) * m.metallic;
        let fresnel = f0 + (1.0 - f0) * (1.0 - vo_h).powi(5);
        let view_fresnel = f0 + (1.0 - f0) * (1.0 - no_v).powi(5);
        let diffuse = (1.0 - fresnel)
            * (1.0 - m.metallic)
            * (1.0 - m.transmission)
            * m.diffuse
            * base[c]
            * no_l
            / std::f32::consts::PI;
        let specular = distribution * geometry * fresnel / (4.0 * no_v).max(1e-8);
        let transmitted =
            transmitted[c] * base[c] * m.transmission * (1.0 - m.metallic) * (1.0 - view_fresnel);
        let ambient = m.ambient * base[c] * (1.0 - m.transmission * (1.0 - m.metallic));
        let result = match output {
            4 => diffuse * radiance[c],
            5 => specular * radiance[c],
            6 => transmitted,
            _ => {
                (diffuse + specular) * radiance[c]
                    + ambient
                    + transmitted
                    + m.emission[c] * m.emission_strength
            }
        };
        result * m.exposure.exp2()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ior_and_refraction_follow_expected_limits() {
        assert!((dielectric_f0(1.5, 0.5) - 0.04).abs() < 1e-6);
        assert_eq!(dielectric_f0(1.0, 0.5), 0.0);
        assert_eq!(refraction_offset([0.6, 0.0, 0.8], 1.0, 20.0), [0.0; 2]);
        assert_eq!(refraction_offset([0.0, 0.0, 1.0], 1.5, 20.0), [0.0; 2]);
        assert!(refraction_offset([0.6, 0.0, 0.8], 1.5, 20.0)[0] < 0.0);
    }

    #[test]
    fn alpha_modes_and_premultiplication_are_independent_of_emission() {
        assert_eq!(alpha(1, 0.25, 0.8), 0.25);
        assert_eq!(alpha(2, 0.25, 0.8), 0.8);
        assert_eq!(alpha(3, 0.0, 0.0), 1.0);
        assert_eq!(pack([10.0; 3], 0.0, false).red, 0.0);
        assert_eq!(pack([2.0; 3], 0.5, false).red, 1.0);
    }

    #[test]
    fn metallic_removes_diffuse_and_transmission_and_hdr_emission_survives() {
        let source = PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
        };
        let n = [0.0, 0.0, 1.0];
        let m = Material {
            metallic: 1.0,
            transmission: 1.0,
            ..Material::default()
        };
        assert_eq!(shade(source, [1.0; 3], n, n, [1.0; 3], 1.0, m, 4), [0.0; 3]);
        assert_eq!(shade(source, [1.0; 3], n, n, [1.0; 3], 1.0, m, 6), [0.0; 3]);
        let m = Material {
            ambient: 0.0,
            emission_strength: 3.0,
            ..Material::default()
        };
        assert_eq!(shade(source, [0.0; 3], n, n, [0.0; 3], 1.0, m, 1), [3.0; 3]);
        for roughness in [0.0, 0.3, 1.0] {
            let m = Material { roughness, ..m };
            assert!(
                shade(source, [0.0; 3], n, n, [1.0; 3], 1.0, m, 1)
                    .iter()
                    .all(|v| v.is_finite())
            );
        }
    }
}
