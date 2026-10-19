use super::*;

const CHANNELS: [Channel; 7] = [
    Channel::Rgb,
    Channel::Rgba,
    Channel::Red,
    Channel::Green,
    Channel::Blue,
    Channel::Alpha,
    Channel::Lightness,
];

#[test]
fn tracked_shrink_coordinates_reconstruct_retained_pixels() {
    for channel in CHANNELS {
        for (w, h, tw, th) in [(17, 13, 13, 9), (37, 29, 11, 9), (1, 9, 1, 3), (9, 1, 3, 1)] {
            for flat in [false, true] {
                let mut input = fixture(w, h, 924);
                if flat {
                    input.pixels.fill([0.3, 0.4, 0.2, 1.0]);
                }
                let original = input.pixels.clone();
                input.track_coordinates(&mut || false).unwrap();
                let actual = super::resize(input, tw, th, channel, || false).unwrap();
                let ordinary = super::resize(
                    Image::new(w, h, original.clone()).unwrap(),
                    tw,
                    th,
                    channel,
                    || false,
                )
                .unwrap();
                assert_eq!(actual.pixels, ordinary.pixels);
                assert!(ordinary.coordinates.is_none());
                for (i, &[x, y]) in actual.coordinates.as_ref().unwrap().iter().enumerate() {
                    assert!(x >= 0.0 && x < w as f32 && y >= 0.0 && y < h as f32);
                    assert_eq!([x.fract(), y.fract()], [0.0, 0.0]);
                    assert_eq!(actual.pixels[i], original[y as usize * w + x as usize]);
                }
            }
        }
    }
}

#[test]
fn tracked_expansion_flip_and_restore_preserve_affine_source_coordinates() {
    // R/G are an independent coordinate oracle: linear color interpolation must
    // agree with coordinate propagation while B/alpha affect the seam decisions.
    for channel in CHANNELS {
        for (w, h, tw, th) in [
            (7, 5, 28, 20),
            (7, 5, 3, 9),
            (7, 5, 10, 2),
            (1, 3, 4, 8),
            (3, 1, 8, 4),
        ] {
            let mut input = fixture(w, h, 394);
            for y in 0..h {
                for x in 0..w {
                    input.pixels[y * w + x][0] = x as f32;
                    input.pixels[y * w + x][1] = y as f32;
                }
            }
            let original = input.pixels.clone();
            input.track_coordinates(&mut || false).unwrap();
            let mut actual = super::resize(input, tw, th, channel, || false).unwrap();
            let mut ordinary =
                super::resize(Image::new(w, h, original).unwrap(), tw, th, channel, || {
                    false
                })
                .unwrap();
            for axes in [[false, false], [true, false], [false, true], [true, true]] {
                actual.flip(axes, &mut || false).unwrap();
                ordinary.flip(axes, &mut || false).unwrap();
                assert_eq!(actual.pixels, ordinary.pixels);
                let mut coordinate_image = Image::new(tw, th, actual.pixels.clone()).unwrap();
                coordinate_image.coordinates = actual.coordinates.clone();
                let coordinate_image = coordinate_image
                    .into_coordinate_image(&mut || false)
                    .unwrap();
                for restore in [false, true] {
                    for y in 0..h {
                        for x in 0..w {
                            let color =
                                sample_output(&actual, [w, h], [x as i64, y as i64], restore);
                            let coordinate = sample_output(
                                &coordinate_image,
                                [w, h],
                                [x as i64, y as i64],
                                restore,
                            );
                            assert_eq!(&color[..2], &coordinate[..2]);
                        }
                    }
                }
                actual.flip(axes, &mut || false).unwrap();
                ordinary.flip(axes, &mut || false).unwrap();
            }
        }
    }
}

