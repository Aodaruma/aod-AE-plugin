use super::image::{Image, luminance, pyramid};
use rayon::prelude::*;

pub type Segment = [f32; 4];
fn distance2(p: [f32; 2], b: [f32; 4]) -> f32 {
    (p[0] - p[0].clamp(b[0], b[2])).powi(2) + (p[1] - p[1].clamp(b[1], b[3])).powi(2)
}
struct Node {
    bounds: [f32; 4],
    start: usize,
    end: usize,
    children: Option<(Box<Node>, Box<Node>)>,
}
impl Node {
    fn build(segments: &mut [Segment], offset: usize) -> Self {
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for s in segments.iter() {
            bounds[0] = bounds[0].min(s[0]).min(s[2]);
            bounds[1] = bounds[1].min(s[1]).min(s[3]);
            bounds[2] = bounds[2].max(s[0]).max(s[2]);
            bounds[3] = bounds[3].max(s[1]).max(s[3]);
        }
        let mut node = Self {
            bounds,
            start: offset,
            end: offset + segments.len(),
            children: None,
        };
        if segments.len() > 8 {
            let axis = usize::from(bounds[3] - bounds[1] > bounds[2] - bounds[0]);
            segments.sort_unstable_by(|a, b| {
                (a[axis] + a[axis + 2]).total_cmp(&(b[axis] + b[axis + 2]))
            });
            let mid = segments.len() / 2;
            let (a, b) = segments.split_at_mut(mid);
            node.children = Some((
                Box::new(Self::build(a, offset)),
                Box::new(Self::build(b, offset + mid)),
            ));
        }
        node
    }
    fn nearest(&self, segments: &[Segment], p: [f32; 2], best: &mut (f32, [f32; 2])) {
        if distance2(p, self.bounds) > best.0 {
            return;
        }
        if let Some((a, b)) = &self.children {
            if distance2(p, a.bounds) < distance2(p, b.bounds) {
                a.nearest(segments, p, best);
                b.nearest(segments, p, best);
            } else {
                b.nearest(segments, p, best);
                a.nearest(segments, p, best);
            }
        } else {
            for s in &segments[self.start..self.end] {
                let d = [s[2] - s[0], s[3] - s[1]];
                let len2 = d[0] * d[0] + d[1] * d[1];
                if len2 < 1e-10 {
                    continue;
                }
                let t = (((p[0] - s[0]) * d[0] + (p[1] - s[1]) * d[1]) / len2).clamp(0., 1.);
                let dist = (p[0] - s[0] - t * d[0]).powi(2) + (p[1] - s[1] - t * d[1]).powi(2);
                if dist < best.0 {
                    *best = (dist, [d[0] / len2.sqrt(), d[1] / len2.sqrt()]);
                }
            }
        }
    }
}

/// Closest-curve tangent and distance field; the BVH avoids O(pixels * segments).
pub fn path_field(width: usize, height: usize, segments: &[Segment], falloff: f32) -> Image {
    let mut result = Image::new(width.div_ceil(4).max(1), height.div_ceil(4).max(1));
    let mut segments = segments
        .iter()
        .copied()
        .filter(|s| s.iter().all(|v| v.is_finite()))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return result;
    }
    let tree = Node::build(&mut segments, 0);
    let fw = result.width;
    let fh = result.height;
    result.pixels.par_iter_mut().enumerate().for_each(|(i, v)| {
        let p = [
            (i % fw) as f32 * width.saturating_sub(1) as f32 / fw.saturating_sub(1).max(1) as f32,
            (i / fw) as f32 * height.saturating_sub(1) as f32 / fh.saturating_sub(1).max(1) as f32,
        ];
        let mut best = (f32::INFINITY, [0.; 2]);
        tree.nearest(&segments, p, &mut best);
        if best.0.is_finite() {
            let distance = best.0.sqrt();
            let influence = if falloff <= 0. {
                1.
            } else {
                let t = (1. - distance / falloff).clamp(0., 1.);
                t * t * (3. - 2. * t)
            };
            *v = [best.1[0], best.1[1], influence, distance];
        }
    });
    result
}

fn feature(p: [f32; 4]) -> f32 {
    (luminance(p).max(0.) + p[3] * 0.25).ln_1p()
}
fn error(a: &Image, b: &Image, x: f32, y: f32, dx: f32, dy: f32) -> f32 {
    let mut sum = 0.;
    for j in -2..=2 {
        for i in -2..=2 {
            let p = feature(a.sample(x + i as f32, y + j as f32, 1, 1));
            let q = feature(b.sample(x + i as f32 + dx, y + j as f32 + dy, 1, 1));
            sum += (p - q).abs();
        }
    }
    sum / 25.
}
/// Deterministic pyramidal patch flow, current -> previous. Flat/changed areas reject motion.
pub fn motion_field(current: &Image, previous: &Image, max_motion: f32) -> Image {
    let a = pyramid(current.clone());
    let b = pyramid(previous.clone());
    let top = a
        .iter()
        .position(|p| p.width.max(p.height) <= 64)
        .unwrap_or(a.len() - 1)
        .min(b.len() - 1);
    let mut field = Image::new(
        current.width.div_ceil(8).max(1),
        current.height.div_ceil(8).max(1),
    );
    let fw = field.width;
    let fh = field.height;
    field.pixels.par_iter_mut().enumerate().for_each(|(i, p)| {
        let x = (i % fw) as f32 * current.width.saturating_sub(1) as f32
            / fw.saturating_sub(1).max(1) as f32;
        let y = (i / fw) as f32 * current.height.saturating_sub(1) as f32
            / fh.saturating_sub(1).max(1) as f32;
        let mut flow = [0.; 2];
        let mut best_error = 0.;
        for level in (0..=top).rev() {
            if level != top {
                flow[0] *= 2.;
                flow[1] *= 2.;
            }
            let scale = 2f32.powi(level as i32);
            let (px, py) = ((x + 0.5) / scale - 0.5, (y + 0.5) / scale - 0.5);
            let base = flow;
            best_error = error(&a[level], &b[level], px, py, base[0], base[1]);
            let mut cost = best_error + 0.0001 * (base[0] * base[0] + base[1] * base[1]);
            for dy in -3..=3 {
                for dx in -3..=3 {
                    let f = [base[0] + dx as f32, base[1] + dy as f32];
                    if f[0].hypot(f[1]) * scale > max_motion {
                        continue;
                    }
                    let e = error(&a[level], &b[level], px, py, f[0], f[1]);
                    let candidate = e + 0.0001 * (f[0] * f[0] + f[1] * f[1]);
                    if candidate < cost {
                        cost = candidate;
                        best_error = e;
                        flow = f;
                    }
                }
            }
        }
        // Refine to quarter-pixel precision without moving the search center mid-pass.
        for step in [0.5, 0.25] {
            let base = flow;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let f = [base[0] + dx as f32 * step, base[1] + dy as f32 * step];
                    if f[0].hypot(f[1]) > max_motion {
                        continue;
                    }
                    let e = error(&a[0], &b[0], x, y, f[0], f[1]);
                    if e < best_error {
                        best_error = e;
                        flow = f;
                    }
                }
            }
        }
        let confidence = (1. - best_error / 0.15).clamp(0., 1.);
        *p = [-flow[0] * confidence, -flow[1] * confidence, 1., confidence];
    });
    field
}
