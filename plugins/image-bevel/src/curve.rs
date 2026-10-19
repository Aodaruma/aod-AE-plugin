use serde::{Deserialize, Serialize};

pub const MAX_POINTS: usize = 16;
const GAP: f32 = 0.002;

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
}
