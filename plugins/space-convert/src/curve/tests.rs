use super::*;
use crate::tests::test_settings;

fn line() -> CurvePath {
    CurvePath::new(
        &[((10.0, 30.0), (1.0, 0.0)), ((110.0, 30.0), (1.0, 0.0))],
        false,
    )
    .unwrap()
}

fn circle() -> CurvePath {
    let points: Vec<_> = (0..=1024)
        .map(|i| {
            let theta = i as f32 / 1024.0 * TAU;
            (
                (100.0 + 40.0 * theta.cos(), 100.0 + 40.0 * theta.sin()),
                (-theta.sin(), theta.cos()),
            )
        })
        .collect();
    CurvePath::new(&points, true).unwrap()
}

fn close(a: Point, b: Point, tolerance: f32) {
    assert!((a.0 - b.0).hypot(a.1 - b.1) < tolerance, "{a:?} != {b:?}");
}

#[test]
fn straight_line_round_trip_with_fit_offsets_and_flips() {
    let path = line();
    for fit in [Fit::Fill, Fit::Contain, Fit::Cover] {
        for reverse in [false, true] {
            for flip_normal in [false, true] {
                let mapping = Mapping::new(
                    &path,
                    CurveSettings {
                        fit,
                        reverse,
                        flip_normal,
                        width: 20.0,
                        offset: 0.1,
                        normal_offset: 5.0,
                        ..CurveSettings::default()
                    },
                    (200.0, 100.0),
                );
                let rect = (90.0, 55.0);
                let curved = mapping.to_curve(rect).unwrap();
                close(mapping.to_rect(curved).unwrap(), rect, 1.0e-4);
            }
        }
    }
}

#[test]
fn contain_letterboxes_cover_crops_and_fill_stretches() {
    let path = line();
    let settings = CurveSettings {
        width: 20.0,
        ..CurveSettings::default()
    };
    let fill = Mapping::new(&path, settings, (200.0, 100.0));
    close(fill.to_curve((0.0, 0.0)).unwrap(), (10.0, 20.0), 1.0e-4);
    close(
        fill.to_curve((200.0, 100.0)).unwrap(),
        (110.0, 40.0),
        1.0e-4,
    );
    let contain = Mapping::new(
        &path,
        CurveSettings {
            fit: Fit::Contain,
            ..settings
        },
        (200.0, 100.0),
    );
    assert!(contain.to_curve((100.0, 0.0)).is_none());
    close(contain.to_rect((10.0, 20.0)).unwrap(), (0.0, 30.0), 1.0e-4);
    assert_eq!(contain.scale.0, contain.scale.1);
    let cover = Mapping::new(
        &path,
        CurveSettings {
            fit: Fit::Cover,
            ..settings
        },
        (200.0, 100.0),
    );
    close(cover.to_curve((0.0, 0.0)).unwrap(), (40.0, 20.0), 1.0e-4);
    assert!(cover.to_rect((10.0, 30.0)).is_none());
    assert_eq!(cover.scale.0, cover.scale.1);
}

#[test]
fn closed_path_wraps_only_when_enabled_and_open_paths_never_wrap() {
    let path = circle();
    let settings = CurveSettings {
        width: 20.0,
        offset: 0.25,
        ..CurveSettings::default()
    };
    let looped = Mapping::new(&path, settings, (200.0, 100.0));
    assert!(looped.to_curve((160.0, 50.0)).is_some());
    assert!(looped.to_curve((-40.0, 50.0)).is_none());
    // The shifted end of one circuit must match the beginning exactly.
    close(
        looped.to_curve((0.0, 50.0)).unwrap(),
        looped.to_curve((200.0, 50.0)).unwrap(),
        1.0e-4,
    );
    let cut = Mapping::new(
        &path,
        CurveSettings {
            loop_path: false,
            ..settings
        },
        (200.0, 100.0),
    );
    assert!(cut.to_curve((160.0, 50.0)).is_none());
    assert!(cut.to_curve((100.0, 50.0)).is_some());
    let open = line();
    let open = Mapping::new(&open, settings, (200.0, 100.0));
    assert!(open.to_curve((160.0, 50.0)).is_none());
    assert!(open.to_rect((5.0, 30.0)).is_none());
    assert!(open.to_rect((115.0, 30.0)).is_none());
}

#[test]
fn curved_band_round_trips_away_from_singularities_and_seam() {
    let path = circle();
    for reverse in [false, true] {
        for offset in [-2.25, 0.0, 0.3, 1.4] {
            let map = Mapping::new(
                &path,
                CurveSettings {
                    width: 20.0,
                    reverse,
                    offset,
                    ..CurveSettings::default()
                },
                (400.0, 100.0),
            );
            for x in [20.0, 110.0, 230.0, 370.0] {
                for y in [10.0, 50.0, 90.0] {
                    let p = (x, y);
                    close(map.to_rect(map.to_curve(p).unwrap()).unwrap(), p, 0.12);
                }
            }
        }
    }
}

