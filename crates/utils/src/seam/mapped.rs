// SPDX-License-Identifier: MPL-2.0
//! Per-pixel width queries sharing the same exact, sequential seam decisions.

use super::{CarveError, Carver, Channel, Image, Pixel, area, buffer, check_cancel};

const END: u32 = u32::MAX;

#[path = "mapped_anchor.rs"]
mod anchor;
pub use anchor::{Settings, apply, scale_at};

fn payload(image: &Image, index: usize, coordinates: bool) -> Pixel {
    if coordinates {
        let [x, y] = image.coordinates.as_ref().unwrap()[index];
        [x, y, 1.0, 1.0]
    } else {
        image.pixels[index]
    }
}

struct Queries<'a> {
    widths: &'a [i32],
    heads: Vec<u32>,
    next: Vec<u32>,
    canvas_width: usize,
    restore: bool,
    coordinates: bool,
}

impl<'a> Queries<'a> {
    fn new(
        image: &Image,
        widths: &'a [i32],
        restore: bool,
        coordinates: bool,
    ) -> Result<Self, CarveError> {
        if widths.len() != image.pixels.len() || widths.contains(&0) {
            return Err(CarveError::InvalidSize);
        }
        let maximum = widths
            .iter()
            .map(|v| v.unsigned_abs() as usize)
            .max()
            .unwrap();
        area(maximum, image.height)?;
        let mut heads = buffer(maximum + 1, END)?;
        let mut next = buffer(widths.len(), END)?;
        for (i, &width) in widths.iter().enumerate() {
            let width = width.unsigned_abs() as usize;
            next[i] = heads[width];
            heads[width] = i as u32;
        }
        Ok(Self {
            widths,
            heads,
            next,
            canvas_width: image.width,
            restore,
            coordinates,
        })
    }

    fn fill(
        &self,
        width: usize,
        output: &mut Image,
        sample: impl Fn(usize, usize) -> Pixel,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<(), CarveError> {
        let Some(&head) = self.heads.get(width) else {
            return Ok(());
        };
        let mut index = head;
        let mut count = 0;
        while index != END {
            if count % 4096 == 0 {
                check_cancel(cancel)?;
            }
            count += 1;
            let i = index as usize;
            let x = i % self.canvas_width;
            let y = i / self.canvas_width;
            let position = if self.restore {
                Some(
                    ((x as f64 + 0.5) * width as f64 / self.canvas_width as f64 - 0.5)
                        .clamp(0.0, (width - 1) as f64),
                )
            } else {
                let x = x as i64 - (self.canvas_width as i64 - width as i64).div_euclid(2);
                (x >= 0 && x < width as i64).then_some(x as f64)
            };
            if let Some(position) = position {
                let x0 = position.floor() as usize;
                let x1 = (x0 + 1).min(width - 1);
                let t = (position - x0 as f64) as f32;
                // Flip the discrete result before resampling, as in SeamScale.
                let column = |x| if self.widths[i] < 0 { width - 1 - x } else { x };
                let a = sample(column(x0), y);
                let b = sample(column(x1), y);
                output.pixels[i] = std::array::from_fn(|c| a[c] * (1.0 - t) + b[c] * t);
            } else if self.coordinates {
                // Continue padding outside the source domain so it is transparent
                // until an anchor's positional correction brings it into view.
                let local = x as i64 - (self.canvas_width as i64 - width as i64).div_euclid(2);
                let source = if local < 0 {
                    local
                } else {
                    self.canvas_width as i64 - 1 + local - (width as i64 - 1)
                };
                let source = if self.widths[i] < 0 {
                    self.canvas_width as i64 - 1 - source
                } else {
                    source
                };
                output.pixels[i] = [source as f32, y as f32, 1.0, 1.0];
            }
            index = self.next[i];
        }
        Ok(())
    }
}

/// One original sample per column, with an optional inserted sample after it.
/// Order statistics avoid rebuilding an entire image at every queried width.
struct Insertions {
    width: usize,
    tree: Vec<u32>,
    selected: Vec<bool>,
}

impl Insertions {
    fn new(width: usize, height: usize) -> Result<Self, CarveError> {
        let mut tree = buffer((width + 1) * height, 0)?;
        for row in tree.chunks_exact_mut(width + 1) {
            for (i, value) in row.iter_mut().enumerate().skip(1) {
                *value = (i & i.wrapping_neg()) as u32;
            }
        }
        Ok(Self {
            width,
            tree,
            selected: buffer(width * height, false)?,
        })
    }

