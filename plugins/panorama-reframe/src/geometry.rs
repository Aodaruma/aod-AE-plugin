//! Camera coordinates: +X right, +Y down, +Z forward; angles in radians.
use std::f64::consts::{FRAC_PI_2, PI};

pub type V3 = [f64; 3];
pub fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn scale(a: V3, s: f64) -> V3 {
    a.map(|v| v * s)
}
pub fn dot(a: V3, b: V3) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    if n > 1e-12 {
        scale(a, 1.0 / n)
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Quat(pub [f64; 4]);
impl Quat {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 0.0]);
    pub fn normalized(self) -> Self {
        let n = self.0.iter().map(|v| v * v).sum::<f64>().sqrt();
        if n.is_finite() && n > 1e-12 {
            Self(self.0.map(|v| v / n))
        } else {
            Self::IDENTITY
        }
    }
    pub fn mul(self, b: Self) -> Self {
        let [w, x, y, z] = self.0;
        let [v, i, j, k] = b.0;
        Self([
            w * v - x * i - y * j - z * k,
            w * i + x * v + y * k - z * j,
            w * j - x * k + y * v + z * i,
            w * k + x * j - y * i + z * v,
        ])
        .normalized()
    }
    pub fn rotate(self, v: V3) -> V3 {
        let [w, x, y, z] = self.0;
        let u = [x, y, z];
        let t = scale(cross(u, v), 2.0);
        add(v, add(scale(t, w), cross(u, t)))
    }
    pub fn from_euler(a: V3) -> Self {
        let [x, y, z] = a.map(|v| (v * 0.5).sin_cos());
        Self([z.1, 0.0, 0.0, z.0])
            .mul(Self([y.1, 0.0, y.0, 0.0]))
            .mul(Self([x.1, x.0, 0.0, 0.0]))
    }
    pub fn euler(self) -> V3 {
        let [w, x, y, z] = self.normalized().0;
        let sin_y = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0);
        if sin_y.abs() > 1.0 - 1e-12 {
            // At gimbal lock choose roll zero while preserving the orientation.
            return [
                (2.0 * (w * x - y * z)).atan2(1.0 - 2.0 * (x * x + z * z)),
                sin_y.signum() * FRAC_PI_2,
                0.0,
            ];
        }
        [
            (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y)),
            sin_y.asin(),
            (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
        ]
    }
    pub fn from_rotation_vector(v: V3) -> Self {
        let angle = dot(v, v).sqrt();
        if angle < 1e-12 {
            return Self::IDENTITY;
        }
        let axis = scale(v, (angle * 0.5).sin() / angle);
        Self([(angle * 0.5).cos(), axis[0], axis[1], axis[2]])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Perspective,
    Fisheye,
    Orthographic,
    Isometric,
}

/// Inverse sampling map of a continuous angular flow on the environment.
/// The first derivative at zero is the constant-depth translational optical flow.
/// Unlike a ray/sphere intersection, this map is defined for every direction.
pub fn environment_warp(direction: V3, position: V3, depth: f64) -> V3 {
    let length = dot(position, position).sqrt();
    if length < 1e-12 {
        return direction;
    }
    let axis = scale(position, 1.0 / length);
    let cosine = dot(direction, axis).clamp(-1.0, 1.0);
    let tangent = add(direction, scale(axis, -cosine));
    let sine = dot(tangent, tangent).sqrt();
    if sine < 1e-12 {
        return direction;
    }
    let strength = length / depth.max(1e-6);
    // exp(-asinh(strength)), avoiding overflow at large displacement.
    let contraction = 1.0 / (strength.hypot(1.0) + strength);
    let half_angle = sine.atan2(cosine) * 0.5;
    let angle = 2.0 * (half_angle.sin() * contraction).atan2(half_angle.cos());
    unit(add(
        scale(axis, angle.cos()),
        scale(tangent, angle.sin() / sine),
    ))
}

