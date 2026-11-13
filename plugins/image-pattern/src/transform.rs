use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    fn rotate(self, angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::new(cos * self.x - sin * self.y, sin * self.x + cos * self.y)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl std::ops::Div<f32> for Vec2 {
    type Output = Self;

    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FeedbackSettings {
    pub step_scale: f32,
    pub step_rotation: f32,
    pub step_offset: Vec2,
    pub vortex: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PolarSettings {
    pub sectors: usize,
    pub mirror: bool,
    pub rings: usize,
    pub twist: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Repeat {
    Tile,
    Mirror,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StripClipping {
    Both,
    NegativeEdge,
    PositiveEdge,
    None,
}

impl StripClipping {
    fn contains(self, y: f32, lower: f32, upper: f32) -> bool {
        match self {
            Self::Both => y >= lower && y < upper,
            Self::NegativeEdge => y >= lower,
            Self::PositiveEdge => y < upper,
            Self::None => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeltaMode {
    Forward,
    Reverse,
    Outward,
    PingPong,
    Random,
}

#[derive(Clone, Copy, Debug)]
pub struct StripDelta {
    pub angle: f32,
    pub width: f32,
    pub offset: Vec2,
    pub mode: DeltaMode,
    pub cycle: i32,
    pub seed: u32,
    pub copies: i32,
    pub split_overlaps: bool,
}

impl Default for StripDelta {
    fn default() -> Self {
        Self {
            angle: 0.0,
            width: 0.0,
            offset: Vec2::default(),
            mode: DeltaMode::Forward,
            cycle: 4,
            seed: 0,
            copies: 32,
            split_overlaps: true,
        }
    }
}

pub fn delta_step(index: i32, mode: DeltaMode, cycle: i32, seed: u32) -> f32 {
    if index == 0 {
        return 0.0;
    }
    let cycle = cycle.clamp(1, 256);
    match mode {
        DeltaMode::Forward => index as f32,
        DeltaMode::Reverse => -(index as f32),
        DeltaMode::Outward => index.unsigned_abs() as f32,
        DeltaMode::PingPong => {
            let phase = index.rem_euclid(2 * cycle);
            phase.min(2 * cycle - phase) as f32
        }
        DeltaMode::Random => {
            let mut hash = (index as u32).wrapping_mul(0x9e3779b9) ^ seed;
            hash ^= hash >> 16;
            hash = hash.wrapping_mul(0x7feb352d);
            hash ^= hash >> 15;
            hash = hash.wrapping_mul(0x846ca68b);
            hash ^= hash >> 16;
            (hash as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32 * cycle as f32
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Boundary {
    position: f32,
    slope: f32,
}

impl Boundary {
    fn new(position: f32, angle: f32) -> Self {
        let (sin, cos) = finite_angle(angle).sin_cos();
        // A near-vertical boundary remains finite in the strip's y(x) chart.
        // Keep the sign so line orientation is periodic through 180 degrees.
        Self {
            position,
            slope: sin / cos.abs().max(1.0e-6).copysign(cos),
        }
    }

    fn height(self, along: f32) -> f32 {
        self.position + self.slope * along
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StripInterval {
    lower: Boundary,
    upper: Boundary,
    band: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct StripSample {
    pub coordinate: Vec2,
    pub direction: Vec2,
    pub index: i32,
}

#[derive(Clone, Debug)]
pub struct StripMapping {
    angle: f32,
    width: f32,
    repeat: Repeat,
    grid_inverse: Matrix,
    source: Matrix,
    boundaries: Vec<Boundary>,
    band_centers: Vec<f32>,
    delta: StripDelta,
    affect: f32,
}

// Line order is constant between crossings. Store only changes for each band,
// rather than sorting all boundaries again for every pixel of every copy.
pub struct StripCopyLookup {
    range: (f32, f32),
    uncertain: Vec<(f64, f64)>,
    bands: Vec<Vec<(f64, Option<StripInterval>)>>,
}

impl StripCopyLookup {
    fn interval(&self, x: f32, band: usize) -> Option<Option<StripInterval>> {
        if x < self.range.0 || x > self.range.1 || !x.is_finite() {
            return None;
        }
        let x = x as f64;
        let n = self.uncertain.partition_point(|range| range.1 < x);
        if self.uncertain.get(n).is_some_and(|range| x >= range.0) {
            return None;
        }
        let runs = self.bands.get(band)?;
        let n = runs.partition_point(|run| run.0 <= x);
        n.checked_sub(1).map(|n| runs[n].1)
    }
}

impl StripMapping {
    pub fn grid_point(&self, point: Vec2, center: Vec2) -> Vec2 {
        self.grid_inverse
            .apply((point - center).rotate(-self.angle))
    }

    pub fn prepare_copies(&self, min_x: f32, max_x: f32) -> StripCopyLookup {
        let mut lookup = StripCopyLookup {
            range: (min_x, max_x),
            uncertain: Vec::new(),
            bands: Vec::new(),
        };
        if self.delta.angle == 0.0
            || self.boundaries.is_empty()
            || !min_x.is_finite()
            || !max_x.is_finite()
            || min_x > max_x
        {
            return lookup;
        }
        let extent = (min_x as f64).abs().max((max_x as f64).abs());
        let mut crossings = Vec::new();
        for (i, a) in self.boundaries.iter().enumerate() {
            for b in &self.boundaries[i + 1..] {
                let slope = a.slope as f64 - b.slope as f64;
                if slope == 0.0 {
                    continue;
                }
                let x = (b.position as f64 - a.position as f64) / slope;
                // Enclose f32 multiply/add rounding and ties. Inside these
                // small ranges use the original exact per-pixel sort.
                let error = 8.0
                    * f32::EPSILON as f64
                    * ((a.position as f64).abs()
                        + (b.position as f64).abs()
                        + ((a.slope as f64).abs() + (b.slope as f64).abs()) * extent
                        + 1.0)
                    / slope.abs();
                let left = (x - error).max(min_x as f64);
                let right = (x + error).min(max_x as f64);
                if left <= right {
                    crossings.push((left, right));
                }
            }
        }
        crossings.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for range in crossings {
            if let Some(last) = lookup.uncertain.last_mut()
                && range.0 <= last.1
            {
                last.1 = last.1.max(range.1);
            } else {
                lookup.uncertain.push(range);
            }
        }
        lookup.bands = vec![Vec::new(); self.band_centers.len()];
        let mut start = min_x as f64;
        for n in 0..=lookup.uncertain.len() {
            let end = lookup.uncertain.get(n).map_or(max_x as f64, |r| r.0);
            if start <= end {
                let x = ((start + end) * 0.5) as f32;
                let intervals = self.intervals_at(x);
                let mut next = intervals.iter().peekable();
                for (band, runs) in lookup.bands.iter_mut().enumerate() {
                    let interval = if next.peek().is_some_and(|i| i.band == band) {
                        next.next().copied()
                    } else {
                        None
                    };
                    if runs.last().is_none_or(|last| last.1 != interval) {
                        runs.push((start, interval));
                    }
                }
            }
            if let Some(range) = lookup.uncertain.get(n) {
                start = range.1;
            }
        }
        lookup
    }

    #[cfg(test)]
    pub fn new(angle: f32, width: f32, repeat: Repeat) -> Self {
        Self::with_controls(
            angle,
            width,
            repeat,
            Transform::default(),
            1.0,
            StripDelta::default(),
        )
    }

    pub fn with_controls(
        angle: f32,
        width: f32,
        repeat: Repeat,
        transform: Transform,
        affect: f32,
        delta: StripDelta,
    ) -> Self {
        let width = positive_size(width, 1.0);
        let mask = transform.matrix(1.0);
        let affect = affect.clamp(0.0, 1.0);
        let source = if affect == 1.0 {
            Transform::default().matrix(1.0)
        } else {
            transform.matrix(affect).inverse().compose(mask)
        };
        let mut boundaries = Vec::new();
        let mut band_centers = Vec::new();
        if delta.angle != 0.0 || delta.width != 0.0 {
            let copies = delta.copies.clamp(1, 256);
            let widths: Vec<_> = (-copies..=copies)
                .map(|index| {
                    (width + delta.width * delta_step(index, delta.mode, delta.cycle, delta.seed))
                        .max(1.0)
                })
                .collect();
            let mut positions = vec![0.0; widths.len() + 1];
            let zero = copies as usize;
            positions[zero] = -width * 0.5;
            positions[zero + 1] = width * 0.5;
            for n in zero + 1..widths.len() {
                positions[n + 1] = positions[n] + widths[n];
            }
            for n in (0..zero).rev() {
                positions[n] = positions[n + 1] - widths[n];
            }
            for n in 0..widths.len() {
                band_centers.push((positions[n] + positions[n + 1]) * 0.5);
            }
            for (n, position) in positions.into_iter().enumerate() {
                // The two edges of the source band retain Angle. Each further
                // shared boundary receives the delta for its outward step.
                let index = n as i32 - copies;
                let step = if index > 0 { index - 1 } else { index };
                let angle = delta.angle * delta_step(step, delta.mode, delta.cycle, delta.seed);
                boundaries.push(Boundary::new(position, angle));
            }
        }
        Self {
            angle: finite_angle(angle),
            width,
            repeat,
            grid_inverse: mask.inverse(),
            source,
            boundaries,
            band_centers,
            delta,
            affect,
        }
    }

    pub fn sample(&self, point: Vec2, center: Vec2) -> Option<StripSample> {
        let grid = self
            .grid_inverse
            .apply((point - center).rotate(-self.angle));
        if !grid.x.is_finite() || !grid.y.is_finite() {
            return None;
        }
        let (local, index, direction) = if self.boundaries.is_empty() {
            let (folded, index) = wrap_cell(grid.y / self.width);
            (
                Vec2::new(grid.x, folded * self.width),
                index as i32,
                Vec2::new(1.0, 0.0),
            )
        } else {
            let interval = self.interval(grid)?;
            let lower = interval.lower.height(grid.x);
            let upper = interval.upper.height(grid.x);
            let span = upper - lower;
            let across = ((grid.y - lower) / span).clamp(0.0, 1.0);
            let anchor = self.band_centers[interval.band];
            let unwarped = grid.y - anchor;
            let fitted = (across - 0.5) * self.width;
            // This blend only mixes positive vertical scales and shear. Its
            // derivative cannot flip or collapse the image between 0 and 100%.
            let local_y = unwarped * (1.0 - self.affect) + fitted * self.affect;
            let slope = interval.lower.slope * (1.0 - across) + interval.upper.slope * across;
            (
                Vec2::new(grid.x, local_y),
                interval.band as i32 - self.band_centers.len() as i32 / 2,
                Vec2::new(1.0, slope * (1.0 - self.affect)),
            )
        };
        Some(self.finish_sample(local, index, direction, center))
    }

    fn finish_sample(&self, local: Vec2, index: i32, direction: Vec2, center: Vec2) -> StripSample {
        let mut local = self.source.apply(local);
        let mut direction = self.source.apply(direction);
        if self.repeat == Repeat::Mirror && index.rem_euclid(2) != 0 {
            local.y = -local.y;
            direction.y = -direction.y;
        }
        let mut coordinate = center + local.rotate(self.angle);
        if self.delta.offset != Vec2::default() {
            // Pan the source image inside the selected region. Neither its
            // boundaries, mirror parity, nor temporal index depend on the pan.
            coordinate = coordinate
                - self.delta.offset
                    * delta_step(index, self.delta.mode, self.delta.cycle, self.delta.seed);
        }
        StripSample {
            coordinate,
            direction: direction.rotate(self.angle),
            index,
        }
    }

    pub fn copy_indices(&self) -> std::ops::RangeInclusive<i32> {
        let copies = self.delta.copies.clamp(1, 256);
        -copies..=copies
    }

    // Return the same intervals used by the closed partition, retaining gaps
    // in repeat numbers when zero-width or folded regions disappear.
    fn intervals_at(&self, x: f32) -> Vec<StripInterval> {
        let mut intervals = Vec::with_capacity(self.band_centers.len());
        if self.delta.split_overlaps {
            let mut boundaries = self.boundaries.clone();
            boundaries.sort_unstable_by(|a, b| a.height(x).total_cmp(&b.height(x)));
            for (band, pair) in boundaries.windows(2).enumerate() {
                if pair[1].height(x) > pair[0].height(x) {
                    intervals.push(StripInterval {
                        lower: pair[0],
                        upper: pair[1],
                        band,
                    });
                }
            }
        } else {
            let center = self.band_centers.len() / 2;
            let mut upper = self.boundaries[center];
            for band in (0..center).rev() {
                let lower = self.boundaries[band];
                if lower.height(x) < upper.height(x) {
                    intervals.push(StripInterval { lower, upper, band });
                    upper = lower;
                }
            }
            intervals.reverse();
            intervals.push(StripInterval {
                lower: self.boundaries[center],
                upper: self.boundaries[center + 1],
                band: center,
            });
            let mut lower = self.boundaries[center + 1];
            for band in center + 1..self.band_centers.len() {
                let upper = self.boundaries[band + 1];
                if upper.height(x) > lower.height(x) {
                    intervals.push(StripInterval { lower, upper, band });
                    lower = upper;
                }
            }
        }
        intervals
    }

    fn open_interval_sample(
        &self,
        grid: Vec2,
        center: Vec2,
        interval: StripInterval,
        clipping: StripClipping,
    ) -> Option<StripSample> {
        let lower = interval.lower.height(grid.x);
        let upper = interval.upper.height(grid.x);
        let span = upper - lower;
        if span <= 0.0 || !span.is_finite() || !clipping.contains(grid.y, lower, upper) {
            return None;
        }
        // Extrapolate the original image mapping through the opened edge.
        // Clamping here would stretch its last source pixel instead.
        let across = (grid.y - lower) / span;
        let unwarped = grid.y - self.band_centers[interval.band];
        let fitted = (across - 0.5) * self.width;
        let slope = interval.lower.slope * (1.0 - across) + interval.upper.slope * across;
        Some(self.finish_sample(
            Vec2::new(
                grid.x,
                unwarped * (1.0 - self.affect) + fitted * self.affect,
            ),
            interval.band as i32 - self.band_centers.len() as i32 / 2,
            Vec2::new(1.0, slope * (1.0 - self.affect)),
            center,
        ))
    }

    fn regular_open_sample(
        &self,
        grid: Vec2,
        center: Vec2,
        index: i32,
        clipping: StripClipping,
    ) -> Option<StripSample> {
        let y = grid.y - index as f32 * self.width;
        clipping
            .contains(y, -self.width * 0.5, self.width * 0.5)
            .then(|| self.finish_sample(Vec2::new(grid.x, y), index, Vec2::new(1.0, 0.0), center))
    }

    pub fn visit_samples(
        &self,
        point: Vec2,
        center: Vec2,
        clipping: StripClipping,
        mut visit: impl FnMut(StripSample),
    ) {
        if clipping == StripClipping::Both {
            if let Some(sample) = self.sample(point, center) {
                visit(sample);
            }
            return;
        }
        let grid = self
            .grid_inverse
            .apply((point - center).rotate(-self.angle));
        if !grid.x.is_finite() || !grid.y.is_finite() {
            return;
        }
        if self.boundaries.is_empty() {
            for index in self.copy_indices() {
                if let Some(sample) = self.regular_open_sample(grid, center, index, clipping) {
                    visit(sample);
                }
            }
        } else if self.delta.angle == 0.0 {
            for (band, pair) in self.boundaries.windows(2).enumerate() {
                let interval = StripInterval {
                    lower: pair[0],
                    upper: pair[1],
                    band,
                };
                if let Some(sample) = self.open_interval_sample(grid, center, interval, clipping) {
                    visit(sample);
                }
            }
        } else {
            for interval in self.intervals_at(grid.x) {
                if let Some(sample) = self.open_interval_sample(grid, center, interval, clipping) {
                    visit(sample);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn sample_copy(
        &self,
        point: Vec2,
        center: Vec2,
        index: i32,
        clipping: StripClipping,
    ) -> Option<StripSample> {
        self.sample_copy_prepared(point, center, index, clipping, None)
    }

    pub fn sample_copy_prepared(
        &self,
        point: Vec2,
        center: Vec2,
        index: i32,
        clipping: StripClipping,
        lookup: Option<&StripCopyLookup>,
    ) -> Option<StripSample> {
        if !self.copy_indices().contains(&index) {
            return None;
        }
        let grid = self
            .grid_inverse
            .apply((point - center).rotate(-self.angle));
        if !grid.x.is_finite() || !grid.y.is_finite() {
            return None;
        }
        if self.boundaries.is_empty() {
            return self.regular_open_sample(grid, center, index, clipping);
        }
        let band = (index + self.band_centers.len() as i32 / 2) as usize;
        let interval = if self.delta.angle == 0.0 {
            StripInterval {
                lower: self.boundaries[band],
                upper: self.boundaries[band + 1],
                band,
            }
        } else if let Some(interval) = lookup.and_then(|lookup| lookup.interval(grid.x, band)) {
            interval?
        } else {
            self.intervals_at(grid.x)
                .into_iter()
                .find(|interval| interval.band == band)?
        };
        self.open_interval_sample(grid, center, interval, clipping)
    }

    fn interval(&self, grid: Vec2) -> Option<StripInterval> {
        if self.delta.split_overlaps {
            // Original arrangement: all crossing lines subdivide the image,
            // and visible regions are numbered by their order at this X.
            let mut below = None;
            let mut above = None;
            let mut lower = f32::NEG_INFINITY;
            let mut upper = f32::INFINITY;
            let mut rank = 0;
            for boundary in &self.boundaries {
                let height = boundary.height(grid.x);
                if height <= grid.y {
                    rank += 1;
                    if height > lower {
                        lower = height;
                        below = Some(*boundary);
                    }
                } else if height < upper {
                    upper = height;
                    above = Some(*boundary);
                }
            }
            return Some(StripInterval {
                lower: below?,
                upper: above?,
                band: rank - 1,
            });
        }

        // Walk outward from the source band in the original repeat order.
        // A boundary folding back into an already occupied region cannot cut
        // that region again. Its repeat collapses until it extends outward.
        let center = self.band_centers.len() / 2;
        if grid.y < -self.width * 0.5 {
            let mut upper = self.boundaries[center];
            for band in (0..center).rev() {
                let lower = self.boundaries[band];
                if lower.height(grid.x) >= upper.height(grid.x) {
                    continue;
                }
                if grid.y >= lower.height(grid.x) {
                    return Some(StripInterval { lower, upper, band });
                }
                upper = lower;
            }
        } else if grid.y >= self.width * 0.5 {
            let mut lower = self.boundaries[center + 1];
            for band in center + 1..self.band_centers.len() {
                let upper = self.boundaries[band + 1];
                if upper.height(grid.x) <= lower.height(grid.x) {
                    continue;
                }
                if grid.y < upper.height(grid.x) {
                    return Some(StripInterval { lower, upper, band });
                }
                lower = upper;
            }
        } else {
            return Some(StripInterval {
                lower: self.boundaries[center],
                upper: self.boundaries[center + 1],
                band: center,
            });
        }
        None
    }

    #[cfg(test)]
    pub fn coordinate(&self, point: Vec2, center: Vec2) -> Vec2 {
        self.sample(point, center)
            .map_or(Vec2::new(f32::NAN, f32::NAN), |s| s.coordinate)
    }

    pub fn index(&self, point: Vec2, center: Vec2) -> i32 {
        let grid = self.grid_point(point, center);
        if !grid.x.is_finite() || !grid.y.is_finite() {
            return 0;
        }
        if self.boundaries.is_empty() {
            wrap_cell(grid.y / self.width).1 as i32
        } else {
            self.interval(grid).map_or(0, |interval| {
                interval.band as i32 - self.band_centers.len() as i32 / 2
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileShape {
    Triangle,
    Quadrilateral,
    Hexagon,
}

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub rotation: f32,
    pub scale: Vec2,
    pub skew: f32,
    pub skew_axis: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            scale: Vec2::new(1.0, 1.0),
            skew: 0.0,
            skew_axis: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Matrix {
    x: Vec2,
    y: Vec2,
}

impl Matrix {
    fn apply(self, p: Vec2) -> Vec2 {
        self.x * p.x + self.y * p.y
    }
    fn compose(self, rhs: Self) -> Self {
        Self {
            x: self.apply(rhs.x),
            y: self.apply(rhs.y),
        }
    }
    fn inverse(self) -> Self {
        let determinant = self.x.x * self.y.y - self.x.y * self.y.x;
        Self {
            x: Vec2::new(self.y.y, -self.x.y) / determinant,
            y: Vec2::new(-self.y.x, self.x.x) / determinant,
        }
    }
}

impl Transform {
    fn matrix(self, amount: f32) -> Matrix {
        // Interpolate parameters, not coordinates: a half-affected 180-degree
        // rotation remains invertible instead of collapsing the image to a point.
        let rotation = if self.rotation.is_finite() {
            self.rotation * amount
        } else {
            0.0
        };
        let scale = Vec2::new(
            positive_size(self.scale.x, 0.001).powf(amount),
            positive_size(self.scale.y, 0.001).powf(amount),
        );
        let axis = finite_angle(self.skew_axis);
        let shear = if self.skew.is_finite() {
            (self
                .skew
                .clamp(-85.0_f32.to_radians(), 85.0_f32.to_radians())
                * amount)
                .tan()
        } else {
            0.0
        };
        let forward = |p: Vec2| {
            let p = Vec2::new(p.x * scale.x, p.y * scale.y).rotate(-axis);
            Vec2::new(p.x + shear * p.y, p.y).rotate(axis + rotation)
        };
        Matrix {
            x: forward(Vec2::new(1.0, 0.0)),
            y: forward(Vec2::new(0.0, 1.0)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TileMapping {
    shape: TileShape,
    size: Vec2,
    repeat: Repeat,
    grid_inverse: Matrix,
    source: Matrix,
}

impl TileMapping {
    pub fn new(
        shape: TileShape,
        size: Vec2,
        repeat: Repeat,
        transform: Transform,
        affect: f32,
    ) -> Self {
        let mask = transform.matrix(1.0);
        let image_inverse = transform.matrix(affect.clamp(0.0, 1.0)).inverse();
        Self {
            shape,
            size: Vec2::new(positive_size(size.x, 1.0), positive_size(size.y, 1.0)),
            repeat,
            grid_inverse: mask.inverse(),
            source: Matrix {
                x: image_inverse.apply(mask.x),
                y: image_inverse.apply(mask.y),
            },
        }
    }

    fn cell(self, point: Vec2, center: Vec2) -> (Vec2, Vec2) {
        let p = self.grid_inverse.apply(point - center);
        let grid = Vec2::new(p.x / self.size.x, p.y / self.size.y);
        let (local, column, row) = match self.shape {
            TileShape::Quadrilateral => {
                let (x, column) = wrap_cell(grid.x);
                let (y, row) = wrap_cell(grid.y);
                (Vec2::new(x, y), column, row)
            }
            TileShape::Triangle => triangle_cell(grid),
            TileShape::Hexagon => hexagon_cell(grid),
        };
        (local, Vec2::new(column, row))
    }

    pub fn index(self, point: Vec2, center: Vec2, origin: Vec2, axis: IndexAxis) -> i32 {
        let (_, cell) = self.cell(point, center);
        let (_, zero) = self.cell(origin, center);
        let d = cell - zero;
        match axis {
            IndexAxis::X => {
                (d.x * if self.shape == TileShape::Triangle {
                    2.0
                } else {
                    1.0
                }) as i32
            }
            IndexAxis::Y => d.y as i32,
            IndexAxis::Distance => match self.shape {
                TileShape::Quadrilateral => d.x.abs().max(d.y.abs()) as i32,
                // Axial lattice distance; half-column triangle IDs distinguish
                // the two triangles that share a lattice parallelogram.
                _ => d.x.abs().max(d.y.abs()).max((d.x + d.y).abs()).ceil() as i32,
            },
        }
    }

    pub fn coordinate(self, point: Vec2, center: Vec2) -> Vec2 {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Vec2::new(f32::NAN, f32::NAN);
        }
        let (local, cell) = self.cell(point, center);
        let mut local = self
            .source
            .apply(Vec2::new(local.x * self.size.x, local.y * self.size.y));
        // Mirror the texture after compensating mask deformation. At Affect=0
        // this remains an orthogonal reflection even for a sheared lattice.
        if self.repeat == Repeat::Mirror {
            if odd_cell(cell.x) {
                local.x = -local.x;
            }
            if odd_cell(cell.y) {
                if self.shape == TileShape::Triangle {
                    local.x = -local.x;
                } else {
                    local.y = -local.y;
                }
            }
        }
        center + local
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexAxis {
    Distance,
    X,
    Y,
}

fn finite_angle(value: f32) -> f32 {
    if value.is_finite() {
        value.rem_euclid(TAU)
    } else {
        0.0
    }
}

fn positive_size(value: f32, minimum: f32) -> f32 {
    if value.is_finite() {
        value.abs().max(minimum)
    } else {
        minimum
    }
}

fn wrap_cell(value: f32) -> (f32, f32) {
    let cell = (value + 0.5).floor();
    (value - cell, cell)
}

fn odd_cell(cell: f32) -> bool {
    cell.rem_euclid(2.0) >= 1.0
}

fn triangle_cell(point: Vec2) -> (Vec2, f32, f32) {
    // Split a triangular-lattice parallelogram into two triangles. The
    // source triangle's centroid is zero and its vertices are
    // (-1/2,-1/3), (1/2,-1/3), (0,2/3). Rotate inverted cells by 180 degrees
    // into this same source triangle. Every point belongs to one cell.
    let p = point + Vec2::new(0.5, 1.0 / 3.0);
    let u = p.x - 0.5 * p.y;
    let column = u.floor();
    let row = p.y.floor();
    let mut local = p - Vec2::new(column + 0.5 * row, row);
    let inverted = u - column + p.y - row > 1.0;
    if inverted {
        local = Vec2::new(1.5, 1.0) - local;
    }
    (
        local - Vec2::new(0.5, 1.0 / 3.0),
        column + if inverted { 0.5 } else { 0.0 },
        row,
    )
}

fn hexagon_cell(point: Vec2) -> (Vec2, f32, f32) {
    // Voronoi cells of the triangular lattice, in coordinates normalized to
    // a point-up hexagon's bounding box (width=height=1). Weighted distance
    // restores the regular hexagon metric before any user scale/skew.
    let nearest_row = (point.y / 0.75).round();
    let mut best_distance = f32::INFINITY;
    let mut best = (Vec2::new(0.0, 0.0), 0.0, 0.0);
    for offset in -1..=1 {
        let row = nearest_row + offset as f32;
        let column = (point.x - row * 0.5).round();
        let local = point - Vec2::new(column + row * 0.5, row * 0.75);
        let distance = 3.0 * local.x * local.x + 4.0 * local.y * local.y;
        if distance < best_distance {
            best_distance = distance;
            best = (local, column, row);
        }
    }
    best
}

pub fn feedback_coordinate(
    point: Vec2,
    center: Vec2,
    iteration: usize,
    settings: FeedbackSettings,
    max_radius: f32,
) -> Vec2 {
    if iteration == 0 {
        return point;
    }
    let offset = point - center;
    let iteration_f = iteration as f32;
    let scale = settings.step_scale.abs().max(1.0e-4).powf(iteration_f);
    let radius_normalized = offset.length() / max_radius.max(1.0);
    let angle = -iteration_f
        * (settings.step_rotation + settings.vortex * radius_normalized.clamp(0.0, 4.0));
    let (sin, cos) = angle.sin_cos();
    let scaled = offset / scale;
    let rotated = Vec2::new(
        scaled.x * cos - scaled.y * sin,
        scaled.x * sin + scaled.y * cos,
    );
    center + rotated - settings.step_offset * iteration_f
}

pub fn polar_coordinate(
    point: Vec2,
    center: Vec2,
    settings: PolarSettings,
    max_radius: f32,
) -> Vec2 {
    let offset = point - center;
    let mut radius = offset.length();
    let radius_normalized = radius / max_radius.max(1.0);
    let mut angle = offset.y.atan2(offset.x) + settings.twist * radius_normalized;

    let sectors = settings.sectors.max(1);
    let sector_width = TAU / sectors as f32;
    let half_sector = sector_width * 0.5;
    let sector_index = ((angle + half_sector) / sector_width).floor() as i64;
    angle = (angle + half_sector).rem_euclid(sector_width) - half_sector;
    if settings.mirror && sector_index.rem_euclid(2) != 0 {
        angle = -angle;
    }

    if settings.rings > 0 {
        let period = max_radius.max(1.0) / settings.rings as f32;
        let folded = radius.rem_euclid(period * 2.0);
        radius = if folded <= period {
            folded
        } else {
            period * 2.0 - folded
        };
    }

    center + Vec2::new(angle.cos() * radius, angle.sin() * radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn assert_vec_near(actual: Vec2, expected: Vec2) {
        assert!((actual.x - expected.x).abs() < 1.0e-4, "x: {actual:?}");
        assert!((actual.y - expected.y).abs() < 1.0e-4, "y: {actual:?}");
    }

    #[test]
    fn feedback_first_iteration_is_identity() {
        let point = Vec2::new(123.0, 45.0);
        let result = feedback_coordinate(
            point,
            Vec2::new(50.0, 50.0),
            0,
            FeedbackSettings {
                step_scale: 0.8,
                step_rotation: PI / 4.0,
                step_offset: Vec2::new(12.0, -4.0),
                vortex: PI,
            },
            100.0,
        );
        assert_eq!(result, point);
    }

    #[test]
    fn one_sector_polar_mapping_is_identity() {
        let point = Vec2::new(80.0, 30.0);
        let center = Vec2::new(50.0, 50.0);
        let result = polar_coordinate(
            point,
            center,
            PolarSettings {
                sectors: 1,
                mirror: false,
                rings: 0,
                twist: 0.0,
            },
            100.0,
        );
        assert_vec_near(result, point);
    }
}

#[cfg(test)]
mod prepared_tests {
    use super::*;

    #[test]
    fn prepared_intervals_match_exact_sort_at_crossings_and_extreme_angles() {
        for mode in [
            DeltaMode::Forward,
            DeltaMode::Reverse,
            DeltaMode::Outward,
            DeltaMode::PingPong,
            DeltaMode::Random,
        ] {
            for split_overlaps in [false, true] {
                for angle in [0.0, 0.07, 0.5_f32.atan(), std::f32::consts::FRAC_PI_2] {
                    let mapping = StripMapping::with_controls(
                        0.0,
                        40.0,
                        Repeat::Mirror,
                        Transform::default(),
                        0.5,
                        StripDelta {
                            angle,
                            width: 3.0,
                            copies: 8,
                            mode,
                            split_overlaps,
                            offset: Vec2::new(7.0, -9.0),
                            ..StripDelta::default()
                        },
                    );
                    let lookup = mapping.prepare_copies(-2000.0, 2000.0);
                    let mut xs: Vec<f32> = (-25..=25).map(|x| x as f32 * 79.125).collect();
                    for (n, a) in mapping.boundaries.iter().enumerate() {
                        for b in &mapping.boundaries[n + 1..] {
                            let x = (b.position as f64 - a.position as f64)
                                / (a.slope as f64 - b.slope as f64);
                            if x.is_finite() && x.abs() <= 2000.0 {
                                let x = x as f32;
                                xs.extend([x.next_down(), x, x.next_up()]);
                            }
                        }
                    }
                    for x in xs {
                        for y in [-77.25, 0.0, 123.75] {
                            let p = Vec2::new(x, y);
                            assert_eq!(
                                mapping.index(p, Vec2::default()),
                                mapping.sample(p, Vec2::default()).map_or(0, |s| s.index)
                            );
                            for i in mapping.copy_indices() {
                                for clipping in [
                                    StripClipping::NegativeEdge,
                                    StripClipping::PositiveEdge,
                                    StripClipping::None,
                                ] {
                                    let exact =
                                        mapping.sample_copy(p, Vec2::default(), i, clipping);
                                    let fast = mapping.sample_copy_prepared(
                                        p,
                                        Vec2::default(),
                                        i,
                                        clipping,
                                        Some(&lookup),
                                    );
                                    assert_eq!(
                                        exact.is_some(),
                                        fast.is_some(),
                                        "{mode:?} {angle} {split_overlaps} {p:?} {i}"
                                    );
                                    if let (Some(a), Some(b)) = (exact, fast) {
                                        assert_eq!(a.coordinate, b.coordinate);
                                        assert_eq!(a.direction, b.direction);
                                        assert_eq!(a.index, b.index);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
