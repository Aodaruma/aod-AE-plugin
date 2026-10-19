use super::transform::*;
use std::f32::consts::PI;

fn rotate(p: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos)
}

fn near(actual: Vec2, expected: Vec2) {
    assert!(
        (actual.x - expected.x).abs() < 0.001,
        "{actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn strip_preserves_source_band_and_repeats_only_along_normal() {
    let center = Vec2::new(83.0, -17.0);
    for angle in [0.0, PI / 6.0, PI / 2.0, -PI / 3.0] {
        let mapping = StripMapping::new(angle, 40.0, Repeat::Tile);
        let point = center + rotate(Vec2::new(123.0, 8.0), angle);
        near(mapping.coordinate(point, center), point);
        for repeat in -6..=6 {
            let shifted = point + rotate(Vec2::new(0.0, repeat as f32 * 40.0), angle);
            near(mapping.coordinate(shifted, center), point);
        }
    }
}

#[test]
fn mirrored_strips_fold_positive_and_negative_neighbors_continuously() {
    let mapping = StripMapping::new(0.0, 40.0, Repeat::Mirror);
    let center = Vec2::new(0.0, 0.0);
    near(
        mapping.coordinate(Vec2::new(9.0, 48.0), center),
        Vec2::new(9.0, -8.0),
    );
    near(
        mapping.coordinate(Vec2::new(9.0, -32.0), center),
        Vec2::new(9.0, -8.0),
    );
    for boundary in [-60.0, -20.0, 20.0, 60.0] {
        let before = mapping.coordinate(Vec2::new(9.0, boundary - 0.001), center);
        let after = mapping.coordinate(Vec2::new(9.0, boundary + 0.001), center);
        assert!((before.y - after.y).abs() < 0.003);
    }
}

const SHAPES: [TileShape; 3] = [
    TileShape::Triangle,
    TileShape::Quadrilateral,
    TileShape::Hexagon,
];

fn tile_mapping(shape: TileShape, repeat: Repeat) -> TileMapping {
    TileMapping::new(
        shape,
        Vec2::new(120.0, 90.0),
        repeat,
        Transform::default(),
        1.0,
    )
}

fn inside_source_cell(p: Vec2, shape: TileShape) -> bool {
    let epsilon = 0.00001;
    match shape {
        TileShape::Quadrilateral => p.x.abs() <= 0.5 + epsilon && p.y.abs() <= 0.5 + epsilon,
        TileShape::Triangle => {
            p.y >= -1.0 / 3.0 - epsilon && p.y + 2.0 * p.x.abs() <= 2.0 / 3.0 + epsilon
        }
        TileShape::Hexagon => {
            p.x.abs() <= 0.5 + epsilon && p.y.abs() + 0.5 * p.x.abs() <= 0.5 + epsilon
        }
    }
}

#[test]
fn every_output_point_maps_inside_selected_polygon_without_gaps() {
    let center = Vec2::new(31.25, -19.5);
    for shape in SHAPES {
        for repeat in [Repeat::Tile, Repeat::Mirror] {
            for (rotation, scale, skew, axis) in [
                (0.0, Vec2::new(1.0, 1.0), 0.0, 0.0),
                (0.72, Vec2::new(0.15, 2.8), 0.92, -0.4),
                (-1.5, Vec2::new(1.3, 0.7), -85.0_f32.to_radians(), 0.8),
            ] {
                let mapping = TileMapping::new(
                    shape,
                    Vec2::new(120.0, 90.0),
                    repeat,
                    Transform {
                        rotation,
                        scale,
                        skew,
                        skew_axis: axis,
                    },
                    1.0,
                );
                for y in -80..=80 {
                    for x in -80..=80 {
                        let point = center + Vec2::new(x as f32 * 13.7, y as f32 * 11.3);
                        let source = mapping.coordinate(point, center) - center;
                        let local = Vec2::new(source.x / 120.0, source.y / 90.0);
                        assert!(
                            inside_source_cell(local, shape),
                            "{shape:?} {repeat:?}: {local:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn polygons_repeat_on_both_lattice_axes_including_negative_cells() {
    let center = Vec2::new(70.0, 33.0);
    let point = center + Vec2::new(7.0, 9.0);
    for shape in SHAPES {
        let mapping = tile_mapping(shape, Repeat::Tile);
        let row_step = match shape {
            TileShape::Triangle => Vec2::new(60.0, 90.0),
            TileShape::Quadrilateral => Vec2::new(0.0, 90.0),
            TileShape::Hexagon => Vec2::new(60.0, 67.5),
        };
        for row in -5..=5 {
            for column in -5..=5 {
                let shifted = point + Vec2::new(column as f32 * 120.0, 0.0) + row_step * row as f32;
                near(mapping.coordinate(shifted, center), point);
            }
        }
    }
}

#[test]
fn central_polygon_keeps_source_coordinates_and_mirror_changes_neighbors() {
    let center = Vec2::new(70.0, 33.0);
    let point = center + Vec2::new(7.0, 9.0);
    for shape in SHAPES {
        let mapping = tile_mapping(shape, Repeat::Mirror);
        near(mapping.coordinate(point, center), point);
        for sign in [-1.0, 1.0] {
            near(
                mapping.coordinate(point + Vec2::new(sign * 120.0, 0.0), center),
                center + Vec2::new(-7.0, 9.0),
            );
        }
    }
}

#[test]
fn rotation_scale_and_axis_skew_transform_entire_lattice() {
    let size = Vec2::new(120.0, 90.0);
    let scale = Vec2::new(1.7, 0.6);
    let rotation = 0.63;
    let skew: f32 = -0.4;
    let axis = 1.1;
    let forward = |p: Vec2| {
        let p = rotate(Vec2::new(p.x * scale.x, p.y * scale.y), -axis);
        rotate(
            rotate(Vec2::new(p.x + skew.tan() * p.y, p.y), axis),
            rotation,
        )
    };
    let center = Vec2::new(34.0, 22.0);
    for shape in SHAPES {
        let mapping = TileMapping::new(
            shape,
            size,
            Repeat::Tile,
            Transform {
                rotation,
                scale,
                skew,
                skew_axis: axis,
            },
            1.0,
        );
        let original = Vec2::new(8.0, 9.0);
        for column in -3..=3 {
            let output = center + forward(original + Vec2::new(120.0 * column as f32, 0.0));
            near(mapping.coordinate(output, center), center + original);
        }
    }
}

#[test]
fn degenerate_controls_are_bounded_and_non_finite_points_are_transparent() {
    let mapping = TileMapping::new(
        TileShape::Hexagon,
        Vec2::new(0.0, f32::NAN),
        Repeat::Tile,
        Transform {
            rotation: f32::INFINITY,
            scale: Vec2::default(),
            skew: f32::NAN,
            skew_axis: f32::NAN,
        },
        1.0,
    );
    let result = mapping.coordinate(Vec2::new(12.0, -9.0), Vec2::default());
    assert!(result.x.is_finite() && result.y.is_finite());
    let result = mapping.coordinate(Vec2::new(f32::NAN, 0.0), Vec2::default());
    assert!(!result.x.is_finite() && !result.y.is_finite());
}

#[test]
fn new_modes_preserve_geometry_with_downsample_and_pixel_aspect() {
    use super::PatternSpace;
    let full = PatternSpace::new(Vec2::default(), Vec2::new(1.0, 1.0), 1.5, 720.0, 576.0);
    let reduced = PatternSpace::new(Vec2::new(4.0, 7.0), Vec2::new(0.5, 0.25), 1.5, 720.0, 576.0);
    let point = full.buffer_point_to_pattern(Vec2::new(230.0, 170.0));
    let reduced_point = reduced.buffer_point_to_pattern(Vec2::new(119.0, 49.5));
    let strip = StripMapping::new(0.71, 53.0, Repeat::Mirror);
    near(
        strip.coordinate(point, full.layer_center()),
        strip.coordinate(reduced_point, reduced.layer_center()),
    );
    for shape in SHAPES {
        let tile = tile_mapping(shape, Repeat::Mirror);
        near(
            tile.coordinate(point, full.layer_center()),
            tile.coordinate(reduced_point, reduced.layer_center()),
        );
    }
}

#[test]
fn mask_only_affect_preserves_texture_and_partial_rotation_stays_invertible() {
    for shape in SHAPES {
        let make = |affect| {
            TileMapping::new(
                shape,
                Vec2::new(120.0, 90.0),
                Repeat::Tile,
                Transform {
                    rotation: PI,
                    scale: Vec2::new(2.0, 0.5),
                    ..Transform::default()
                },
                affect,
            )
        };
        let output = Vec2::new(-16.0, -4.0);
        near(make(0.0).coordinate(output, Vec2::default()), output);
        near(
            make(1.0).coordinate(output, Vec2::default()),
            Vec2::new(8.0, 8.0),
        );
        let halfway = make(0.5).coordinate(output, Vec2::default());
        assert!(halfway.x.hypot(halfway.y) > 5.0);
    }
}

#[test]
fn strip_affect_controls_mask_and_image_independently() {
    let transform = Transform {
        rotation: PI / 2.0,
        scale: Vec2::new(2.0, 3.0),
        ..Transform::default()
    };
    let make = |affect| {
        StripMapping::with_controls(
            0.0,
            40.0,
            Repeat::Tile,
            transform,
            affect,
            StripDelta::default(),
        )
    };
    let output = Vec2::new(-24.0, 20.0);
    near(make(0.0).coordinate(output, Vec2::default()), output);
    near(
        make(1.0).coordinate(output, Vec2::default()),
        Vec2::new(10.0, 8.0),
    );
    near(
        make(0.0).coordinate(output + Vec2::new(-120.0, 0.0), Vec2::default()),
        output,
    );
}

#[test]
fn delta_modes_are_centered_signed_and_random_is_repeatable() {
    assert_eq!(delta_step(-3, DeltaMode::Forward, 4, 1), -3.0);
    assert_eq!(delta_step(-3, DeltaMode::Reverse, 4, 1), 3.0);
    assert_eq!(delta_step(-3, DeltaMode::Outward, 4, 1), 3.0);
    assert_eq!(delta_step(5, DeltaMode::PingPong, 4, 1), 3.0);
    assert_eq!(delta_step(-5, DeltaMode::PingPong, 4, 1), 3.0);
    for mode in [
        DeltaMode::Forward,
        DeltaMode::Reverse,
        DeltaMode::Outward,
        DeltaMode::PingPong,
        DeltaMode::Random,
    ] {
        assert_eq!(delta_step(0, mode, 4, 10), 0.0);
    }
    for index in -20..20 {
        let a = delta_step(index, DeltaMode::Random, 4, 123);
        assert_eq!(a, delta_step(index, DeltaMode::Random, 4, 123));
        assert!(a.abs() <= 4.0);
    }
    assert_ne!(
        delta_step(1, DeltaMode::Random, 4, 123),
        delta_step(1, DeltaMode::Random, 4, 124)
    );
}

#[test]
fn delta_width_changes_spacing_and_clamps_before_band_inversion() {
    let delta = StripDelta {
        width: 10.0,
        copies: 3,
        ..StripDelta::default()
    };
    let strip =
        StripMapping::with_controls(0.0, 40.0, Repeat::Tile, Transform::default(), 1.0, delta);
    for (y, expected) in [
        (-79.0, -3),
        (-74.0, -3),
        (-55.0, -2),
        (-21.0, -1),
        (19.0, 0),
        (20.1, 1),
        (69.9, 1),
        (70.1, 2),
    ] {
        assert_eq!(
            strip
                .sample(Vec2::new(0.0, y), Vec2::default())
                .unwrap()
                .index,
            expected
        );
    }
    for mode in [
        DeltaMode::Forward,
        DeltaMode::Reverse,
        DeltaMode::Outward,
        DeltaMode::PingPong,
        DeltaMode::Random,
    ] {
        let strip = StripMapping::with_controls(
            0.0,
            10.0,
            Repeat::Tile,
            Transform::default(),
            0.0,
            StripDelta {
                width: -100.0,
                mode,
                ..delta
            },
        );
        for y in -20..20 {
            if let Some(sample) = strip.sample(Vec2::new(0.0, y as f32), Vec2::default()) {
                assert!(sample.coordinate.y.is_finite());
            }
        }
    }
}

#[test]
fn delta_angle_changes_shared_boundaries_and_fits_the_region_between_them() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Tile,
        Transform::default(),
        1.0,
        StripDelta {
            angle: 0.5_f32.atan(),
            copies: 1,
            ..StripDelta::default()
        },
    );
    // The next shared boundary is y=60+0.5*x. At x=40, the neighboring
    // band spans y=20..80, so its center and two ends fit the source strip.
    for (y, source_y) in [
        (20.0, -20.0),
        (35.0, -10.0),
        (50.0, 0.0),
        (65.0, 10.0),
        (79.999, 19.999333),
    ] {
        let sample = strip.sample(Vec2::new(40.0, y), Vec2::default()).unwrap();
        assert_eq!(sample.index, 1);
        near(sample.coordinate, Vec2::new(40.0, source_y));
    }
    near(
        strip.coordinate(Vec2::new(40.0, 0.0), Vec2::default()),
        Vec2::new(40.0, 0.0),
    );
}

#[test]
fn shared_boundary_partition_has_no_holes_even_where_boundaries_cross() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Tile,
        Transform::default(),
        1.0,
        StripDelta {
            angle: 0.5_f32.atan(),
            copies: 1,
            ..StripDelta::default()
        },
    );
    // Four lines define three adjacent regions. The sloping outer lines
    // cross the source strip's edges at x=-80; zero-width regions disappear.
    for x in [-160.0, -100.0, -80.0, -40.0, 0.0, 100.0] {
        let outer = (60.0_f32 + 0.5 * x).abs().max(20.0);
        for y in -130..=130 {
            let point = Vec2::new(x, y as f32 + 0.125);
            let sample = strip.sample(point, Vec2::default());
            assert_eq!(
                sample.is_some(),
                point.y >= -outer && point.y < outer,
                "{point:?}"
            );
            if let Some(sample) = sample {
                assert!((-1..=1).contains(&sample.index));
                assert!(sample.coordinate.y >= -20.001 && sample.coordinate.y <= 20.001);
            }
        }
    }
}

#[test]
fn mirror_is_continuous_across_sloped_shared_boundaries() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Mirror,
        Transform::default(),
        1.0,
        StripDelta {
            angle: 0.5_f32.atan(),
            copies: 2,
            ..StripDelta::default()
        },
    );
    for x in [-10.0, 0.0, 40.0, 100.0] {
        for boundary in [-60.0 - 0.5 * x, -20.0, 20.0, 60.0 + 0.5 * x] {
            let before = strip
                .sample(Vec2::new(x, boundary - 0.001), Vec2::default())
                .unwrap();
            let after = strip
                .sample(Vec2::new(x, boundary + 0.001), Vec2::default())
                .unwrap();
            assert_eq!(before.index + 1, after.index);
            assert!((before.coordinate.y - after.coordinate.y).abs() < 0.005);
        }
    }
}

#[test]
fn boundary_affect_zero_preserves_image_and_intermediate_values_stay_finite() {
    let make = |affect| {
        StripMapping::with_controls(
            0.0,
            40.0,
            Repeat::Tile,
            Transform::default(),
            affect,
            StripDelta {
                angle: 0.5_f32.atan(),
                copies: 2,
                ..StripDelta::default()
            },
        )
    };
    near(
        make(0.0).coordinate(Vec2::new(40.0, 50.0), Vec2::default()),
        Vec2::new(40.0, 10.0),
    );
    near(
        make(0.5).coordinate(Vec2::new(40.0, 50.0), Vec2::default()),
        Vec2::new(40.0, 5.0),
    );
    near(
        make(1.0).coordinate(Vec2::new(40.0, 50.0), Vec2::default()),
        Vec2::new(40.0, 0.0),
    );
    for affect in [0.0, 0.5, 1.0] {
        for mode in [
            DeltaMode::Forward,
            DeltaMode::Reverse,
            DeltaMode::Outward,
            DeltaMode::PingPong,
            DeltaMode::Random,
        ] {
            let strip = StripMapping::with_controls(
                0.0,
                40.0,
                Repeat::Mirror,
                Transform::default(),
                affect,
                StripDelta {
                    angle: PI / 2.0,
                    width: -50.0,
                    mode,
                    copies: 3,
                    ..StripDelta::default()
                },
            );
            for x in -20..20 {
                if let Some(sample) =
                    strip.sample(Vec2::new(x as f32 * 10.0, 0.125), Vec2::default())
                {
                    assert!(sample.coordinate.x.is_finite() && sample.coordinate.y.is_finite());
                    assert!(sample.direction.x.is_finite() && sample.direction.y.is_finite());
                }
            }
        }
    }
}

#[test]
fn time_origin_changes_indices_without_changing_polygon_coordinates() {
    let center = Vec2::new(100.0, 100.0);
    for shape in SHAPES {
        let mapping = tile_mapping(shape, Repeat::Tile);
        let point = center + Vec2::new(120.0, 0.0);
        assert_eq!(mapping.index(point, center, point, IndexAxis::Distance), 0);
        assert!(mapping.index(center, center, point, IndexAxis::Distance) > 0);
        assert!(mapping.index(center, center, point, IndexAxis::X) < 0);
        near(mapping.coordinate(point, center), center);
    }
    let triangle = tile_mapping(TileShape::Triangle, Repeat::Tile);
    assert_eq!(
        triangle.index(
            Vec2::new(60.0, 30.0),
            Vec2::default(),
            Vec2::default(),
            IndexAxis::Distance
        ),
        1
    );
}

#[test]
fn strip_edge_moves_along_tangent_and_preserves_cross_strip_position() {
    use super::{SampleEdge, along_strip_edge};
    let point = Vec2::new(150.0, 100.0);
    let direction = Vec2::new(1.0, 1.0);
    near(
        along_strip_edge(point, direction, 100, 100, SampleEdge::Clamp).unwrap(),
        Vec2::new(99.0, 49.0),
    );
    for edge in [SampleEdge::Clamp, SampleEdge::Tile, SampleEdge::Mirror] {
        let result = along_strip_edge(point, direction, 100, 100, edge).unwrap();
        assert!((result.x - result.y - 50.0).abs() < 0.001);
        assert!((0.0..=99.001).contains(&result.x) && (0.0..=99.001).contains(&result.y));
        near(
            along_strip_edge(Vec2::new(99.0, 49.0), direction, 100, 100, edge).unwrap(),
            Vec2::new(99.0, 49.0),
        );
    }
    assert!(
        along_strip_edge(
            Vec2::new(150.0, -100.0),
            direction,
            100,
            100,
            SampleEdge::Clamp
        )
        .is_none()
    );
    near(
        along_strip_edge(
            Vec2::new(150.0, 50.0),
            Vec2::new(1.0, 0.0),
            100,
            100,
            SampleEdge::Clamp,
        )
        .unwrap(),
        Vec2::new(99.0, 50.0),
    );
}

#[test]
fn mask_only_mirrored_texture_stays_undistorted_under_shear() {
    let transform = Transform {
        scale: Vec2::new(2.0, 0.7),
        skew: 0.75_f32.atan(),
        ..Transform::default()
    };
    let forward = |p: Vec2| Vec2::new(2.0 * p.x + 0.75 * 0.7 * p.y, 0.7 * p.y);
    let polygon = TileMapping::new(
        TileShape::Quadrilateral,
        Vec2::new(120.0, 90.0),
        Repeat::Mirror,
        transform,
        0.0,
    );
    let local = forward(Vec2::new(8.0, 9.0));
    near(
        polygon.coordinate(forward(Vec2::new(128.0, 9.0)), Vec2::default()),
        Vec2::new(-local.x, local.y),
    );
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Mirror,
        transform,
        0.0,
        StripDelta::default(),
    );
    near(
        strip.coordinate(forward(Vec2::new(8.0, 49.0)), Vec2::default()),
        Vec2::new(local.x, -local.y),
    );
}

