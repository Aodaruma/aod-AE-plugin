use after_effects::PixelF32;

#[derive(Clone, Copy)]
pub struct DepthRange {
    pub near: f64,
    pub far: f64,
    pub invert: bool,
}
impl DepthRange {
    /// AE layer pixels are premultiplied. Missing/invalid depth uses the base distance.
    pub fn distance(self, pixel: PixelF32, fallback: f64) -> f64 {
        let channels = [pixel.red, pixel.green, pixel.blue, pixel.alpha];
        if !channels.iter().all(|v| v.is_finite()) || pixel.alpha <= 1e-6 {
            return fallback;
        }
        let mut level = ((0.2126 * pixel.red as f64
            + 0.7152 * pixel.green as f64
            + 0.0722 * pixel.blue as f64)
            / pixel.alpha as f64)
            .clamp(0.0, 1.0);
        if self.invert {
            level = 1.0 - level;
        }
        let distance = self.near + level * (self.far - self.near);
        fallback + pixel.alpha.clamp(0.0, 1.0) as f64 * (distance - fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn depth_range_handles_levels_alpha_and_invalid_values() {
        let range = DepthRange {
            near: 1000.0,
            far: 10000.0,
            invert: false,
        };
        let pixel = |v, a| PixelF32 {
            red: v,
            green: v,
            blue: v,
            alpha: a,
        };
        assert_eq!(range.distance(pixel(0.0, 1.0), 5000.0), 1000.0);
        assert!((range.distance(pixel(1.0, 1.0), 5000.0) - 10000.0).abs() < 1e-8);
        assert!((range.distance(pixel(0.25, 0.5), 5000.0) - 5250.0).abs() < 1e-8);
        assert_eq!(range.distance(pixel(0.0, 0.0), 5000.0), 5000.0);
        assert_eq!(range.distance(pixel(f32::NAN, 1.0), 5000.0), 5000.0);
        assert_eq!(range.distance(pixel(-2.0, 1.0), 5000.0), 1000.0);
        assert!((range.distance(pixel(4.0, 1.0), 5000.0) - 10000.0).abs() < 1e-8);
        let inverted = DepthRange {
            invert: true,
            ..range
        };
        assert_eq!(inverted.distance(pixel(0.0, 1.0), 5000.0), 10000.0);
    }
}
