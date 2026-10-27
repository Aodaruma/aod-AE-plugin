use serde::{Deserialize, Serialize};

pub const MAX_POINTS: usize = 16;
const GAP: f32 = 0.002;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interpolation {
    ShapePreserving,
    Linear,
    BSpline,
}

impl Interpolation {
    pub fn from_popup(value: i32) -> Self {
        // Keep the original popup IDs so saved projects retain their appearance.
        match value {
            2 => Self::Linear,
            3 => Self::BSpline,
            _ => Self::ShapePreserving,
        }
    }
}

pub struct Sampler<'a> {
    curve: &'a Curve,
    interpolation: Interpolation,
    segments: Vec<CubicSegment>,
}

#[derive(Clone, Copy)]
struct CubicSegment {
    coefficients: [[f64; 4]; 2],
}

impl CubicSegment {
    fn value(&self, axis: usize, t: f64) -> f64 {
        let [a, b, c, d] = self.coefficients[axis];
        ((a * t + b) * t + c) * t + d
    }

    fn derivative(&self, axis: usize, t: f64) -> f64 {
        let [a, b, c, _] = self.coefficients[axis];
        (3.0 * a * t + 2.0 * b) * t + c
    }
}

impl Sampler<'_> {
    pub fn sample(&self, x: f32) -> f32 {
        if self.interpolation != Interpolation::BSpline {
            return self
                .curve
                .sample(x, self.interpolation == Interpolation::ShapePreserving);
        }
        let x = if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if x == 0.0 {
            return self.curve.points[0][1];
        }
        if x == 1.0 {
            return self.curve.points[self.curve.len() - 1][1];
        }
        let x = f64::from(x);
        let segment = self
            .segments
            .iter()
            .find(|s| x <= s.value(0, 1.0))
            .unwrap_or_else(|| self.segments.last().unwrap());
        let mut lo = 0.0;
        let mut hi = 1.0;
        let mut t = (x - segment.value(0, 0.0)) / (segment.value(0, 1.0) - segment.value(0, 0.0));
        // Ordered control-point X values make x(t) strictly increasing. Invert it
        // with safeguarded Newton steps, without a piecewise-linear lookup table.
        for _ in 0..32 {
            let error = segment.value(0, t) - x;
            if error.abs() < 1e-12 {
                break;
            }
            if error < 0.0 {
                lo = t;
            } else {
                hi = t;
            }
            let next = t - error / segment.derivative(0, t);
            t = if next > lo && next < hi {
                next
            } else {
                (lo + hi) * 0.5
            };
        }
        segment.value(1, t).clamp(0.0, 1.0) as f32
    }
}

fn spline_point(curve: &Curve, knots: &[f64], degree: usize, span: usize, u: f64) -> [f64; 2] {
    let mut d = [[0.0; 2]; 4];
    for (j, point) in d.iter_mut().enumerate().take(degree + 1) {
        *point = curve.points[span - degree + j].map(f64::from);
    }
    // de Boor evaluation of a clamped B-spline (equivalently, unit-weight NURBS).
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = span - degree + j;
            let alpha = (u - knots[i]) / (knots[i + degree + 1 - r] - knots[i]);
            d[j] = std::array::from_fn(|axis| (1.0 - alpha) * d[j - 1][axis] + alpha * d[j][axis]);
        }
    }
    d[degree]
}

// Fixed storage keeps AE handles self-contained. The version is part of the saved format.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Curve {
    pub version: u32,
    pub count: u32,
    pub points: [[f32; 2]; MAX_POINTS],
}

impl Default for Curve {
    fn default() -> Self {
        Self::preset(2)
    }
}

