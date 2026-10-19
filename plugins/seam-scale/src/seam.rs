// SPDX-License-Identifier: MPL-2.0
//! Exact, sequential minimum-energy seam carving. No resolution proxies or seam batches.

pub type Pixel = [f32; 4];
type Feature = [f64; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Rgb,
    Rgba,
    Red,
    Green,
    Blue,
    Alpha,
    Lightness,
}

impl Channel {
    pub fn from_popup(value: i32) -> Self {
        match value {
            1 => Self::Rgb,
            3 => Self::Red,
            4 => Self::Green,
            5 => Self::Blue,
            6 => Self::Alpha,
            7 => Self::Lightness,
            _ => Self::Rgba,
        }
    }
}
#[path = "linked.rs"]
mod linked;
use linked::LinkedCarver;
pub const MAX_PIXELS: usize = 32 * 1024 * 1024;
pub const MAX_DIMENSION: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarveError {
    InvalidSize,
    OutOfMemory,
    Cancelled,
}

pub fn area(width: usize, height: usize) -> Result<usize, CarveError> {
    let size = width.checked_mul(height).ok_or(CarveError::InvalidSize)?;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || size > MAX_PIXELS
    {
        return Err(CarveError::InvalidSize);
    }
    Ok(size)
}

pub fn buffer<T: Clone>(size: usize, value: T) -> Result<Vec<T>, CarveError> {
    let mut out = Vec::new();
    out.try_reserve_exact(size)
        .map_err(|_| CarveError::OutOfMemory)?;
    out.resize(size, value);
    Ok(out)
}

#[derive(Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Pixel>,
    coordinates: Option<Vec<[f32; 2]>>,
}

impl Image {
    pub fn new(width: usize, height: usize, pixels: Vec<Pixel>) -> Result<Self, CarveError> {
        if area(width, height)? != pixels.len() {
            return Err(CarveError::InvalidSize);
        }
        Ok(Self {
            width,
            height,
            pixels,
            coordinates: None,
        })
    }

    /// Track source coordinates independently of the color used for seam energy.
    pub fn track_coordinates(
        &mut self,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<(), CarveError> {
        let mut coordinates = buffer(self.pixels.len(), [0.0; 2])?;
        for y in 0..self.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            for x in 0..self.width {
                coordinates[y * self.width + x] = [x as f32, y as f32];
            }
        }
        self.coordinates = Some(coordinates);
        Ok(())
    }

    fn blank_like(&self, width: usize, height: usize) -> Result<Self, CarveError> {
        let n = area(width, height)?;
        let mut image = Self::new(width, height, buffer(n, [0.0; 4])?)?;
        if self.coordinates.is_some() {
            image.coordinates = Some(buffer(n, [0.0; 2])?);
        }
        Ok(image)
    }

    fn copy_sample(&mut self, to: usize, original: &Self, from: usize) {
        self.pixels[to] = original.pixels[from];
        if let (Some(dst), Some(src)) = (&mut self.coordinates, &original.coordinates) {
            dst[to] = src[from];
        }
    }

    /// Replace colors only after all seam decisions, including the second axis.
    pub fn into_coordinate_image(
        mut self,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<Self, CarveError> {
        let coordinates = self.coordinates.take().ok_or(CarveError::InvalidSize)?;
        for y in 0..self.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            for x in 0..self.width {
                let i = y * self.width + x;
                let [sx, sy] = coordinates[i];
                self.pixels[i] = [sx, sy, 1.0, 1.0];
            }
        }
        Ok(self)
    }