#[test]
fn displacement_map_decodes_source_coordinates_and_coverage() {
    let (w, h) = (17, 13);
    for (tw, th) in [(17, 13), (8, 6), (24, 19), (1, 1)] {
        for axes in [[false, false], [true, false], [false, true], [true, true]] {
            let mut input = fixture(w, h, 682);
            input.track_coordinates(&mut || false).unwrap();
            let mut output = super::resize(input, tw, th, Channel::Rgba, || false).unwrap();
            output.flip(axes, &mut || false).unwrap();
            let output = output.into_coordinate_image(&mut || false).unwrap();
            for restore in [false, true] {
                for y in 0..h {
                    for x in 0..w {
                        let xy = [x as i64, y as i64];
                        let expected = sample_output(&output, [w, h], xy, restore);
                        let map = sample_displacement_output(&output, [w, h], xy, restore);
                        assert_eq!(map[3], 1.0);
                        assert!(map.iter().all(|v| (0.0..=1.0).contains(v)));
                        if expected[3] == 0.0 {
                            assert_eq!(map, [0.5, 0.5, 0.0, 1.0]);
                        } else {
                            let sx = x as f32 + (map[0] * 2.0 - 1.0) * w as f32;
                            let sy = y as f32 + (map[1] * 2.0 - 1.0) * h as f32;
                            assert!((sx - expected[0]).abs() < 0.00001);
                            assert!((sy - expected[1]).abs() < 0.00001);
                            assert_eq!(map[2], 1.0);
                            if tw == w && th == h && axes == [false, false] {
                                assert_eq!(&map[..2], &[0.5, 0.5]);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn coordinate_tracking_honors_cancellation() {
    let mut image = fixture(4, 3, 1);
    assert_eq!(
        image.track_coordinates(&mut || true),
        Err(CarveError::Cancelled)
    );
    assert!(image.coordinates.is_none());
    image.track_coordinates(&mut || false).unwrap();
    assert!(matches!(
        image.into_coordinate_image(&mut || true),
        Err(CarveError::Cancelled)
    ));
    assert!(matches!(
        fixture(4, 3, 1).into_coordinate_image(&mut || false),
        Err(CarveError::InvalidSize)
    ));
}

// Keep the original regression fixtures on the legacy energy model.
fn resize(
    image: Image,
    w: usize,
    h: usize,
    cancel: impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    super::resize(image, w, h, Channel::Rgba, cancel)
}

fn shrink_packed(
    image: Image,
    w: usize,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    super::shrink_packed(image, w, Channel::Rgba, cancel)
}

fn reference_feature(p: Pixel, channel: Channel) -> Vec<f64> {
    let sanitize = |v: f32| if v.is_finite() { v as f64 } else { 0.0 };
    let a = sanitize(p[3]).clamp(0.0, 1.0);
    let rgb: Vec<f64> = p[..3]
        .iter()
        .map(|&v| if a > 0.0 { sanitize(v) } else { 0.0 })
        .collect();
    // Preserve the pre-channel-selector formula independently of production code.
    let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    match channel {
        Channel::Rgb => rgb,
        Channel::Rgba => vec![luma, a],
        Channel::Red => vec![rgb[0]],
        Channel::Green => vec![rgb[1]],
        Channel::Blue => vec![rgb[2]],
        Channel::Alpha => vec![a],
        Channel::Lightness => vec![luma],
    }
}

#[test]
fn vectorized_recurrence_matches_scalar_bit_for_bit() {
    for n in 1..129 {
        for seed in 0..24 {
            let mut previous: Vec<f64> = (0..n + 2)
                .map(|i| match seed {
                    0 => 0.0,
                    1 => -0.0,
                    2..=11 => ((i * 19 + seed) % 11) as f64 * 1.0e30,
                    _ => {
                        let bits = (i as u64 + seed as u64 * 137).wrapping_mul(0x9e3779b97f4a7c15);
                        // Include tiny values, large finite costs, and irregular
                        // exponents without introducing NaNs into the DP input.
                        f64::from_bits(bits & 0x7fefffffffffffff)
                    }
                })
                .collect();
            previous[0] = f64::INFINITY;
            previous[n + 1] = f64::INFINITY;
            let energy: Vec<f64> = (0..n).map(|i| ((i * 7 + seed) % 13) as f64).collect();
            let mut scalar = vec![0.0; n];
            let mut vector = vec![0.0; n];
            let mut scalar_parents = vec![0; n];
            let mut vector_parents = vec![0; n];
            dp_row_scalar(&previous, &energy, &mut scalar, &mut scalar_parents);
            dp_row(&previous, &energy, &mut vector, &mut vector_parents);
            assert_eq!(
                scalar.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                vector.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
            assert_eq!(scalar_parents, vector_parents);
        }
    }
}

fn fixture(w: usize, h: usize, mut seed: u32) -> Image {
    let pixels = (0..w * h)
        .map(|_| {
            std::array::from_fn(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as f32 / 255.0
            })
        })
        .collect();
    Image::new(w, h, pixels).unwrap()
}

// Independent scalar reference: new image, new energy, full DP table per removal.
fn reference_seam(image: &Image) -> Vec<usize> {
    reference_seam_channel(image, Channel::Rgba)
}

fn reference_seam_channel(image: &Image, channel: Channel) -> Vec<usize> {
    let w = image.width;
    let h = image.height;
    let mut costs = vec![0.0; w * h];
    let mut parents = vec![0; w * h];
    for y in 0..h {
        for x in 0..w {
            let left = reference_feature(image.pixels[y * w + x.saturating_sub(1)], channel);
            let right = reference_feature(image.pixels[y * w + (x + 1).min(w - 1)], channel);
            let up = reference_feature(image.pixels[y.saturating_sub(1) * w + x], channel);
            let down = reference_feature(image.pixels[(y + 1).min(h - 1) * w + x], channel);
            let mut energy = 0.0;
            for c in 0..left.len() {
                energy += (right[c] - left[c]).abs();
                energy += (down[c] - up[c]).abs();
            }
            let mut prev = 0.0;
            if y > 0 {
                prev = f64::INFINITY;
                for px in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    if costs[(y - 1) * w + px] < prev {
                        prev = costs[(y - 1) * w + px];
                        parents[y * w + x] = px;
                    }
                }
            }
            costs[y * w + x] = prev + energy;
        }
    }
    let mut end = 0;
    for x in 1..w {
        if costs[(h - 1) * w + x] < costs[(h - 1) * w + end] {
            end = x;
        }
    }
    let mut path = vec![0; h];
    for y in (0..h).rev() {
        path[y] = end;
        end = parents[y * w + end];
    }
    path
}

fn reference_remove(image: Image, path: &[usize]) -> Image {
    let mut pixels = Vec::new();
    for (y, &sx) in path.iter().enumerate() {
        for x in 0..image.width {
            if x != sx {
                pixels.push(image.pixels[y * image.width + x]);
            }
        }
    }
    Image::new(image.width - 1, image.height, pixels).unwrap()
}

#[test]
fn exact_dp_matches_exhaustive_paths() {
    fn visit(c: &Carver, path: &mut Vec<usize>, cost: f64, best: &mut (f64, Vec<usize>)) {
        let y = path.len();
        if y == c.height {
            if cost < best.0
                || (cost == best.0 && path.iter().rev().cmp(best.1.iter().rev()).is_lt())
            {
                *best = (cost, path.clone());
            }
            return;
        }
        for x in 0..c.width {
            if y > 0 && x.abs_diff(path[y - 1]) > 1 {
                continue;
            }
            path.push(x);
            visit(c, path, cost + c.energy[c.rows[y] + x], best);
            path.pop();
        }
    }
    for seed in 0..24 {
        let image = fixture(4, 5, seed);
        let mut c = Carver::new(&image, Channel::Rgba, &mut || false).unwrap();
        // Integer energies test exact ties without floating-point association ambiguity.
        for (i, e) in c.energy.iter_mut().enumerate() {
            *e = ((i * 7 + seed as usize * 13) % 5) as f64;
        }
        let mut best = (f64::INFINITY, Vec::new());
        visit(&c, &mut Vec::new(), 0.0, &mut best);
        c.find_seam(&mut || false).unwrap();
        assert_eq!(c.seam, best.1);
    }
}

#[test]
fn local_energy_and_output_match_full_recomputation() {
    for w in 2..18 {
        for h in 1..13 {
            for seed in 0..6 {
                let mut image = fixture(w, h, seed);
                let original = fixture(w, h, seed);
                let mut c = Carver::new(&image, Channel::Rgba, &mut || false).unwrap();
                while c.width > 1 {
                    c.find_seam(&mut || false).unwrap();
                    let reference = reference_seam(&image);
                    assert_eq!(c.seam, reference, "{w}x{h}, seed={seed}, width={}", c.width);
                    image = reference_remove(image, &reference);
                    c.remove_seam();
                    for y in 0..h {
                        for x in 0..c.width {
                            assert_eq!(c.energy[c.rows[y] + x], c.energy_at(x, y));
                            assert_eq!(
                                original.pixels[c.indices[c.rows[y] + x] as usize],
                                image.pixels[y * c.width + x]
                            );
                        }
                    }
                }
                let actual = resize(original, 1, h, || false).unwrap();
                assert_eq!(actual.pixels, image.pixels);
            }
        }
    }
}

#[test]
fn removals_at_alternating_edges_preserve_rows_and_pixels() {
    for w in [2, 3, 16, 17, 65, 127] {
        let h = 5;
        let original = fixture(w, h, 531);
        let mut expected = fixture(w, h, 531);
        let mut c = Carver::new(&original, Channel::Rgba, &mut || false).unwrap();
        let mut step = 0;
        while c.width > 1 {
            let x = match step % 3 {
                0 => 0,
                1 => c.width - 1,
                _ => c.width / 2,
            };
            c.seam = (0..h).map(|y| (x + y % 2).min(c.width - 1)).collect();
            expected = reference_remove(expected, &c.seam);
            c.remove_seam();
            for y in 0..h {
                assert!(c.rows[y] >= y * w && c.rows[y] + c.width <= (y + 1) * w);
                for x in 0..c.width {
                    assert_eq!(
                        original.pixels[c.indices[c.rows[y] + x] as usize],
                        expected.pixels[y * c.width + x]
                    );
                    assert_eq!(c.energy[c.rows[y] + x], c.energy_at(x, y));
                }
            }
            step += 1;
        }
    }
}

#[test]
fn flat_ties_remove_left_edge() {
    let image = Image::new(8, 7, vec![[0.4; 4]; 56]).unwrap();
    let mut c = Carver::new(&image, Channel::Rgba, &mut || false).unwrap();
    c.find_seam(&mut || false).unwrap();
    assert_eq!(c.seam, vec![0; 7]);
}

#[test]
fn linked_dp_matches_packed_after_every_seam() {
    for (w, h) in [(2, 41), (41, 2), (63, 37), (47, 65)] {
        for kind in 0..8 {
            let mut image = fixture(w, h, kind);
            for (i, p) in image.pixels.iter_mut().enumerate() {
                match kind {
                    0 => *p = [0.5; 4],
                    1 => *p = [(i % w) as f32, (i / w) as f32, 0.0, 1.0],
                    2 => *p = [((i % w + i / w) % 2) as f32; 4],
                    3 => {
                        p[0] *= 1.0e30;
                        p[1] *= -1.0e30;
                    }
                    _ => (),
                }
            }
            let mut packed = Carver::new(&image, Channel::Rgba, &mut || false).unwrap();
            let mut linked = LinkedCarver::new(&image, Channel::Rgba, &mut || false).unwrap();
            while packed.width > 1 {
                packed.find_seam(&mut || false).unwrap();
                linked.find_seam(&mut || false).unwrap();
                for y in 0..h {
                    assert_eq!(
                        linked.seam[y],
                        packed.indices[packed.rows[y] + packed.seam[y]],
                        "{w}x{h}, kind={kind}, remaining={}, row={y}",
                        packed.width
                    );
                }
                packed.remove_seam();
                linked.remove_seam();
            }
        }
    }
}

#[test]
fn adaptive_switch_matches_packed_in_both_axes() {
    for seed in 0..12 {
        let input = fixture(71, 53, seed);
        let result = resize(input, 35, 26, || false).unwrap();
        let reference = shrink_packed(fixture(71, 53, seed), 35, &mut || false).unwrap();
        let reference = shrink_packed(reference.transpose(&mut || false).unwrap(), 26, &mut || {
            false
        })
        .unwrap()
        .transpose(&mut || false)
        .unwrap();
        assert_eq!(result.pixels, reference.pixels);
    }
}

#[test]
fn horizontal_equals_transposed_vertical() {
    let actual = resize(fixture(7, 9, 19), 7, 4, || false).unwrap();
    let image = fixture(7, 9, 19).transpose(&mut || false).unwrap();
    let mut reference = image;
    while reference.width > 4 {
        let path = reference_seam(&reference);
        reference = reference_remove(reference, &path);
    }
    let expected = reference.transpose(&mut || false).unwrap();
    assert_eq!(actual.pixels, expected.pixels);
}

#[test]
fn insertion_retains_originals_and_uses_distinct_seams() {
    let source = fixture(8, 5, 25);
    let result = resize(fixture(8, 5, 25), 15, 5, || false).unwrap();
    let mut work = fixture(8, 5, 25);
    let mut map: Vec<Vec<usize>> = (0..5).map(|_| (0..8).collect()).collect();
    let mut marks = vec![vec![false; 8]; 5];
    for _ in 0..7 {
        let path = reference_seam(&work);
        for y in 0..5 {
            marks[y][map[y].remove(path[y])] = true;
        }
        work = reference_remove(work, &path);
    }
    for (y, row) in marks.iter().enumerate() {
        let mut expected = Vec::new();
        for (x, &marked) in row.iter().enumerate() {
            let p = source.pixels[y * 8 + x];
            expected.push(p);
            if marked {
                let n = source.pixels[y * 8 + if x < 7 { x + 1 } else { x - 1 }];
                expected.push(std::array::from_fn(|c| p[c] * 0.5 + n[c] * 0.5));
            }
        }
        assert_eq!(&result.pixels[y * 15..(y + 1) * 15], expected);
    }
}

#[test]
fn singleton_and_multi_stage_expansion() {
    let color = [-0.5, 2.0, 0.25, 0.5];
    for (w, h) in [(1, 1), (1, 4), (4, 1), (5, 7)] {
        let image = Image::new(w, h, vec![color; w * h]).unwrap();
        let result = resize(image, w * 4, h * 4, || false).unwrap();
        assert_eq!((result.width, result.height), (w * 4, h * 4));
        assert!(result.pixels.iter().all(|p| *p == color));
    }
}

#[test]
fn channels_match_independent_features_and_popup_order() {
    for (index, channel) in CHANNELS.into_iter().enumerate() {
        assert_eq!(Channel::from_popup(index as i32 + 1), channel);
        let mut pixels = fixture(17, 11, 401).pixels;
        pixels.extend([
            [-2.0, 3.5, 0.4, 0.7],
            [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0],
            [20.0, -5.0, 1.0, 0.0],
            [2.0, 3.0, 4.0, f32::NAN],
        ]);
        for pixel in pixels {
            let actual = feature(pixel, channel);
            let expected = reference_feature(pixel, channel);
            assert_eq!(&actual[..expected.len()], expected, "{channel:?}");
            assert!(actual[expected.len()..].iter().all(|v| *v == 0.0));
            assert!(actual.iter().all(|v| v.is_finite()));
        }
    }
    assert_eq!(Channel::from_popup(0), Channel::Rgba);
    assert_eq!(Channel::from_popup(8), Channel::Rgba);
}

#[test]
fn selected_channel_changes_seam_but_moves_all_output_channels_together() {
    let pixels = vec![
        [0.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0, 0.8],
        [1.0, 1.0, 0.0, 0.4],
        [1.0, 1.0, 1.0, 0.4],
    ];
    for (channel, removed) in [
        (Channel::Red, 0),
        (Channel::Green, 2),
        (Channel::Blue, 1),
        (Channel::Alpha, 3),
    ] {
        let result = super::resize(
            Image::new(4, 1, pixels.clone()).unwrap(),
            3,
            1,
            channel,
            || false,
        )
        .unwrap();
        let mut expected = pixels.clone();
        expected.remove(removed);
        assert_eq!(result.pixels, expected, "{channel:?}");
    }
}

#[test]
fn all_channels_match_full_reference_after_every_removal() {
    for channel in CHANNELS {
        for (w, h) in [(2, 7), (9, 1), (17, 13)] {
            for seed in 0..4 {
                let original = fixture(w, h, seed);
                let mut work = fixture(w, h, seed);
                let mut packed = Carver::new(&original, channel, &mut || false).unwrap();
                let mut linked = LinkedCarver::new(&original, channel, &mut || false).unwrap();
                while work.width > 1 {
                    let seam = reference_seam_channel(&work, channel);
                    packed.find_seam(&mut || false).unwrap();
                    linked.find_seam(&mut || false).unwrap();
                    assert_eq!(packed.seam, seam, "{channel:?}, {w}x{h}, seed={seed}");
                    for (y, &x) in seam.iter().enumerate() {
                        assert_eq!(linked.seam[y], packed.indices[packed.rows[y] + x]);
                    }
                    packed.remove_seam();
                    linked.remove_seam();
                    work = reference_remove(work, &seam);
                    assert_eq!(linked.image(&original).unwrap().pixels, work.pixels);
                }
                let adaptive = super::resize(original, 1, h, channel, || false).unwrap();
                assert_eq!(adaptive.pixels, work.pixels);
            }
        }
    }
}

fn reference_resize_width(mut image: Image, target: usize, channel: Channel) -> Image {
    if target < image.width {
        while image.width > target {
            let path = reference_seam_channel(&image, channel);
            image = reference_remove(image, &path);
        }
        return image;
    }
    while image.width < target {
        let w = image.width;
        let h = image.height;
        let count = (target - w).min((w - 1).max(1));
        let mut work = Image::new(w, h, image.pixels.clone()).unwrap();
        let mut positions: Vec<Vec<usize>> = (0..h).map(|_| (0..w).collect()).collect();
        let mut selected = vec![Vec::new(); h];
        for _ in 0..count {
            let seam = reference_seam_channel(&work, channel);
            for y in 0..h {
                selected[y].push(positions[y].remove(seam[y]));
            }
            if work.width > 1 {
                work = reference_remove(work, &seam);
            }
        }
        let mut pixels = Vec::new();
        for (y, row) in selected.iter().enumerate() {
            for x in 0..w {
                let p = image.pixels[y * w + x];
                pixels.push(p);
                if row.contains(&x) {
                    let neighbor = if x + 1 < w {
                        x + 1
                    } else {
                        x.saturating_sub(1)
                    };
                    let q = image.pixels[y * w + neighbor];
                    pixels.push(std::array::from_fn(|c| p[c] * 0.5 + q[c] * 0.5));
                }
            }
        }
        image = Image::new(w + count, h, pixels).unwrap();
    }
    image
}

#[test]
fn all_channels_expand_shrink_and_mix_both_axes_exactly() {
    for channel in CHANNELS {
        for (w, h, tw, th) in [
            (7, 5, 28, 20),
            (17, 13, 8, 6),
            (7, 5, 14, 2),
            (7, 5, 3, 10),
            (1, 3, 4, 12),
        ] {
            let result = super::resize(fixture(w, h, 44), tw, th, channel, || false).unwrap();
            let reference = reference_resize_width(fixture(w, h, 44), tw, channel);
            let reference =
                reference_resize_width(reference.transpose(&mut || false).unwrap(), th, channel)
                    .transpose(&mut || false)
                    .unwrap();
            assert_eq!((result.width, result.height), (tw, th));
            assert_eq!(
                result.pixels, reference.pixels,
                "{channel:?}, {w}x{h} -> {tw}x{th}"
            );
        }
    }
}

#[test]
fn flips_preserve_positive_scale_pixels_for_each_axis_and_restore_mode() {
    for (w, h, tw, th) in [
        (9, 7, 9, 7),
        (8, 6, 4, 3),
        (9, 7, 5, 4),
        (5, 3, 12, 8),
        (1, 3, 1, 12),
    ] {
        for channel in CHANNELS {
            let positive = super::resize(fixture(w, h, 55), tw, th, channel, || false).unwrap();
            for axes in [[false, false], [true, false], [false, true], [true, true]] {
                let mut flipped = Image::new(tw, th, positive.pixels.clone()).unwrap();
                flipped.flip(axes, &mut || false).unwrap();
                let mut expected = Vec::new();
                for y in 0..th {
                    for x in 0..tw {
                        let sx = if axes[0] { tw - 1 - x } else { x };
                        let sy = if axes[1] { th - 1 - y } else { y };
                        expected.push(positive.pixels[sy * tw + sx]);
                    }
                }
                assert_eq!(flipped.pixels, expected);
                let reference = Image::new(tw, th, expected).unwrap();
                for restore in [false, true] {
                    for y in 0..h {
                        for x in 0..w {
                            assert_eq!(
                                sample_output(&flipped, [w, h], [x as i64, y as i64], restore),
                                sample_output(&reference, [w, h], [x as i64, y as i64], restore)
                            );
                        }
                    }
                }
                flipped.flip(axes, &mut || false).unwrap();
                assert_eq!(flipped.pixels, positive.pixels);
            }
        }
    }
    let mut image = fixture(3, 5, 1);
    assert_eq!(
        image.flip([true, false], &mut || true),
        Err(CarveError::Cancelled)
    );
    assert_eq!(
        image.flip([false, true], &mut || true),
        Err(CarveError::Cancelled)
    );
}

#[test]
fn restore_and_centered_canvas() {
    let image = Image::new(2, 1, vec![[0.0, 0.0, 0.0, 0.0], [2.0, 0.0, 0.0, 1.0]]).unwrap();
    assert_eq!(sample_output(&image, [4, 1], [0, 0], false), [0.0; 4]);
    assert_eq!(
        sample_output(&image, [4, 1], [2, 0], false),
        image.pixels[1]
    );
    assert_eq!(sample_output(&image, [4, 1], [3, 0], false), [0.0; 4]);
    assert_eq!(
        sample_output(&image, [4, 1], [1, 0], true),
        [0.5, 0.0, 0.0, 0.25]
    );
    assert_eq!(
        sample_output(&image, [4, 1], [2, 0], true),
        [1.5, 0.0, 0.0, 0.75]
    );
    assert_eq!(sample_output(&image, [4, 1], [3, 0], true), image.pixels[1]);
}

#[test]
fn identity_preserves_hdr_and_transparent_rgb() {
    let pixels = vec![[-2.0, 4.0, 0.25, 0.0], [2.5, 1.0, 0.4, 1.0]];
    let result = resize(Image::new(2, 1, pixels.clone()).unwrap(), 2, 1, || false).unwrap();
    assert_eq!(result.pixels, pixels);
    for (x, pixel) in pixels.iter().enumerate() {
        assert_eq!(sample_output(&result, [2, 1], [x as i64, 0], true), *pixel);
    }
}

#[test]
fn alpha_edges_have_energy_and_nonfinite_values_do_not_poison_dp() {
    let image = Image::new(
        3,
        1,
        vec![
            [0.0; 4],
            [0.0, 0.0, 0.0, 1.0],
            [f32::NAN, f32::INFINITY, 0.0, 0.0],
        ],
    )
    .unwrap();
    let mut c = Carver::new(&image, Channel::Rgba, &mut || false).unwrap();
    assert_eq!(c.energy, vec![1.0, 0.0, 1.0]);
    c.find_seam(&mut || false).unwrap();
    assert_eq!(c.seam, vec![1]);
}

#[test]
fn size_guards_and_cancellation() {
    assert!(Image::new(0, 2, vec![]).is_err());
    assert!(Image::new(2, 2, vec![]).is_err());
    assert_eq!(area(usize::MAX, 2), Err(CarveError::InvalidSize));
    assert_eq!(
        resize(fixture(2, 2, 1), MAX_DIMENSION + 1, 2, || false).unwrap_err(),
        CarveError::InvalidSize
    );
    assert_eq!(
        resize(fixture(2, 2, 1), 1, 2, || true).unwrap_err(),
        CarveError::Cancelled
    );
    let mut calls = 0;
    assert_eq!(
        resize(fixture(40, 96, 1), 20, 96, || {
            calls += 1;
            calls > 12
        })
        .unwrap_err(),
        CarveError::Cancelled
    );
}

#[test]
#[ignore = "explicit performance measurement"]
fn benchmark_exact() {
    use std::time::Instant;
    for (w, h, target) in [(512, 512, 384), (1920, 1080, 1440)] {
        let image = fixture(w, h, 42);
        let start = Instant::now();
        let fast = resize(image, target, h, || false).unwrap();
        let elapsed = start.elapsed();
        let mut reference = fixture(w, h, 42);
        let start = Instant::now();
        while reference.width > target {
            let seam = reference_seam(&reference);
            reference = reference_remove(reference, &seam);
        }
        let baseline = start.elapsed();
        assert_eq!(fast.pixels, reference.pixels);
        eprintln!(
            "{w}x{h} -> {target}x{h}: optimized={elapsed:?}, full-recompute={baseline:?}, speedup={:.2}x; exact pixels",
            baseline.as_secs_f64() / elapsed.as_secs_f64()
        );
    }
}

#[test]
#[ignore = "explicit 1080p video-frame performance measurement"]
fn benchmark_video_1080() {
    use std::time::Instant;
    let (w, h) = (1920, 1080);
    for kind in ["flat", "gradient", "shapes", "noise"] {
        let mut pixels = fixture(w, h, 42).pixels;
        for (i, p) in pixels.iter_mut().enumerate() {
            let x = (i % w) as f32 / w as f32;
            let y = (i / w) as f32 / h as f32;
            *p = match kind {
                "flat" => [0.2, 0.3, 0.4, 1.0],
                "gradient" => [x, y, (x + y) * 0.5, 1.0],
                "shapes" if (x - 0.45).powi(2) + (y - 0.55).powi(2) < 0.08 => [0.8, 0.3, 0.2, 1.0],
                "shapes" => [0.1, x * 0.1 + 0.2, y * 0.1 + 0.3, 1.0],
                _ => [p[0], p[1], p[2], 1.0],
            };
        }
        let input = Image::new(w, h, pixels.clone()).unwrap();
        let start = Instant::now();
        let result = resize(input, 960, 540, || false).unwrap();
        let elapsed = start.elapsed();
        let reference =
            shrink_packed(Image::new(w, h, pixels).unwrap(), 960, &mut || false).unwrap();
        let reference = shrink_packed(
            reference.transpose(&mut || false).unwrap(),
            540,
            &mut || false,
        )
        .unwrap()
        .transpose(&mut || false)
        .unwrap();
        assert_eq!(result.pixels, reference.pixels);
        eprintln!("1080p XY50% {kind}: {elapsed:?}/frame; exact pixels vs full DP");
    }
}
