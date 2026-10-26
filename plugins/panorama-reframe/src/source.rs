use crate::geometry::{V3, cube_direction, cube_uv, latlong};
use after_effects::{Error, PixelF32};
use std::borrow::Cow;
use utils::image::{SampleEdge, SampleFilter, TRANSPARENT, sample_filtered, sample_nearest};

const PAD: usize = 9;

fn pixel_buffer(width: usize, height: usize) -> Result<Vec<PixelF32>, Error> {
    let len = width.checked_mul(height).ok_or(Error::OutOfMemory)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|_| Error::OutOfMemory)?;
    pixels.resize(len, TRANSPARENT);
    Ok(pixels)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    LatLong,
    StripHorizontal,
    StripVertical,
    Grid,
    Cross,
}
impl Layout {
    pub fn grid(self) -> (usize, usize) {
        match self {
            Self::LatLong => (1, 1),
            Self::StripHorizontal => (6, 1),
            Self::StripVertical => (1, 6),
            Self::Grid => (3, 2),
            Self::Cross => (4, 3),
        }
    }
    pub fn cell(self, face: usize) -> (usize, usize) {
        match self {
            Self::LatLong => (0, 0),
            Self::StripHorizontal => (face, 0),
            Self::StripVertical => (0, face),
            Self::Grid => (face % 3, face / 3),
            Self::Cross => [(2, 1), (0, 1), (1, 2), (1, 0), (1, 1), (3, 1)][face],
        }
    }
}

