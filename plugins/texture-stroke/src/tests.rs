use super::*;

pub(super) fn settings() -> Settings {
    Settings {
        path_source: PathSource::Auto,
        output_mode: OutputMode::StrokeOnly,
        stamp_order: StampOrder::StartToEnd,
        stroke_width: 12.0,

        stroke_width_source: StrokeWidthSource::StrokeWidth,
        feather_width_scale: 1.0,
        feather_debug_logging: false,
        texture_time_mode: TextureTimeMode::Current,
        fixed_frame: 0,
        time_range_frames: 0,
        time_samples: 1,
        time_seed: 0,
        fallback_brush_shape: FallbackBrushShape::Square,
        fallback_brush_softness: 0.0,
        brush_size_min: 1.0,
        brush_size_max: 1.0,
        brush_size_randomness: 0.0,
        brush_size_noise_scale: 0.0,
        brush_size_seed: 0,
        stamp_density: 1.0,
        spacing_min: 1.0,
        spacing_max: 1.0,
        spacing_randomness: 0.0,
        spacing_noise_scale: 0.0,
        spacing_seed: 0,
        brush_opacity_min: 1.0,
        brush_opacity_max: 1.0,
        brush_opacity_randomness: 0.0,
        brush_opacity_noise_scale: 0.0,
        brush_opacity_seed: 0,
        rotate_with_stroke: true,
        direction_offset: 0.0,
        rotation_min: 0.0,
        rotation_max: 0.0,
        rotation_randomness: 0.0,
        rotation_noise_scale: 0.0,
        rotation_seed: 0,
        reverse_direction: false,
        texture_opacity: 1.0,
        stroke_blend_mode: BlendMode::Normal,
    }
}

fn texture() -> BrushTexture {
    BrushTexture {
        has_texture_layer: false,
        frames: vec![TextureFrame {
            width: 1,
            height: 1,
            pixels: vec![premultiply([1.0; 3], 1.0)],
        }],
    }
}

fn stamp(x: f32, y: f32) -> BrushStamp {
    BrushStamp {
        path_index: 0,
        x,
        y,

        along: x,
        stamp_index: 0,
        size: 12.0,
        opacity: 0.5,
        rotation: 0.0,
    }
}

fn canvas(width: usize, height: usize) -> Canvas {
    Canvas {
        width,
        height,
        origin: [0.0; 2],
        scale: [1.0; 2],
    }
}

#[test]
fn overlapping_stamps_accumulate_in_stroke_only() {
    let out = render_texture_stroke(
        &vec![transparent(); 32 * 32],
        &[stamp(12.0, 16.0), stamp(16.0, 16.0)],
        canvas(32, 32),
        &texture(),
        settings(),
    );
    assert!((out[16 * 32 + 14].alpha - 0.75).abs() < 1.0e-6);
    assert!((out[16 * 32 + 7].alpha - 0.5).abs() < 1.0e-6);
}

#[test]
fn transparent_texture_holes_preserve_previous_stamps() {
    let tex = BrushTexture {
        has_texture_layer: true,
        frames: vec![TextureFrame {
            width: 2,
            height: 1,
            pixels: vec![transparent(), premultiply([1.0; 3], 1.0)],
        }],
    };
    let one = render_texture_stroke(
        &vec![transparent(); 32 * 32],
        &[stamp(12.0, 16.0)],
        canvas(32, 32),
        &tex,
        settings(),
    );
    let two = render_texture_stroke(
        &vec![transparent(); 32 * 32],
        &[stamp(12.0, 16.0), stamp(18.0, 16.0)],
        canvas(32, 32),
        &tex,
        settings(),
    );
    assert!(one[16 * 32 + 14].alpha > 0.0);
    assert_eq!(one[16 * 32 + 14].alpha, two[16 * 32 + 14].alpha);
}

#[test]
fn blend_modes_preserve_color_over_transparency() {
    let color = premultiply([0.8, 0.4, 0.2], 0.5);
    for mode in [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Add,
        BlendMode::Overlay,
        BlendMode::Difference,
    ] {
        let out = composite_pixel(transparent(), color, mode);
        assert!((out.red - color.red).abs() < 1.0e-6);
        assert!((out.alpha - color.alpha).abs() < 1.0e-6);
    }
}