#[test]
fn disabling_overlap_splits_keeps_crossing_boundaries_out_of_the_source_band() {
    let make = |split_overlaps| {
        StripMapping::with_controls(
            0.0,
            40.0,
            Repeat::Tile,
            Transform::default(),
            1.0,
            StripDelta {
                angle: 0.5_f32.atan(),
                copies: 1,
                split_overlaps,
                ..StripDelta::default()
            },
        )
    };
    let point = Vec2::new(-100.0, 15.0);
    let split = make(true).sample(point, Vec2::default()).unwrap();
    assert_eq!(split.index, 1);
    near(split.coordinate, Vec2::new(-100.0, 0.0));
    let unsplit = make(false).sample(point, Vec2::default()).unwrap();
    assert_eq!(unsplit.index, 0);
    near(unsplit.coordinate, point);
}

#[test]
fn overlap_toggle_preserves_non_crossing_boundaries_and_zero_deltas() {
    for repeat in [Repeat::Tile, Repeat::Mirror] {
        for affect in [0.0, 0.5, 1.0] {
            for angle in [0.0, 0.5_f32.atan()] {
                let make = |split_overlaps| {
                    StripMapping::with_controls(
                        0.0,
                        40.0,
                        repeat,
                        Transform::default(),
                        affect,
                        StripDelta {
                            angle,
                            copies: 2,
                            split_overlaps,
                            ..StripDelta::default()
                        },
                    )
                };
                for x in [0.0, 10.0, 40.0] {
                    for y in -150..150 {
                        let point = Vec2::new(x, y as f32 + 0.125);
                        let a = make(true).sample(point, Vec2::default());
                        let b = make(false).sample(point, Vec2::default());
                        assert_eq!(a.is_some(), b.is_some());
                        if let (Some(a), Some(b)) = (a, b) {
                            assert_eq!(a.index, b.index);
                            near(a.coordinate, b.coordinate);
                            near(a.direction, b.direction);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn unsplit_boundaries_skip_folded_regions_without_holes_or_renumbering_later_repeats() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Tile,
        Transform::default(),
        1.0,
        StripDelta {
            angle: 0.5_f32.atan(),
            mode: DeltaMode::PingPong,
            cycle: 1,
            copies: 2,
            split_overlaps: false,
            ..StripDelta::default()
        },
    );
    // At x=-100, the first positive boundary is y=10, inside the source
    // band. The second is y=100 and still belongs to repeat 2 after skipping 1.
    let sample = strip
        .sample(Vec2::new(-100.0, 50.0), Vec2::default())
        .unwrap();
    assert_eq!(sample.index, 2);
    near(sample.coordinate, Vec2::new(-100.0, -5.0));
    for y in -130..130 {
        let point = Vec2::new(-100.0, y as f32 + 0.125);
        let sample = strip.sample(point, Vec2::default());
        assert_eq!(sample.is_some(), point.y >= -110.0 && point.y < 100.0);
    }
}

#[test]
fn strip_xy_pans_source_pixels_without_moving_or_limiting_the_strips() {
    let center = Vec2::new(128.0, 96.0);
    for (repeat, positive, negative) in [
        (Repeat::Tile, Vec2::new(1.5, 11.25), Vec2::new(16.5, 4.75)),
        (
            Repeat::Mirror,
            Vec2::new(1.5, -4.75),
            Vec2::new(16.5, -11.25),
        ),
    ] {
        let strip = StripMapping::with_controls(
            0.0,
            40.0,
            repeat,
            Transform::default(),
            1.0,
            StripDelta {
                offset: Vec2::new(7.5, -3.25),
                copies: 1,
                ..StripDelta::default()
            },
        );
        near(
            strip.coordinate(center + Vec2::new(9.0, 48.0), center),
            center + positive,
        );
        near(
            strip.coordinate(center + Vec2::new(9.0, -32.0), center),
            center + negative,
        );
        near(
            strip.coordinate(center + Vec2::new(9.0, 8.0), center),
            center + Vec2::new(9.0, 8.0),
        );
        assert_eq!(strip.index(center + Vec2::new(0.0, 280.0), center), 7);
        assert_eq!(strip.index(center + Vec2::new(0.0, 19.999), center), 0);
        assert_eq!(strip.index(center + Vec2::new(0.0, 20.0), center), 1);
    }
}

#[test]
fn strip_xy_obeys_delta_modes_and_seed_without_changing_repeat_indices() {
    let make = |mode, seed| {
        StripMapping::with_controls(
            0.0,
            40.0,
            Repeat::Tile,
            Transform::default(),
            1.0,
            StripDelta {
                offset: Vec2::new(8.0, -4.0),
                mode,
                cycle: 2,
                seed,
                ..StripDelta::default()
            },
        )
    };
    for (mode, steps) in [
        (DeltaMode::Forward, [-2.0, 0.0, 1.0, 3.0]),
        (DeltaMode::Reverse, [2.0, 0.0, -1.0, -3.0]),
        (DeltaMode::Outward, [2.0, 0.0, 1.0, 3.0]),
        (DeltaMode::PingPong, [2.0, 0.0, 1.0, 1.0]),
    ] {
        for (index, step) in [-2, 0, 1, 3].into_iter().zip(steps) {
            let sample = make(mode, 0)
                .sample(Vec2::new(30.0, index as f32 * 40.0 + 5.0), Vec2::default())
                .unwrap();
            assert_eq!(sample.index, index);
            near(
                sample.coordinate,
                Vec2::new(30.0 - 8.0 * step, 5.0 + 4.0 * step),
            );
        }
    }
    for index in -8..=8 {
        let point = Vec2::new(30.0, index as f32 * 40.0 + 5.0);
        let a = make(DeltaMode::Random, 7)
            .sample(point, Vec2::default())
            .unwrap();
        let b = make(DeltaMode::Random, 19)
            .sample(point, Vec2::default())
            .unwrap();
        assert_eq!(a.index, index);
        near(
            a.coordinate,
            make(DeltaMode::Random, 7).coordinate(point, Vec2::default()),
        );
        assert!((a.coordinate.x - 30.0).abs() <= 16.0);
        assert!((a.coordinate.y - 5.0).abs() <= 8.0);
        assert_eq!(a.coordinate == b.coordinate, index == 0);
    }
}

#[test]
fn strip_xy_preserves_crossings_time_indices_and_edge_directions_under_transforms() {
    for split_overlaps in [true, false] {
        for affect in [0.0, 0.5, 1.0] {
            let make = |offset| {
                StripMapping::with_controls(
                    PI / 6.0,
                    40.0,
                    Repeat::Mirror,
                    Transform {
                        rotation: 0.2,
                        scale: Vec2::new(1.5, 0.7),
                        skew: 0.3,
                        ..Transform::default()
                    },
                    affect,
                    StripDelta {
                        angle: 0.5_f32.atan(),
                        width: 2.0,
                        copies: 2,
                        split_overlaps,
                        offset,
                        ..StripDelta::default()
                    },
                )
            };
            let original = make(Vec2::default());
            let shifted = make(Vec2::new(200.0, -80.0));
            for x in -15..=15 {
                for y in -15..=15 {
                    let point = Vec2::new(x as f32 * 10.0, y as f32 * 10.0 + 0.125);
                    let a = original.sample(point, Vec2::default());
                    let b = shifted.sample(point, Vec2::default());
                    assert_eq!(a.is_some(), b.is_some());
                    if let (Some(a), Some(b)) = (a, b) {
                        assert_eq!(a.index, b.index);
                        near(a.direction, b.direction);
                        near(
                            b.coordinate,
                            a.coordinate - Vec2::new(200.0, -80.0) * a.index as f32,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn open_strip_edges_extend_source_coordinates_and_keep_copy_count_finite() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Tile,
        Transform::default(),
        1.0,
        StripDelta {
            copies: 1,
            ..StripDelta::default()
        },
    );
    for (clipping, indices) in [
        (StripClipping::Both, vec![0]),
        (StripClipping::NegativeEdge, vec![-1, 0]),
        (StripClipping::PositiveEdge, vec![0, 1]),
        (StripClipping::None, vec![-1, 0, 1]),
    ] {
        let mut actual = Vec::new();
        strip.visit_samples(Vec2::new(3.0, 7.0), Vec2::default(), clipping, |sample| {
            actual.push(sample.index);
            near(
                sample.coordinate,
                Vec2::new(3.0, 7.0 - sample.index as f32 * 40.0),
            );
        });
        assert_eq!(actual, indices);
    }
    let mut count = 0;
    strip.visit_samples(
        Vec2::new(0.0, 10000.0),
        Vec2::default(),
        StripClipping::None,
        |_| count += 1,
    );
    assert_eq!(count, 3);
    assert!(
        strip
            .sample_copy(Vec2::default(), Vec2::default(), 2, StripClipping::None)
            .is_none()
    );
    assert!(
        strip
            .sample_copy(
                Vec2::new(0.0, -20.0),
                Vec2::default(),
                0,
                StripClipping::NegativeEdge
            )
            .is_some()
    );
    assert!(
        strip
            .sample_copy(
                Vec2::new(0.0, 20.0),
                Vec2::default(),
                0,
                StripClipping::PositiveEdge
            )
            .is_none()
    );
}

#[test]
fn opening_edges_preserves_each_original_interval_and_temporal_copy_sampling() {
    let center = Vec2::new(13.0, -7.0);
    for split_overlaps in [true, false] {
        for affect in [0.0, 0.5, 1.0] {
            for angle in [0.0, 0.5_f32.atan()] {
                let strip = StripMapping::with_controls(
                    0.3,
                    40.0,
                    Repeat::Mirror,
                    Transform {
                        rotation: -0.2,
                        scale: Vec2::new(1.3, 0.8),
                        skew: 0.25,
                        ..Transform::default()
                    },
                    affect,
                    StripDelta {
                        angle,
                        width: 3.0,
                        offset: Vec2::new(7.0, -11.0),
                        copies: 2,
                        split_overlaps,
                        ..StripDelta::default()
                    },
                );
                for x in -12..=12 {
                    for y in -12..=12 {
                        let point = Vec2::new(x as f32 * 13.0, y as f32 * 11.0 + 0.125);
                        let original = strip.sample(point, center);
                        for clipping in [
                            StripClipping::NegativeEdge,
                            StripClipping::PositiveEdge,
                            StripClipping::None,
                        ] {
                            let mut found = false;
                            let mut indices = Vec::new();
                            strip.visit_samples(point, center, clipping, |sample| {
                                indices.push(sample.index);
                                let single = strip
                                    .sample_copy(point, center, sample.index, clipping)
                                    .unwrap();
                                near(single.coordinate, sample.coordinate);
                                near(single.direction, sample.direction);
                                if let Some(original) = original
                                    && original.index == sample.index
                                {
                                    found = true;
                                    near(sample.coordinate, original.coordinate);
                                    near(sample.direction, original.direction);
                                }
                            });
                            assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
                            assert_eq!(found, original.is_some());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn unclipped_folded_strips_keep_their_original_numbers() {
    let strip = StripMapping::with_controls(
        0.0,
        40.0,
        Repeat::Tile,
        Transform::default(),
        1.0,
        StripDelta {
            angle: 0.5_f32.atan(),
            mode: DeltaMode::PingPong,
            cycle: 1,
            copies: 2,
            split_overlaps: false,
            ..StripDelta::default()
        },
    );
    let mut indices = Vec::new();
    strip.visit_samples(
        Vec2::new(-100.0, 0.0),
        Vec2::default(),
        StripClipping::None,
        |sample| indices.push(sample.index),
    );
    assert_eq!(indices, [-1, 0, 2]);
}