    /// Flip after carving so signed scales retain the positive scale's exact seams.
    pub fn flip(
        &mut self,
        axes: [bool; 2],
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<(), CarveError> {
        if axes[0] {
            for (y, row) in self.pixels.chunks_exact_mut(self.width).enumerate() {
                if y % 32 == 0 {
                    check_cancel(cancel)?;
                }
                row.reverse();
                if let Some(coordinates) = &mut self.coordinates {
                    coordinates[y * self.width..(y + 1) * self.width].reverse();
                }
            }
        }
        if axes[1] {
            for y in 0..self.height / 2 {
                if y % 32 == 0 {
                    check_cancel(cancel)?;
                }
                let opposite = (self.height - 1 - y) * self.width;
                let (top, bottom) = self.pixels.split_at_mut(opposite);
                top[y * self.width..(y + 1) * self.width]
                    .swap_with_slice(&mut bottom[..self.width]);
                if let Some(coordinates) = &mut self.coordinates {
                    let (top, bottom) = coordinates.split_at_mut(opposite);
                    top[y * self.width..(y + 1) * self.width]
                        .swap_with_slice(&mut bottom[..self.width]);
                }
            }
        }
        Ok(())
    }

    fn transpose(self, cancel: &mut impl FnMut() -> bool) -> Result<Self, CarveError> {
        let mut output = self.blank_like(self.height, self.width)?;
        // Tiling keeps both sides of a large transpose in cache.
        for by in (0..self.height).step_by(32) {
            check_cancel(cancel)?;
            for bx in (0..self.width).step_by(32) {
                for y in by..(by + 32).min(self.height) {
                    for x in bx..(bx + 32).min(self.width) {
                        output.copy_sample(x * self.height + y, &self, y * self.width + x);
                    }
                }
            }
        }
        Ok(output)
    }
}

fn check_cancel(cancel: &mut impl FnMut() -> bool) -> Result<(), CarveError> {
    if cancel() {
        Err(CarveError::Cancelled)
    } else {
        Ok(())
    }
}

fn finite(v: f32) -> f64 {
    if v.is_finite() { f64::from(v) } else { 0.0 }
}

fn feature(pixel: Pixel, channel: Channel) -> Feature {
    // Use the host's premultiplied working RGB. Invisible RGB has no energy.
    let alpha = finite(pixel[3]).clamp(0.0, 1.0);
    let [r, g, b] = if alpha > 0.0 {
        [finite(pixel[0]), finite(pixel[1]), finite(pixel[2])]
    } else {
        [0.0; 3]
    };
    match channel {
        Channel::Rgb => [r, g, b],
        // RGBA includes Lightness + Alpha, preserving the original plugin's look.
        Channel::Rgba => [0.2126 * r + 0.7152 * g + 0.0722 * b, alpha, 0.0],
        Channel::Red => [r, 0.0, 0.0],
        Channel::Green => [g, 0.0, 0.0],
        Channel::Blue => [b, 0.0, 0.0],
        Channel::Alpha => [alpha, 0.0, 0.0],
        Channel::Lightness => [0.2126 * r + 0.7152 * g + 0.0722 * b, 0.0, 0.0],
    }
}

fn energy(left: Feature, right: Feature, up: Feature, down: Feature) -> f64 {
    // Preserve the legacy addition order, including exact floating-point ties.
    (right[0] - left[0]).abs()
        + (down[0] - up[0]).abs()
        + (right[1] - left[1]).abs()
        + (down[1] - up[1]).abs()
        + (right[2] - left[2]).abs()
        + (down[2] - up[2]).abs()
}

struct Carver {
    width: usize,
    height: usize,
    stride: usize,
    rows: Vec<usize>,
    features: Vec<Feature>,
    indices: Vec<u32>,
    energy: Vec<f64>,
    parents: Vec<i8>,
    previous: Vec<f64>,
    current: Vec<f64>,
    seam: Vec<usize>,
}

impl Carver {
    fn new(
        image: &Image,
        channel: Channel,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<Self, CarveError> {
        let n = image.pixels.len();
        let mut this = Self {
            width: image.width,
            height: image.height,
            stride: image.width,
            rows: buffer(image.height, 0)?,
            features: buffer(n, [0.0; 3])?,
            indices: buffer(n, 0)?,
            energy: buffer(n, 0.0)?,
            parents: buffer(n, 0)?,
            previous: buffer(image.width + 2, f64::INFINITY)?,
            current: buffer(image.width + 2, f64::INFINITY)?,
            seam: buffer(image.height, 0)?,
        };
        for y in 0..image.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            this.rows[y] = y * image.width;
            for x in 0..image.width {
                let i = y * image.width + x;
                this.features[i] = feature(image.pixels[i], channel);
                this.indices[i] = i as u32;
            }
        }
        this.recompute_energy(cancel)?;
        Ok(this)
    }

    fn energy_at(&self, x: usize, y: usize) -> f64 {
        let left = self.features[self.indices[self.rows[y] + x.saturating_sub(1)] as usize];
        let right =
            self.features[self.indices[self.rows[y] + (x + 1).min(self.width - 1)] as usize];
        let up = self.features[self.indices[self.rows[y.saturating_sub(1)] + x] as usize];
        let down =
            self.features[self.indices[self.rows[(y + 1).min(self.height - 1)] + x] as usize];
        energy(left, right, up, down)
    }