impl Curve {
    pub fn preset(preset: i32) -> Self {
        let nodes: &[[f32; 2]] = match preset {
            2 => &[
                [0.0, 0.0],
                [0.25, 0.66],
                [0.5, 0.87],
                [0.75, 0.97],
                [1.0, 1.0],
            ],
            3 => &[
                [0.0, 0.0],
                [0.25, 0.03],
                [0.5, 0.13],
                [0.75, 0.34],
                [1.0, 1.0],
            ],
            4 => &[
                [0.0, 0.0],
                [0.2, 0.0],
                [0.4, 0.9],
                [0.6, 0.9],
                [0.8, 0.0],
                [1.0, 0.0],
            ],
            5 => &[
                [0.0, 0.0],
                [0.25, 0.0],
                [0.3, 0.45],
                [0.6, 0.45],
                [0.65, 1.0],
                [1.0, 1.0],
            ],
            _ => &[[0.0, 0.0], [1.0, 1.0]],
        };
        let mut curve = Self {
            version: 1,
            count: nodes.len() as u32,
            points: [[0.0; 2]; MAX_POINTS],
        };
        curve.points[..nodes.len()].copy_from_slice(nodes);
        curve
    }

    pub fn valid(&self) -> bool {
        let n = self.count as usize;
        self.version == 1
            && (2..=MAX_POINTS).contains(&n)
            && self.points[0][0] == 0.0
            && self.points[n - 1][0] == 1.0
            && self.points[..n]
                .iter()
                .all(|p| p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            && self.points[..n]
                .windows(2)
                .all(|p| p[1][0] - p[0][0] >= GAP * 0.99)
    }

    pub fn sanitized(self) -> Self {
        if self.valid() { self } else { Self::default() }
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn sampler(&self, interpolation: Interpolation) -> Sampler<'_> {
        let mut segments = Vec::new();
        if interpolation == Interpolation::BSpline {
            let n = self.len();
            let degree = (n - 1).min(3);
            let spans = n - degree;
            let mut knots = [0.0; MAX_POINTS + 4];
            for (i, knot) in knots.iter_mut().enumerate().take(n + degree + 1) {
                *knot = if i >= n {
                    1.0
                } else {
                    i.saturating_sub(degree) as f64 / spans as f64
                };
            }
            for k in 0..spans {
                let samples: [[f64; 2]; 4] = std::array::from_fn(|j| {
                    spline_point(
                        self,
                        &knots,
                        degree,
                        k + degree,
                        (k as f64 + j as f64 / 3.0) / spans as f64,
                    )
                });
                // Each knot span is a polynomial of degree <= 3. Compile it once
                // for both the editor and rendering, preserving C2 continuity.
                let coefficients = std::array::from_fn(|axis| {
                    let [p0, p1, p2, p3] = samples.map(|p| p[axis]);
                    [
                        4.5 * (p3 - 3.0 * p2 + 3.0 * p1 - p0),
                        4.5 * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3),
                        (-11.0 * p0 + 18.0 * p1 - 9.0 * p2 + 2.0 * p3) * 0.5,
                        p0,
                    ]
                });
                segments.push(CubicSegment { coefficients });
            }
        }
        Sampler {
            curve: self,
            interpolation,
            segments,
        }
    }

