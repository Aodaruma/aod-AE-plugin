// SPDX-License-Identifier: MPL-2.0
//! Offline algorithm preview. This does not launch or access After Effects.
use std::{fs, io::Write, path::Path, time::Instant};
use utils::seam::{
    Channel, Image,
    mapped::{self, Settings},
};

fn ppm(path: &Path, width: usize, height: usize, pixels: &[[f32; 4]]) {
    let mut out = std::io::BufWriter::new(fs::File::create(path).unwrap());
    write!(out, "P6\n{width} {height}\n255\n").unwrap();
    for p in pixels {
        out.write_all(&[p[0], p[1], p[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .unwrap();
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let directory = Path::new(args.get(1).expect("Output directory is required"));
    fs::create_dir_all(directory).unwrap();
    let width = args.get(2).map_or(240, |v| v.parse().unwrap());
    let height = args.get(3).map_or(160, |v| v.parse().unwrap());
    let mut pixels = vec![[0.0; 4]; width * height];
    let mut mask = vec![0.0; width * height];
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            let nx = x as f32 / (width - 1) as f32;
            let ny = y as f32 / (height - 1) as f32;
            let grid = x % (width / 12).max(1) < 2 || y % (height / 8).max(1) < 2;
            pixels[i] = if grid {
                [0.9, 0.9, 0.9, 1.0]
            } else {
                [0.12 + 0.55 * nx, 0.12 + 0.55 * ny, 0.22, 1.0]
            };
            mask[i] = nx;
        }
    }
    ppm(&directory.join("source.ppm"), width, height, &pixels);
    ppm(
        &directory.join("map.ppm"),
        width,
        height,
        &mask.iter().map(|&m| [m, m, m, 1.0]).collect::<Vec<_>>(),
    );
    for anchor in [0.3, 0.7] {
        let now = Instant::now();
        let image = mapped::apply(
            Image::new(width, height, pixels.clone()).unwrap(),
            &mask,
            Settings {
                scales: [[50.0, 150.0], [100.0, 100.0]],
                anchor,
                tolerance: 0.002,
                restore: false,
                channel: Channel::Rgba,
            },
            &mut || false,
        )
        .unwrap();
        println!(
            "{width}x{height} X50..150 Y100 anchor={anchor}: {:.3}s",
            now.elapsed().as_secs_f64()
        );
        ppm(
            &directory.join(format!("anchor-{}.ppm", (anchor * 100.0) as u32)),
            width,
            height,
            &image.pixels,
        );
    }
}
