use ae::sys::{PF_Pixel, PF_PixelFloat};
use ae::{Pixel8, Pixel16, PixelF32};
use after_effects as ae;

pub trait ToPixel {
    fn to_pixel32(&self) -> PixelF32;
    fn to_pixel16(&self) -> Pixel16;
    fn to_pixel8(&self) -> Pixel8;
}

impl ToPixel for PF_Pixel {
    fn to_pixel32(&self) -> PixelF32 {
        // PF_Pixel は 8bpc 相当として扱う
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

// impl ToPixel for Pixel8 {
//     fn to_pixel32(&self) -> PixelF32 {
//         PixelF32 {
//             red: self.red as f32 / ae::MAX_CHANNEL8 as f32,
//             green: self.green as f32 / ae::MAX_CHANNEL8 as f32,
//             blue: self.blue as f32 / ae::MAX_CHANNEL8 as f32,
//             alpha: self.alpha as f32 / ae::MAX_CHANNEL8 as f32,
//         }
//     }
//     fn to_pixel16(&self) -> Pixel16 {
//         Pixel16 {
//             red: (self.red as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
//             green: (self.green as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
//             blue: (self.blue as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
//             alpha: (self.alpha as f32 / ae::MAX_CHANNEL8 as f32 * ae::MAX_CHANNEL16 as f32) as u16,
//         }
//     }
//     fn to_pixel8(&self) -> Pixel8 {
//         *self
//     }
// }
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