    pub fn sample(&self, x: f32, smooth: bool) -> f32 {
        let x = if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let n = self.len();
        let k = (0..n - 1)
            .find(|&i| x <= self.points[i + 1][0])
            .unwrap_or(n - 2);
        let [x0, y0] = self.points[k];
        let [x1, y1] = self.points[k + 1];
        let dx = x1 - x0;
        let t = ((x - x0) / dx).clamp(0.0, 1.0);
        if !smooth {
            return y0 + (y1 - y0) * t;
        }
        let slope = |i: usize| {
            (self.points[i + 1][1] - self.points[i][1])
                / (self.points[i + 1][0] - self.points[i][0])
        };
        // Weighted harmonic tangents preserve peaks, troughs and zero plateaus.
        let tangent = |i: usize| {
            if i == 0 {
                return slope(0);
            }
            if i == n - 1 {
                return slope(n - 2);
            }
            let a = slope(i - 1);
            let b = slope(i);
            if a * b <= 0.0 {
                return 0.0;
            }
            let h0 = self.points[i][0] - self.points[i - 1][0];
            let h1 = self.points[i + 1][0] - self.points[i][0];
            let w0 = 2.0 * h1 + h0;
            let w1 = h1 + 2.0 * h0;
            (w0 + w1) / (w0 / a + w1 / b)
        };
        let t2 = t * t;
        let t3 = t2 * t;
        ((2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * dx * tangent(k)
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * dx * tangent(k + 1))
        .clamp(y0.min(y1), y0.max(y1))
    }

    pub fn insert(&mut self, x: f32, y: f32) -> Option<usize> {
        if self.len() == MAX_POINTS || !x.is_finite() || !y.is_finite() {
            return None;
        }
        let k = (1..self.len()).find(|&i| x < self.points[i][0])?;
        if x - self.points[k - 1][0] < GAP || self.points[k][0] - x < GAP {
            return None;
        }
        let n = self.len();
        self.points.copy_within(k..n, k + 1);
        self.points[k] = [x, y.clamp(0.0, 1.0)];
        self.count += 1;
        Some(k)
    }

    pub fn move_point(&mut self, k: usize, x: f32, y: f32) {
        if k >= self.len() || !x.is_finite() || !y.is_finite() {
            return;
        }
        if k > 0 && k + 1 < self.len() {
            self.points[k][0] = x.clamp(self.points[k - 1][0] + GAP, self.points[k + 1][0] - GAP);
        }
        self.points[k][1] = y.clamp(0.0, 1.0);
    }

    pub fn remove(&mut self, k: usize) {
        if k == 0 || k + 1 >= self.len() {
            return;
        }
        let n = self.len();
        self.points.copy_within(k + 1..n, k);
        self.count -= 1;
        self.points[self.len()] = [0.0; 2];
    }
}

impl after_effects::ArbitraryData<Self> for Curve {
    fn interpolate(&self, other: &Self, value: f64) -> Self {
        let a = self.sanitized();
        let b = other.sanitized();
        let t = (value as f32).clamp(0.0, 1.0);
        if t <= 0.0 {
            return a;
        }
        if t >= 1.0 {
            return b;
        }
        if a.count == b.count {
            let mut c = a;
            for i in 0..c.len() {
                for axis in 0..2 {
                    c.points[i][axis] = a.points[i][axis] * (1.0 - t) + b.points[i][axis] * t;
                }
            }
            c
        } else {
            // Different point counts interpolate as sampled profiles, without indexing unmatched nodes.
            let mut c = Self {
                version: 1,
                count: MAX_POINTS as u32,
                points: [[0.0; 2]; MAX_POINTS],
            };
            for i in 0..MAX_POINTS {
                let x = i as f32 / (MAX_POINTS - 1) as f32;
                c.points[i] = [x, a.sample(x, true) * (1.0 - t) + b.sample(x, true) * t];
            }
            c
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_profile_survives_ae_flatten_format() {
        let mut curve = Curve::preset(4);
        curve.insert(0.52, 0.12).unwrap();
        let encoded = bincode::serde::encode_to_vec(curve, bincode::config::legacy()).unwrap();
        let (decoded, consumed): (Curve, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::legacy()).unwrap();
        assert_eq!(decoded, curve);
        assert_eq!(consumed, encoded.len());
        assert!(decoded.valid());
    }
    use after_effects::ArbitraryData;

    #[test]
    fn curves_preserve_nodes_and_zero_plateaus_without_overshoot() {
        for preset in 1..=5 {
            let c = Curve::preset(preset);
            assert!(c.valid());
            for p in &c.points[..c.len()] {
                assert!((c.sample(p[0], true) - p[1]).abs() < 1e-5);
            }
            for i in 0..=1000 {
                assert!((0.0..=1.0).contains(&c.sample(i as f32 / 1000.0, true)));
            }
        }
        assert_eq!(Curve::preset(4).sample(0.9, true), 0.0);
    }

    #[test]
    fn edits_and_unequal_keyframes_remain_valid() {
        let mut a = Curve::preset(1);
        let i = a.insert(0.4, 0.7).unwrap();
        a.move_point(i, 2.0, -1.0);
        assert!(a.valid());
        a.remove(0);
        a.remove(i);
        assert_eq!(a, Curve::preset(1));
        assert!(a.interpolate(&Curve::preset(4), 0.5).valid());
    }

    #[test]
    fn spline_matches_linear_quadratic_and_cubic_bezier_cases() {
        let line = Curve::preset(1);
        let sampler = line.sampler(Interpolation::BSpline);
        assert!((sampler.sample(0.37) - 0.37).abs() < 1e-6);
        let mut quadratic = line;
        quadratic.points[1][1] = 0.0;
        quadratic.insert(0.5, 1.0).unwrap();
        let sampler = quadratic.sampler(Interpolation::BSpline);
        assert_eq!(sampler.sample(0.0), 0.0);
        assert_eq!(sampler.sample(1.0), 0.0);
        assert!((sampler.sample(0.5) - 0.5).abs() < 1e-6);
        let mut cubic = line;
        cubic.insert(1.0 / 3.0, 1.0).unwrap();
        cubic.insert(2.0 / 3.0, 0.0).unwrap();
        let sampler = cubic.sampler(Interpolation::BSpline);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            let expected = 3.0 * x - 6.0 * x * x + 4.0 * x * x * x;
            assert!((sampler.sample(x) - expected).abs() < 1e-6);
        }
        assert_eq!(sampler.sample(f32::NAN), 0.0);
    }

    #[test]
    fn spline_has_continuous_slope_and_curvature_at_internal_knots() {
        let mut curve = Curve::preset(4);
        curve.move_point(1, 0.08, 0.7);
        curve.move_point(2, 0.25, 0.2);
        let sampler = curve.sampler(Interpolation::BSpline);
        let derivatives = |s: &CubicSegment, t: f64| {
            let dx = s.derivative(0, t);
            let dy = s.derivative(1, t);
            let ddx = 6.0 * s.coefficients[0][0] * t + 2.0 * s.coefficients[0][1];
            let ddy = 6.0 * s.coefficients[1][0] * t + 2.0 * s.coefficients[1][1];
            [dy / dx, (ddy * dx - dy * ddx) / dx.powi(3)]
        };
        for pair in sampler.segments.windows(2) {
            assert!((pair[0].value(1, 1.0) - pair[1].value(1, 0.0)).abs() < 1e-10);
            for (left, right) in derivatives(&pair[0], 1.0)
                .into_iter()
                .zip(derivatives(&pair[1], 0.0))
            {
                assert!((left - right).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn spline_inverts_uneven_control_points_without_overshoot() {
        for count in 2..=MAX_POINTS {
            let mut curve = Curve::preset(1);
            for i in 1..count - 1 {
                curve
                    .insert(i as f32 * 0.0021, (i % 3) as f32 * 0.5)
                    .unwrap();
            }
            let sampler = curve.sampler(Interpolation::BSpline);
            for segment in &sampler.segments {
                for i in 0..=32 {
                    let t = i as f64 / 32.0;
                    let x = segment.value(0, t) as f32;
                    let expected = segment.value(1, t) as f32;
                    assert!((sampler.sample(x) - expected).abs() < 2e-6);
                }
            }
            for i in 0..=1000 {
                assert!((0.0..=1.0).contains(&sampler.sample(i as f32 / 1000.0)));
            }
            assert_eq!(sampler.sample(0.0), curve.points[0][1]);
            assert_eq!(sampler.sample(1.0), curve.points[count - 1][1]);
        }
    }

    #[test]
    fn saved_interpolation_ids_keep_the_original_rendering() {
        let curve = Curve::preset(4);
        for (id, smooth) in [(1, true), (2, false)] {
            let sampler = curve.sampler(Interpolation::from_popup(id));
            for i in 0..=1000 {
                let x = i as f32 / 1000.0;
                assert_eq!(sampler.sample(x), curve.sample(x, smooth));
            }
        }
        assert_eq!(Interpolation::from_popup(3), Interpolation::BSpline);
    }
}