#[test]
fn cropped_output_matches_full_render() {
    let stamps = [stamp(18.0, 16.0), stamp(22.0, 20.0)];
    let full = render_texture_stroke(
        &vec![transparent(); 40 * 40],
        &stamps,
        canvas(40, 40),
        &texture(),
        settings(),
    );
    let crop = Canvas {
        origin: [13.0, 11.0],
        ..canvas(16, 18)
    };
    let cropped = render_texture_stroke(
        &vec![transparent(); 16 * 18],
        &stamps,
        crop,
        &texture(),
        settings(),
    );
    for y in 0..crop.height {
        for x in 0..crop.width {
            assert_eq!(
                cropped[y * crop.width + x].alpha,
                full[(y + 11) * 40 + x + 13].alpha
            );
        }
    }
}

#[test]
fn half_resolution_keeps_layer_position_and_width() {
    let s = BrushStamp {
        opacity: 1.0,
        ..stamp(20.0, 20.0)
    };
    let out = render_texture_stroke(
        &vec![transparent(); 20 * 20],
        &[s],
        Canvas {
            scale: [0.5; 2],
            ..canvas(20, 20)
        },
        &texture(),
        settings(),
    );
    let coords: Vec<_> = out
        .iter()
        .enumerate()
        .filter(|(_, p)| p.alpha > 0.0)
        .map(|(i, _)| (i % 20, i / 20))
        .collect();
    assert_eq!(coords.iter().map(|p| p.0).min(), Some(7));
    assert_eq!(coords.iter().map(|p| p.0).max(), Some(12));
    assert_eq!(coords.len(), 36);
}

#[test]
fn composite_aligns_different_input_output_extents() {
    let src = vec![premultiply([0.2, 0.4, 0.8], 1.0); 6];
    let out = align_source(
        &src,
        3,
        2,
        [10.0, 12.0],
        Canvas {
            origin: [8.0, 11.0],
            ..canvas(8, 5)
        },
    );
    assert_eq!(out.iter().filter(|p| p.alpha > 0.0).count(), 6);
    assert_eq!(out[8 + 2].blue, 0.8);
    assert_eq!(out[0].alpha, 0.0);
}

#[test]
fn empty_paths_respect_output_mode() {
    let src = vec![premultiply([1.0; 3], 1.0); 4];
    let out = render_texture_stroke(&src, &[], canvas(2, 2), &texture(), settings());
    assert!(out.iter().all(|p| p.alpha == 0.0));
    for output_mode in [OutputMode::CompositeFront, OutputMode::CompositeBehind] {
        let s = Settings {
            output_mode,
            ..settings()
        };
        assert!(
            render_texture_stroke(&src, &[], canvas(2, 2), &texture(), s)
                .iter()
                .all(|p| p.alpha == 1.0)
        );
    }
}

fn red_blue_texture() -> BrushTexture {
    BrushTexture {
        has_texture_layer: true,
        frames: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]
            .map(|color| TextureFrame {
                pixels: vec![premultiply(color, 1.0)],
                width: 1,
                height: 1,
            })
            .into(),
    }
}

#[test]
fn reversing_stamp_order_changes_overlap_without_reassigning_texture_frames() {
    let stamps = [
        BrushStamp {
            along: 0.0,
            ..stamp(12.0, 16.0)
        },
        BrushStamp {
            along: 20.0,
            stamp_index: 1,
            ..stamp(18.0, 16.0)
        },
    ];
    let s = Settings {
        texture_time_mode: TextureTimeMode::AlongStroke,
        ..settings()
    };
    let src = vec![transparent(); 32 * 32];
    let forward = render_texture_stroke(&src, &stamps, canvas(32, 32), &red_blue_texture(), s);
    let reverse = render_texture_stroke(
        &src,
        &stamps,
        canvas(32, 32),
        &red_blue_texture(),
        Settings {
            stamp_order: StampOrder::EndToStart,
            ..s
        },
    );
    let middle = 16 * 32 + 14;
    assert_eq!([forward[middle].red, forward[middle].blue], [0.25, 0.5]);
    assert_eq!([reverse[middle].red, reverse[middle].blue], [0.5, 0.25]);
    for i in [16 * 32 + 8, 16 * 32 + 21] {
        assert_eq!(forward[i].red, reverse[i].red);
        assert_eq!(forward[i].blue, reverse[i].blue);
    }
    assert!(
        forward
            .iter()
            .zip(&reverse)
            .all(|(a, b)| a.alpha == b.alpha)
    );
}

