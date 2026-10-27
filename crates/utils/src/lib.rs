use ae::sys::{PF_Pixel, PF_PixelFloat};
use ae::{Pixel8, Pixel16, PixelF32};
use after_effects as ae;

const ALPHA_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RgbInterpretation {
    Premultiplied,
    Straight,
}

pub fn interpreted_rgb(px: PixelF32, interpretation: RgbInterpretation) -> [f32; 3] {
    match interpretation {
        RgbInterpretation::Premultiplied => [px.red, px.green, px.blue],
        RgbInterpretation::Straight => straight_rgb(px),
    }
}

pub fn straight_rgb(px: PixelF32) -> [f32; 3] {
    if px.alpha > ALPHA_EPSILON {
        [px.red / px.alpha, px.green / px.alpha, px.blue / px.alpha]
    } else {
        [0.0, 0.0, 0.0]
    }
}

pub fn premultiply_rgb(rgb: [f32; 3], alpha: f32) -> PixelF32 {
    let alpha = sanitize_channel(alpha);
    if alpha <= ALPHA_EPSILON {
        return PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
    }

    PixelF32 {
        alpha,
        red: sanitize_channel(rgb[0]) * alpha,
        green: sanitize_channel(rgb[1]) * alpha,
        blue: sanitize_channel(rgb[2]) * alpha,
    }
}

fn sanitize_channel(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

pub mod image;
#[cfg(feature = "spectral")]
pub mod spectral;
#[cfg(feature = "spectral_wgpu")]
pub mod spectral_wgpu;

pub trait ToPixel {
    fn to_pixel32(&self) -> PixelF32;
    fn to_pixel16(&self) -> Pixel16;
    fn to_pixel8(&self) -> Pixel8;
}

impl ToPixel for PF_Pixel {
    fn to_pixel32(&self) -> PixelF32 {
        PixelF32 {
            red: self.red as f32 / ae::MAX_CHANNEL8 as f32,
            green: self.green as f32 / ae::MAX_CHANNEL8 as f32,
            blue: self.blue as f32 / ae::MAX_CHANNEL8 as f32,
            alpha: self.alpha as f32 / ae::MAX_CHANNEL8 as f32,
        }
    }

    fn to_pixel16(&self) -> Pixel16 {
        Pixel16 {
            red: (self.red as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
            green: (self.green as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
            blue: (self.blue as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
            alpha: (self.alpha as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
        }
    }

    fn to_pixel8(&self) -> Pixel8 {
        Pixel8 {
            red: self.red,
            green: self.green,
            blue: self.blue,
            alpha: self.alpha,
        }
    }
}

impl ToPixel for Pixel16 {
    fn to_pixel32(&self) -> PixelF32 {
        PixelF32 {
            red: self.red as f32 / ae::MAX_CHANNEL16 as f32,
            green: self.green as f32 / ae::MAX_CHANNEL16 as f32,
            blue: self.blue as f32 / ae::MAX_CHANNEL16 as f32,
            alpha: self.alpha as f32 / ae::MAX_CHANNEL16 as f32,
        }
    }
    fn to_pixel16(&self) -> Pixel16 {
        *self
    }
    fn to_pixel8(&self) -> Pixel8 {
        Pixel8 {
            red: (self.red as f32 / ae::MAX_CHANNEL16 as f32 * ae::MAX_CHANNEL8 as f32) as u8,
            green: (self.green as f32 / ae::MAX_CHANNEL16 as f32 * ae::MAX_CHANNEL8 as f32) as u8,
            blue: (self.blue as f32 / ae::MAX_CHANNEL16 as f32 * ae::MAX_CHANNEL8 as f32) as u8,
            alpha: (self.alpha as f32 / ae::MAX_CHANNEL16 as f32 * ae::MAX_CHANNEL8 as f32) as u8,
        }
    }
}
impl ToPixel for PixelF32 {
    fn to_pixel32(&self) -> PF_PixelFloat {
        *self
    }
    fn to_pixel16(&self) -> Pixel16 {
        Pixel16 {
            red: (self.red.clamp(0.0, 1.0) * ae::MAX_CHANNEL16 as f32) as u16,
            green: (self.green.clamp(0.0, 1.0) * ae::MAX_CHANNEL16 as f32) as u16,
            blue: (self.blue.clamp(0.0, 1.0) * ae::MAX_CHANNEL16 as f32) as u16,
            alpha: (self.alpha.clamp(0.0, 1.0) * ae::MAX_CHANNEL16 as f32) as u16,
        }
    }
    fn to_pixel8(&self) -> Pixel8 {
        Pixel8 {
            red: (self.red.clamp(0.0, 1.0) * ae::MAX_CHANNEL8 as f32) as u8,
            green: (self.green.clamp(0.0, 1.0) * ae::MAX_CHANNEL8 as f32) as u8,
            blue: (self.blue.clamp(0.0, 1.0) * ae::MAX_CHANNEL8 as f32) as u8,
            alpha: (self.alpha.clamp(0.0, 1.0) * ae::MAX_CHANNEL8 as f32) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_interpretation_removes_alpha_multiplication() {
        let px = PixelF32 {
            alpha: 0.25,
            red: 0.2,
            green: 0.1,
            blue: 0.05,
        };

        assert_eq!(
            interpreted_rgb(px, RgbInterpretation::Straight),
            [0.8, 0.4, 0.2]
        );
        assert_eq!(
            interpreted_rgb(px, RgbInterpretation::Premultiplied),
            [0.2, 0.1, 0.05]
        );
    }

    #[test]
    fn premultiply_rgb_preserves_straight_color_at_new_alpha() {
        let px = premultiply_rgb([0.8, 0.4, 0.2], 0.25);

        assert_eq!(px.alpha, 0.25);
        assert_eq!([px.red, px.green, px.blue], [0.2, 0.1, 0.05]);
    }
}
