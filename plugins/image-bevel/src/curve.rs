mod format;

pub const MAX_POINTS: usize = 16;
const GAP: f32 = 0.002;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interpolation {
    Bezier,
    Linear,
    BSpline,
}

impl Interpolation {
    pub fn from_popup(value: i32) -> Self {
        // Keep the original popup IDs so saved projects retain their appearance.
        match value {
            2 => Self::Linear,
            3 => Self::BSpline,
            _ => Self::Bezier,
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
        if self.interpolation == Interpolation::Linear
            || (self.interpolation == Interpolation::Bezier && self.curve.manual == 0)
        {
            // Untouched legacy profiles retain their exact floating-point evaluation.
            return self
                .curve
                .sample(x, self.interpolation == Interpolation::Bezier);
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
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Curve {
    pub version: u32,
    pub count: u32,
    pub points: [[f32; 2]; MAX_POINTS],
    // A bit per anchor; unset bits use automatic tangents. Handles are offsets.
    manual: u32,
    handles: [[[f32; 2]; 2]; MAX_POINTS],
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
            manual: 0,
            handles: [[[0.0; 2]; 2]; MAX_POINTS],
        };
        curve.points[..nodes.len()].copy_from_slice(nodes);
        curve
    }

    pub fn valid(&self) -> bool {
        let n = self.count as usize;
        matches!(self.version, 1 | 2)
            && (2..=MAX_POINTS).contains(&n)
            && self.manual >> n == 0
            && (self.version == 2 || self.manual == 0)
            && self
                .handles
                .iter()
                .flatten()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1.0)
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
        } else if interpolation == Interpolation::Bezier && self.manual != 0 {
            for k in 0..self.len() - 1 {
                let controls = [
                    self.points[k],
                    self.handles(k)[1],
                    self.handles(k + 1)[0],
                    self.points[k + 1],
                ];
                let coefficients = std::array::from_fn(|axis| {
                    let [a, b, c, d] = controls.map(|p| f64::from(p[axis]));
                    [
                        -a + 3.0 * b - 3.0 * c + d,
                        3.0 * a - 6.0 * b + 3.0 * c,
                        3.0 * (b - a),
                        a,
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
        let t2 = t * t;
        let t3 = t2 * t;
        ((2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * dx * self.tangent(k)
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * dx * self.tangent(k + 1))
        .clamp(y0.min(y1), y0.max(y1))
    }

    fn tangent(&self, i: usize) -> f32 {
        let n = self.len();
        let slope = |i: usize| {
            (self.points[i + 1][1] - self.points[i][1])
                / (self.points[i + 1][0] - self.points[i][0])
        };
        // Weighted harmonic tangents preserve peaks, troughs and zero plateaus.
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
    }

    pub fn has_handle(&self, k: usize, side: usize) -> bool {
        k < self.len() && ((side == 0 && k > 0) || (side == 1 && k + 1 < self.len()))
    }

    fn limit_handle(&self, k: usize, side: usize, offset: [f32; 2]) -> [f32; 2] {
        if !self.has_handle(k, side) {
            return self.points[k];
        }
        let anchor = self.points[k];
        let neighbor = self.points[if side == 0 { k - 1 } else { k + 1 }];
        // Scale toward the anchor when clipping, retaining the tangent direction.
        let bounds = if side == 0 {
            [neighbor[0], anchor[0]]
        } else {
            [anchor[0], neighbor[0]]
        };
        let mut scale = 1.0_f32;
        for (axis, [lo, hi]) in [bounds, [0.0, 1.0]].into_iter().enumerate() {
            if offset[axis] > 0.0 {
                scale = scale.min((hi - anchor[axis]) / offset[axis]);
            }
            if offset[axis] < 0.0 {
                scale = scale.min((lo - anchor[axis]) / offset[axis]);
            }
        }
        std::array::from_fn(|axis| anchor[axis] + offset[axis] * scale)
    }

    pub fn handles(&self, k: usize) -> [[f32; 2]; 2] {
        std::array::from_fn(|side| {
            if !self.has_handle(k, side) {
                return self.points[k];
            }
            let offset = if self.manual & (1 << k) != 0 {
                self.handles[k][side]
            } else {
                let neighbor = if side == 0 { k - 1 } else { k + 1 };
                let dx = (self.points[neighbor][0] - self.points[k][0]) / 3.0;
                [dx, dx * self.tangent(k)]
            };
            self.limit_handle(k, side, offset)
        })
    }

    pub fn move_handle(&mut self, k: usize, side: usize, x: f32, y: f32, independent: bool) {
        if !self.has_handle(k, side) || !x.is_finite() || !y.is_finite() {
            return;
        }
        let anchor = self.points[k];
        let current = self.handles(k);
        let offset = [x.clamp(0.0, 1.0) - anchor[0], y.clamp(0.0, 1.0) - anchor[1]];
        let point = self.limit_handle(k, side, offset);
        self.handles[k] = current.map(|p| [p[0] - anchor[0], p[1] - anchor[1]]);
        let vector = [point[0] - anchor[0], point[1] - anchor[1]];
        self.handles[k][side] = vector;
        if !independent {
            let length = vector[0].hypot(vector[1]);
            if length > 1e-6 {
                let other = self.handles[k][1 - side];
                let ratio = -other[0].hypot(other[1]) / length;
                let opposite = self.limit_handle(k, 1 - side, vector.map(|v| v * ratio));
                self.handles[k][1 - side] = [opposite[0] - anchor[0], opposite[1] - anchor[1]];
            }
        }
        self.manual |= 1 << k;
        self.version = 2;
    }

    pub fn reset_handles(&mut self, k: usize) {
        if k < self.len() {
            self.manual &= !(1 << k);
            self.handles[k] = [[0.0; 2]; 2];
        }
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
        self.handles.copy_within(k..n, k + 1);
        self.handles[k] = [[0.0; 2]; 2];
        let lower = (1 << k) - 1;
        self.manual = (self.manual & lower) | ((self.manual & !lower) << 1);
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
        self.handles.copy_within(k + 1..n, k);
        let lower = (1 << k) - 1;
        self.manual = (self.manual & lower) | ((self.manual >> 1) & !lower);
        self.count -= 1;
        self.points[self.len()] = [0.0; 2];
        self.handles[self.len()] = [[0.0; 2]; 2];
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
            c.manual = a.manual | b.manual;
            c.version = a.version.max(b.version);
            for i in 0..c.len() {
                for axis in 0..2 {
                    c.points[i][axis] = a.points[i][axis] * (1.0 - t) + b.points[i][axis] * t;
                }
                if c.manual & (1 << i) != 0 {
                    let ah = a.handles(i);
                    let bh = b.handles(i);
                    c.handles[i] = std::array::from_fn(|side| {
                        std::array::from_fn(|axis| {
                            ah[side][axis] * (1.0 - t) + bh[side][axis] * t - c.points[i][axis]
                        })
                    });
                }
            }
            c
        } else {
            // Different point counts interpolate as sampled profiles, without indexing unmatched nodes.
            let mut c = Self {
                version: 1,
                count: MAX_POINTS as u32,
                points: [[0.0; 2]; MAX_POINTS],
                manual: 0,
                handles: [[[0.0; 2]; 2]; MAX_POINTS],
            };
            let sa = a.sampler(Interpolation::Bezier);
            let sb = b.sampler(Interpolation::Bezier);
            for i in 0..MAX_POINTS {
                let x = i as f32 / (MAX_POINTS - 1) as f32;
                c.points[i] = [x, sa.sample(x) * (1.0 - t) + sb.sample(x) * t];
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

    #[test]
    fn reads_original_version_one_bytes_and_roundtrips_edited_handles() {
        #[derive(serde::Serialize)]
        struct OriginalCurve {
            version: u32,
            count: u32,
            points: [[f32; 2]; MAX_POINTS],
        }
        let config = bincode::config::legacy();
        for preset in 1..=5 {
            let original = Curve::preset(preset);
            let bytes = bincode::serde::encode_to_vec(
                OriginalCurve {
                    version: 1,
                    count: original.count,
                    points: original.points,
                },
                config,
            )
            .unwrap();
            assert_eq!(bytes.len(), 136);
            let (mut curve, size): (Curve, usize) =
                bincode::serde::decode_from_slice(&bytes, config).unwrap();
            assert_eq!(size, bytes.len());
            assert_eq!(curve, original);
            assert_eq!(bincode::serde::encode_to_vec(curve, config).unwrap(), bytes);
            curve.move_handle(0, 1, 0.1, 0.7, false);
            let bytes = bincode::serde::encode_to_vec(curve, config).unwrap();
            assert_eq!(bytes.len(), 396);
            let (decoded, size): (Curve, usize) =
                bincode::serde::decode_from_slice(&bytes, config).unwrap();
            assert_eq!(size, bytes.len());
            assert_eq!(decoded, curve);
            assert!(decoded.valid());
            assert!(
                bincode::serde::decode_from_slice::<Curve, _>(&bytes[..bytes.len() - 1], config)
                    .is_err()
            );
        }
    }

    #[test]
    fn bezier_handles_match_analytic_cubic_and_invert_crossed_controls() {
        let mut curve = Curve::preset(1);
        curve.move_handle(0, 1, 1.0 / 3.0, 1.0, true);
        curve.move_handle(1, 0, 2.0 / 3.0, 0.0, true);
        let sampler = curve.sampler(Interpolation::Bezier);
        for i in 0..=1000 {
            let x = i as f32 / 1000.0;
            let expected = 3.0 * x - 6.0 * x * x + 4.0 * x * x * x;
            assert!((sampler.sample(x) - expected).abs() < 1e-6);
        }
        curve.move_handle(0, 1, 1.0, 0.0, true);
        curve.move_handle(1, 0, 0.0, 1.0, true);
        let sampler = curve.sampler(Interpolation::Bezier);
        for i in 0..=1000 {
            let t = i as f64 / 1000.0;
            let x = 3.0 * t - 6.0 * t * t + 4.0 * t * t * t;
            let y = 3.0 * t * t - 2.0 * t * t * t;
            // At the vertical tangent, f32 X quantization amplifies Y error.
            assert!((sampler.sample(x as f32) as f64 - y).abs() < 0.004);
        }
        assert_eq!(sampler.sample(f32::NAN), 0.0);
    }

    #[test]
    fn linked_and_independent_handles_survive_anchor_edits_and_keyframes() {
        let mut a = Curve::preset(1);
        a.insert(0.5, 0.5).unwrap();
        a.move_handle(1, 1, 0.7, 0.8, false);
        let [left, right] = a.handles(1).map(|p| [p[0] - 0.5, p[1] - 0.5]);
        assert!((left[0] * right[1] - left[1] * right[0]).abs() < 1e-7);
        let left = a.handles(1)[0];
        a.move_handle(1, 1, 0.8, 0.3, true);
        assert_eq!(a.handles(1)[0], left);
        let before = a.handles(1);
        let index = a.insert(0.2, 0.1).unwrap();
        assert_eq!(a.handles(2), before);
        a.remove(index);
        assert_eq!(a.handles(1), before);
        let b = Curve::preset(3);
        for t in [0.0, 0.3, 0.7, 1.0] {
            let unequal = a.interpolate(&b, t);
            assert!(unequal.valid());
            let mut same = a;
            same.move_point(1, 0.4, 0.6);
            assert!(a.interpolate(&same, t).valid());
        }
        a.move_point(1, -2.0, 5.0);
        assert!(a.valid());
        for i in 0..=1000 {
            assert!(
                a.sampler(Interpolation::Bezier)
                    .sample(i as f32 / 1000.0)
                    .is_finite()
            );
        }
        a.reset_handles(1);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            assert_eq!(
                a.sampler(Interpolation::Bezier).sample(x),
                a.sample(x, true)
            );
        }
    }

    #[test]
    fn materializing_auto_tangents_preserves_legacy_shapes() {
        for preset in 1..=5 {
            let original = Curve::preset(preset);
            let mut edited = original;
            let p = edited.handles(0)[1];
            edited.move_handle(0, 1, p[0], p[1], true);
            let sampler = edited.sampler(Interpolation::Bezier);
            for i in 0..=1000 {
                let x = i as f32 / 1000.0;
                assert!((sampler.sample(x) - original.sample(x, true)).abs() < 1e-6);
            }
        }
    }
}