    fn recompute_energy(&mut self, cancel: &mut impl FnMut() -> bool) -> Result<(), CarveError> {
        for y in 0..self.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            for x in 0..self.width {
                self.energy[self.rows[y] + x] = self.energy_at(x, y);
            }
        }
        Ok(())
    }

    fn find_seam(&mut self, cancel: &mut impl FnMut() -> bool) -> Result<(), CarveError> {
        let w = self.width;
        self.previous[1..=w].copy_from_slice(&self.energy[self.rows[0]..self.rows[0] + w]);
        self.previous[0] = f64::INFINITY;
        self.previous[w + 1] = f64::INFINITY;
        self.current[0] = f64::INFINITY;
        self.current[w + 1] = f64::INFINITY;
        for y in 1..self.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let row = y * self.stride;
            dp_row(
                &self.previous[..w + 2],
                &self.energy[self.rows[y]..self.rows[y] + w],
                &mut self.current[1..=w],
                &mut self.parents[row..row + w],
            );
            std::mem::swap(&mut self.previous, &mut self.current);
        }
        let mut x = 0;
        for candidate in 1..w {
            if self.previous[candidate + 1] < self.previous[x + 1] {
                x = candidate;
            }
        }
        for y in (0..self.height).rev() {
            self.seam[y] = x;
            if y > 0 {
                x = (x as isize + self.parents[y * self.stride + x] as isize) as usize;
            }
        }
        Ok(())
    }

    fn remove_seam(&mut self) {
        debug_assert!(self.width > 1);
        for y in 0..self.height {
            let row = self.rows[y];
            let x = row + self.seam[y];
            // Keep each row contiguous, moving the shorter side toward the gap.
            // Advancing the row's start avoids copying a long suffix near the left edge.
            if self.seam[y] < self.width / 2 {
                self.indices.copy_within(row..x, row + 1);
                self.energy.copy_within(row..x, row + 1);
                self.rows[y] += 1;
            } else {
                self.indices.copy_within(x + 1..row + self.width, x);
                self.energy.copy_within(x + 1..row + self.width, x);
            }
        }
        self.width -= 1;
        for y in 0..self.height {
            // Only these central-difference stencils can have changed. Include
            // adjacent rows' seams; never reuse energy across a changed stencil.
            let up = self.seam[y.saturating_sub(1)];
            let down = self.seam[(y + 1).min(self.height - 1)];
            let lo = self.seam[y].min(up).min(down).saturating_sub(1);
            let hi = (self.seam[y].max(up).max(down) + 1).min(self.width - 1);
            for x in lo..=hi {
                self.energy[self.rows[y] + x] = self.energy_at(x, y);
            }
        }
    }
}

fn dp_row(previous: &[f64], energy: &[f64], current: &mut [f64], parents: &mut [i8]) {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime feature detection gates this function; it bounds every load/store.
        unsafe {
            dp_row_avx2(previous, energy, current, parents);
        }
        return;
    }
    dp_row_scalar(previous, energy, current, parents);
}

fn dp_row_scalar(previous: &[f64], energy: &[f64], current: &mut [f64], parents: &mut [i8]) {
    // Padded boundaries and independent lanes allow LLVM to vectorize this
    // exact recurrence. Strict comparisons preserve leftmost ties on all CPUs.
    for (((neighbors, &e), cost), parent) in
        previous.windows(3).zip(energy).zip(current).zip(parents)
    {
        let left = neighbors[0];
        let center = neighbors[1];
        let right = neighbors[2];
        let use_center = center < left;
        let best = if use_center { center } else { left };
        let use_right = right < best;
        *cost = e + if use_right { right } else { best };
        *parent = if use_right {
            1
        } else if use_center {
            0
        } else {
            -1
        };
    }
}