/// Integrate the same angular flow through a source-aligned depth field.
/// Exact for constant depth; midpoint sampling follows depth changes along the ray.
/// This is a depth-guided image warp, not a visibility/occlusion reconstruction.
pub fn depth_warp(direction: V3, position: V3, depth_at: impl Fn(V3) -> f64) -> V3 {
    let length = dot(position, position).sqrt();
    if length < 1e-12 {
        return direction;
    }
    let axis = scale(position, 1.0 / length);
    let cosine = dot(direction, axis).clamp(-1.0, 1.0);
    let tangent = add(direction, scale(axis, -cosine));
    let sine = dot(tangent, tangent).sqrt();
    if sine < 1e-12 {
        return direction;
    }
    let half = sine.atan2(cosine) * 0.5;
    let at = |flow: f64| {
        let angle = 2.0 * (half.sin() * (-flow).exp()).atan2(half.cos());
        unit(add(
            scale(axis, angle.cos()),
            scale(tangent, angle.sin() / sine),
        ))
    };
    let mut flow = 0.0;
    // Quadratic spacing resolves the faster initial motion of nearby samples.
    const STEPS: usize = 12;
    for i in 0..STEPS {
        let start = length * (i as f64 / STEPS as f64).powi(2);
        let end = length * ((i + 1) as f64 / STEPS as f64).powi(2);
        let mid = (start + end) * 0.5;
        let depth = depth_at(at(flow)).max(1e-6);
        let half_step = (mid / depth).asinh() - (start / depth).asinh();
        let depth = depth_at(at(flow + half_step)).max(1e-6);
        flow += (end / depth).asinh() - (start / depth).asinh();
    }
    at(flow)
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub projection: Projection,
    pub fov: f64,
    pub ortho_width: f64,
    pub sphere_scale: f64,
    pub radius: f64,
    pub position: V3,
    pub rotation: Quat,
    pub basis: [V3; 3],
    pub aspect: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            projection: Projection::Perspective,
            fov: FRAC_PI_2,
            ortho_width: 1000.0,
            sphere_scale: 1.0,
            radius: 1000.0,
            position: [0.0; 3],
            rotation: Quat::IDENTITY,
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            aspect: 1.0,
        }
    }
}
impl Camera {
    /// Projected sphere radius in centered image coordinates (half the width = 1).
    pub fn sphere_radius(&self) -> Option<f64> {
        let radius = match self.projection {
            Projection::Perspective => return None,
            Projection::Fisheye => 1.0 / self.aspect.max(1.0),
            Projection::Orthographic | Projection::Isometric => {
                2.0 * self.radius / self.ortho_width.max(1.0)
            }
        };
        Some(radius * self.sphere_scale.clamp(0.01, 10.0))
    }
    fn orient(&self, v: V3) -> V3 {
        let v = self.rotation.rotate(v);
        add(
            add(scale(self.basis[0], v[0]), scale(self.basis[1], v[1])),
            scale(self.basis[2], v[2]),
        )
    }
    /// Base direction before translation. Horizontal image edges are at +/-1.
    pub fn direction(&self, x: f64, y: f64) -> Option<V3> {
        // Scale the projected image without changing FOV or parallax distances.
        let scale = if self.projection == Projection::Perspective {
            1.0
        } else {
            self.sphere_scale.clamp(0.01, 10.0)
        };
        let (x, y) = (x / scale, y / scale);
        let mut origin = [0.0; 3];
        let ray = match self.projection {
            Projection::Perspective => {
                let t = (self.fov.clamp(0.01, 179.0_f64.to_radians()) * 0.5).tan();
                self.orient(unit([x * t, y * t, 1.0]))
            }
            Projection::Fisheye => {
                // Equidistant circular lens; the diameter is the shorter image dimension.
                let r = x.hypot(y) * self.aspect.max(1.0);
                if r > 1.0 {
                    return None;
                }
                let theta = r * self.fov.clamp(0.01, 2.0 * PI) * 0.5;
                let n = x.hypot(y);
                self.orient(if n < 1e-12 {
                    [0.0, 0.0, 1.0]
                } else {
                    [x / n * theta.sin(), y / n * theta.sin(), theta.cos()]
                })
            }
            Projection::Orthographic | Projection::Isometric => {
                let iso = if self.projection == Projection::Isometric {
                    Quat::from_euler([35.26438968_f64.to_radians(), PI / 4.0, 0.0])
                } else {
                    Quat::IDENTITY
                };
                origin = add(
                    origin,
                    self.orient(iso.rotate([
                        x * self.ortho_width * 0.5,
                        y * self.ortho_width * 0.5,
                        0.0,
                    ])),
                );
                self.orient(iso.rotate([0.0, 0.0, 1.0]))
            }
        };
        let ray = unit(ray);
        if matches!(
            self.projection,
            Projection::Orthographic | Projection::Isometric
        ) {
            sphere_hit(origin, ray, self.radius).map(unit)
        } else {
            Some(ray)
        }
    }
}

pub fn sphere_hit(origin: V3, direction: V3, radius: f64) -> Option<V3> {
    let b = dot(origin, direction);
    let c = dot(origin, origin) - radius * radius;
    let disc = b * b - c;
    if !disc.is_finite() || disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let near = -b - root;
    let far = -b + root;
    let t = if near > 1e-8 {
        near
    } else if far > 1e-8 {
        far
    } else {
        return None;
    };
    Some(add(origin, scale(direction, t)))
}