#[test]
fn reverse_stamp_order_preserves_stacking_between_independent_paths() {
    let stamps = [
        BrushStamp {
            along: 0.0,
            opacity: 1.0,
            ..stamp(16.0, 16.0)
        },
        BrushStamp {
            path_index: 1,
            along: 20.0,
            stamp_index: 1,
            opacity: 1.0,
            ..stamp(16.0, 16.0)
        },
    ];
    for stamp_order in [StampOrder::StartToEnd, StampOrder::EndToStart] {
        let out = render_texture_stroke(
            &vec![transparent(); 32 * 32],
            &stamps,
            canvas(32, 32),
            &red_blue_texture(),
            Settings {
                stamp_order,
                texture_time_mode: TextureTimeMode::AlongStroke,
                ..settings()
            },
        );
        assert_eq!(out[16 * 32 + 16].blue, 1.0);
        assert_eq!(out[16 * 32 + 16].red, 0.0);
    }
}

#[test]
fn composite_behind_places_source_over_the_whole_stroke_once() {
    let stamps = [
        BrushStamp {
            along: 0.0,
            ..stamp(12.0, 16.0)
        },
        BrushStamp {
            along: 20.0,
            stamp_index: 1,
            ..stamp(18.0, 16.0)
        },
    ];
    for alpha in [0.0, 0.5, 1.0] {
        let src = vec![premultiply([0.0, 1.0, 0.0], alpha); 32 * 32];
        let s = Settings {
            texture_time_mode: TextureTimeMode::AlongStroke,
            ..settings()
        };
        let front = render_texture_stroke(
            &src,
            &stamps,
            canvas(32, 32),
            &red_blue_texture(),
            Settings {
                output_mode: OutputMode::CompositeFront,
                ..s
            },
        );
        let behind = render_texture_stroke(
            &src,
            &stamps,
            canvas(32, 32),
            &red_blue_texture(),
            Settings {
                output_mode: OutputMode::CompositeBehind,
                ..s
            },
        );
        let f = front[16 * 32 + 14];
        let b = behind[16 * 32 + 14];
        assert_eq!(
            [f.red, f.green, f.blue, f.alpha],
            [0.25, alpha * 0.25, 0.5, 0.75 + alpha * 0.25]
        );
        assert_eq!(
            [b.red, b.green, b.blue, b.alpha],
            [
                0.25 * (1.0 - alpha),
                alpha,
                0.5 * (1.0 - alpha),
                0.75 + alpha * 0.25
            ]
        );
        assert_eq!(behind[0].green, src[0].green);
        assert_eq!(behind[0].alpha, src[0].alpha);
    }
}

#[test]
fn independent_short_paths_each_get_a_stamp() {
    let point = |path_index, along, x| PathPoint {
        path_index,
        along,
        x,
        y: 0.0,
        tangent_x: 1.0,
        tangent_y: 0.0,

        stroke_width: 20.0,
    };
    let points = [
        point(0, 0.0, 0.0),
        point(0, 1.0, 1.0),
        point(1, 1.0, 100.0),
        point(1, 2.0, 101.0),
    ];
    let stamps = stamps_from_points(&points, settings());
    assert_eq!(stamps.len(), 2);
    assert_eq!(stamps[1].x, 100.0);
}

#[test]
fn direction_offset_is_applied_once() {
    let point = PathPoint {
        path_index: 0,
        x: 0.0,
        y: 0.0,
        tangent_x: 1.0,
        tangent_y: 0.0,

        along: 0.0,
        stroke_width: 10.0,
    };
    let s = Settings {
        direction_offset: 0.5,
        ..settings()
    };
    let stamps = stamps_from_points(&[point], s);
    assert_eq!(stamps[0].rotation, 0.5);
}

#[test]
fn zero_base_width_can_still_use_mask_feather() {
    let profile = PathFeatherProfile {
        uniform_radius: Some(20.0),
        samples: vec![],
    };
    let s = Settings {
        stroke_width: 0.0,
        stroke_width_source: StrokeWidthSource::MaskFeather,
        ..settings()
    };
    assert_eq!(stroke_width_at(s, Some(&profile), 0, 0.0), 20.0);
    assert_eq!(
        stroke_width_at(
            Settings {
                stroke_width_source: StrokeWidthSource::StrokeWidth,
                ..s
            },
            Some(&profile),
            0,
            0.0
        ),
        0.0
    );
}