#[cfg(target_arch = "x86_64")]
static DP_DIRECTIONS: [[i8; 4]; 256] = {
    let mut table = [[0; 4]; 256];
    let mut mask = 0;
    while mask < 256 {
        let mut lane = 0;
        while lane < 4 {
            table[mask][lane] = if mask & (1 << (lane + 4)) != 0 {
                1
            } else if mask & (1 << lane) != 0 {
                0
            } else {
                -1
            };
            lane += 1;
        }
        mask += 1;
    }
    table
};

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dp_row_avx2(previous: &[f64], energy: &[f64], current: &mut [f64], parents: &mut [i8]) {
    use std::arch::x86_64::*;
    let n = energy.len();
    assert_eq!(previous.len(), n + 2);
    assert_eq!(current.len(), n);
    assert_eq!(parents.len(), n);
    let mut x = 0;
    // No fused arithmetic, quantization or precision reduction: four independent
    // f64 DP nodes use exactly the scalar recurrence and its strict tie rule.
    unsafe {
        macro_rules! step {
            ($x:expr) => {{
                let lane = $x;
                let left = _mm256_loadu_pd(previous.as_ptr().add(lane));
                let center = _mm256_loadu_pd(previous.as_ptr().add(lane + 1));
                let right = _mm256_loadu_pd(previous.as_ptr().add(lane + 2));
                let use_center = _mm256_cmp_pd::<_CMP_LT_OQ>(center, left);
                // MINPD keeps its second operand on ties, including signed zero.
                let best = _mm256_min_pd(center, left);
                let use_right = _mm256_cmp_pd::<_CMP_LT_OQ>(right, best);
                let best = _mm256_min_pd(right, best);
                let e = _mm256_loadu_pd(energy.as_ptr().add(lane));
                _mm256_storeu_pd(current.as_mut_ptr().add(lane), _mm256_add_pd(e, best));
                // Extract decisions directly; converting f64 directions to packed
                // bytes adds unnecessary floating-point work to every DP node.
                let mask = (_mm256_movemask_pd(use_right) << 4) | _mm256_movemask_pd(use_center);
                std::ptr::copy_nonoverlapping(
                    DP_DIRECTIONS[mask as usize].as_ptr(),
                    parents.as_mut_ptr().add(lane),
                    4,
                );
            }};
        }
        // Independent groups keep multiple exact DP chains in flight.
        while x + 16 <= n {
            step!(x);
            step!(x + 4);
            step!(x + 8);
            step!(x + 12);
            x += 16;
        }
        while x + 4 <= n {
            step!(x);
            x += 4;
        }
    }
    dp_row_scalar(
        &previous[x..],
        &energy[x..],
        &mut current[x..],
        &mut parents[x..],
    );
}

fn resize_width(
    mut image: Image,
    target: usize,
    channel: Channel,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    if target == image.width {
        return Ok(image);
    }
    if target < image.width {
        if image.width - target < 8 {
            return shrink_packed(image, target, channel, cancel);
        }
        let mut carver = match LinkedCarver::new(&image, channel, cancel) {
            Ok(carver) => carver,
            Err(CarveError::OutOfMemory) => return shrink_packed(image, target, channel, cancel),
            Err(error) => return Err(error),
        };
        while carver.width > target {
            check_cancel(cancel)?;
            carver.find_seam(cancel)?;
            if carver.updated_nodes > carver.width * image.height / 8 {
                // Dense propagation favors contiguous SIMD. Both paths solve the
                // same recurrence; switching never changes energy, ties or pixels.
                let remaining = carver.image(&image)?;
                drop(carver);
                drop(image);
                return shrink_packed(remaining, target, channel, cancel);
            }
            carver.remove_seam();
        }
        return carver.image(&image);
    }
    while image.width < target {
        check_cancel(cancel)?;
        if image.width == 1 {
            let mut output = image.blank_like(2, image.height)?;
            for y in 0..image.height {
                output.copy_sample(y * 2, &image, y);
                output.copy_sample(y * 2 + 1, &image, y);
            }
            image = output;
            continue;
        }
        // Discover seams by sequential removal on a working copy, mapping each
        // back to the original image. This prevents reinserting the same seam.
        let count = (target - image.width).min(image.width - 1);
        let mut selected = buffer(image.pixels.len(), false)?;
        {
            let mut carver = Carver::new(&image, channel, cancel)?;
            for _ in 0..count {
                check_cancel(cancel)?;
                carver.find_seam(cancel)?;
                for y in 0..image.height {
                    selected[carver.indices[carver.rows[y] + carver.seam[y]] as usize] = true;
                }
                carver.remove_seam();
            }
        }
        let width = image.width + count;
        let mut output = image.blank_like(width, image.height)?;
        for y in 0..image.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let mut out = y * width;
            for x in 0..image.width {
                let i = y * image.width + x;
                output.copy_sample(out, &image, i);
                out += 1;
                if selected[i] {
                    let neighbor = if x + 1 < image.width { i + 1 } else { i - 1 };
                    output.pixels[out] = std::array::from_fn(|c| {
                        image.pixels[i][c] * 0.5 + image.pixels[neighbor][c] * 0.5
                    });
                    if let (Some(dst), Some(src)) = (&mut output.coordinates, &image.coordinates) {
                        dst[out] =
                            std::array::from_fn(|c| src[i][c] * 0.5 + src[neighbor][c] * 0.5);
                    }
                    out += 1;
                }
            }
        }
        image = output;
    }
    Ok(image)
}

