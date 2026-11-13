//! Exposure integration and deliberately stylized spatial blur. All buffers are premultiplied RGBA.
pub mod curve;
mod curve_ui;
pub mod field;
#[cfg(feature = "blur_wgpu")]
mod gpu;
pub mod host;
pub mod image;
#[cfg(test)]
mod tests;
use image::{Image, Pixel, add, luminance, mix, mul, pyramid};
use rayon::prelude::*;

#[derive(Clone, Debug)]
pub struct Settings {
    /// 0 direction, 1 spin, 2 zoom, 3 automatic motion, 4 vectors, 5 path; 17 displacement, 18 mipmap, 19 downsample.
    pub mode: u32,
    pub length: f32,
    pub angle: f32,
    pub center: [f32; 2],
    pub scale: [f32; 2],
    pub samples: u32,
    pub shutter: f32,
    pub bias: f32,
    pub curve: u32,
    pub curve_table: [f32; curve::TABLE_SIZE],
    pub edge: u32,
    pub interpolation: u32,
    pub transfer: u32,
    pub boost: f32,
    pub threshold: f32,
    pub mix: f32,
    pub evolution: f32,
    /// 0 auto, 1 CPU, 2 GPU (an unavailable GPU is an error).
    pub backend: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: 0,
            length: 32.,
            angle: 0.,
            center: [0.; 2],
            scale: [1.; 2],
            samples: 64,
            shutter: 0.5,
            bias: 0.,
            curve: 0,
            curve_table: [1.; curve::TABLE_SIZE],
            edge: 0,
            interpolation: 1,
            transfer: 0,
            boost: 0.,
            threshold: 1.,
            mix: 1.,
            evolution: 0.,
            backend: 0,
        }
    }
}
pub struct Prepared {
    pub levels: Vec<Image>,
    pub field: Image,
    pub settings: Settings,
    /// One direction per stacked copy, shared by every output pixel and both backends.
    pub displacement: Vec<[f32; 2]>,
}
fn transfer(v: f32, mode: u32, inverse: bool) -> f32 {
    let sign = v.signum();
    let v = v.abs();
    let r = match (mode, inverse) {
        (1, false) => {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        (1, true) => {
            if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        }
        (2, false) => v.powf(2.2),
        (2, true) => v.powf(1. / 2.2),
        _ => v,
    };
    sign * r
}
pub fn prepare(source: &Image, field: Image, settings: &Settings) -> Prepared {
    let mut input = source.clone();
    if settings.mode < 16 {
        input.pixels.par_iter_mut().for_each(|p| {
            let a = p[3].clamp(0., 1.);
            if a <= 1e-8 {
                *p = [0.; 4];
                return;
            }
            for v in &mut p[..3] {
                *v = transfer(*v / a, settings.transfer, false);
            }
            let l = luminance(*p).max(0.);
            let gain = 1. + settings.boost * (l - settings.threshold).max(0.) / l.max(1e-6);
            for v in &mut p[..3] {
                *v *= a * gain;
            }
            p[3] = a;
        });
    }
    let levels = if settings.mode >= 18 {
        pyramid(input)
    } else {
        vec![input]
    };
    Prepared {
        levels,
        field,
        settings: settings.clone(),
        displacement: if settings.mode == 17 {
            displacement_directions(settings.samples, settings.evolution)
        } else {
            Vec::new()
        },
    }
}
pub fn finish(mut rendered: Image, source: &Image, prepared: &Prepared) -> Image {
    let s = &prepared.settings;
    rendered
        .pixels
        .par_iter_mut()
        .zip(&source.pixels)
        .enumerate()
        .for_each(|(i, (p, original))| {
            let f = field_at(
                prepared,
                (i % source.width) as f32,
                (i / source.width) as f32,
            );
            if f[2] <= 1e-6
                || ((s.mode == 3 || s.mode == 4) && f[0].hypot(f[1]) < 1e-6)
                || (s.mode == 1 && s.angle.abs() < 1e-8)
            {
                *p = *original;
                return;
            }
            if s.mode < 16 && s.transfer != 0 {
                let a = p[3].clamp(0., 1.);
                if a > 1e-8 {
                    for v in &mut p[..3] {
                        *v = transfer(*v / a, s.transfer, true) * a;
                    }
                } else {
                    *p = [0.; 4];
                }
            }
            *p = mix(*original, *p, s.mix);
            for v in p.iter_mut() {
                if !v.is_finite() {
                    *v = 0.;
                }
            }
            p[3] = p[3].clamp(0., 1.);
        });
    rendered
}
pub fn render(source: &Image, field: Image, settings: &Settings) -> Result<Image, String> {
    if source.width == 0
        || source.height == 0
        || settings.mix <= 0.
        || settings.length <= 0.
        || (settings.mode == 3 || settings.mode == 4) && settings.shutter <= 0.
    {
        return Ok(source.clone());
    }
    let prepared = prepare(source, field, settings);
    let work = source
        .pixels
        .len()
        .saturating_mul(settings.samples as usize);
    #[cfg(feature = "blur_wgpu")]
    if settings.backend == 2 || settings.backend == 0 && settings.mode < 18 && work >= 250_000 {
        match gpu::render(&prepared) {
            Ok(result) => return Ok(finish(result, source, &prepared)),
            Err(e) if settings.backend == 2 => return Err(e),
            Err(_) => {}
        }
    }
    #[cfg(not(feature = "blur_wgpu"))]
    {
        let _ = work;
        if settings.backend == 2 {
            return Err("GPU support was not compiled".into());
        }
    }
    Ok(finish(render_cpu(&prepared), source, &prepared))
}
fn field_at(p: &Prepared, x: f32, y: f32) -> Pixel {
    let source = &p.levels[0];
    p.field.sample(
        x * p.field.width.saturating_sub(1) as f32 / source.width.saturating_sub(1).max(1) as f32,
        y * p.field.height.saturating_sub(1) as f32 / source.height.saturating_sub(1).max(1) as f32,
        1,
        1,
    )
}
fn direction(p: &Prepared, x: f32, y: f32) -> [f32; 2] {
    let f = field_at(p, x, y);
    let len = (f[0] / p.settings.scale[0])
        .hypot(f[1] / p.settings.scale[1])
        .max(1e-8);
    [f[0] / len, f[1] / len]
}
fn advance(p: &Prepared, pos: [f32; 2], step: f32) -> [f32; 2] {
    let a = direction(p, pos[0], pos[1]);
    let b = direction(p, pos[0] + a[0] * step * 0.5, pos[1] + a[1] * step * 0.5);
    [pos[0] + b[0] * step, pos[1] + b[1] * step]
}
fn weight(t: f32, settings: &Settings) -> f32 {
    match settings.curve {
        1 => (1. - (2. * t).abs()).max(0.),
        2 => (std::f32::consts::PI * t).cos().powi(2),
        // Gathering samples runs opposite to the apparent motion of a source point.
        3 => curve::sample(&settings.curve_table, 0.5 - t),
        _ => 1.,
    }
}
fn displacement_directions(samples: u32, evolution: f32) -> Vec<[f32; 2]> {
    let phase = if evolution.is_finite() {
        evolution.rem_euclid(360.).to_radians()
    } else {
        0.
    };
    (0..samples.clamp(2, 512))
        .map(|j| {
            let mut seed = (j + 1).wrapping_mul(2246822519);
            seed = (seed ^ (seed >> 13)).wrapping_mul(1274126177);
            let random = ((seed ^ (seed >> 16)) & 0xffffff) as f32 / 16777216.;
            let angle = random * std::f32::consts::TAU + phase;
            let (sin, cos) = angle.sin_cos();
            [cos, sin]
        })
        .collect()
}
pub fn render_cpu(p: &Prepared) -> Image {
    let src = &p.levels[0];
    let s = &p.settings;
    let mut output = Image::new(src.width, src.height);
    output
        .pixels
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, out)| {
            let (x, y) = ((i % src.width) as f32, (i / src.width) as f32);
            let f = field_at(p, x, y);
            let amount = s.length * f[2].clamp(0., 1.);
            if amount <= 1e-6 {
                *out = src.pixels[i];
                return;
            }
            if s.mode >= 18 {
                let mut lod = (1. + amount * s.scale[0].min(s.scale[1]))
                    .log2()
                    .clamp(0., (p.levels.len() - 1) as f32);
                if s.mode == 19 {
                    lod = lod.round();
                }
                let a = lod.floor() as usize;
                let b = (a + 1).min(p.levels.len() - 1);
                let read = |level: usize| {
                    let img = &p.levels[level];
                    img.sample(
                        (x + 0.5) * img.width as f32 / src.width as f32 - 0.5,
                        (y + 0.5) * img.height as f32 / src.height as f32 - 0.5,
                        s.edge,
                        s.interpolation,
                    )
                };
                *out = mix(read(a), read(b), lod - a as f32);
                return;
            }
            let n = s.samples.clamp(2, 512);
            let mut sum = [0.; 4];
            let mut total = 0.;
            if s.mode == 5 {
                // Integrate both directions from the center; no O(samples^2) retracing.
                let half = n.div_ceil(2);
                let dt = amount / (2. * half as f32);
                let mut center = [x, y];
                let shift = amount * s.bias;
                let steps = (shift.abs() / 2.).ceil().max(1.) as u32;
                for _ in 0..steps {
                    center = advance(p, center, shift / steps as f32);
                }
                for sign in [-1., 1.] {
                    let mut pos = advance(p, center, sign * dt * 0.5);
                    for j in 0..half {
                        let t = sign * (j as f32 + 0.5) / (2. * half as f32);
                        let w = weight(t, s);
                        sum = add(
                            sum,
                            mul(src.sample(pos[0], pos[1], s.edge, s.interpolation), w),
                        );
                        total += w;
                        pos = advance(p, pos, sign * dt);
                    }
                }
            } else {
                for j in 0..n {
                    let t = (j as f32 + 0.5) / n as f32 - 0.5;
                    let u = t + s.bias;
                    let mut pos = [x, y];
                    match s.mode {
                        0 => {
                            pos[0] += s.angle.cos() * amount * u * s.scale[0];
                            pos[1] += s.angle.sin() * amount * u * s.scale[1];
                        }
                        1 => {
                            let theta = s.angle * u;
                            let (a, b) = (
                                (x - s.center[0]) / s.scale[0],
                                (y - s.center[1]) / s.scale[1],
                            );
                            pos = [
                                s.center[0] + (a * theta.cos() - b * theta.sin()) * s.scale[0],
                                s.center[1] + (a * theta.sin() + b * theta.cos()) * s.scale[1],
                            ];
                        }
                        2 => {
                            let zoom = (amount * 0.01 * u).exp();
                            pos = [
                                s.center[0] + (x - s.center[0]) * zoom,
                                s.center[1] + (y - s.center[1]) * zoom,
                            ];
                        }
                        3 | 4 => {
                            pos[0] += f[0] * s.shutter * u;
                            pos[1] += f[1] * s.shutter * u;
                        }
                        17 => {
                            let direction = p.displacement[j as usize];
                            pos[0] += direction[0] * amount * s.scale[0];
                            pos[1] += direction[1] * amount * s.scale[1];
                        }
                        _ => {}
                    }
                    let w = if s.mode == 17 { 1. } else { weight(t, s) };
                    sum = add(
                        sum,
                        mul(src.sample(pos[0], pos[1], s.edge, s.interpolation), w),
                    );
                    total += w;
                }
            }
            *out = if total > 1e-8 {
                mul(sum, 1. / total)
            } else {
                src.pixels[i]
            };
        });
    output
}