pub fn latlong(d: V3) -> [f64; 2] {
    [
        0.5 + d[0].atan2(d[2]) / (2.0 * PI),
        0.5 + d[1].clamp(-1.0, 1.0).asin() / PI,
    ]
}

/// Faces in +X, -X, +Y (down), -Y (up), +Z, -Z order.
pub fn cube_uv(d: V3, equiangular: bool) -> (usize, [f64; 2]) {
    let [x, y, z] = d;
    let [ax, ay, az] = d.map(f64::abs);
    let (face, u, v) = if ax >= ay && ax >= az {
        if x >= 0.0 {
            (0, -z / ax, y / ax)
        } else {
            (1, z / ax, y / ax)
        }
    } else if ay >= az {
        if y >= 0.0 {
            (2, x / ay, -z / ay)
        } else {
            (3, x / ay, z / ay)
        }
    } else if z >= 0.0 {
        (4, x / az, y / az)
    } else {
        (5, -x / az, y / az)
    };
    let uv = [u, v].map(|t| {
        0.5 + 0.5
            * if equiangular {
                t.atan() / (PI / 4.0)
            } else {
                t
            }
    });
    (face, uv)
}
pub fn cube_direction(face: usize, uv: [f64; 2], equiangular: bool) -> V3 {
    let [u, v] = uv.map(|t| {
        let p = 2.0 * t - 1.0;
        if equiangular { (p * PI / 4.0).tan() } else { p }
    });
    unit(match face {
        0 => [1.0, v, -u],
        1 => [-1.0, v, u],
        2 => [u, 1.0, -v],
        3 => [u, -1.0, v],
        4 => [u, v, 1.0],
        _ => [-u, v, -1.0],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: V3, b: V3) {
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1e-7, "{a:?} != {b:?}");
        }
    }
    #[test]
    fn quaternion_round_trip_and_sign() {
        for a in [[0.2, 0.4, -0.8], [-1.0, 1.2, 2.0], [0.0; 3]] {
            let q = Quat::from_euler(a);
            close(q.euler(), a);
            close(
                q.rotate([1.0, 2.0, 3.0]),
                Quat(q.0.map(|x| -x)).rotate([1.0, 2.0, 3.0]),
            );
        }
        close(
            Quat([0.0; 4]).normalized().rotate([1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0],
        );
    }
    #[test]
    fn cube_round_trip_all_faces_and_eac() {
        for eac in [false, true] {
            for f in 0..6 {
                for uv in [[0.5, 0.5], [0.01, 0.99], [0.87, 0.16]] {
                    let d = cube_direction(f, uv, eac);
                    let (actual, p) = cube_uv(d, eac);
                    assert_eq!(actual, f);
                    assert!((p[0] - uv[0]).abs() < 1e-9 && (p[1] - uv[1]).abs() < 1e-9);
                }
            }
        }
    }
    #[test]
    fn perspective_edges_and_rotation() {
        let c = Camera::default();
        close(c.direction(0.0, 0.0).unwrap(), [0.0, 0.0, 1.0]);
        close(c.direction(1.0, 0.0).unwrap(), unit([1.0, 0.0, 1.0]));
        let c = Camera {
            rotation: Quat::from_euler([0.0, FRAC_PI_2, 0.0]),
            ..c
        };
        close(c.direction(0.0, 0.0).unwrap(), [1.0, 0.0, 0.0]);
    }
    #[test]
    fn sphere_position_and_misses() {
        close(
            sphere_hit([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0).unwrap(),
            [1.0, 0.0, 3.0_f64.sqrt()],
        );
        assert!(sphere_hit([3.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0).is_none());
        assert!(sphere_hit([0.0, 0.0, 3.0], [0.0, 0.0, 1.0], 2.0).is_none());
    }
    #[test]
    fn environment_translation_is_reversible_and_has_no_sphere_boundary() {
        for position in [[0.5, -0.2, 0.3], [4.0, 2.0, -3.0], [0.0, 0.0, 20.0]] {
            for direction in [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.3, -0.6, 0.5]] {
                let direction = unit(direction);
                let warped = environment_warp(direction, position, 1.0);
                assert!(warped.iter().all(|v| v.is_finite()));
                close(
                    environment_warp(warped, scale(position, -1.0), 1.0),
                    direction,
                );
            }
            let camera = Camera {
                position,
                radius: 1.0,
                ..Camera::default()
            };
            assert!(camera.direction(0.0, 0.0).is_some());
            assert!(camera.direction(-1.0, 1.0).is_some());
        }
    }
    #[test]
    fn environment_warp_matches_small_translation_optical_flow() {
        let direction = unit([0.3, -0.2, 1.0]);
        let position = [1e-5, 2e-5, -3e-5];
        let expected = unit(add(direction, position));
        close(environment_warp(direction, position, 1.0), expected);
        close(environment_warp(direction, [0.0; 3], 1.0), direction);
    }
    #[test]
    fn constant_depth_matches_environment_at_all_distances() {
        for direction in [[0.0, 0.0, 1.0], [0.3, -0.8, -0.2], [-1.0, 0.0, 0.0]] {
            let direction = unit(direction);
            for position in [
                [0.0; 3],
                [500.0, 100.0, -250.0],
                [30000.0, -20000.0, 10000.0],
            ] {
                for depth in [1.0, 1000.0, 10000.0, 1000000.0] {
                    close(
                        depth_warp(direction, position, |_| depth),
                        environment_warp(direction, position, depth),
                    );
                }
            }
        }
    }
    #[test]
    fn spatial_depth_gives_near_regions_more_parallax() {
        let depth = |d: V3| if d[1] < 0.0 { 1000.0 } else { 10000.0 };
        let position = [500.0, 0.0, 0.0];
        let near = depth_warp(unit([0.0, -0.5, 1.0]), position, depth);
        let far = depth_warp(unit([0.0, 0.5, 1.0]), position, depth);
        assert!(near[0] > far[0] * 5.0);
        for y in [-1.0, -0.01, 0.0, 0.01, 1.0] {
            let warped = depth_warp(unit([0.5, y, 1.0]), [30000.0; 3], depth);
            assert!(warped.iter().all(|v| v.is_finite()));
            assert!((dot(warped, warped) - 1.0).abs() < 1e-8);
        }
    }
    #[test]
    fn fisheye_mask_and_orthographic_extent() {
        let c = Camera {
            projection: Projection::Fisheye,
            fov: PI,
            ..Camera::default()
        };
        assert!(c.direction(1.1, 0.0).is_none());
        close(c.direction(1.0, 0.0).unwrap(), [1.0, 0.0, 0.0]);
        let c = Camera {
            projection: Projection::Orthographic,
            ortho_width: 4000.0,
            ..c
        };
        assert!(c.direction(1.0, 0.0).is_none());
    }
    #[test]
    fn sphere_scale_changes_extent_without_changing_directions() {
        for projection in [
            Projection::Fisheye,
            Projection::Orthographic,
            Projection::Isometric,
        ] {
            for aspect in [0.5, 1.0, 2.0] {
                let base = Camera {
                    projection,
                    aspect,
                    ortho_width: 4000.0,
                    ..Camera::default()
                };
                let radius = base.sphere_radius().unwrap();
                for factor in [0.01, 0.5, 2.0, 10.0] {
                    let scaled = Camera {
                        sphere_scale: factor,
                        ..base
                    };
                    let extent = scaled.sphere_radius().unwrap();
                    assert!((extent - radius * factor).abs() < 1e-10);
                    assert!(scaled.direction(extent * 1.01, 0.0).is_none());
                    for [x, y] in [[0.0, 0.0], [radius * 0.3, radius * -0.4]] {
                        close(
                            scaled.direction(x * factor, y * factor).unwrap(),
                            base.direction(x, y).unwrap(),
                        );
                    }
                }
            }
        }
        let base = Camera::default();
        let scaled = Camera {
            sphere_scale: 0.5,
            ..base
        };
        assert!(scaled.sphere_radius().is_none());
        close(
            scaled.direction(0.9, 0.4).unwrap(),
            base.direction(0.9, 0.4).unwrap(),
        );
    }
    #[test]
    fn drag_rotation_continues_past_multiple_turns_and_reverses() {
        let step = [0.0, 0.05, 0.0];
        let mut q = Quat::IDENTITY;
        for i in 1..=300 {
            q = Quat::from_rotation_vector(step).mul(q);
            close(
                q.rotate([0.0, 0.0, 1.0]),
                [f64::sin(i as f64 * 0.05), 0.0, f64::cos(i as f64 * 0.05)],
            );
        }
        for _ in 0..300 {
            q = Quat::from_rotation_vector(scale(step, -1.0)).mul(q);
        }
        close(q.rotate([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]);
    }
    #[test]
    fn euler_conversion_preserves_gimbal_lock_orientation() {
        for y in [-FRAC_PI_2, FRAC_PI_2] {
            let q = Quat::from_euler([0.7, y, -0.3]);
            let back = Quat::from_euler(q.euler());
            for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
                close(q.rotate(axis), back.rotate(axis));
            }
        }
    }
}
