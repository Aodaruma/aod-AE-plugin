use super::*;

const ELIGIBLE: u8 = 1;
const HORIZONTAL_CORE: u8 = 2;
const CORE: u8 = 4;
const SEARCHED: u8 = 8;
const SELECTED_CORE: u8 = 16;
const HORIZONTAL_REACH: u8 = 32;
const SELECTED: u8 = 64;

pub(super) fn is_selected(state: u8) -> bool {
    state & SELECTED != 0
}

pub(super) fn preview_radius(
    settings: &RenderSettings,
    x: ae::RationalScale,
    y: ae::RationalScale,
) -> usize {
    let reduced = |s: ae::RationalScale| s.num > 0 && s.den > 0 && (s.num as u32) < s.den;
    if settings.stabilize_preview && (reduced(x) || reduced(y)) {
        settings.preview_gap_radius
    } else {
        0
    }
}

// Erode matching pixels, select a connected interior, then restore only its
// immediate edge. Unlimited reconstruction would reopen the original leak.
// Every buffer and operation uses the current preview resolution.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_region(
    selection: &[PixelF32],
    width: usize,
    height: usize,
    seed: usize,
    settings: &RenderSettings,
    radius: usize,
    state: &mut Vec<u8>,
    queue: &mut VecDeque<usize>,
    abort: &mut impl FnMut() -> Result<(), Error>,
) -> Result<(), Error> {
    let seed_px = selection[seed];
    let seed_rgb = interpreted_rgb(seed_px, settings.color_interpretation);
    state.clear();
    state.resize(selection.len(), 0);
    for (idx, &px) in selection.iter().enumerate() {
        if idx.is_multiple_of(16_384) {
            abort()?;
        }
        if is_region_pixel(px, seed_rgb, seed_px.alpha, settings) {
            state[idx] = ELIGIBLE;
        }
    }

    // Separable square erosion: O(radius * preview pixels), not O(radius^2).
    // Ignore samples outside the image so touching its edge is not penalized.
    for y in 0..height {
        if y.is_multiple_of(32) {
            abort()?;
        }
        for x in 0..width {
            if (x.saturating_sub(radius)..=(x + radius).min(width - 1))
                .all(|xx| state[y * width + xx] & ELIGIBLE != 0)
            {
                state[y * width + x] |= HORIZONTAL_CORE;
            }
        }
    }
    for y in 0..height {
        if y.is_multiple_of(32) {
            abort()?;
        }
        for x in 0..width {
            if (y.saturating_sub(radius)..=(y + radius).min(height - 1))
                .all(|yy| state[yy * width + x] & HORIZONTAL_CORE != 0)
            {
                state[y * width + x] |= CORE;
            }
        }
    }

    let neighbors: &[(isize, isize)] = match settings.connectivity {
        Connectivity::Four => &NEIGHBORS_4,
        Connectivity::Eight => &NEIGHBORS_8,
    };
    // A seed on an edge can reach the nearest interior through eligible pixels.
    // Limit this search: a seed in a thin feature must not jump to a remote region.
    queue.clear();
    queue.push_back(seed);
    state[seed] |= SEARCHED;
    let mut anchor = None;
    'search: for distance in 0..=radius {
        for _ in 0..queue.len() {
            let idx = queue.pop_front().unwrap();
            if state[idx] & CORE != 0 {
                anchor = Some(idx);
                break 'search;
            }
            if distance == radius {
                continue;
            }
            for &(dx, dy) in neighbors {
                if let Some(next) = offset_index(idx % width, idx / width, width, height, dx, dy)
                    && state[next] & (ELIGIBLE | SEARCHED) == ELIGIBLE
                {
                    state[next] |= SEARCHED;
                    queue.push_back(next);
                }
            }
        }
    }
    let Some(anchor) = anchor else {
        // Conservative fallback for thin features; never flood across a gap.
        for value in state.iter_mut() {
            if *value & SEARCHED != 0 {
                *value |= SELECTED;
            }
        }
        return Ok(());
    };

    select_core_spans(
        state,
        width,
        height,
        anchor,
        settings.connectivity,
        queue,
        abort,
    )?;
    // Bounded dilation restores the selected side's edge, but cannot propagate
    // through the narrow passage into the adjacent large component.
    for y in 0..height {
        if y.is_multiple_of(32) {
            abort()?;
        }
        for x in 0..width {
            if (x.saturating_sub(radius)..=(x + radius).min(width - 1))
                .any(|xx| state[y * width + xx] & SELECTED_CORE != 0)
            {
                state[y * width + x] |= HORIZONTAL_REACH;
            }
        }
    }
    for y in 0..height {
        if y.is_multiple_of(32) {
            abort()?;
        }
        for x in 0..width {
            if state[y * width + x] & ELIGIBLE != 0
                && (y.saturating_sub(radius)..=(y + radius).min(height - 1))
                    .any(|yy| state[yy * width + x] & HORIZONTAL_REACH != 0)
            {
                state[y * width + x] |= SELECTED;
            }
        }
    }
    Ok(())
}

