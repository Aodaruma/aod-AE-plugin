use super::*;
#[test]
fn resampling_preserves_constant_alpha_and_colors_exactly() {
    let value = PixelF32 {
        alpha: 1.0,
        red: 0.2,
        green: 0.4,
        blue: 3.0,
    };
    let r = Rotation::new([75.6, -68.4, 17.0]);
    for y in 0..53 {
        for x in 0..107 {
            let d = Projection::Equirectangular
                .direction([(f64::from(x) + 0.5) / 107.0, (f64::from(y) + 0.5) / 53.0])
                .unwrap();
            let p = sample(
                Projection::Equirectangular,
                r.apply(d),
                [257.0, 129.0],
                true,
                &|_, _| value,
            );
            assert_eq!(
                [p.alpha, p.red, p.green, p.blue],
                [value.alpha, value.red, value.green, value.blue]
            );
        }
    }
}
fn close(a: [f64; 3], b: [f64; 3]) {
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() < 1e-9, "{a:?} != {b:?}");
    }
}
fn color(d: [f64; 3]) -> PixelF32 {
    PixelF32 {
        alpha: 1.0,
        red: (d[0] * 0.5 + 0.5) as f32,
        green: (d[1] * 0.5 + 0.5) as f32,
        blue: (d[2] * 0.5 + 0.5) as f32,
    }
}
#[test]
fn projection_roundtrips_all_valid_cells() {
    for i in 1..=5 {
        let p = Projection::from_index(i);
        for y in 0..59 {
            for x in 0..83 {
                let uv = [(f64::from(x) + 0.31) / 83.0, (f64::from(y) + 0.37) / 59.0];
                if let Some(d) = p.direction(uv) {
                    close(p.direction(p.uv(d).unwrap()).unwrap(), d);
                }
            }
        }
    }
}
#[test]
fn axes_and_cube_order_are_explicit() {
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    for (i, d) in directions.into_iter().enumerate() {
        close(
            Projection::Cube6x1
                .direction([(i as f64 + 0.5) / 6.0, 0.5])
                .unwrap(),
            d,
        );
        close(
            Projection::Cube3x2
                .direction([((i % 3) as f64 + 0.5) / 3.0, ((i / 3) as f64 + 0.5) / 2.0])
                .unwrap(),
            d,
        );
    }
    close(
        Projection::Equirectangular.direction([0.5, 0.5]).unwrap(),
        [0.0, 0.0, 1.0],
    );
    close(
        Projection::Equirectangular.direction([0.75, 0.5]).unwrap(),
        [1.0, 0.0, 0.0],
    );
    close(
        Projection::Cross4x3.direction([0.375, 0.5]).unwrap(),
        [0.0, 0.0, 1.0],
    );
}
#[test]
fn cross_holes_and_missing_fisheye_hemisphere_are_transparent() {
    assert!(Projection::Cross4x3.direction([0.1, 0.1]).is_none());
    assert!(Projection::Fisheye.direction([0.01, 0.01]).is_none());
    assert!(Projection::Fisheye.uv([0.0, 0.0, -1.0]).is_none());
    let p = sample(
        Projection::Fisheye,
        [0.0, 0.0, -1.0],
        [100.0, 100.0],
        true,
        &|_, _| panic!("must not read missing hemisphere"),
    );
    assert_eq!(p.alpha, 0.0);
}
#[test]
fn cube_sampling_is_continuous_across_all_edges() {
    for format in [
        Projection::Cube3x2,
        Projection::Cube6x1,
        Projection::Cross4x3,
    ] {
        let grid = format.grid().unwrap();
        let size = [grid[0] * 64, grid[1] * 64];
        let read = |x, y| {
            let uv = [
                (f64::from(x) + 0.5) / f64::from(size[0]),
                (f64::from(y) + 0.5) / f64::from(size[1]),
            ];
            color(format.direction(uv).expect("must never read a cross hole"))
        };
        for face in FACES {
            for step in 0..33 {
                let a = (f64::from(step) + 0.1) / 33.0;
                for uv in [[1e-5, a], [1.0 - 1e-5, a], [a, 1e-5], [a, 1.0 - 1e-5]] {
                    let d = face_direction(face, uv);
                    let p = sample(format, d, size.map(f64::from), true, &read);
                    let expected = color(d);
                    assert!(
                        (p.red - expected.red).abs() < 0.012,
                        "format={format:?} face={face:?} uv={uv:?} d={d:?} got=({}, {}, {}) expected=({}, {}, {})",
                        p.red,
                        p.green,
                        p.blue,
                        expected.red,
                        expected.green,
                        expected.blue
                    );
                    assert!((p.green - expected.green).abs() < 0.012);
                    assert!((p.blue - expected.blue).abs() < 0.012);
                    assert!((p.alpha - 1.0).abs() < 1e-6);
                }
            }
        }
    }
}
#[test]
fn equirectangular_seam_wraps_and_poles_clamp() {
    let read = |x, y| {
        assert!((0..8).contains(&x));
        assert!((0..4).contains(&y));
        PixelF32 {
            alpha: 1.0,
            red: x as f32,
            green: y as f32,
            blue: 0.0,
        }
    };
    let p = sample(
        Projection::Equirectangular,
        [0.0, 0.0, -1.0],
        [8.0, 4.0],
        true,
        &read,
    );
    assert!((p.red - 3.5).abs() < 1e-6);
    let p = sample(
        Projection::Equirectangular,
        [0.0, 1.0, 0.0],
        [8.0, 4.0],
        true,
        &read,
    );
    assert_eq!(p.green, 0.0);
}
#[test]
fn identity_retains_premultiplied_alpha_and_hdr() {
    let read = |x, y| PixelF32 {
        alpha: 0.25,
        red: x as f32 - 10.0,
        green: y as f32 + 3.0,
        blue: 4.0,
    };
    for y in 0..16 {
        for x in 0..32 {
            let d = Projection::Equirectangular
                .direction([(f64::from(x) + 0.5) / 32.0, (f64::from(y) + 0.5) / 16.0])
                .unwrap();
            let p = sample(Projection::Equirectangular, d, [32.0, 16.0], true, &read);
            let expected = read(x, y);
            assert!((p.red - expected.red).abs() < 1e-5);
            assert!((p.green - expected.green).abs() < 1e-5);
            assert_eq!(p.alpha, 0.25);
            assert_eq!(p.blue, 4.0);
        }
    }
}
#[test]
fn bounds_resize_around_original_center() {
    let g = Geometry::new([800, 400], [1.0, 1.0], Projection::Cross4x3, 800.0).unwrap();
    assert_eq!(g.bounds, [0, -100, 800, 500]);
    assert_eq!(g.uv([400.0, 200.0]), [0.5, 0.5]);
    let g = Geometry::new([800, 400], [1.0, 1.0], Projection::Equirectangular, 400.0).unwrap();
    assert_eq!(g.bounds, [200, 100, 600, 300]);
}
#[test]
fn preview_scale_does_not_move_projection() {
    let a = Geometry::new([802, 401], [1.0, 1.0], Projection::Cube3x2, 600.0).unwrap();
    let b = Geometry::new([802, 401], [0.5, 0.25], Projection::Cube3x2, 600.0).unwrap();
    assert_eq!(a.uv([201.0, 123.0]), b.uv([100.5, 30.75]));
    assert_eq!(b.source, [401, 101]);
}
#[test]
fn output_rounding_and_invalid_input() {
    assert_eq!(Projection::Cube3x2.output_size(800.0), Some([801, 534]));
    assert_eq!(Projection::Cube6x1.output_size(1024.0), Some([1026, 171]));
    assert!(Projection::Cube3x2.valid_size([300, 200]));
    assert!(!Projection::Cube3x2.valid_size([301, 200]));
    assert!(!Projection::Fisheye.valid_size([200, 100]));
    for width in [-1.0, f64::NAN, f64::INFINITY, 30001.0] {
        assert!(
            Geometry::new([800, 400], [1.0, 1.0], Projection::Equirectangular, width).is_none()
        );
    }
}
#[test]
fn pan_rotates_sampling_direction_and_preserves_length() {
    close(
        Rotation::new([0.0, 90.0, 0.0]).apply([0.0, 0.0, 1.0]),
        [1.0, 0.0, 0.0],
    );
    let d = Rotation::new([21.0, 32.0, -76.0]).apply(unit([1.0, 2.0, 3.0]));
    assert!((d.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-12);
}
