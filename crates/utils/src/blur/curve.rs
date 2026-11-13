//! Normalized shutter opening over exposure time. No Blender code is used.
use serde::{Deserialize, Serialize};

pub const TABLE_SIZE: usize = 256;
pub const MAX_POINTS: usize = 16;
const GAP: f32 = 0.002;

#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ShutterCurve {
    pub points: Vec<[f32; 2]>,
    pub smooth: bool,
}
impl Default for ShutterCurve {
    fn default() -> Self {
        Self {
            points: vec![[0., 1.], [1., 1.]],
            smooth: true,
        }
    }
}
impl ShutterCurve {
    pub fn preset(index: u32) -> Self {
        let points = match index {
            1 => vec![[0., 0.], [0.5, 1.], [1., 0.]],
            2 => (0..=8)
                .map(|i| {
                    let x = i as f32 / 8.;
                    [x, (std::f32::consts::PI * x).sin().powi(2)]
                })
                .collect(),
            3 => vec![[0., 0.], [1., 1.]],
            4 => vec![[0., 1.], [1., 0.]],
            _ => return Self::default(),
        };
        Self {
            points,
            smooth: index != 1,
        }
    }
    pub fn sanitized(&self) -> Self {
        let mut points: Vec<_> = self
            .points
            .iter()
            .copied()
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .map(|p| [p[0].clamp(0., 1.), p[1].clamp(0., 1.)])
            .collect();
        points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        points.dedup_by(|b, a| b[0] - a[0] < GAP);
        points.truncate(MAX_POINTS);
        if points.len() < 2 {
            return Self::default();
        }
        points[0][0] = 0.;
        points.last_mut().unwrap()[0] = 1.;
        Self {
            points,
            smooth: self.smooth,
        }
    }
    /// PCHIP slopes prevent overshoot between nonnegative opening values.
    fn slope(&self, i: usize) -> f32 {
        let n = self.points.len();
        let secant = |j: usize| {
            (self.points[j + 1][1] - self.points[j][1])
                / (self.points[j + 1][0] - self.points[j][0])
        };
        if i == 0 {
            return secant(0);
        }
        if i == n - 1 {
            return secant(n - 2);
        }
        let a = secant(i - 1);
        let b = secant(i);
        if a * b <= 0. {
            return 0.;
        }
        let left = self.points[i][0] - self.points[i - 1][0];
        let right = self.points[i + 1][0] - self.points[i][0];
        let w1 = 2. * right + left;
        let w2 = right + 2. * left;
        (w1 + w2) / (w1 / a + w2 / b)
    }
    /// Requires sanitized, ordered control points.
    pub fn evaluate(&self, time: f32) -> f32 {
        let x = time.clamp(0., 1.);
        let i = self
            .points
            .partition_point(|p| p[0] <= x)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let [x0, y0] = self.points[i];
        let [x1, y1] = self.points[i + 1];
        let h = x1 - x0;
        let t = (x - x0) / h;
        if !self.smooth {
            return y0 + (y1 - y0) * t;
        }
        let t2 = t * t;
        let t3 = t2 * t;
        ((2. * t3 - 3. * t2 + 1.) * y0
            + (t3 - 2. * t2 + t) * h * self.slope(i)
            + (-2. * t3 + 3. * t2) * y1
            + (t3 - t2) * h * self.slope(i + 1))
        .clamp(0., 1.)
    }
    pub fn table(&self) -> [f32; TABLE_SIZE] {
        let curve = self.sanitized();
        let table = std::array::from_fn(|i| curve.evaluate(i as f32 / (TABLE_SIZE - 1) as f32));
        // A zero-area opening has no normalized exposure distribution.
        if table.iter().all(|&v| v <= 1e-8) {
            [1.; TABLE_SIZE]
        } else {
            table
        }
    }
    pub fn insert(&mut self, point: [f32; 2]) -> Option<usize> {
        if self.points.len() >= MAX_POINTS {
            return None;
        }
        let x = point[0].clamp(GAP, 1. - GAP);
        if self.points.iter().any(|p| (p[0] - x).abs() < GAP) {
            return None;
        }
        let i = self.points.partition_point(|p| p[0] < x);
        self.points.insert(i, [x, point[1].clamp(0., 1.)]);
        Some(i)
    }
    pub fn move_point(&mut self, i: usize, point: [f32; 2]) {
        if i >= self.points.len() {
            return;
        }
        let x = if i == 0 {
            0.
        } else if i == self.points.len() - 1 {
            1.
        } else {
            point[0].clamp(self.points[i - 1][0] + GAP, self.points[i + 1][0] - GAP)
        };
        self.points[i] = [x, point[1].clamp(0., 1.)];
    }
    pub fn remove(&mut self, i: usize) {
        if i > 0 && i + 1 < self.points.len() {
            self.points.remove(i);
        }
    }
}
impl after_effects::ArbitraryData<Self> for ShutterCurve {
    fn interpolate(&self, other: &Self, amount: f64) -> Self {
        let a = self.sanitized();
        let b = other.sanitized();
        let t = amount.clamp(0., 1.) as f32;
        if t == 0. {
            return a;
        }
        if t == 1. {
            return b;
        }
        if a.points.len() == b.points.len() {
            return Self {
                points: a
                    .points
                    .iter()
                    .zip(&b.points)
                    .map(|(p, q)| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t])
                    .collect(),
                smooth: if t < 0.5 { a.smooth } else { b.smooth },
            }
            .sanitized();
        }
        Self {
            points: (0..MAX_POINTS)
                .map(|i| {
                    let x = i as f32 / (MAX_POINTS - 1) as f32;
                    [x, a.evaluate(x) * (1. - t) + b.evaluate(x) * t]
                })
                .collect(),
            smooth: true,
        }
    }
}
pub fn sample(table: &[f32; TABLE_SIZE], t: f32) -> f32 {
    let x = t.clamp(0., 1.) * (TABLE_SIZE - 1) as f32;
    let a = x.floor() as usize;
    let b = (a + 1).min(TABLE_SIZE - 1);
    table[a] + (table[b] - table[a]) * (x - a as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use after_effects::ArbitraryData;
    #[test]
    fn curve_editing_and_interpolation_stay_ordered_and_bounded() {
        let mut curve = ShutterCurve::default();
        let i = curve.insert([0.3, 0.1]).unwrap();
        curve.move_point(i, [2., -3.]);
        assert!(curve.points[i][0] < 1.);
        assert_eq!(curve.points[i][1], 0.);
        let interpolated = curve.interpolate(&ShutterCurve::preset(2), 0.4);
        for c in [curve.sanitized(), interpolated] {
            assert!(c.points.windows(2).all(|p| p[0][0] < p[1][0]));
            for x in 0..1000 {
                assert!((0. ..=1.).contains(&c.evaluate(x as f32 / 999.)));
            }
        }
        curve.remove(0);
        assert_eq!(curve.points.len(), 3);
        curve.remove(i);
        assert_eq!(curve, ShutterCurve::default());
    }
    #[test]
    fn malformed_and_closed_curves_have_finite_safe_tables() {
        for points in [
            vec![],
            vec![[f32::NAN, 1.]],
            vec![[0., 0.], [1., 0.]],
            vec![[1., 1.], [0., 1.], [0., 0.]],
        ] {
            let table = ShutterCurve {
                points,
                smooth: true,
            }
            .table();
            assert!(table.iter().all(|v| v.is_finite() && *v >= 0. && *v <= 1.));
            assert!(table.iter().sum::<f32>() > 0.);
        }
        assert_eq!(ShutterCurve::default().table(), [1.; TABLE_SIZE]);
        let bell = ShutterCurve::preset(2).table();
        assert_eq!(bell[0], 0.);
        assert!(sample(&bell, 0.5) > 0.99);
    }
}
