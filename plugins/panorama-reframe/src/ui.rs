use super::*;
use std::f64::consts::{PI, TAU};

// All control geometry is in full-resolution layer coordinates. Convert each
// point separately so comp zoom, layer transforms and pixel aspect are respected.
fn to_frame(d: InData, e: &mut ae::EventExtra, p: [f64; 2]) -> Result<[f64; 2], Error> {
    let mut q = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from(p[0] as f32).as_fixed(),
        y: ae::Fixed::from(p[1] as f32).as_fixed(),
    };
    if e.window_type() == ae::WindowType::Comp {
        e.callbacks()
            .layer_to_comp(d.current_time(), d.time_scale(), &mut q)?;
    }
    e.callbacks().source_to_frame(&mut q)?;
    Ok([
        ae::Fixed::from_fixed(q.x).as_f32() as f64,
        ae::Fixed::from_fixed(q.y).as_f32() as f64,
    ])
}
fn mouse(d: InData, e: &mut ae::EventExtra) -> Result<[f64; 2], Error> {
    let pt = e.screen_point();
    let mut q = ae::sys::PF_FixedPoint {
        x: ae::Fixed::from_int(pt.h).as_fixed(),
        y: ae::Fixed::from_int(pt.v).as_fixed(),
    };
    e.callbacks().frame_to_source(&mut q)?;
    if e.window_type() == ae::WindowType::Comp {
        e.callbacks()
            .comp_to_layer(d.current_time(), d.time_scale(), &mut q)?;
    }
    Ok([
        ae::Fixed::from_fixed(q.x).as_f32() as f64,
        ae::Fixed::from_fixed(q.y).as_f32() as f64,
    ])
}
fn center(d: InData) -> ([f64; 2], f64) {
    (
        [d.width() as f64 * 0.5, d.height() as f64 * 0.5],
        d.width().min(d.height()) as f64 * 0.2,
    )
}
fn drag_angles(delta: [f64; 2], radius: f64, sensitivity: f64) -> V3 {
    let gain = PI / (2.0 * radius.max(1.0)) * sensitivity;
    [-delta[1] * gain, delta[0] * gain, 0.0]
}
fn handles(p: &Parameters<Params>, c: [f64; 2], r: f64) -> Result<[[f64; 2]; 2], Error> {
    let view = if popup(p, Params::Projection)? < 3 {
        value(p, Params::Fov)? / 180.0
    } else {
        (value(p, Params::OrthoWidth)? / 10000.0).sqrt()
    };
    let radius = (value(p, Params::Radius)? / 10000.0).sqrt();
    Ok([
        [c[0] + r * (1.2 + view.min(2.0)), c[1]],
        [c[0], c[1] + r * (1.2 + radius.min(2.0))],
    ])
}
struct SphereRig {
    radii: [f64; 2],
    handle: [f64; 2],
    base_radius: f64,
    pixel_aspect: f64,
}
fn sphere_rig(d: InData, p: &Parameters<Params>) -> Result<Option<SphereRig>, Error> {
    let projection = match popup(p, Params::Projection)? {
        2 => Projection::Fisheye,
        3 => Projection::Orthographic,
        4 => Projection::Isometric,
        _ => return Ok(None),
    };
    let par = f64::from(f32::from(d.pixel_aspect_ratio())).max(1e-6);
    let w = d.width() as f64;
    let h = d.height() as f64;
    let camera = Camera {
        projection,
        aspect: w * par / h.max(1.0),
        ortho_width: value(p, Params::OrthoWidth)?,
        radius: value(p, Params::Radius)?,
        ..Camera::default()
    };
    let base_radius = camera.sphere_radius().unwrap() * w * par * 0.5;
    let radius = base_radius * value(p, Params::SphereScale)?.clamp(1.0, 1000.0) / 100.0;
    // Keep the handle reachable when the actual outline is tiny or outside the canvas.
    let short = (w * par).min(h).max(1.0);
    let handle_radius = radius.clamp(short * 0.27, short * 0.45 * 2.0_f64.sqrt());
    let diagonal = handle_radius / 2.0_f64.sqrt();
    Ok(Some(SphereRig {
        radii: [radius / par, radius],
        handle: [w * 0.5 + diagonal / par, h * 0.5 - diagonal],
        base_radius,
        pixel_aspect: par,
    }))
}
fn scale_by_drag(current: f64, delta: f64, base_radius: f64) -> f64 {
    (current + delta * 100.0 / base_radius.max(1e-6)).clamp(1.0, 1000.0)
}
fn line(
    d: InData,
    e: &mut ae::EventExtra,
    points: &[[f64; 2]],
    color: [f32; 3],
) -> Result<(), Error> {
    let draw = e.context_handle().drawing_reference()?;
    let mut path = draw.supplier()?.new_path()?;
    for (i, &p) in points.iter().enumerate() {
        let f = to_frame(d, e, p)?;
        if i == 0 {
            path.move_to(f[0] as f32, f[1] as f32)?;
        } else {
            path.line_to(f[0] as f32, f[1] as f32)?;
        }
    }
    let outline = draw.supplier()?.new_pen(
        &ae::drawbot::ColorRgba {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.8,
        },
        3.5,
    )?;
    draw.surface()?.stroke_path(&outline, &path)?;
    let pen = draw.supplier()?.new_pen(
        &ae::drawbot::ColorRgba {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: 1.0,
        },
        1.5,
    )?;
    draw.surface()?.stroke_path(&pen, &path)
}
fn draw(d: InData, p: &Parameters<Params>, e: &mut ae::EventExtra) -> Result<(), Error> {
    let (c, r) = center(d);
    let h = handles(p, c, r)?;
    let ring: Vec<_> = (0..=64)
        .map(|i| {
            let t = i as f64 * TAU / 64.0;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect();
    line(d, e, &ring, [0.8, 0.8, 0.8])?;
    // Both rotation representations share the same visible rig.
    let label = if popup(p, Params::RotationMode)? == 2 {
        "Quaternion: drag to rotate"
    } else {
        "Euler: drag to rotate"
    };
    let f = to_frame(d, e, [c[0] - r, c[1] - r])?;
    let draw = e.context_handle().drawing_reference()?;
    let supplier = draw.supplier()?;
    let surface = draw.surface()?;
    let font = supplier.new_default_font(supplier.default_font_size()?)?;
    surface.paint_rect(
        &ae::drawbot::ColorRgba {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.75,
        },
        &ae::drawbot::RectF32 {
            left: f[0] as f32 - 3.0,
            top: f[1] as f32 - 23.0,
            width: 190.0,
            height: 21.0,
        },
    )?;
    let brush = supplier.new_brush(&ae::drawbot::ColorRgba {
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    })?;
    surface.draw_string(
        &brush,
        &font,
        label,
        &ae::drawbot::PointF32 {
            x: f[0] as f32,
            y: f[1] as f32 - 7.0,
        },
        ae::drawbot::TextAlignment::Left,
        ae::drawbot::TextTruncation::None,
        0.0,
    )?;
    let q = rotation(p)?;
    for (axis, color) in [
        ([1.0, 0.0, 0.0], [1.0, 0.25, 0.25]),
        ([0.0, 1.0, 0.0], [0.25, 1.0, 0.25]),
        ([0.0, 0.0, 1.0], [0.3, 0.6, 1.0]),
    ] {
        let v = q.rotate(axis);
        line(d, e, &[c, [c[0] + v[0] * r, c[1] + v[1] * r]], color)?;
    }
    let half = h[0][0] - c[0];
    let height = half * d.height() as f64 / d.width().max(1) as f64;
    line(
        d,
        e,
        &[
            [c[0] - half, c[1] - height],
            [c[0] + half, c[1] - height],
            [c[0] + half, c[1] + height],
            [c[0] - half, c[1] + height],
            [c[0] - half, c[1] - height],
        ],
        [0.2, 0.8, 1.0],
    )?;
    line(d, e, &[c, h[1]], [1.0, 0.75, 0.2])?;
    for (pt, color) in h.into_iter().zip([[0.2, 0.8, 1.0], [1.0, 0.75, 0.2]]) {
        let f = to_frame(d, e, pt)?;
        let draw = e.context_handle().drawing_reference()?;
        draw.surface()?.paint_rect(
            &ae::drawbot::ColorRgba {
                red: color[0],
                green: color[1],
                blue: color[2],
                alpha: 1.0,
            },
            &ae::drawbot::RectF32 {
                left: f[0] as f32 - 4.0,
                top: f[1] as f32 - 4.0,
                width: 8.0,
                height: 8.0,
            },
        )?;
    }
    if let Some(rig) = sphere_rig(d, p)? {
        let color = [0.95, 0.4, 1.0];
        // PF coordinate callbacks use 16.16 fixed point. Avoid unrepresentable points.
        if c[0] + rig.radii[0] < 30000.0 && c[1] + rig.radii[1] < 30000.0 {
            let outline: Vec<_> = (0..=96)
                .map(|i| {
                    let t = i as f64 * TAU / 96.0;
                    [c[0] + rig.radii[0] * t.cos(), c[1] + rig.radii[1] * t.sin()]
                })
                .collect();
            line(d, e, &outline, color)?;
            let edge = [
                c[0] + rig.radii[0] / 2.0_f64.sqrt(),
                c[1] - rig.radii[1] / 2.0_f64.sqrt(),
            ];
            line(d, e, &[edge, rig.handle], color)?;
        }
        let f = to_frame(d, e, rig.handle)?;
        let draw = e.context_handle().drawing_reference()?;
        let supplier = draw.supplier()?;
        let surface = draw.surface()?;
        surface.paint_rect(
            &ae::drawbot::ColorRgba {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.9,
            },
            &ae::drawbot::RectF32 {
                left: f[0] as f32 - 6.0,
                top: f[1] as f32 - 6.0,
                width: 12.0,
                height: 12.0,
            },
        )?;
        let tint = ae::drawbot::ColorRgba {
            red: color[0],
            green: color[1],
            blue: color[2],
            alpha: 1.0,
        };
        surface.paint_rect(
            &tint,
            &ae::drawbot::RectF32 {
                left: f[0] as f32 - 4.0,
                top: f[1] as f32 - 4.0,
                width: 8.0,
                height: 8.0,
            },
        )?;
        surface.paint_rect(
            &ae::drawbot::ColorRgba {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.75,
            },
            &ae::drawbot::RectF32 {
                left: f[0] as f32 + 8.0,
                top: f[1] as f32 - 23.0,
                width: 110.0,
                height: 21.0,
            },
        )?;
        let font = supplier.new_default_font(supplier.default_font_size()?)?;
        let brush = supplier.new_brush(&tint)?;
        surface.draw_string(
            &brush,
            &font,
            &format!("Size: {:.1}%", value(p, Params::SphereScale)?),
            &ae::drawbot::PointF32 {
                x: f[0] as f32 + 11.0,
                y: f[1] as f32 - 7.0,
            },
            ae::drawbot::TextAlignment::Left,
            ae::drawbot::TextTruncation::None,
            0.0,
        )?;
    }
    Ok(())
}
fn remember(e: &mut ae::EventExtra, m: [f64; 2]) {
    e.set_continue_refcon(1, (m[0] * 65536.0) as _);
    e.set_continue_refcon(2, (m[1] * 65536.0) as _);
}
fn click(d: InData, p: &Parameters<Params>, e: &mut ae::EventExtra) -> Result<bool, Error> {
    let (c, r) = center(d);
    let h = handles(p, c, r)?;
    let m = mouse(d, e)?;
    let screen = e.screen_point();
    let mut mode = 0;
    for (i, pt) in h.into_iter().enumerate() {
        let f = to_frame(d, e, pt)?;
        if (f[0] - screen.h as f64).hypot(f[1] - screen.v as f64) < 12.0 {
            mode = i + 3;
        }
    }
    if let Some(rig) = sphere_rig(d, p)? {
        let f = to_frame(d, e, rig.handle)?;
        if (f[0] - screen.h as f64).hypot(f[1] - screen.v as f64) < 12.0 {
            mode = 5;
        }
    }
    if mode == 3 && checked(p, Params::SyncCamera)? && popup(p, Params::Projection)? == 1 {
        return Ok(false);
    }
    if mode == 0 {
        let distance = (m[0] - c[0]).hypot(m[1] - c[1]);
        if distance < r * 0.88 {
            mode = 1;
        } else if distance < r * 1.12 {
            mode = 2;
        }
    }
    if mode == 0 {
        return Ok(false);
    }
    e.set_continue_refcon(0, mode as _);
    remember(e, m);
    e.set_send_drag(true);
    Ok(true)
}
fn drag(d: InData, p: &mut Parameters<Params>, e: &mut ae::EventExtra) -> Result<bool, Error> {
    let mode = e.continue_refcon(0);
    if mode == 0 {
        return Ok(false);
    }
    let old = [
        e.continue_refcon(1) as f64 / 65536.0,
        e.continue_refcon(2) as f64 / 65536.0,
    ];
    let m = mouse(d, e)?;
    let (c, r) = center(d);
    match mode {
        1 => {
            let delta = drag_angles(
                [m[0] - old[0], m[1] - old[1]],
                r,
                value(p, Params::RigSensitivity)?,
            );
            add_rotation(p, delta)?;
        }
        2 => {
            let angle = (m[1] - c[1]).atan2(m[0] - c[0]) - (old[1] - c[1]).atan2(old[0] - c[0]);
            let angle = (angle + PI).rem_euclid(TAU) - PI;
            add_rotation(p, [0.0, 0.0, angle * value(p, Params::RigSensitivity)?])?;
        }
        3 => {
            if popup(p, Params::Projection)? < 3 {
                let max = if popup(p, Params::Projection)? == 1 {
                    179.0
                } else {
                    360.0
                };
                set_float(
                    p,
                    Params::Fov,
                    (value(p, Params::Fov)? + (m[0] - old[0]) / r * 180.0).clamp(1.0, max),
                )?;
            } else {
                set_float(
                    p,
                    Params::OrthoWidth,
                    (value(p, Params::OrthoWidth)? * ((m[0] - old[0]) / r).exp())
                        .clamp(1.0, 1000000.0),
                )?;
            }
        }
        4 => {
            set_float(
                p,
                Params::Radius,
                (value(p, Params::Radius)? * ((m[1] - old[1]) / r).exp()).clamp(1.0, 1000000.0),
            )?;
        }
        5 => {
            if let Some(rig) = sphere_rig(d, p)? {
                let distance =
                    |pt: [f64; 2]| ((pt[0] - c[0]) * rig.pixel_aspect).hypot(pt[1] - c[1]);
                set_float(
                    p,
                    Params::SphereScale,
                    scale_by_drag(
                        value(p, Params::SphereScale)?,
                        distance(m) - distance(old),
                        rig.base_radius,
                    ),
                )?;
            }
        }
        _ => return Ok(false),
    }
    remember(e, m);
    e.set_send_drag(!e.last_time());
    if e.last_time() {
        e.set_continue_refcon(0, 0);
    }
    Ok(true)
}

pub fn event(d: InData, p: &mut Parameters<Params>, e: &mut ae::EventExtra) -> Result<(), Error> {
    if d.is_premiere()
        || !matches!(
            e.window_type(),
            ae::WindowType::Comp | ae::WindowType::Layer
        )
        || !matches!(
            e.event(),
            ae::Event::Draw(_) | ae::Event::Click(_) | ae::Event::Drag(_)
        )
    {
        return Ok(());
    }
    // Other event selectors may not supply a parameter array. Never check out
    // parameters during PF_Cmd_EVENT: AE forbids that callback here.
    if p.raw_params().is_empty() || !checked(p, Params::ShowControls)? {
        return Ok(());
    }
    let handled = match e.event() {
        ae::Event::Draw(_) => {
            draw(d, p, e)?;
            true
        }
        ae::Event::Click(_) => {
            if e.send_drag() {
                drag(d, p, e)?
            } else {
                click(d, p, e)?
            }
        }
        ae::Event::Drag(_) => drag(d, p, e)?,
        _ => false,
    };
    if handled {
        let mut flags = ae::EventOutFlags::HANDLED_EVENT;
        if matches!(e.event(), ae::Event::Click(_) | ae::Event::Drag(_)) {
            flags |= ae::EventOutFlags::ALWAYS_UPDATE;
        }
        e.set_event_out_flags(flags);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_is_linear_without_a_hemisphere_limit() {
        let total = drag_angles([1600.0, -400.0], 100.0, 1.0);
        assert!((total[1] - 8.0 * PI).abs() < 1e-12);
        let step = drag_angles([16.0, -4.0], 100.0, 1.0);
        for i in 0..3 {
            assert!((total[i] - step[i] * 100.0).abs() < 1e-12);
        }
        assert_eq!(drag_angles([16.0, -4.0], 100.0, 2.0), step.map(|v| v * 2.0));
    }
    #[test]
    fn sphere_size_drag_is_linear_reversible_and_bounded() {
        assert_eq!(scale_by_drag(100.0, -128.0, 256.0), 50.0);
        assert_eq!(scale_by_drag(50.0, 128.0, 256.0), 100.0);
        assert_eq!(scale_by_drag(100.0, 256.0, 256.0), 200.0);
        assert_eq!(scale_by_drag(100.0, -1000.0, 256.0), 1.0);
        assert_eq!(scale_by_drag(100.0, 10000.0, 256.0), 1000.0);
    }
}