fn shrink_packed(
    image: Image,
    target: usize,
    channel: Channel,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    let mut carver = Carver::new(&image, channel, cancel)?;
    while carver.width > target {
        check_cancel(cancel)?;
        carver.find_seam(cancel)?;
        carver.remove_seam();
    }
    let mut output = image.blank_like(target, image.height)?;
    for y in 0..image.height {
        if y % 32 == 0 {
            check_cancel(cancel)?;
        }
        for x in 0..target {
            output.copy_sample(
                y * target + x,
                &image,
                carver.indices[carver.rows[y] + x] as usize,
            );
        }
    }
    Ok(output)
}

pub fn resize(
    image: Image,
    width: usize,
    height: usize,
    channel: Channel,
    mut cancel: impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    area(width, height)?;
    area(width, image.height)?;
    check_cancel(&mut cancel)?;
    // Fixed X -> Y order: changing the scale-link checkbox never changes seams.
    let image = resize_width(image, width, channel, &mut cancel)?;
    if image.height == height {
        return Ok(image);
    }
    resize_width(image.transpose(&mut cancel)?, height, channel, &mut cancel)?
        .transpose(&mut cancel)
}

/// Pixel-aligned centering when not restoring; half-pixel bilinear resampling otherwise.
pub fn sample_output(image: &Image, canvas: [usize; 2], xy: [i64; 2], restore: bool) -> Pixel {
    if xy[0] < 0 || xy[1] < 0 || xy[0] >= canvas[0] as i64 || xy[1] >= canvas[1] as i64 {
        return [0.0; 4];
    }
    if !restore {
        let x = xy[0] - (canvas[0] as i64 - image.width as i64).div_euclid(2);
        let y = xy[1] - (canvas[1] as i64 - image.height as i64).div_euclid(2);
        if x < 0 || y < 0 || x >= image.width as i64 || y >= image.height as i64 {
            return [0.0; 4];
        }
        return image.pixels[y as usize * image.width + x as usize];
    }
    let x = ((xy[0] as f64 + 0.5) * image.width as f64 / canvas[0] as f64 - 0.5)
        .clamp(0.0, (image.width - 1) as f64);
    let y = ((xy[1] as f64 + 0.5) * image.height as f64 / canvas[1] as f64 - 0.5)
        .clamp(0.0, (image.height - 1) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(image.width - 1);
    let y1 = (y0 + 1).min(image.height - 1);
    let tx = (x - x0 as f64) as f32;
    let ty = (y - y0 as f64) as f32;
    std::array::from_fn(|c| {
        let a = image.pixels[y0 * image.width + x0][c] * (1.0 - tx)
            + image.pixels[y0 * image.width + x1][c] * tx;
        let b = image.pixels[y1 * image.width + x0][c] * (1.0 - tx)
            + image.pixels[y1 * image.width + x1][c] * tx;
        a * (1.0 - ty) + b * ty
    })
}

/// Encode the inverse sampling field for AE's Displacement Map effect.
/// R/G use the canvas width/height as maximum displacement; B is coverage.
/// Keep alpha opaque so transparent padding cannot premultiply the neutral map.
pub fn sample_displacement_output(
    image: &Image,
    canvas: [usize; 2],
    xy: [i64; 2],
    restore: bool,
) -> Pixel {
    let sample = sample_output(image, canvas, xy, restore);
    if sample[3] == 0.0 {
        return [0.5, 0.5, 0.0, 1.0];
    }
    [
        (0.5 + (f64::from(sample[0]) - xy[0] as f64) / (2.0 * canvas[0] as f64)) as f32,
        (0.5 + (f64::from(sample[1]) - xy[1] as f64) / (2.0 * canvas[1] as f64)) as f32,
        sample[2],
        1.0,
    ]
}

#[cfg(test)]
#[path = "seam_tests.rs"]
mod tests;
