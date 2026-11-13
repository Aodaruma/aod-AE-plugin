use crate::{
    Params,
    curve::{Curve, Interpolation},
};
use ae::drawbot::{ColorRgba, PointF32, RectF32};
use after_effects as ae;

pub const WIDTH: u16 = 340;
pub const HEIGHT: u16 = 280;
const DELETE_MARGIN: i32 = 32;

#[derive(Clone, Copy)]
struct Plot {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl Plot {
    fn new(frame: ae::Rect) -> Self {
        Self {
            left: frame.left as f32 + 20.0,
            top: frame.top as f32 + 14.0,
            width: (frame.width() as f32 - 32.0).max(16.0),
            height: (frame.height() as f32 - 114.0).max(16.0),
        }
    }
    fn point(self, p: [f32; 2]) -> [f32; 2] {
        [
            self.left + self.width * p[0],
            self.top + self.height * (1.0 - p[1]),
        ]
    }
    fn value(self, p: ae::Point) -> [f32; 2] {
        [
            (p.h as f32 - self.left) / self.width,
            1.0 - (p.v as f32 - self.top) / self.height,
        ]
    }
}

fn hit_handle(curve: &Curve, plot: Plot, mouse: ae::Point) -> Option<(usize, usize)> {
    (0..curve.len())
        .flat_map(|k| (0..2).map(move |side| (k, side)))
        .filter(|&(k, side)| curve.has_handle(k, side))
        .filter_map(|(k, side)| {
            let p = plot.point(curve.handles(k)[side]);
            let distance = (p[0] - mouse.h as f32).powi(2) + (p[1] - mouse.v as f32).powi(2);
            (distance <= 49.0).then_some((distance, k, side))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, k, side)| (k, side))
}

fn hit_anchor(curve: &Curve, plot: Plot, mouse: ae::Point, bezier: bool) -> Option<usize> {
    let distance = |p| {
        let p = plot.point(p);
        (p[0] - mouse.h as f32).powi(2) + (p[1] - mouse.v as f32).powi(2)
    };
    let selected = (0..curve.len())
        .map(|k| (k, distance(curve.points[k])))
        .filter(|&(_, d)| d <= 81.0)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    // Prefer the nearest control, so short handles can still be grabbed or reset.
    let handle_distance = if bezier {
        hit_handle(curve, plot, mouse).map(|(k, side)| distance(curve.handles(k)[side]))
    } else {
        None
    };
    selected
        .filter(|&(_, d)| handle_distance.is_none_or(|h| d <= h))
        .map(|(k, _)| k)
}

fn apply_drag(
    curve: &mut Curve,
    k: usize,
    frame: ae::Rect,
    mouse: ae::Point,
    released: bool,
) -> bool {
    if k >= curve.len() {
        return false;
    }
    let outside = mouse.h <= frame.left - DELETE_MARGIN
        || mouse.h >= frame.right + DELETE_MARGIN
        || mouse.v <= frame.top - DELETE_MARGIN
        || mouse.v >= frame.bottom + DELETE_MARGIN;
    if k > 0 && k + 1 < curve.len() && outside {
        // Keep the point while the mouse is down so returning to the UI cancels deletion.
        if released {
            curve.remove(k);
        }
        released
    } else {
        let [x, y] = Plot::new(frame).value(mouse);
        curve.move_point(k, x, y);
        true
    }
}

fn color(r: f32, g: f32, b: f32) -> ColorRgba {
    ColorRgba {
        red: r,
        green: g,
        blue: b,
        alpha: 1.0,
    }
}

pub fn event(
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
) -> Result<(), ae::Error> {
    if event.window_type() != ae::WindowType::Effect
        || event.effect_area() != ae::EffectArea::Control
        || Some(event.param_index()) != params.index(Params::Profile)
    {
        return Ok(());
    }
    let mut curve = (*params
        .get(Params::Profile)?
        .as_arbitrary()?
        .value::<Curve>()?)
    .sanitized();
    let plot = Plot::new(event.current_frame());
    match event.event() {
        ae::Event::Draw(_) => draw(params, event, curve, plot)?,
        ae::Event::AdjustCursor(_) => {
            event.set_cursor(ae::CursorType::Crosshairs);
            event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
        }
        ae::Event::Click(_) => {
            event.set_continue_refcon(0, 0);
            event.set_continue_refcon(1, 0);
            let mouse = event.screen_point();
            let [x, y] = plot.value(mouse);
            let bezier =
                Interpolation::from_popup(params.get(Params::Interpolation)?.as_popup()?.value())
                    == Interpolation::Bezier;
            let selected = hit_anchor(&curve, plot, mouse, bezier);
            if selected.is_none()
                && bezier
                && let Some((k, side)) = hit_handle(&curve, plot, mouse)
            {
                if event.modifiers().contains(ae::Modifiers::CMD_CTRL_KEY) {
                    curve.reset_handles(k);
                    commit(params, event, curve)?;
                } else {
                    event.set_continue_refcon(0, (k + 1) as _);
                    event.set_continue_refcon(1, (side + 1) as _);
                    event.set_send_drag(true);
                    event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
                }
                return Ok(());
            }
            if let Some(k) = selected
                && event.modifiers().contains(ae::Modifiers::CMD_CTRL_KEY)
            {
                curve.remove(k);
                commit(params, event, curve)?;
            } else if (-0.03..=1.03).contains(&x)
                && (-0.05..=1.05).contains(&y)
                && let Some(k) = selected.or_else(|| curve.insert(x, y))
            {
                event.set_continue_refcon(0, (k + 1) as _);
                event.set_send_drag(true);
                commit(params, event, curve)?;
            }
        }
        ae::Event::Drag(_) => {
            let selection = event.continue_refcon(0);
            if selection > 0 {
                let handle = event.continue_refcon(1);
                let changed = if handle > 0 {
                    let [x, y] = plot.value(event.screen_point());
                    curve.move_handle(
                        selection as usize - 1,
                        handle as usize - 1,
                        x,
                        y,
                        event.modifiers().contains(ae::Modifiers::OPT_ALT_KEY),
                    );
                    true
                } else {
                    apply_drag(
                        &mut curve,
                        selection as usize - 1,
                        event.current_frame(),
                        event.screen_point(),
                        event.last_time(),
                    )
                };
                event.set_send_drag(!event.last_time());
                if event.last_time() {
                    event.set_continue_refcon(0, 0);
                    event.set_continue_refcon(1, 0);
                }
                if changed {
                    commit(params, event, curve)?;
                } else {
                    event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn commit(
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
    curve: Curve,
) -> Result<(), ae::Error> {
    // set_value also sets PF_ChangeFlag_CHANGED_VALUE, enabling AE undo and keyframes.
    params
        .get_mut(Params::Profile)?
        .as_arbitrary_mut()?
        .set_value(curve)?;
    ae::suites::App::new()?.invalidate_rect(event.context_handle(), Some(event.current_frame()))?;
    event.set_event_out_flags(
        ae::EventOutFlags::HANDLED_EVENT
            | ae::EventOutFlags::ALWAYS_UPDATE
            | ae::EventOutFlags::UPDATE_NOW,
    );
    Ok(())
}

fn draw(
    params: &ae::Parameters<Params>,
    event: &mut ae::EventExtra,
    curve: Curve,
    plot: Plot,
) -> Result<(), ae::Error> {
    let drawbot = event.context_handle().drawing_reference()?;
    let supplier = drawbot.supplier()?;
    let surface = drawbot.surface()?;
    let frame = event.current_frame();
    surface.paint_rect(
        &color(0.13, 0.13, 0.13),
        &RectF32 {
            left: frame.left as f32,
            top: frame.top as f32,
            width: frame.width() as f32,
            height: frame.height() as f32,
        },
    )?;
    surface.paint_rect(
        &color(0.075, 0.08, 0.09),
        &RectF32 {
            left: plot.left,
            top: plot.top,
            width: plot.width,
            height: plot.height,
        },
    )?;
    let grid_pen = supplier.new_pen(&color(0.26, 0.28, 0.31), 1.0)?;
    let mut grid = supplier.new_path()?;
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        grid.move_to(plot.left + t * plot.width, plot.top)?;
        grid.line_to(plot.left + t * plot.width, plot.top + plot.height)?;
        grid.move_to(plot.left, plot.top + t * plot.height)?;
        grid.line_to(plot.left + plot.width, plot.top + t * plot.height)?;
    }
    surface.stroke_path(&grid_pen, &grid)?;
    let interpolation =
        Interpolation::from_popup(params.get(Params::Interpolation)?.as_popup()?.value());
    if interpolation == Interpolation::BSpline {
        let mut polygon = supplier.new_path()?;
        for k in 0..curve.len() {
            let p = plot.point(curve.points[k]);
            if k == 0 {
                polygon.move_to(p[0], p[1])?;
            } else {
                polygon.line_to(p[0], p[1])?;
            }
        }
        surface.stroke_path(&supplier.new_pen(&color(0.38, 0.42, 0.47), 1.0)?, &polygon)?;
    }
    if interpolation == Interpolation::Bezier {
        let pen = supplier.new_pen(&color(0.44, 0.62, 0.74), 1.0)?;
        for k in 0..curve.len() {
            let anchor = plot.point(curve.points[k]);
            for (side, handle) in curve.handles(k).into_iter().enumerate() {
                if !curve.has_handle(k, side) {
                    continue;
                }
                let p = plot.point(handle);
                let mut line = supplier.new_path()?;
                line.move_to(anchor[0], anchor[1])?;
                line.line_to(p[0], p[1])?;
                surface.stroke_path(&pen, &line)?;
                surface.paint_rect(
                    &color(0.40, 0.76, 0.96),
                    &RectF32 {
                        left: p[0] - 3.0,
                        top: p[1] - 3.0,
                        width: 6.0,
                        height: 6.0,
                    },
                )?;
            }
        }
    }
    let sampler = curve.sampler(interpolation);
    let mut path = supplier.new_path()?;
    for i in 0..=256 {
        let x = i as f32 / 256.0;
        let p = plot.point([x, sampler.sample(x)]);
        if i == 0 {
            path.move_to(p[0], p[1])?;
        } else {
            path.line_to(p[0], p[1])?;
        }
    }
    surface.stroke_path(&supplier.new_pen(&color(0.30, 0.74, 1.0), 2.0)?, &path)?;
    let brush = supplier.new_brush(&color(0.88, 0.94, 1.0))?;
    for k in 0..curve.len() {
        let p = plot.point(curve.points[k]);
        let mut node = supplier.new_path()?;
        node.add_arc(&PointF32 { x: p[0], y: p[1] }, 3.7, 0.0, 360.0)?;
        node.close()?;
        surface.fill_path(&brush, &node, ae::drawbot::FillType::Winding)?;
    }
    let font = supplier.new_default_font(11.0)?;
    for (label, x, y) in [
        ("1", plot.left - 14.0, plot.top + 9.0),
        ("0", plot.left - 14.0, plot.top + plot.height),
        (
            "0   Distance / Width   1",
            plot.left,
            plot.top + plot.height + 15.0,
        ),
        (
            "Click: add | Drag: move | Ctrl/Cmd-click: delete",
            plot.left,
            plot.top + plot.height + 33.0,
        ),
        (
            "Drag 32 px beyond UI and release: delete",
            plot.left,
            plot.top + plot.height + 51.0,
        ),
        (
            if interpolation == Interpolation::Bezier {
                "Handles: drag | Alt/Option: break tangent"
            } else {
                ""
            },
            plot.left,
            plot.top + plot.height + 69.0,
        ),
        (
            if interpolation == Interpolation::Bezier {
                "Ctrl/Cmd-click handle: reset auto tangents"
            } else {
                ""
            },
            plot.left,
            plot.top + plot.height + 87.0,
        ),
    ] {
        surface.draw_string(
            &brush,
            &font,
            label,
            &PointF32 { x, y },
            ae::drawbot::TextAlignment::Left,
            ae::drawbot::TextTruncation::None,
            0.0,
        )?;
    }
    event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> ae::Rect {
        ae::Rect {
            left: 100,
            top: 200,
            right: 440,
            bottom: 444,
        }
    }

    #[test]
    fn short_handles_take_precedence_over_neighboring_anchor_hit_areas() {
        let mut curve = Curve::preset(1);
        curve.insert(0.05, 0.05).unwrap();
        let plot = Plot::new(frame());
        let p = plot.point(curve.handles(0)[1]);
        let mouse = ae::Point {
            h: p[0].round() as _,
            v: p[1].round() as _,
        };
        assert_eq!(hit_handle(&curve, plot, mouse), Some((0, 1)));
        assert_eq!(hit_anchor(&curve, plot, mouse, true), None);
        assert_eq!(hit_anchor(&curve, plot, mouse, false), Some(0));
        let p = plot.point(curve.points[0]);
        let mouse = ae::Point {
            h: p[0] as _,
            v: p[1] as _,
        };
        assert_eq!(hit_anchor(&curve, plot, mouse, true), Some(0));
    }

    #[test]
    fn drag_deletion_requires_release_beyond_the_ui_margin() {
        for mouse in [
            ae::Point { h: 68, v: 300 },
            ae::Point { h: 472, v: 300 },
            ae::Point { h: 270, v: 168 },
            ae::Point { h: 270, v: 476 },
        ] {
            let mut curve = Curve::default();
            let original = curve;
            assert!(!apply_drag(&mut curve, 2, frame(), mouse, false));
            assert_eq!(curve, original);
            assert!(apply_drag(&mut curve, 2, frame(), mouse, true));
            assert_eq!(curve.len(), original.len() - 1);
            assert!(curve.valid());
        }
    }

    #[test]
    fn returning_inside_cancels_deletion_and_endpoints_are_protected() {
        let outside = ae::Point { h: 600, v: 600 };
        let mut curve = Curve::default();
        let count = curve.len();
        apply_drag(&mut curve, 2, frame(), outside, false);
        apply_drag(&mut curve, 2, frame(), ae::Point { h: 280, v: 290 }, true);
        assert_eq!(curve.len(), count);
        for k in [0, count - 1] {
            apply_drag(&mut curve, k, frame(), outside, true);
            assert_eq!(curve.len(), count);
            assert!(curve.valid());
        }
        for mouse in [
            ae::Point { h: 69, v: 300 },
            ae::Point { h: 471, v: 300 },
            ae::Point { h: 270, v: 169 },
            ae::Point { h: 270, v: 475 },
        ] {
            apply_drag(&mut curve, 2, frame(), mouse, true);
            assert_eq!(curve.len(), count);
            assert!(curve.valid());
        }
    }
}