pub struct Panorama {
    faces: Vec<Vec<PixelF32>>,
    width: usize,
    height: usize,
    layout: Layout,
    equiangular: bool,
    filter: SampleFilter,
}
impl Panorama {
    pub fn new(
        pixels: &[PixelF32],
        width: usize,
        height: usize,
        layout: Layout,
        equiangular: bool,
        edge: Option<SampleEdge>,
        filter: SampleFilter,
    ) -> Result<Self, Error> {
        let (cols, rows) = layout.grid();
        // Faces may differ by one downsampled pixel after AE rounds dimensions.
        if width < cols || height < rows || pixels.len() != width.saturating_mul(height) {
            return Err(Error::BadCallbackParameter);
        }
        let fw = width / cols;
        let fh = height / rows;
        let count = if layout == Layout::LatLong { 1 } else { 6 };
        if layout != Layout::LatLong && fw.abs_diff(fh) > 1 {
            return Err(Error::BadCallbackParameter);
        }
        let mut raw = Vec::with_capacity(count);
        for f in 0..count {
            if layout == Layout::LatLong {
                raw.push(Cow::Borrowed(pixels));
                continue;
            }
            let (cx, cy) = layout.cell(f);
            let mut face = pixel_buffer(fw, fh)?;
            for y in 0..fh {
                face[y * fw..(y + 1) * fw].copy_from_slice(
                    &pixels[(cy * fh + y) * width + cx * fw..(cy * fh + y) * width + cx * fw + fw],
                );
            }
            raw.push(Cow::Owned(face));
        }
        let pw = fw + 2 * PAD;
        let ph = fh + 2 * PAD;
        let mut faces = Vec::with_capacity(count);
        for f in 0..count {
            let mut padded = pixel_buffer(pw, ph)?;
            for py in 0..ph {
                for px in 0..pw {
                    let x = px as i64 - PAD as i64;
                    let y = py as i64 - PAD as i64;
                    padded[py * pw + px] = if x >= 0 && x < fw as i64 && y >= 0 && y < fh as i64 {
                        raw[f][y as usize * fw + x as usize]
                    } else if let Some(edge) = edge {
                        sample_nearest(&raw[f], fw, fh, x as f32, y as f32, edge)
                    } else if layout == Layout::LatLong {
                        // Longitude wraps; latitude clamps to avoid mixing the opposite pole.
                        raw[0][y.clamp(0, fh as i64 - 1) as usize * fw
                            + x.rem_euclid(fw as i64) as usize]
                    } else {
                        // Reproject filter support across the cube edge, never into an unrelated atlas cell.
                        let d = cube_direction(
                            f,
                            [(x as f64 + 0.5) / fw as f64, (y as f64 + 0.5) / fh as f64],
                            equiangular,
                        );
                        let (other, uv) = cube_uv(d, equiangular);
                        sample_filtered(
                            &raw[other],
                            fw,
                            fh,
                            (uv[0] * fw as f64 - 0.5) as f32,
                            (uv[1] * fh as f64 - 0.5) as f32,
                            SampleEdge::Clamp,
                            SampleFilter::Bilinear,
                        )
                    };
                }
            }
            faces.push(padded);
        }
        Ok(Self {
            faces,
            width: fw,
            height: fh,
            layout,
            equiangular,
            filter,
        })
    }
    pub fn sample(&self, d: V3) -> PixelF32 {
        let (f, uv) = if self.layout == Layout::LatLong {
            (0, latlong(d))
        } else {
            cube_uv(d, self.equiangular)
        };
        sample_filtered(
            &self.faces[f],
            self.width + 2 * PAD,
            self.height + 2 * PAD,
            (uv[0] * self.width as f64 - 0.5 + PAD as f64) as f32,
            (uv[1] * self.height as f64 - 0.5 + PAD as f64) as f32,
            SampleEdge::Clamp,
            self.filter,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::unit;
    fn color(r: f32, g: f32, b: f32) -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red: r,
            green: g,
            blue: b,
        }
    }
    #[test]
    fn latlong_seam_wraps_but_poles_do_not() {
        let p = vec![
            color(1.0, 0.0, 0.0),
            color(0.0, 1.0, 0.0),
            color(0.0, 0.0, 1.0),
            color(0.0, 0.0, 0.0),
        ];
        let source = Panorama::new(
            &p,
            2,
            2,
            Layout::LatLong,
            false,
            None,
            SampleFilter::Bilinear,
        )
        .unwrap();
        let north = source.sample([0.0, -1.0, 0.0]);
        assert!(north.blue < 1e-6);
        let a = source.sample(unit([1e-8, 0.0, -1.0]));
        let b = source.sample(unit([-1e-8, 0.0, -1.0]));
        assert!((a.red - b.red).abs() < 1e-5);
    }
    #[test]
    fn cube_layouts_and_filters_preserve_constant_hdr() {
        for layout in [
            Layout::StripHorizontal,
            Layout::StripVertical,
            Layout::Grid,
            Layout::Cross,
        ] {
            let (c, r) = layout.grid();
            let (w, h) = (c * 16, r * 16);
            let mut pixels = vec![TRANSPARENT; w * h];
            for f in 0..6 {
                let (cx, cy) = layout.cell(f);
                for y in 0..16 {
                    for x in 0..16 {
                        pixels[(cy * 16 + y) * w + cx * 16 + x] = color(4.0, 2.0, -0.5);
                    }
                }
            }
            for eac in [false, true] {
                for filter in [
                    SampleFilter::Nearest,
                    SampleFilter::Bilinear,
                    SampleFilter::Lanczos { lobes: 3.0 },
                ] {
                    let source = Panorama::new(&pixels, w, h, layout, eac, None, filter).unwrap();
                    for d in [[1.0, 1.0, 1.0], [1.0, 0.0, -1.0], [0.0, -1.0, 0.0]] {
                        let p = source.sample(unit(d));
                        assert!((p.red - 4.0).abs() < 1e-4);
                        assert!((p.alpha - 1.0).abs() < 1e-4);
                    }
                }
            }
        }
    }
    #[test]
    fn transparent_edge_does_not_read_adjacent_atlas_cell() {
        let p = vec![color(1.0, 0.0, 0.0); 6 * 16 * 16];
        let source = Panorama::new(
            &p,
            96,
            16,
            Layout::StripHorizontal,
            false,
            Some(SampleEdge::Transparent),
            SampleFilter::Bilinear,
        )
        .unwrap();
        assert!((source.sample(unit([1.0, 0.0, 1.0])).alpha - 0.5).abs() < 1e-5);
    }
}
