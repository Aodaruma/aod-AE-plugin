//! Spherical mapping: +X right, +Y up, +Z front. Texture V axes point down.
use after_effects::PixelF32;
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use utils::image::TRANSPARENT;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    Equirectangular,
    Cube3x2,
    Cube6x1,
    Cross4x3,
    Fisheye,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Face {
    Right,
    Left,
    Up,
    Down,
    Front,
    Back,
}
const FACES: [Face; 6] = [
    Face::Right,
    Face::Left,
    Face::Up,
    Face::Down,
    Face::Front,
    Face::Back,
];
impl Projection {
    pub fn from_index(i: i32) -> Self {
        match i {
            2 => Self::Cube3x2,
            3 => Self::Cube6x1,
            4 => Self::Cross4x3,
            5 => Self::Fisheye,
            _ => Self::Equirectangular,
        }
    }
    pub fn grid(self) -> Option<[i32; 2]> {
        match self {
            Self::Cube3x2 => Some([3, 2]),
            Self::Cube6x1 => Some([6, 1]),
            Self::Cross4x3 => Some([4, 3]),
            _ => None,
        }
    }
    fn cell(self, face: Face) -> [i32; 2] {
        let index = FACES.iter().position(|f| *f == face).unwrap() as i32;
        match self {
            Self::Cube3x2 => [index % 3, index / 3],
            Self::Cube6x1 => [index, 0],
            Self::Cross4x3 => match face {
                Face::Left => [0, 1],
                Face::Front => [1, 1],
                Face::Right => [2, 1],
                Face::Back => [3, 1],
                Face::Up => [1, 0],
                Face::Down => [1, 2],
            },
            _ => [0, 0],
        }
    }
    fn face_at(self, cell: [i32; 2]) -> Option<Face> {
        FACES.into_iter().find(|f| self.cell(*f) == cell)
    }
    pub fn valid_size(self, size: [i32; 2]) -> bool {
        if size.iter().any(|v| *v <= 0) {
            return false;
        }
        if let Some([cols, rows]) = self.grid() {
            size[0] % cols == 0 && size[1] % rows == 0 && size[0] / cols == size[1] / rows
        } else {
            self != Self::Fisheye || size[0] == size[1]
        }
    }
    pub fn output_size(self, width: f64) -> Option<[i32; 2]> {
        if !width.is_finite() || !(1.0..=30000.0).contains(&width) {
            return None;
        }
        let [cols, rows] = self.grid().unwrap_or(if self == Self::Fisheye {
            [1, 1]
        } else {
            [2, 1]
        });
        let unit = (width / f64::from(cols)).round().max(1.0) as i32;
        let size = [unit * cols, unit * rows];
        if size.iter().any(|v| *v > 30000) {
            None
        } else {
            Some(size)
        }
    }
    pub fn direction(self, uv: [f64; 2]) -> Option<[f64; 3]> {
        if uv.iter().any(|v| !v.is_finite() || *v < 0.0 || *v >= 1.0) {
            return None;
        }
        if let Some([cols, rows]) = self.grid() {
            let p = [uv[0] * f64::from(cols), uv[1] * f64::from(rows)];
            let cell = [p[0].floor() as i32, p[1].floor() as i32];
            let face = self.face_at(cell)?;
            return Some(face_direction(
                face,
                [p[0] - f64::from(cell[0]), p[1] - f64::from(cell[1])],
            ));
        }
        match self {
            Self::Equirectangular => {
                let lon = (uv[0] - 0.5) * TAU;
                let lat = (0.5 - uv[1]) * PI;
                Some([lon.sin() * lat.cos(), lat.sin(), lon.cos() * lat.cos()])
            }
            Self::Fisheye => {
                let x = 2.0 * uv[0] - 1.0;
                let y = 1.0 - 2.0 * uv[1];
                let r = x.hypot(y);
                if r > 1.0 {
                    None
                } else if r < 1e-12 {
                    Some([0.0, 0.0, 1.0])
                } else {
                    let theta = r * FRAC_PI_2;
                    Some([x / r * theta.sin(), y / r * theta.sin(), theta.cos()])
                }
            }
            _ => unreachable!(),
        }
    }
    pub fn uv(self, dir: [f64; 3]) -> Option<[f64; 2]> {
        let d = unit(dir);
        if let Some([cols, rows]) = self.grid() {
            let (face, uv) = cube_uv(d);
            let c = self.cell(face);
            return Some([
                (f64::from(c[0]) + uv[0]) / f64::from(cols),
                (f64::from(c[1]) + uv[1]) / f64::from(rows),
            ]);
        }
        match self {
            Self::Equirectangular => Some([
                (d[0].atan2(d[2]) / TAU + 0.5).rem_euclid(1.0),
                0.5 - d[1].clamp(-1.0, 1.0).asin() / PI,
            ]),
            Self::Fisheye => {
                if d[2] < -1e-12 {
                    return None;
                }
                let r = d[0].hypot(d[1]);
                let scale = if r < 1e-12 {
                    0.0
                } else {
                    d[2].clamp(-1.0, 1.0).acos() / PI / r
                };
                Some([0.5 + d[0] * scale, 0.5 - d[1] * scale])
            }
            _ => unreachable!(),
        }
    }
}
fn unit(d: [f64; 3]) -> [f64; 3] {
    let n = d[0].hypot(d[1]).hypot(d[2]);
    if n < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        d.map(|v| v / n)
    }
}
fn face_direction(face: Face, uv: [f64; 2]) -> [f64; 3] {
    let a = uv[0] * 2.0 - 1.0;
    let b = uv[1] * 2.0 - 1.0;
    unit(match face {
        Face::Right => [1.0, -b, -a],
        Face::Left => [-1.0, -b, a],
        Face::Up => [a, 1.0, b],
        Face::Down => [a, -1.0, -b],
        Face::Front => [a, -b, 1.0],
        Face::Back => [-a, -b, -1.0],
    })
}
fn cube_uv(d: [f64; 3]) -> (Face, [f64; 2]) {
    let [x, y, z] = d;
    let [ax, ay, az] = d.map(f64::abs);
    let (f, a, b) = if ax >= ay && ax >= az {
        if x >= 0.0 {
            (Face::Right, -z / ax, -y / ax)
        } else {
            (Face::Left, z / ax, -y / ax)
        }
    } else if ay >= az {
        if y >= 0.0 {
            (Face::Up, x / ay, z / ay)
        } else {
            (Face::Down, x / ay, -z / ay)
        }
    } else if z >= 0.0 {
        (Face::Front, x / az, -y / az)
    } else {
        (Face::Back, -x / az, -y / az)
    };
    (f, [a * 0.5 + 0.5, b * 0.5 + 0.5])
}
/// Sample across cube edges on the sphere, never across unrelated atlas cells.
pub fn sample(
    format: Projection,
    d: [f64; 3],
    size: [f64; 2],
    bilinear: bool,
    read: &impl Fn(i32, i32) -> PixelF32,
) -> PixelF32 {
    let pixels = size.map(|v| v.ceil() as i32);
    let Some(uv) = format.uv(d) else {
        return TRANSPARENT;
    };
    let p = [uv[0] * size[0] - 0.5, uv[1] * size[1] - 0.5];
    let cube = format.grid().map(|grid| {
        let (face, _) = cube_uv(d);
        (
            face,
            format.cell(face),
            [size[0] / f64::from(grid[0]), size[1] / f64::from(grid[1])],
        )
    });
    let tap = |mut x: i32, mut y: i32| {
        if let Some((face, c, extent)) = cube {
            let local = [
                (f64::from(x) + 0.5) / extent[0] - f64::from(c[0]),
                (f64::from(y) + 0.5) / extent[1] - f64::from(c[1]),
            ];
            if local.iter().any(|v| *v < 0.0 || *v >= 1.0) {
                let (neighbor, uv) = cube_uv(face_direction(face, local));
                let c = format.cell(neighbor);
                // At a cube corner the reprojected coordinate can lie exactly
                // on another edge. Keep it inside the selected neighbor face.
                let index = |axis: usize| {
                    let low = (f64::from(c[axis]) * extent[axis]).ceil() as i32;
                    let high = ((f64::from(c[axis] + 1) * extent[axis]).ceil() as i32 - 1).max(low);
                    (((f64::from(c[axis]) + uv[axis]) * extent[axis]).floor() as i32)
                        .clamp(low, high)
                };
                x = index(0);
                y = index(1);
            }
            x = x.clamp(0, pixels[0] - 1);
            y = y.clamp(0, pixels[1] - 1);
        } else if format == Projection::Equirectangular {
            x = x.rem_euclid(pixels[0]);
            y = y.clamp(0, pixels[1] - 1);
        } else {
            // The fisheye's unused corners must not leak into the lens.
            let q = [
                (f64::from(x) + 0.5) / size[0],
                (f64::from(y) + 0.5) / size[1],
            ];
            if format.direction(q).is_none() {
                return TRANSPARENT;
            }
        }
        read(x, y)
    };
    if !bilinear {
        return tap((p[0] + 0.5).floor() as i32, (p[1] + 0.5).floor() as i32);
    }
    let x = p[0].floor() as i32;
    let y = p[1].floor() as i32;
    let fx = (p[0] - f64::from(x)) as f32;
    let fy = (p[1] - f64::from(y)) as f32;
    // Difference-form lerps preserve constant channels exactly. Summing four
    // weights can turn opaque alpha into 0.99999994 and lose a code value.
    let mix = |a: PixelF32, b: PixelF32, t: f32| PixelF32 {
        alpha: a.alpha + (b.alpha - a.alpha) * t,
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
    };
    mix(
        mix(tap(x, y), tap(x + 1, y), fx),
        mix(tap(x, y + 1), tap(x + 1, y + 1), fx),
        fy,
    )
}
#[derive(Clone, Copy)]
pub struct Rotation([[f64; 3]; 3]);
impl Rotation {
    pub fn new(angles: [f64; 3]) -> Self {
        let [x, y, z] = angles.map(|v| v.rem_euclid(360.0).to_radians().sin_cos());
        let rotate = |d: [f64; 3]| {
            let d = [d[0], x.1 * d[1] - x.0 * d[2], x.0 * d[1] + x.1 * d[2]];
            let d = [y.1 * d[0] + y.0 * d[2], d[1], -y.0 * d[0] + y.1 * d[2]];
            [z.1 * d[0] - z.0 * d[1], z.0 * d[0] + z.1 * d[1], d[2]]
        };
        Self([
            rotate([1.0, 0.0, 0.0]),
            rotate([0.0, 1.0, 0.0]),
            rotate([0.0, 0.0, 1.0]),
        ])
    }
    pub fn apply(self, d: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| self.0[0][i] * d[0] + self.0[1][i] * d[1] + self.0[2][i] * d[2])
    }
}
#[derive(Clone, Copy)]
pub struct Geometry {
    pub source: [i32; 2],
    pub source_extent: [f64; 2],
    pub bounds: [i32; 4],
    start: [f64; 2],
    extent: [f64; 2],
}
impl Geometry {
    pub fn new(size: [i32; 2], scale: [f64; 2], format: Projection, width: f64) -> Option<Self> {
        if !width.is_finite()
            || width < 0.0
            || size.iter().any(|v| *v <= 0)
            || scale
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1.0)
        {
            return None;
        }
        let target = format.output_size(if width == 0.0 {
            f64::from(size[0])
        } else {
            width
        })?;
        // Align at full resolution so preview resolution cannot move the image.
        let full_start: [f64; 2] =
            std::array::from_fn(|i| (f64::from(size[i] - target[i]) * 0.5).floor());
        let start: [f64; 2] = std::array::from_fn(|i| full_start[i] * scale[i]);
        let extent: [f64; 2] = std::array::from_fn(|i| f64::from(target[i]) * scale[i]);
        Some(Self {
            source_extent: std::array::from_fn(|i| f64::from(size[i]) * scale[i]),
            source: std::array::from_fn(|i| (f64::from(size[i]) * scale[i]).ceil() as i32),
            bounds: [
                start[0].floor() as i32,
                start[1].floor() as i32,
                (start[0] + extent[0]).ceil() as i32,
                (start[1] + extent[1]).ceil() as i32,
            ],
            start,
            extent,
        })
    }
    pub fn uv(self, p: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|i| (p[i] - self.start[i]) / self.extent[i])
    }
}
#[cfg(test)]
mod tests;