    fn insert(&mut self, source_index: usize) {
        debug_assert!(!self.selected[source_index]);
        self.selected[source_index] = true;
        let row = source_index / self.width * (self.width + 1);
        let mut i = source_index % self.width + 1;
        while i <= self.width {
            self.tree[row + i] += 1;
            i += i & i.wrapping_neg();
        }
    }

    fn sample(&self, image: &Image, column: usize, y: usize, coordinates: bool) -> Pixel {
        let row = y * (self.width + 1);
        let mut rank = column as u32;
        let mut index = 0;
        let mut step = 1usize << self.width.ilog2();
        while step > 0 {
            let next = index + step;
            if next <= self.width && self.tree[row + next] <= rank {
                rank -= self.tree[row + next];
                index = next;
            }
            step >>= 1;
        }
        debug_assert!(index < self.width);
        let i = y * self.width + index;
        if rank == 0 {
            return payload(image, i, coordinates);
        }
        debug_assert!(rank == 1 && self.selected[i]);
        let neighbor = if index + 1 < self.width { i + 1 } else { i - 1 };
        let a = payload(image, i, coordinates);
        let b = payload(image, neighbor, coordinates);
        std::array::from_fn(|c| a[c] * 0.5 + b[c] * 0.5)
    }

    fn materialize(
        &self,
        image: &Image,
        width: usize,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<Image, CarveError> {
        let mut result = image.blank_like(width, image.height)?;
        for y in 0..image.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let mut to = y * width;
            for x in 0..image.width {
                let i = y * image.width + x;
                result.copy_sample(to, image, i);
                to += 1;
                if self.selected[i] {
                    let neighbor = if x + 1 < image.width { i + 1 } else { i - 1 };
                    result.pixels[to] = std::array::from_fn(|c| {
                        image.pixels[i][c] * 0.5 + image.pixels[neighbor][c] * 0.5
                    });
                    if let (Some(dst), Some(src)) = (&mut result.coordinates, &image.coordinates) {
                        dst[to] = std::array::from_fn(|c| src[i][c] * 0.5 + src[neighbor][c] * 0.5);
                    }
                    to += 1;
                }
            }
            debug_assert_eq!(to, (y + 1) * width);
        }
        Ok(result)
    }
}

/// Each output pixel requests its own signed seam-carved width. The output keeps
/// the original canvas; negative widths flip the corresponding carved image.
/// Results match independent one-axis resizing at every requested integer width.
pub fn width(
    image: Image,
    widths: &[i32],
    restore: bool,
    channel: Channel,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    query_width(image, widths, restore, channel, false, cancel)
}

fn query_width(
    mut image: Image,
    widths: &[i32],
    restore: bool,
    channel: Channel,
    coordinates: bool,
    cancel: &mut impl FnMut() -> bool,
) -> Result<Image, CarveError> {
    check_cancel(cancel)?;
    let queries = Queries::new(&image, widths, restore, coordinates)?;
    if coordinates {
        image.track_coordinates(cancel)?;
    }
    let minimum = widths
        .iter()
        .map(|v| v.unsigned_abs() as usize)
        .min()
        .unwrap();
    let maximum = queries.heads.len() - 1;
    let mut output = Image::new(
        image.width,
        image.height,
        buffer(image.pixels.len(), [0.0; 4])?,
    )?;
    queries.fill(
        image.width,
        &mut output,
        |x, y| payload(&image, y * image.width + x, coordinates),
        cancel,
    )?;
    if minimum < image.width {
        let mut carver = Carver::new(&image, channel, cancel)?;
        while carver.width > minimum {
            check_cancel(cancel)?;
            carver.find_seam(cancel)?;
            carver.remove_seam();
            queries.fill(
                carver.width,
                &mut output,
                |x, y| {
                    payload(
                        &image,
                        carver.indices[carver.rows[y] + x] as usize,
                        coordinates,
                    )
                },
                cancel,
            )?;
        }
    }
    while image.width < maximum {
        check_cancel(cancel)?;
        if image.width == 1 {
            let mut doubled = image.blank_like(2, image.height)?;
            for y in 0..image.height {
                doubled.copy_sample(2 * y, &image, y);
                doubled.copy_sample(2 * y + 1, &image, y);
            }
            image = doubled;
            queries.fill(
                2,
                &mut output,
                |x, y| payload(&image, 2 * y + x, coordinates),
                cancel,
            )?;
            continue;
        }
        let count = (maximum - image.width).min(image.width - 1);
        let mut insertions = Insertions::new(image.width, image.height)?;
        let mut carver = Carver::new(&image, channel, cancel)?;
        for added in 1..=count {
            check_cancel(cancel)?;
            carver.find_seam(cancel)?;
            for y in 0..image.height {
                insertions.insert(carver.indices[carver.rows[y] + carver.seam[y]] as usize);
            }
            carver.remove_seam();
            queries.fill(
                image.width + added,
                &mut output,
                |x, y| insertions.sample(&image, x, y, coordinates),
                cancel,
            )?;
        }
        drop(carver);
        let next_width = image.width + count;
        if next_width == maximum {
            break;
        }
        image = insertions.materialize(&image, next_width, cancel)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::{resize_width, sample_output};

    fn fixture(w: usize, h: usize) -> Image {
        Image::new(
            w,
            h,
            (0..w * h)
                .map(|i| {
                    [
                        ((i * 17 + 3) % 23) as f32 / 23.0,
                        ((i * 11) % 19) as f32 / 19.0,
                        (i % 11) as f32 / 11.0,
                        1.0,
                    ]
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn mixed_queries_match_independent_exact_resizes_including_expansion_and_flips() {
        for (w, h) in [(1, 7), (7, 1), (11, 9), (25, 17)] {
            for restore in [false, true] {
                for channel in [
                    Channel::Rgb,
                    Channel::Rgba,
                    Channel::Red,
                    Channel::Green,
                    Channel::Blue,
                    Channel::Alpha,
                    Channel::Lightness,
                ] {
                    let targets: Vec<_> = (0..w * h)
                        .map(|i| {
                            let value = (1 + (i * 13 + i / w) % (w * 4)) as i32;
                            if i % 3 == 0 { -value } else { value }
                        })
                        .collect();
                    let actual =
                        width(fixture(w, h), &targets, restore, channel, &mut || false).unwrap();
                    for target in 1..=w * 4 {
                        let reference =
                            resize_width(fixture(w, h), target, channel, &mut || false).unwrap();
                        for (i, &wanted) in targets.iter().enumerate() {
                            if wanted.unsigned_abs() as usize != target {
                                continue;
                            }
                            let x = i % w;
                            let y = i / w;
                            let expected = if wanted < 0 {
                                let mut flipped =
                                    Image::new(reference.width, h, reference.pixels.clone())
                                        .unwrap();
                                flipped.flip([true, false], &mut || false).unwrap();
                                sample_output(&flipped, [w, h], [x as i64, y as i64], restore)
                            } else {
                                sample_output(&reference, [w, h], [x as i64, y as i64], restore)
                            };
                            assert_eq!(
                                actual.pixels[i], expected,
                                "{w}x{h} target={wanted} restore={restore} channel={channel:?} pixel={i}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn coordinate_queries_match_tracked_reference_seams() {
        let (w, h) = (9, 7);
        for restore in [false, true] {
            let targets: Vec<_> = (0..w * h)
                .map(|i| {
                    let t = (i % (w * 4) + 1) as i32;
                    if i % 2 == 0 { -t } else { t }
                })
                .collect();
            let actual = query_width(
                fixture(w, h),
                &targets,
                restore,
                Channel::Rgba,
                true,
                &mut || false,
            )
            .unwrap();
            for (i, &target) in targets.iter().enumerate() {
                let mut reference = fixture(w, h);
                reference.track_coordinates(&mut || false).unwrap();
                let mut reference = resize_width(
                    reference,
                    target.unsigned_abs() as usize,
                    Channel::Rgba,
                    &mut || false,
                )
                .unwrap();
                reference.flip([target < 0, false], &mut || false).unwrap();
                let reference = reference.into_coordinate_image(&mut || false).unwrap();
                let expected = sample_output(
                    &reference,
                    [w, h],
                    [(i % w) as i64, (i / w) as i64],
                    restore,
                );
                if expected[3] == 0.0 {
                    assert!(actual.pixels[i][0] < 0.0 || actual.pixels[i][0] >= w as f32);
                } else {
                    assert_eq!(
                        actual.pixels[i], expected,
                        "pixel={i} target={target} restore={restore}"
                    );
                }
            }
        }
    }

    #[test]
    fn invalid_queries_and_cancellation_are_reported() {
        for targets in [vec![], vec![0; 6], vec![i32::MIN; 6], vec![32769; 6]] {
            assert_eq!(
                width(fixture(3, 2), &targets, false, Channel::Rgba, &mut || false).unwrap_err(),
                CarveError::InvalidSize
            );
        }
        assert_eq!(
            width(fixture(3, 2), &[2; 6], false, Channel::Rgba, &mut || true).unwrap_err(),
            CarveError::Cancelled
        );
    }
}
