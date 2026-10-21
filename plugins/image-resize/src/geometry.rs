#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub fn width(self) -> i32 {
        (self.right - self.left).max(0)
    }
    pub fn height(self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    pub fn intersection(self, other: Self) -> Self {
        let b = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        if b.width() == 0 || b.height() == 0 {
            Self::default()
        } else {
            b
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub percent: bool,
    pub all: f64,
    // Left, top, right, bottom: positive values inset each edge.
    pub sides: [f64; 4],
    pub angle: f64,
    pub output_size: Option<[f64; 2]>,
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub scale: [f64; 2],
    pub bounds: Bounds,
    size: [f64; 2],
    crop: [f64; 4],
    par: f64,
    sin: f64,
    cos: f64,
    output_crop: Option<[f64; 4]>,
}

impl Geometry {
    pub fn new(size: [f64; 2], scale: [f64; 2], par: f64, s: Settings) -> Option<Self> {
        if size
            .into_iter()
            .chain(scale)
            .chain([par])
            .any(|v| !v.is_finite() || v <= 0.0)
            || s.sides
                .into_iter()
                .chain([s.all, s.angle])
                .any(|v| !v.is_finite())
        {
            return None;
        }
        let output_size = match s.output_size {
            Some(size)
                if size
                    .iter()
                    .any(|v| !v.is_finite() || *v < 1.0 || *v > 30000.0) =>
            {
                return None;
            }
            Some(size) => Some(size.map(f64::round)),
            None => None,
        };
        let amount = s.all.clamp(0.0, 100.0) * 0.01;
        let unit = if s.percent {
            [size[0] * 0.01, size[1] * 0.01]
        } else {
            [1.0; 2]
        };
        let sides = std::array::from_fn::<_, 4, _>(|i| s.sides[i] * unit[i % 2] * amount);
        let crop = [sides[0], sides[1], size[0] - sides[2], size[1] - sides[3]];
        let (sin, cos) = s.angle.rem_euclid(360.0).to_radians().sin_cos();
        let mut g = Self {
            size,
            scale,
            crop,
            par,
            sin,
            cos,
            output_crop: None,
            bounds: Bounds::default(),
        };
        if output_size.is_none() && (crop[0] >= crop[2] || crop[1] >= crop[3]) {
            return Some(g);
        }
        let points = [
            [crop[0], crop[1]],
            [crop[2], crop[1]],
            [crop[0], crop[3]],
            [crop[2], crop[3]],
        ]
        .map(|p| g.forward(p));
        let min_x = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let min_y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let mut limits = [
            snap(min_x).floor(),
            snap(min_y).floor(),
            snap(max_x).ceil(),
            snap(max_y).ceil(),
        ];
        if let Some(target_extent) = output_size {
            let center = g.forward([(crop[0] + crop[2]) * 0.5, (crop[1] + crop[3]) * 0.5]);
            // Align in full-resolution coordinates so odd sizes and reduced
            // previews retain the same placement and exact full-resolution size.
            let target_min: [f64; 2] = std::array::from_fn(|i| {
                snap(center[i] / scale[i] - target_extent[i] * 0.5).floor()
            });
            let crop = [
                target_min[0] * scale[0],
                target_min[1] * scale[1],
                (target_min[0] + target_extent[0]) * scale[0],
                (target_min[1] + target_extent[1]) * scale[1],
            ];
            g.output_crop = Some(crop);
            limits = [
                snap(crop[0]).floor(),
                snap(crop[1]).floor(),
                snap(crop[2]).ceil(),
                snap(crop[3]).ceil(),
            ];
        }
        if limits
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
            || limits[2] - limits[0] > 30000.0
            || limits[3] - limits[1] > 30000.0
        {
            return None;
        }
        g.bounds = Bounds {
            left: limits[0] as i32,
            top: limits[1] as i32,
            right: limits[2] as i32,
            bottom: limits[3] as i32,
        };
        Some(g)
    }

    pub fn source_bounds(self) -> Bounds {
        Bounds {
            left: 0,
            top: 0,
            right: (self.size[0] * self.scale[0]).ceil() as i32,
            bottom: (self.size[1] * self.scale[1]).ceil() as i32,
        }
    }

    fn forward(self, p: [f64; 2]) -> [f64; 2] {
        let dx = (p[0] - self.size[0] * 0.5) * self.par;
        let dy = p[1] - self.size[1] * 0.5;
        let p = [
            (self.cos * dx - self.sin * dy) / self.par + self.size[0] * 0.5,
            self.sin * dx + self.cos * dy + self.size[1] * 0.5,
        ];
        [p[0] * self.scale[0], p[1] * self.scale[1]]
    }

    pub fn inverse(self, p: [f64; 2]) -> [f64; 2] {
        let p = [p[0] / self.scale[0], p[1] / self.scale[1]];
        let dx = (p[0] - self.size[0] * 0.5) * self.par;
        let dy = p[1] - self.size[1] * 0.5;
        [
            (self.cos * dx + self.sin * dy) / self.par + self.size[0] * 0.5,
            -self.sin * dx + self.cos * dy + self.size[1] * 0.5,
        ]
    }

    pub fn coverage(self, p: [f64; 2]) -> f64 {
        if self.bounds.width() == 0 || self.bounds.height() == 0 {
            return 0.0;
        }
        if let Some(crop) = self.output_crop {
            // Fixed dimensions change only the output window, never sampling.
            let overlap = |i: usize| {
                ((p[i] + 0.5).min(crop[i + 2]) - (p[i] - 0.5).max(crop[i])).clamp(0.0, 1.0)
            };
            return overlap(0) * overlap(1);
        }
        let local = self.inverse(p);
        let sample_scale = self.scale.map(f64::recip);
        let half = [
            0.5 * (self.cos.abs() * sample_scale[0] + self.sin.abs() * sample_scale[1] / self.par),
            0.5 * (self.sin.abs() * self.par * sample_scale[0] + self.cos.abs() * sample_scale[1]),
        ];
        if local[0] + half[0] <= self.crop[0]
            || local[0] - half[0] >= self.crop[2]
            || local[1] + half[1] <= self.crop[1]
            || local[1] - half[1] >= self.crop[3]
        {
            return 0.0;
        }
        if local[0] - half[0] >= self.crop[0]
            && local[0] + half[0] <= self.crop[2]
            && local[1] - half[1] >= self.crop[1]
            && local[1] + half[1] <= self.crop[3]
        {
            return 1.0;
        }
        if self.sin.abs() < 1.0e-12 {
            let overlap = |i: usize| {
                ((local[i] + half[i]).min(self.crop[i + 2])
                    - (local[i] - half[i]).max(self.crop[i]))
                .max(0.0)
                    / (2.0 * half[i])
            };
            return overlap(0) * overlap(1);
        }
        // Supersample only boundary pixels; interior pixels retain sharp sampling.
        let mut count = 0;
        for y in 0..4 {
            for x in 0..4 {
                let q = self.inverse([
                    p[0] + (f64::from(x) + 0.5) / 4.0 - 0.5,
                    p[1] + (f64::from(y) + 0.5) / 4.0 - 0.5,
                ]);
                if q[0] >= self.crop[0]
                    && q[0] < self.crop[2]
                    && q[1] >= self.crop[1]
                    && q[1] < self.crop[3]
                {
                    count += 1;
                }
            }
        }
        f64::from(count) / 16.0
    }
}

// Remove trigonometric roundoff at quarter turns, not subpixel cuts.
fn snap(v: f64) -> f64 {
    if (v - v.round()).abs() < 1.0e-9 {
        v.round()
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            percent: true,
            all: 100.0,
            sides: [0.0; 4],
            angle: 0.0,
            output_size: None,
        }
    }

    #[test]
    fn percent_uses_original_opposite_edge_on_each_axis() {
        let g = Geometry::new(
            [200.0, 100.0],
            [1.0; 2],
            1.0,
            Settings {
                sides: [25.0, 10.0, 20.0, 30.0],
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(
            g.bounds,
            Bounds {
                left: 50,
                top: 10,
                right: 160,
                bottom: 70
            }
        );
    }

    #[test]
    fn all_interpolates_both_crop_and_extension() {
        for (all, expected) in [
            (0.0, [0, 0, 200, 100]),
            (50.0, [-10, 5, 170, 120]),
            (100.0, [-20, 10, 140, 140]),
        ] {
            let g = Geometry::new(
                [200.0, 100.0],
                [1.0; 2],
                1.0,
                Settings {
                    percent: false,
                    all,
                    sides: [-20.0, 10.0, 60.0, -40.0],
                    ..settings()
                },
            )
            .unwrap();
            assert_eq!(
                g.bounds,
                Bounds {
                    left: expected[0],
                    top: expected[1],
                    right: expected[2],
                    bottom: expected[3]
                }
            );
        }
    }

    #[test]
    fn collapsed_or_crossed_edges_are_empty() {
        for right in [50.0, 75.0] {
            let g = Geometry::new(
                [200.0, 100.0],
                [1.0; 2],
                1.0,
                Settings {
                    sides: [50.0, 0.0, right, 0.0],
                    ..settings()
                },
            )
            .unwrap();
            assert_eq!(g.bounds, Bounds::default());
            assert_eq!(g.coverage([100.5, 50.5]), 0.0);
        }
    }

    #[test]
    fn rotation_is_independent_of_all_and_expands_bounds() {
        let g = Geometry::new(
            [200.0, 100.0],
            [1.0; 2],
            1.0,
            Settings {
                all: 0.0,
                sides: [50.0; 4],
                angle: 90.0,
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(
            g.bounds,
            Bounds {
                left: 50,
                top: -50,
                right: 150,
                bottom: 150
            }
        );
        let q = g.inverse([100.0, -50.0]);
        assert!((q[0]).abs() < 1.0e-9 && (q[1] - 50.0).abs() < 1.0e-9);
    }

    #[test]
    fn downsample_and_pixel_aspect_preserve_physical_rotation() {
        let g = Geometry::new(
            [200.0, 100.0],
            [0.5, 0.25],
            2.0,
            Settings {
                angle: 90.0,
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(
            g.bounds,
            Bounds {
                left: 37,
                top: -38,
                right: 63,
                bottom: 63
            }
        );
        let p = [23.25, 37.75];
        let q = g.inverse(g.forward(p));
        assert!((q[0] - p[0]).abs() < 1.0e-9 && (q[1] - p[1]).abs() < 1.0e-9);
    }

    #[test]
    fn fractional_crop_has_partial_alpha() {
        let g = Geometry::new(
            [10.0; 2],
            [1.0; 2],
            1.0,
            Settings {
                percent: false,
                sides: [0.25, 0.0, 0.0, 0.0],
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(g.coverage([0.5, 5.5]), 0.75);
        assert_eq!(g.coverage([1.5, 5.5]), 1.0);
        assert_eq!(g.coverage([-0.5, 5.5]), 0.0);
    }

    #[test]
    fn bounds_are_independent_of_requested_roi() {
        let g = Geometry::new(
            [200.0, 100.0],
            [1.0; 2],
            1.0,
            Settings {
                sides: [-50.0; 4],
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(
            g.bounds.intersection(Bounds {
                left: -120,
                top: -75,
                right: 0,
                bottom: 0
            }),
            Bounds {
                left: -100,
                top: -50,
                right: 0,
                bottom: 0
            }
        );
        assert_eq!(
            g.bounds.intersection(Bounds {
                left: 400,
                top: 400,
                right: 410,
                bottom: 410
            }),
            Bounds::default()
        );
    }

    #[test]
    fn invalid_or_excessive_geometry_is_rejected() {
        for sides in [[f64::NAN; 4], [-10000.0; 4]] {
            assert!(
                Geometry::new(
                    [1000.0; 2],
                    [1.0; 2],
                    1.0,
                    Settings {
                        sides,
                        ..settings()
                    }
                )
                .is_none()
            );
        }
    }

    #[test]
    fn exact_size_crops_or_extends_without_scaling_or_translation() {
        for (extent, expected) in [
            ([40.0, 20.0], [20, 10, 60, 30]),
            ([120.0, 60.0], [-20, -10, 100, 50]),
            ([41.2, 21.4], [19, 9, 60, 30]),
        ] {
            let g = Geometry::new(
                [80.0, 40.0],
                [1.0; 2],
                1.0,
                Settings {
                    output_size: Some(extent),
                    ..settings()
                },
            )
            .unwrap();
            assert_eq!(
                g.bounds,
                Bounds {
                    left: expected[0],
                    top: expected[1],
                    right: expected[2],
                    bottom: expected[3]
                }
            );
            for p in [[0.5, 0.5], [21.5, 11.5], [79.5, 39.5]] {
                assert_eq!(g.inverse(p), p);
                assert_eq!(g.forward(p), p);
            }
            assert_eq!(
                g.coverage([f64::from(expected[0]) + 0.5, f64::from(expected[1]) + 0.5]),
                1.0
            );
            assert_eq!(
                g.coverage([f64::from(expected[0]) - 0.5, f64::from(expected[1]) + 0.5]),
                0.0
            );
        }
    }

    #[test]
    fn exact_size_keeps_adjusted_center_and_overrides_crossed_edges() {
        for (all, center) in [(0.0, 40), (50.0, 45), (100.0, 50)] {
            let g = Geometry::new(
                [80.0, 40.0],
                [1.0; 2],
                1.0,
                Settings {
                    all,
                    sides: [25.0, 0.0, 0.0, 0.0],
                    output_size: Some([40.0, 20.0]),
                    ..settings()
                },
            )
            .unwrap();
            assert_eq!(
                g.bounds,
                Bounds {
                    left: center - 20,
                    top: 10,
                    right: center + 20,
                    bottom: 30
                }
            );
        }
        let g = Geometry::new(
            [80.0, 40.0],
            [1.0; 2],
            1.0,
            Settings {
                sides: [100.0; 4],
                output_size: Some([120.0, 60.0]),
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(g.bounds.width(), 120);
        assert_eq!(g.bounds.height(), 60);
        assert_eq!(g.coverage([0.5, 0.5]), 1.0);
    }

    #[test]
    fn exact_size_after_rotation_preserves_sampling_and_pixel_aspect() {
        for angle in [0.0, 27.0, 90.0] {
            let s = Settings {
                angle,
                sides: [25.0, 0.0, 0.0, 0.0],
                ..settings()
            };
            let original = Geometry::new([80.0, 40.0], [1.0; 2], 2.0, s).unwrap();
            let fixed = Geometry::new(
                [80.0, 40.0],
                [1.0; 2],
                2.0,
                Settings {
                    output_size: Some([120.0, 20.0]),
                    ..s
                },
            )
            .unwrap();
            assert_eq!(fixed.bounds.width(), 120);
            assert_eq!(fixed.bounds.height(), 20);
            for p in [[-10.5, 50.5], [25.5, 19.5]] {
                assert_eq!(fixed.inverse(p), original.inverse(p));
            }
        }
    }

    #[test]
    fn exact_size_downsample_preserves_subpixel_bounds() {
        let g = Geometry::new(
            [80.0, 40.0],
            [0.5; 2],
            1.0,
            Settings {
                output_size: Some([41.0, 21.0]),
                ..settings()
            },
        )
        .unwrap();
        assert_eq!(
            g.bounds,
            Bounds {
                left: 9,
                top: 4,
                right: 30,
                bottom: 15
            }
        );
        assert_eq!(g.coverage([9.5, 4.5]), 0.25);
        assert_eq!(g.coverage([10.5, 5.5]), 1.0);
        assert_eq!(g.inverse([10.5, 5.5]), [21.0, 11.0]);
    }

    #[test]
    fn invalid_exact_sizes_are_rejected() {
        for dimension in [f64::NAN, f64::INFINITY, -1.0, 0.0, 30001.0] {
            assert!(
                Geometry::new(
                    [80.0, 40.0],
                    [1.0; 2],
                    1.0,
                    Settings {
                        output_size: Some([dimension, 20.0]),
                        ..settings()
                    }
                )
                .is_none()
            );
        }
    }
}
