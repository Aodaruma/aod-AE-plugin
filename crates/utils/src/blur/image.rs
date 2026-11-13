use rayon::prelude::*;

pub type Pixel = [f32; 4];
pub fn mix(a: Pixel, b: Pixel, t: f32) -> Pixel {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub fn add(a: Pixel, b: Pixel) -> Pixel {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn mul(a: Pixel, t: f32) -> Pixel {
    a.map(|v| v * t)
}

#[derive(Clone, Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Pixel>,
}
impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0.; 4]; width * height],
        }
    }
    pub fn at(&self, x: i32, y: i32, edge: u32) -> Pixel {
        if self.width == 0 || self.height == 0 {
            return [0.; 4];
        }
        if edge == 0 && (x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32) {
            return [0.; 4];
        }
        self.pixels[y.clamp(0, self.height as i32 - 1) as usize * self.width
            + x.clamp(0, self.width as i32 - 1) as usize]
    }
    pub fn sample(&self, x: f32, y: f32, edge: u32, interpolation: u32) -> Pixel {
        if !x.is_finite() || !y.is_finite() {
            return [0.; 4];
        }
        if interpolation == 0 {
            return self.at(
                ((x * 4096. + 0.5).floor() / 4096. + 0.5).floor() as i32,
                ((y * 4096. + 0.5).floor() / 4096. + 0.5).floor() as i32,
                edge,
            );
        }
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (tx, ty) = (x - x.floor(), y - y.floor());
        if interpolation == 1 {
            return mix(
                mix(self.at(ix, iy, edge), self.at(ix + 1, iy, edge), tx),
                mix(self.at(ix, iy + 1, edge), self.at(ix + 1, iy + 1, edge), tx),
                ty,
            );
        }
        let wx = cubic_weights(tx);
        let wy = cubic_weights(ty);
        let mut result = [0.; 4];
        for (j, &vy) in wy.iter().enumerate() {
            for (i, &vx) in wx.iter().enumerate() {
                result = add(
                    result,
                    mul(self.at(ix + i as i32 - 1, iy + j as i32 - 1, edge), vx * vy),
                );
            }
        }
        result
    }
    pub fn downsample(&self) -> Self {
        let mut out = Self::new(self.width.div_ceil(2), self.height.div_ceil(2));
        // Area averaging includes each valid texel once, including odd edges.
        out.pixels.par_iter_mut().enumerate().for_each(|(i, p)| {
            let (x, y) = ((i % out.width) * 2, (i / out.width) * 2);
            let mut n = 0.;
            for dy in 0..2 {
                for dx in 0..2 {
                    if x + dx < self.width && y + dy < self.height {
                        *p = add(*p, self.pixels[(y + dy) * self.width + x + dx]);
                        n += 1.;
                    }
                }
            }
            *p = mul(*p, 1. / n);
        });
        out
    }
}
pub fn cubic_weights(t: f32) -> [f32; 4] {
    // Positive cubic B-spline weights avoid negative alpha and HDR ringing.
    [
        (1. - t).powi(3) / 6.,
        (3. * t * t * t - 6. * t * t + 4.) / 6.,
        (-3. * t * t * t + 3. * t * t + 3. * t + 1.) / 6.,
        t * t * t / 6.,
    ]
}
pub fn pyramid(image: Image) -> Vec<Image> {
    let mut levels = vec![image];
    while levels.last().unwrap().width > 1 || levels.last().unwrap().height > 1 {
        levels.push(levels.last().unwrap().downsample());
    }
    levels
}
pub fn luminance(p: Pixel) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}