#[test]
fn bvh_matches_exhaustive_nearest_search_and_breaks_ties_by_path_order() {
    let path = circle();
    for y in (30..170).step_by(7) {
        for x in (30..170).step_by(9) {
            let p = (x as f32, y as f32);
            let mut accelerated = (f32::INFINITY, usize::MAX, 0.0);
            path.nearest(0, p, &mut accelerated);
            let mut reference = (f32::INFINITY, usize::MAX, 0.0);
            for (i, s) in path.segments.iter().enumerate() {
                let (distance, t) = s.closest(p);
                if distance < reference.0 {
                    reference = (distance, i, t);
                }
            }
            assert_eq!(accelerated, reference);
        }
    }
}

#[test]
fn degenerate_and_nonfinite_paths_are_rejected() {
    assert!(CurvePath::new(&[], false).is_none());
    assert!(CurvePath::new(&[((0.0, 0.0), (0.0, 0.0)); 3], false).is_none());
    assert!(
        CurvePath::new(
            &[((f32::NAN, 0.0), (0.0, 0.0)), ((1.0, 0.0), (0.0, 0.0))],
            false
        )
        .is_none()
    );
    let p = CurvePath::new(
        &[
            ((0.0, 0.0), (0.0, 0.0)),
            ((0.0, 0.0), (0.0, 0.0)),
            ((10.0, 0.0), (0.0, 0.0)),
        ],
        false,
    )
    .unwrap();
    close(p.frame(5.0).1, (0.0, 1.0), 1.0e-4);
}

#[test]
fn rendered_straight_band_is_identity_in_both_directions() {
    let path =
        CurvePath::new(&[((0.0, 3.0), (1.0, 0.0)), ((8.0, 3.0), (1.0, 0.0))], false).unwrap();
    let source: Vec<_> = (0..63)
        .map(|i| PixelF32 {
            alpha: 1.0,
            red: i as f32 / 63.0,
            green: 0.2,
            blue: 0.3,
        })
        .collect();
    for direction in [Direction::Forward, Direction::Inverse] {
        let settings = Settings {
            direction,
            curve: CurveSettings {
                width: 6.0,
                ..CurveSettings::default()
            },
            ..test_settings(Space::CurveAlonged)
        };
        let output = render(
            &source,
            (9, 7),
            (9, 7),
            ae::Point { h: 0, v: 0 },
            ae::Point { h: 0, v: 0 },
            (8.0, 6.0),
            Some(&path),
            settings,
        );
        for (a, b) in output.iter().zip(&source) {
            assert!((a.red - b.red).abs() < 1.0e-6);
            assert_eq!(a.alpha, b.alpha);
        }
        let missing = render(
            &source,
            (9, 7),
            (9, 7),
            ae::Point { h: 0, v: 0 },
            ae::Point { h: 0, v: 0 },
            (8.0, 6.0),
            None,
            settings,
        );
        assert!(missing.iter().all(|p| p.alpha == 0.0));
    }
}

#[test]
fn render_respects_pixel_geometry_origins_and_post_transform() {
    let path = CurvePath::new(
        &[((0.0, 12.0), (1.0, 0.0)), ((16.0, 12.0), (1.0, 0.0))],
        false,
    )
    .unwrap();
    let source: Vec<_> = (0..49)
        .map(|i| PixelF32 {
            alpha: 1.0,
            red: (i % 7) as f32 / 7.0,
            green: (i / 7) as f32 / 7.0,
            blue: 0.0,
        })
        .collect();
    let origin = ae::Point { h: -1, v: -1 };
    for direction in [Direction::Forward, Direction::Inverse] {
        let settings = Settings {
            direction,
            pixel_scale: (4.0, 6.0),
            curve: CurveSettings {
                width: 24.0,
                ..CurveSettings::default()
            },
            ..test_settings(Space::CurveAlonged)
        };
        let output = render(
            &source,
            (7, 7),
            (7, 7),
            origin,
            origin,
            (16.0, 24.0),
            Some(&path),
            settings,
        );
        for y in 1..=5 {
            for x in 1..=5 {
                let i = y * 7 + x;
                close(
                    (output[i].red, output[i].green),
                    (source[i].red, source[i].green),
                    1.0e-6,
                );
            }
        }
        let moved = Settings {
            post_transform: PostTransform {
                position: (5.0, 3.0),
                ..settings.post_transform
            },
            ..settings
        };
        let shifted = render(
            &source,
            (7, 7),
            (7, 7),
            origin,
            origin,
            (16.0, 24.0),
            Some(&path),
            moved,
        );
        close(
            (shifted[3 * 7 + 3].red, shifted[3 * 7 + 3].green),
            (output[3 * 7 + 2].red, output[3 * 7 + 2].green),
            1.0e-6,
        );
        let collapsed = Settings {
            post_transform: PostTransform {
                scale: (0.0, 1.0),
                ..settings.post_transform
            },
            ..settings
        };
        assert!(
            render(
                &source,
                (7, 7),
                (7, 7),
                origin,
                origin,
                (16.0, 24.0),
                Some(&path),
                collapsed
            )
            .iter()
            .all(|p| p.alpha == 0.0)
        );
    }
}
