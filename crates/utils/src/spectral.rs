#![allow(clippy::needless_range_loop)]

use anyhow::{Result, anyhow};
use num_complex::Complex32;
use rustfft::FftPlanner;

pub struct Spectrum2D {
    pub real: Vec<f32>,
    pub imag: Vec<f32>,
    pub width: usize,
    pub height: usize,
}

pub fn fft2_rgba(input_centered_rgba: &[f32], width: usize, height: usize) -> Result<Spectrum2D> {
    validate_len(input_centered_rgba.len(), width, height)?;

    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| anyhow!("image size overflow"))?;
    let mut real = vec![0.0f32; pixels * 4];
    let mut imag = vec![0.0f32; pixels * 4];

    for channel in 0..4 {
        let mut work = vec![Complex32::new(0.0, 0.0); pixels];
        for i in 0..pixels {
            work[i].re = input_centered_rgba[i * 4 + channel];
        }

        fft2_in_place(&mut work, width, height, false)?;

        for i in 0..pixels {
            real[i * 4 + channel] = work[i].re;
            imag[i * 4 + channel] = work[i].im;
        }
    }

    Ok(Spectrum2D {
        real,
        imag,
        width,
        height,
    })
}

pub fn ifft2_rgba(
    real_rgba: &[f32],
    imag_rgba: &[f32],
    width: usize,
    height: usize,
) -> Result<Vec<f32>> {
    validate_len(real_rgba.len(), width, height)?;
    validate_len(imag_rgba.len(), width, height)?;

    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| anyhow!("image size overflow"))?;
    let mut output = vec![0.0f32; pixels * 4];

    for channel in 0..4 {
        let mut work = vec![Complex32::new(0.0, 0.0); pixels];
        for i in 0..pixels {
            work[i] = Complex32::new(real_rgba[i * 4 + channel], imag_rgba[i * 4 + channel]);
        }

        fft2_in_place(&mut work, width, height, true)?;

        for i in 0..pixels {
            output[i * 4 + channel] = work[i].re;
        }
    }

    Ok(output)
}

fn fft2_in_place(data: &mut [Complex32], width: usize, height: usize, inverse: bool) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(anyhow!("width/height must be > 0"));
    }

    let expected = width
        .checked_mul(height)
        .ok_or_else(|| anyhow!("image size overflow"))?;
    if data.len() != expected {
        return Err(anyhow!("invalid working buffer length"));
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft_w = if inverse {
        planner.plan_fft_inverse(width)
    } else {
        planner.plan_fft_forward(width)
    };

    for row in data.chunks_exact_mut(width) {
        fft_w.process(row);
    }

    let fft_h = if inverse {
        planner.plan_fft_inverse(height)
    } else {
        planner.plan_fft_forward(height)
    };

    let mut column = vec![Complex32::new(0.0, 0.0); height];
    for x in 0..width {
        for y in 0..height {
            column[y] = data[y * width + x];
        }

        fft_h.process(&mut column);

        for y in 0..height {
            data[y * width + x] = column[y];
        }
    }

    if inverse {
        let norm = (width * height) as f32;
        for v in data.iter_mut() {
            *v /= norm;
        }
    }

    Ok(())
}

fn validate_len(len: usize, width: usize, height: usize) -> Result<()> {
    let expected = width
        .checked_mul(height)
        .and_then(|p| p.checked_mul(4))
        .ok_or_else(|| anyhow!("image size overflow"))?;
    if len != expected {
        return Err(anyhow!(
            "invalid RGBA length: expected {}, got {}",
            expected,
            len
        ));
    }
    Ok(())
}