// Queue horizontal runs, not every pixel. The one-pixel wider neighboring span
// for 8-connectivity preserves diagonal connections exactly.
fn select_core_spans(
    state: &mut [u8],
    width: usize,
    height: usize,
    anchor: usize,
    connectivity: Connectivity,
    queue: &mut VecDeque<usize>,
    abort: &mut impl FnMut() -> Result<(), Error>,
) -> Result<(), Error> {
    let available = |value: u8| value & (CORE | SELECTED_CORE) == CORE;
    let diagonal = usize::from(matches!(connectivity, Connectivity::Eight));
    queue.clear();
    queue.push_back(anchor);
    let mut visited = 0;
    while let Some(idx) = queue.pop_front() {
        if !available(state[idx]) {
            continue;
        }
        let y = idx / width;
        let row = y * width;
        let mut left = idx;
        let mut right = idx;
        while left > row && available(state[left - 1]) {
            left -= 1;
        }
        while right + 1 < row + width && available(state[right + 1]) {
            right += 1;
        }
        for value in &mut state[left..=right] {
            *value |= SELECTED_CORE;
        }
        visited += right - left + 1;
        if visited >= 16_384 {
            abort()?;
            visited = 0;
        }
        let first_x = (left - row).saturating_sub(diagonal);
        let last_x = (right - row + diagonal).min(width - 1);
        for next_y in y.saturating_sub(1)..=(y + 1).min(height - 1) {
            if next_y == y {
                continue;
            }
            let mut next = next_y * width + first_x;
            let end = next_y * width + last_x;
            while next <= end {
                if available(state[next]) {
                    queue.push_back(next);
                    while next <= end && available(state[next]) {
                        next += 1;
                    }
                } else {
                    next += 1;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_selection_matches_pixel_flood_on_varied_topologies() {
        let mut random = 19u32;
        for (width, height) in [(1, 1), (1, 17), (23, 1), (17, 11), (32, 24)] {
            for pattern in 0..32 {
                let initial: Vec<u8> = (0..width * height)
                    .map(|_| {
                        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        if pattern == 0 || (pattern != 1 && (random >> 24) % 4 != 0) {
                            CORE
                        } else {
                            0
                        }
                    })
                    .collect();
                for anchor in [0, initial.len() / 2, initial.len() - 1] {
                    for connectivity in [Connectivity::Four, Connectivity::Eight] {
                        let mut expected = initial.clone();
                        expected[anchor] |= CORE | SELECTED_CORE;
                        let neighbors: &[(isize, isize)] = match connectivity {
                            Connectivity::Four => &NEIGHBORS_4,
                            Connectivity::Eight => &NEIGHBORS_8,
                        };
                        let mut queue = VecDeque::from([anchor]);
                        while let Some(idx) = queue.pop_front() {
                            for &(dx, dy) in neighbors {
                                if let Some(next) =
                                    offset_index(idx % width, idx / width, width, height, dx, dy)
                                    && expected[next] & (CORE | SELECTED_CORE) == CORE
                                {
                                    expected[next] |= SELECTED_CORE;
                                    queue.push_back(next);
                                }
                            }
                        }
                        let mut actual = initial.clone();
                        actual[anchor] |= CORE;
                        select_core_spans(
                            &mut actual,
                            width,
                            height,
                            anchor,
                            connectivity,
                            &mut queue,
                            &mut || Ok(()),
                        )
                        .unwrap();
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    fn red() -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        }
    }
    fn black() -> PixelF32 {
        PixelF32 { red: 0.0, ..red() }
    }
    fn settings() -> RenderSettings {
        RenderSettings {
            seed_points: vec![SeedPoint {
                x: 4.0,
                y: 12.0,
                opacity_scale: 0.0,
            }],
            color_threshold_sq: 0.12,
            alpha_threshold: 0.1,
            min_alpha: 0.01,
            selection_source: SelectionSource::InputLayerData,
            color_interpretation: RgbInterpretation::Straight,
            connectivity: Connectivity::Eight,
            stabilize_preview: true,
            preview_gap_radius: 1,
        }
    }
    fn barrier(width: usize, height: usize, gap: std::ops::Range<usize>) -> Vec<PixelF32> {
        let mut pixels = vec![red(); width * height];
        for y in 0..height {
            if !gap.contains(&y) {
                pixels[y * width + width / 2] = black();
            }
        }
        pixels
    }
    fn render(
        source: &[PixelF32],
        w: usize,
        h: usize,
        radius: usize,
        settings: &RenderSettings,
    ) -> Vec<PixelF32> {
        let mut out = source.to_vec();
        mask_pixels(
            &mut out,
            source,
            w,
            h,
            Point { h: 0, v: 0 },
            settings,
            radius,
            || Ok(()),
        )
        .unwrap();
        out
    }

    #[test]
    fn tiny_holes_do_not_expose_the_adjacent_large_region() {
        for gap in [12..13, 12..14] {
            for connectivity in [Connectivity::Four, Connectivity::Eight] {
                let settings = RenderSettings {
                    connectivity,
                    ..settings()
                };
                let source = barrier(32, 24, gap.clone());
                assert_eq!(
                    render(&source, 32, 24, 0, &settings)[12 * 32 + 28].alpha,
                    0.0
                );
                let out = render(&source, 32, 24, 1, &settings);
                assert_eq!(out[12 * 32 + 4].alpha, 0.0);
                assert_eq!(out[12 * 32 + 28].alpha, 1.0);
                assert_eq!(out[4 * 32 + 15].alpha, 0.0, "restore the selected edge");
                assert_eq!(out[4 * 32 + 16].alpha, 1.0, "do not erase the barrier");
            }
        }
    }
    #[test]
    fn broad_passages_are_still_connected() {
        let source = barrier(32, 24, 9..16);
        assert_eq!(
            render(&source, 32, 24, 1, &settings())[12 * 32 + 28].alpha,
            0.0
        );
    }
    #[test]
    fn radius_controls_the_passage_width() {
        let source = barrier(32, 24, 10..14);
        assert_eq!(
            render(&source, 32, 24, 1, &settings())[12 * 32 + 28].alpha,
            0.0
        );
        assert_eq!(
            render(&source, 32, 24, 2, &settings())[12 * 32 + 28].alpha,
            1.0
        );
    }
    #[test]
    fn completely_missing_boundaries_cannot_be_reconstructed() {
        let source = vec![red(); 32 * 24];
        assert!(
            render(&source, 32, 24, 1, &settings())
                .iter()
                .all(|p| p.alpha == 0.0)
        );
    }
    #[test]
    fn seed_on_region_edge_reaches_its_interior() {
        let source = barrier(32, 24, 12..13);
        let settings = RenderSettings {
            seed_points: vec![SeedPoint {
                x: 15.0,
                y: 4.0,
                opacity_scale: 0.0,
            }],
            ..settings()
        };
        let out = render(&source, 32, 24, 1, &settings);
        assert_eq!(out[4 * 32 + 2].alpha, 0.0);
        assert_eq!(out[4 * 32 + 28].alpha, 1.0);
    }
    #[test]
    fn thin_feature_fallback_does_not_jump_to_a_remote_region() {
        let mut source = vec![black(); 32 * 24];
        for x in 0..32 {
            source[12 * 32 + x] = red();
        }
        for y in 0..24 {
            for x in 16..32 {
                source[y * 32 + x] = red();
            }
        }
        let out = render(&source, 32, 24, 1, &settings());
        assert_eq!(out[12 * 32 + 4].alpha, 0.0);
        assert_eq!(out[12 * 32 + 28].alpha, 1.0);
    }
    #[test]
    fn transparent_passages_respect_alpha_threshold() {
        let mut source = vec![red(); 32 * 24];
        for y in 0..24 {
            source[y * 32 + 16].alpha = 0.0;
        }
        source[12 * 32 + 16] = red();
        let out = render(&source, 32, 24, 1, &settings());
        assert_eq!(out[12 * 32 + 28].alpha, 1.0);
    }
    #[test]
    fn multiple_points_keep_minimum_opacity_without_dimming_unselected_pixels() {
        let source = barrier(32, 24, 12..13);
        let settings = RenderSettings {
            seed_points: vec![
                SeedPoint {
                    x: 4.0,
                    y: 12.0,
                    opacity_scale: 0.5,
                },
                SeedPoint {
                    x: 5.0,
                    y: 10.0,
                    opacity_scale: 0.25,
                },
            ],
            ..settings()
        };
        let out = render(&source, 32, 24, 1, &settings);
        assert_eq!(out[12 * 32 + 4].alpha, 0.25);
        assert_eq!(out[12 * 32 + 28].alpha, 1.0);
        assert_eq!(out[12 * 32 + 28].red, 1.0);
    }
    #[test]
    fn full_resolution_and_disabled_protection_use_original_flood_fill() {
        let full = ae::RationalScale { num: 1, den: 1 };
        let quarter = ae::RationalScale { num: 1, den: 4 };
        assert_eq!(preview_radius(&settings(), full, full), 0);
        assert_eq!(preview_radius(&settings(), quarter, quarter), 1);
        assert_eq!(preview_radius(&settings(), full, quarter), 1);
        assert_eq!(
            preview_radius(
                &RenderSettings {
                    stabilize_preview: false,
                    ..settings()
                },
                quarter,
                quarter
            ),
            0
        );
        assert_eq!(
            preview_radius(
                &RenderSettings {
                    preview_gap_radius: 0,
                    ..settings()
                },
                quarter,
                quarter
            ),
            0
        );
    }
    #[test]
    fn cancellation_is_propagated() {
        let source = barrier(32, 24, 12..13);
        assert!(matches!(
            mask_pixels(
                &mut source.clone(),
                &source,
                32,
                24,
                Point { h: 0, v: 0 },
                &settings(),
                1,
                || Err(Error::InterruptCancel)
            ),
            Err(Error::InterruptCancel)
        ));
    }
    #[test]
    #[ignore = "Explicit release-mode timing comparison; no timing assertions"]
    fn benchmark_preview_cost() {
        use std::{hint::black_box, time::Instant};
        for (label, w, h, radius) in [
            ("Full", 2048, 1024, 0),
            ("Quarter OFF", 512, 256, 0),
            ("Quarter ON", 512, 256, 1),
        ] {
            // Full retains a closed wall; sampling introduces one missing row
            // in the preview. This models the same intended region at both sizes.
            let source = barrier(w, h, h / 2..h / 2 + usize::from(radius > 0 || w == 512));
            let settings = RenderSettings {
                seed_points: vec![SeedPoint {
                    x: (w / 4) as f32,
                    y: (h / 2) as f32,
                    opacity_scale: 0.0,
                }],
                ..settings()
            };
            let mut times = Vec::new();
            for _ in 0..9 {
                let start = Instant::now();
                black_box(render(&source, w, h, radius, &settings));
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            println!("{label}: median {:.3} ms; {} pixels", times[4], w * h);
        }
    }
}
