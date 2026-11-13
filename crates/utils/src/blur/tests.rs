use super::*;
fn constant_field() -> Image {
    Image {
        width: 1,
        height: 1,
        pixels: vec![[0., 0., 1., 0.]],
    }
}
fn fixture(w: usize, h: usize) -> Image {
    let mut image = Image::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let a = ((x * 13 + y * 7) % 19) as f32 / 18.;
            image.pixels[y * w + x] = [
                a * (x as f32 * 0.13).sin().abs() * 2.,
                a * y as f32 / h as f32,
                a * 0.4,
                a,
            ];
        }
    }
    image
}
fn close(a: &Image, b: &Image, epsilon: f32) {
    assert_eq!(a.pixels.len(), b.pixels.len());
    let error = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .flat_map(|(a, b)| a.iter().zip(b))
        .map(|(a, b)| (a - b).abs())
        .fold(0f32, f32::max);
    assert!(error < epsilon, "maximum error {error} exceeds {epsilon}");
}
#[test]
fn zero_radius_and_mix_preserve_hdr_exactly() {
    let src = fixture(15, 13);
    for mode in [0, 1, 2, 3, 4, 5, 17, 18, 19] {
        let s = Settings {
            mode,
            length: 0.,
            boost: 8.,
            transfer: 1,
            backend: 1,
            ..Default::default()
        };
        assert_eq!(
            render(&src, constant_field(), &s).unwrap().pixels,
            src.pixels
        );
        let s = Settings {
            length: 64.,
            mix: 0.,
            ..s
        };
        assert_eq!(
            render(&src, constant_field(), &s).unwrap().pixels,
            src.pixels
        );
    }
}
#[test]
fn directional_impulse_spreads_energy_and_preserves_hdr() {
    let mut src = Image::new(65, 9);
    src.pixels[4 * 65 + 32] = [64., 32., 16., 1.];
    let s = Settings {
        length: 24.,
        samples: 192,
        backend: 1,
        ..Default::default()
    };
    let out = render(&src, constant_field(), &s).unwrap();
    let sum: f32 = out.pixels.iter().map(|p| p[0]).sum();
    assert!((sum - 64.).abs() < 0.01);
    assert!(out.pixels[4 * 65 + 24][0] > 1.);
    assert_eq!(out.pixels[3 * 65 + 32], [0.; 4]);
    let boosted = render(&src, constant_field(), &Settings { boost: 2., ..s }).unwrap();
    assert!(boosted.pixels[4 * 65 + 24][0] > out.pixels[4 * 65 + 24][0]);
}
#[test]
fn linear_exposure_is_brighter_than_encoded_averaging() {
    let mut src = Image::new(17, 1);
    src.pixels.fill([0., 0., 0., 1.]);
    src.pixels[8] = [1.; 4];
    let s = Settings {
        length: 8.,
        samples: 64,
        backend: 1,
        ..Default::default()
    };
    let encoded = render(&src, constant_field(), &s).unwrap();
    let linear = render(&src, constant_field(), &Settings { transfer: 1, ..s }).unwrap();
    assert!(linear.pixels[8][0] > encoded.pixels[8][0] * 2.);
}
#[test]
fn zero_alpha_hidden_rgb_does_not_bleed_into_exposure() {
    let mut src = Image::new(21, 1);
    src.pixels.fill([100., 0., 0., 0.]);
    src.pixels[10] = [0., 1., 0., 1.];
    let out = render(
        &src,
        constant_field(),
        &Settings {
            length: 8.,
            backend: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.pixels.iter().all(|p| p[0] == 0.));
    assert!(out.pixels[8][1] > 0. && out.pixels[8][3] > 0.);
}
#[test]
fn absent_path_or_motion_preserves_input_even_with_highlight_boost() {
    let src = fixture(19, 13);
    for mode in [3, 4, 5] {
        let s = Settings {
            mode,
            boost: 8.,
            transfer: 1,
            interpolation: 2,
            backend: 1,
            ..Default::default()
        };
        let field = Image {
            width: 1,
            height: 1,
            pixels: vec![if mode == 5 { [0.; 4] } else { [0., 0., 1., 0.] }],
        };
        assert_eq!(render(&src, field, &s).unwrap().pixels, src.pixels);
    }
}
#[test]
fn black_map_is_exact_passthrough_and_white_map_blurs() {
    let src = fixture(17, 11);
    for mode in [17, 18, 19] {
        let s = Settings {
            mode,
            backend: 1,
            ..Default::default()
        };
        let black = Image {
            width: 1,
            height: 1,
            pixels: vec![[0.; 4]],
        };
        assert_eq!(render(&src, black, &s).unwrap().pixels, src.pixels);
        assert_ne!(
            render(&src, constant_field(), &s).unwrap().pixels,
            src.pixels
        );
    }
}
#[test]
fn displacement_has_the_same_shape_everywhere_and_preserves_energy() {
    let mut src = Image::new(84, 76);
    let centers = [(18, 20), (61, 53)];
    for (x, y) in centers {
        src.pixels[y * src.width + x] = [6., 3., 1.5, 0.5];
    }
    for interpolation in 0..3 {
        let out = render(
            &src,
            constant_field(),
            &Settings {
                mode: 17,
                length: 9.,
                samples: 7,
                evolution: 73.,
                interpolation,
                backend: 1,
                ..Default::default()
            },
        )
        .unwrap();
        for dy in -12..=12 {
            for dx in -12..=12 {
                let a = out.at(18 + dx, 20 + dy, 0);
                let b = out.at(61 + dx, 53 + dy, 0);
                for channel in 0..4 {
                    assert!((a[channel] - b[channel]).abs() < 1e-5);
                }
            }
        }
        for (channel, expected) in [12., 6., 3., 1.].into_iter().enumerate() {
            let energy: f32 = out.pixels.iter().map(|p| p[channel]).sum();
            assert!((energy - expected).abs() < 1e-4, "energy={energy}");
        }
    }
}
#[test]
fn displacement_map_scales_distance_at_each_pixel() {
    let src = fixture(37, 29);
    let strengths = [0., 0.25, 0.5, 1.];
    let mut map = Image::new(src.width, src.height);
    for (i, p) in map.pixels.iter_mut().enumerate() {
        p[2] = strengths[(i % src.width) % strengths.len()];
    }
    let settings = Settings {
        mode: 17,
        length: 12.,
        samples: 19,
        scale: [0.5, 0.75],
        evolution: -120.,
        backend: 1,
        ..Default::default()
    };
    let mapped = render(&src, map, &settings).unwrap();
    for (band, strength) in strengths.into_iter().enumerate() {
        let uniform = render(
            &src,
            constant_field(),
            &Settings {
                length: settings.length * strength,
                ..settings.clone()
            },
        )
        .unwrap();
        for (i, pixel) in mapped.pixels.iter().enumerate() {
            if (i % src.width) % strengths.len() == band {
                assert_eq!(*pixel, uniform.pixels[i]);
            }
        }
    }
}
#[test]
fn displacement_directions_are_stable_unit_vectors_and_evolve_smoothly() {
    let directions = displacement_directions(512, 0.);
    assert_eq!(directions, displacement_directions(512, 0.));
    assert_eq!(directions, displacement_directions(999, 360.));
    assert_eq!(&directions[..2], displacement_directions(0, 0.));
    assert!(directions.windows(2).all(|pair| pair[0] != pair[1]));
    for evolution in [-100000., 0.001, 90., 100000.] {
        let evolved = displacement_directions(512, evolution);
        for (a, b) in directions.iter().zip(evolved) {
            assert!((b[0].hypot(b[1]) - 1.).abs() < 1e-6);
            if evolution == 0.001 {
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 0.0001);
            }
            if evolution == 90. {
                assert!((a[0] - b[1]).abs() < 1e-6);
                assert!((a[1] + b[0]).abs() < 1e-6);
            }
        }
    }
}
#[test]
fn mipmap_handles_one_pixel_and_odd_dimensions() {
    for (w, h) in [(1, 1), (1, 9), (9, 1), (13, 17)] {
        let src = Image {
            width: w,
            height: h,
            pixels: vec![[3., 2., 1., 0.5]; w * h],
        };
        for mode in [18, 19] {
            for interpolation in 0..3 {
                let s = Settings {
                    mode,
                    length: 100.,
                    edge: 1,
                    interpolation,
                    backend: 1,
                    ..Default::default()
                };
                close(&render(&src, constant_field(), &s).unwrap(), &src, 1e-5);
            }
        }
    }
}
#[test]
fn path_field_tracks_circle_tangent_and_distance() {
    let mut segments = Vec::new();
    for i in 0..256 {
        let a = i as f32 * std::f32::consts::TAU / 256.;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 256.;
        segments.push([
            32. + 20. * a.cos(),
            32. + 20. * a.sin(),
            32. + 20. * b.cos(),
            32. + 20. * b.sin(),
        ]);
    }
    let field = field::path_field(65, 65, &segments, 10.);
    let p = Prepared {
        levels: vec![Image::new(65, 65)],
        field,
        displacement: Vec::new(),
        settings: Settings {
            mode: 5,
            ..Default::default()
        },
    };
    let tangent = field_at(&p, 52., 32.);
    assert!(tangent[1] > 0.98 && tangent[0].abs() < 0.1);
    assert!(field_at(&p, 64., 32.)[2] < 0.05);
    let mut pos = [52., 32.];
    for _ in 0..30 {
        pos = advance(&p, pos, 0.5);
    }
    assert!(((pos[0] - 32.).hypot(pos[1] - 32.) - 20.).abs() < 0.5);
    assert!(pos[0] < 49. && pos[1] > 43.);
}
#[test]
fn automatic_motion_detects_translation_and_rejects_static_input() {
    let src = fixture(96, 72);
    let static_flow = field::motion_field(&src, &src, 16.);
    assert!(
        static_flow
            .pixels
            .iter()
            .all(|p| p[0].abs() < 0.01 && p[1].abs() < 0.01)
    );
    let mut shifted = Image::new(96, 72);
    for y in 0..72 {
        for x in 0..96 {
            shifted.pixels[y * 96 + x] = src.at(x as i32 - 5, y as i32 - 2, 0);
        }
    }
    let flow = field::motion_field(&shifted, &src, 16.);
    let center = flow.sample((flow.width / 2) as f32, (flow.height / 2) as f32, 1, 1);
    assert!(
        (center[0] - 5.).abs() < 0.75 && (center[1] - 2.).abs() < 0.75,
        "flow: {center:?}"
    );
}
#[cfg(feature = "blur_wgpu")]
#[test]
#[ignore = "requires a working hardware GPU; run explicitly"]
fn gpu_matches_cpu_for_every_kernel_and_interpolation() {
    let src = fixture(41, 33);
    for mode in [0, 1, 2, 3, 4, 5, 17, 18, 19] {
        for interpolation in 0..3 {
            for edge in 0..2 {
                let settings = Settings {
                    mode,
                    length: 11.,
                    angle: 0.73,
                    center: [20., 16.],
                    scale: [0.5, 0.75],
                    samples: 32,
                    interpolation,
                    edge,
                    bias: 0.2,
                    curve: 3,
                    curve_table: curve::ShutterCurve {
                        points: vec![[0., 0.05], [0.2, 1.], [0.65, 0.2], [1., 0.]],
                        smooth: true,
                    }
                    .table(),
                    evolution: 1.5,
                    ..Default::default()
                };
                let field = if mode == 5 {
                    field::path_field(41, 33, &[[0., 3., 18., 15.], [18., 15., 40., 29.]], 0.)
                } else {
                    Image {
                        width: 2,
                        height: 2,
                        pixels: vec![
                            [4., 2., 1., 1.],
                            [7., 1., 0.3, 1.],
                            [1., 3., 0.5, 1.],
                            [2., 1., 0.8, 1.],
                        ],
                    }
                };
                let p = prepare(&src, field, &settings);
                let cpu = render_cpu(&p);
                let gpu = gpu::render(&p).unwrap();
                eprintln!("parity mode={mode} interpolation={interpolation} edge={edge}");
                close(&cpu, &gpu, 4e-4);
            }
        }
    }
}
#[cfg(feature = "blur_wgpu")]
#[test]
#[ignore = "requires a working hardware GPU; run explicitly"]
fn gpu_displacement_matches_cpu_with_maps_and_maximum_samples() {
    let src = fixture(37, 29);
    let mut map = Image::new(src.width, src.height);
    for (i, p) in map.pixels.iter_mut().enumerate() {
        p[2] = [0., 0.25, 0.5, 1.][(i % src.width) % 4];
    }
    for (samples, evolution) in [(2, -100000.), (3, 0.), (511, 73.), (512, 100000.)] {
        for interpolation in 0..3 {
            for edge in 0..2 {
                let settings = Settings {
                    mode: 17,
                    length: 12.,
                    scale: [0.5, 0.75],
                    samples,
                    evolution,
                    interpolation,
                    edge,
                    mix: 0.65,
                    backend: 1,
                    ..Default::default()
                };
                let cpu = render(&src, map.clone(), &settings).unwrap();
                let gpu = render(
                    &src,
                    map.clone(),
                    &Settings {
                        backend: 2,
                        ..settings
                    },
                )
                .unwrap();
                close(&cpu, &gpu, 4e-4);
                for (i, p) in gpu.pixels.iter().enumerate() {
                    if map.pixels[i][2] == 0. {
                        assert_eq!(*p, src.pixels[i]);
                    }
                }
                eprintln!(
                    "displacement parity samples={samples} evolution={evolution} interpolation={interpolation} edge={edge}"
                );
            }
        }
    }
}
#[cfg(feature = "blur_wgpu")]
#[test]
#[ignore = "hardware performance probe"]
fn benchmark_hd() {
    let src = fixture(1920, 1080);
    for mode in [0, 5, 17, 18] {
        let s = Settings {
            mode,
            length: 48.,
            samples: 64,
            ..Default::default()
        };
        let f = if mode == 5 {
            field::path_field(
                1920,
                1080,
                &[[0., 0., 960., 700.], [960., 700., 1920., 500.]],
                0.,
            )
        } else {
            constant_field()
        };
        let p = prepare(&src, f, &s);
        let _ = gpu::render(&p).unwrap();
        let now = std::time::Instant::now();
        let cpu = render_cpu(&p);
        let cpu_ms = now.elapsed().as_secs_f64() * 1000.;
        let now = std::time::Instant::now();
        let gpu = gpu::render(&p).unwrap();
        let gpu_ms = now.elapsed().as_secs_f64() * 1000.;
        close(&cpu, &gpu, 0.01);
        eprintln!("mode={mode},1920x1080,64 samples,CPU={cpu_ms:.2}ms,GPU={gpu_ms:.2}ms");
    }
}
#[test]
fn shutter_curve_preserves_energy_and_changes_trail_distribution() {
    let mut src = Image::new(129, 1);
    src.pixels[64] = [12., 6., 3., 1.];
    let settings = Settings {
        length: 40.,
        samples: 256,
        backend: 1,
        curve: 3,
        ..Default::default()
    };
    let flat = render(&src, constant_field(), &settings).unwrap();
    let uniform = render(
        &src,
        constant_field(),
        &Settings {
            curve: 0,
            ..settings.clone()
        },
    )
    .unwrap();
    close(&flat, &uniform, 1e-5);
    let centroid = |im: &Image| {
        let energy = im.pixels.iter().map(|p| p[0]).sum::<f32>();
        assert!((energy - 12.).abs() < 0.05);
        im.pixels
            .iter()
            .enumerate()
            .map(|(i, p)| i as f32 * p[0])
            .sum::<f32>()
            / energy
    };
    let a = render(
        &src,
        constant_field(),
        &Settings {
            curve_table: curve::ShutterCurve::preset(3).table(),
            ..settings.clone()
        },
    )
    .unwrap();
    let b = render(
        &src,
        constant_field(),
        &Settings {
            curve_table: curve::ShutterCurve::preset(4).table(),
            ..settings.clone()
        },
    )
    .unwrap();
    assert!(centroid(&a) > centroid(&flat) + 5.);
    assert!(centroid(&b) < centroid(&flat) - 5.);
    let flat_src = Image {
        width: 129,
        height: 1,
        pixels: vec![[4., 2., 1., 1.]; 129],
    };
    let closed = render(
        &flat_src,
        constant_field(),
        &Settings {
            curve_table: [0.; 256],
            edge: 1,
            ..settings
        },
    )
    .unwrap();
    assert_eq!(closed.pixels, flat_src.pixels);
}
