// SPDX-License-Identifier: MPL-2.0
//! Stable pixel IDs avoid moving whole rows after every seam. Cached DP costs
//! are updated only where an edge/energy changed, or a changed cost propagates.
use super::{CarveError, Channel, Feature, Image, buffer, check_cancel, dp_row, energy, feature};

const NONE: u32 = u32::MAX;

pub(super) struct LinkedCarver {
    pub width: usize,
    pub updated_nodes: usize,
    height: usize,
    features: Vec<Feature>,
    left: Vec<u32>,
    right: Vec<u32>,
    up: Vec<u32>,
    down: Vec<u32>,
    first: Vec<u32>,
    energy: Vec<f64>,
    cost: Vec<f64>,
    parent: Vec<u32>,
    dirty: Vec<Option<(u32, u32)>>,
    pub seam: Vec<u32>,
}

impl LinkedCarver {
    pub fn new(
        image: &Image,
        channel: Channel,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<Self, CarveError> {
        let n = image.pixels.len();
        let w = image.width;
        let h = image.height;
        let mut c = Self {
            width: w,
            updated_nodes: 0,
            height: h,
            features: buffer(n, [0.0; 3])?,
            left: buffer(n, NONE)?,
            right: buffer(n, NONE)?,
            up: buffer(n, 0)?,
            down: buffer(n, 0)?,
            first: buffer(h, 0)?,
            energy: buffer(n, 0.0)?,
            cost: buffer(n, 0.0)?,
            parent: buffer(n, NONE)?,
            dirty: buffer(h, None)?,
            seam: buffer(h, 0)?,
        };
        for y in 0..h {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            c.first[y] = (y * w) as u32;
            for x in 0..w {
                let i = y * w + x;
                c.features[i] = feature(image.pixels[i], channel);
                if x > 0 {
                    c.left[i] = (i - 1) as u32;
                }
                if x + 1 < w {
                    c.right[i] = (i + 1) as u32;
                }
                c.up[i] = (y.saturating_sub(1) * w + x) as u32;
                c.down[i] = ((y + 1).min(h - 1) * w + x) as u32;
            }
        }
        let mut previous = buffer(w + 2, f64::INFINITY)?;
        let mut directions = buffer(w, 0)?;
        for y in 0..h {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let row = y * w;
            for i in row..row + w {
                c.energy[i] = c.energy_at(i);
            }
            if y == 0 {
                c.cost[..w].copy_from_slice(&c.energy[..w]);
            } else {
                previous[1..=w].copy_from_slice(&c.cost[row - w..row]);
                dp_row(
                    &previous,
                    &c.energy[row..row + w],
                    &mut c.cost[row..row + w],
                    &mut directions,
                );
                for (x, &direction) in directions.iter().enumerate() {
                    c.parent[row + x] =
                        (row as isize - w as isize + x as isize + direction as isize) as u32;
                }
            }
        }
        Ok(c)
    }

    fn energy_at(&self, i: usize) -> f64 {
        let l = self.features[if self.left[i] == NONE {
            i
        } else {
            self.left[i] as usize
        }];
        let r = self.features[if self.right[i] == NONE {
            i
        } else {
            self.right[i] as usize
        }];
        let u = self.features[self.up[i] as usize];
        let d = self.features[self.down[i] as usize];
        energy(l, r, u, d)
    }

    fn mark(&mut self, y: usize, id: u32) {
        if id == NONE {
            return;
        }
        self.dirty[y] = Some(match self.dirty[y] {
            Some((lo, hi)) => (lo.min(id), hi.max(id)),
            None => (id, id),
        });
    }

    pub fn find_seam(&mut self, cancel: &mut impl FnMut() -> bool) -> Result<(), CarveError> {
        self.updated_nodes = 0;
        for y in 0..self.height {
            if y % 32 == 0 {
                check_cancel(cancel)?;
            }
            let Some((mut id, last)) = self.dirty[y].take() else {
                continue;
            };
            while id != NONE && id <= last {
                let i = id as usize;
                self.updated_nodes += 1;
                let mut best = self.up[i];
                let mut cost = 0.0;
                if y > 0 {
                    let l = self.left[best as usize];
                    let r = self.right[best as usize];
                    if l != NONE && self.cost[l as usize] <= self.cost[best as usize] {
                        best = l;
                    }
                    if r != NONE && self.cost[r as usize] < self.cost[best as usize] {
                        best = r;
                    }
                    cost = self.cost[best as usize];
                    self.parent[i] = best;
                }
                let new_cost = self.energy[i] + cost;
                if new_cost != self.cost[i] {
                    self.cost[i] = new_cost;
                    if y + 1 < self.height {
                        let child = self.down[i];
                        self.mark(y + 1, self.left[child as usize]);
                        self.mark(y + 1, child);
                        self.mark(y + 1, self.right[child as usize]);
                    }
                }
                id = self.right[i];
            }
        }
        let mut best = self.first[self.height - 1];
        let mut id = self.right[best as usize];
        while id != NONE {
            if self.cost[id as usize] < self.cost[best as usize] {
                best = id;
            }
            id = self.right[id as usize];
        }
        for y in (0..self.height).rev() {
            self.seam[y] = best;
            if y > 0 {
                best = self.parent[best as usize];
            }
        }
        Ok(())
    }

    pub fn remove_seam(&mut self) {
        debug_assert!(self.width > 1);
        // Before unlinking, reconnect the one vertical pair affected by each
        // diagonal seam step. Other surviving vertical neighbors remain valid.
        for y in 0..self.height - 1 {
            let upper = self.seam[y] as usize;
            let lower = self.seam[y + 1] as usize;
            let above_lower = self.up[lower];
            if above_lower == self.right[upper] {
                let a = self.right[upper] as usize;
                let b = self.left[lower] as usize;
                self.down[a] = b as u32;
                self.up[b] = a as u32;
            } else if above_lower == self.left[upper] {
                let a = self.left[upper] as usize;
                let b = self.right[lower] as usize;
                self.down[a] = b as u32;
                self.up[b] = a as u32;
            } else {
                debug_assert_eq!(above_lower, upper as u32);
            }
        }
        for y in 0..self.height {
            let id = self.seam[y] as usize;
            let l = self.left[id];
            let r = self.right[id];
            if l != NONE {
                self.right[l as usize] = r;
            } else {
                self.first[y] = r;
            }
            if r != NONE {
                self.left[r as usize] = l;
            }
            // A removed node can only change central differences and incoming
            // DP edges within two columns. Include the full conservative stencil.
            self.mark(y, l);
            self.mark(y, r);
            if l != NONE {
                self.mark(y, self.left[l as usize]);
            }
            if r != NONE {
                self.mark(y, self.right[r as usize]);
            }
        }
        self.width -= 1;
        for y in 0..self.height {
            if let Some((mut id, last)) = self.dirty[y] {
                while id != NONE && id <= last {
                    self.energy[id as usize] = self.energy_at(id as usize);
                    id = self.right[id as usize];
                }
            }
        }
    }

    pub fn image(&self, original: &Image) -> Result<Image, CarveError> {
        let mut pixels = buffer(self.width * self.height, [0.0; 4])?;
        for y in 0..self.height {
            let mut id = self.first[y];
            for x in 0..self.width {
                pixels[y * self.width + x] = original.pixels[id as usize];
                id = self.right[id as usize];
            }
            debug_assert_eq!(id, NONE);
        }
        Image::new(self.width, self.height, pixels)
    }
}
