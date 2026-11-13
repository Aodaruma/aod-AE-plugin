//! Effect Controls shutter editor. Interaction state belongs to the AE drag event.
use super::{curve::ShutterCurve, host::Params};
use after_effects::{self as ae, drawbot as db};

struct Graph {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}
impl Graph {
    fn new(frame: ae::Rect) -> Self {
        Self {
            left: frame.left as f32 + 24.,
            top: frame.top as f32 + 44.,
            width: (frame.width() as f32 - 36.).max(60.),
            height: (frame.height() as f32 - 84.).max(40.),
        }
    }
    fn screen(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.left + p[0] * self.width,
            self.top + (1. - p[1]) * self.height,
        ]
    }
    fn normalized(&self, p: ae::Point) -> [f32; 2] {
        [
            (p.h as f32 - self.left) / self.width,
            1. - (p.v as f32 - self.top) / self.height,
        ]
    }
    fn hit(&self, p: ae::Point) -> bool {
        let q = self.normalized(p);
        q.iter().all(|v| (-0.04..=1.04).contains(v))
    }
    fn nearest(&self, curve: &ShutterCurve, p: ae::Point) -> Option<usize> {
        curve
            .points
            .iter()
            .enumerate()
            .map(|(i, &point)| {
                let xy = self.screen(point);
                (i, (xy[0] - p.h as f32).hypot(xy[1] - p.v as f32))
            })
            .filter(|(_, d)| *d <= 10.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}
fn active(params: &ae::Parameters<Params>) -> Result<ShutterCurve, ae::Error> {
    let profile = params.get(Params::Curve)?.as_popup()?.value();
    if profile < 4 {
        return Ok(ShutterCurve::preset((profile - 1).max(0) as u32));
    }
    Ok(params
        .get(Params::ExposureCurve)?
        .as_arbitrary()?
        .value::<ShutterCurve>()?
        .sanitized())
}
fn write(params: &mut ae::Parameters<Params>, curve: ShutterCurve) -> Result<(), ae::Error> {
    params
        .get_mut(Params::ExposureCurve)?
        .as_arbitrary_mut()?
        .set_value(curve)?;
    params.get_mut(Params::Curve)?.as_popup_mut()?.set_value(4);
    Ok(())
}
fn color(r: f32, g: f32, b: f32) -> db::ColorRgba {
    db::ColorRgba {
        red: r,
        green: g,
        blue: b,
        alpha: 1.,
    }
}
fn draw(params: &ae::Parameters<Params>, event: &mut ae::EventExtra) -> Result<(), ae::Error> {
    if event.in_flags().contains(ae::EventInFlags::DONT_DRAW) {
        return Ok(());
    }
    let frame = event.current_frame();
    let g = Graph::new(frame);
    let curve = active(params)?;
    let reference = event.context_handle().drawing_reference()?;
    let supplier = reference.supplier()?;
    let surface = reference.surface()?;
    surface.paint_rect(
        &color(0.13, 0.13, 0.13),
        &db::RectF32 {
            left: frame.left as f32,
            top: frame.top as f32,
            width: frame.width() as f32,
            height: frame.height() as f32,
        },
    )?;
    surface.paint_rect(
        &color(0.075, 0.075, 0.075),
        &db::RectF32 {
            left: g.left,
            top: g.top,
            width: g.width,
            height: g.height,
        },
    )?;
    let grid_pen = supplier.new_pen(&color(0.25, 0.25, 0.25), 1.)?;
    let curve_pen = supplier.new_pen(&color(1., 0.63, 0.22), 2.)?;
    let text_brush = supplier.new_brush(&color(0.85, 0.85, 0.85))?;
    let point_brush = supplier.new_brush(&color(1., 0.72, 0.34))?;
    let font = supplier.new_default_font(11.)?;
    let label = |text: &str, x: f32, y: f32| {
        surface.draw_string(
            &text_brush,
            &font,
            text,
            &db::PointF32 { x, y },
            db::TextAlignment::Left,
            db::TextTruncation::None,
            0.,
        )
    };
    let mut grid = supplier.new_path()?;
    for i in 0..=4 {
        let t = i as f32 / 4.;
        grid.move_to(g.left + t * g.width, g.top)?;
        grid.line_to(g.left + t * g.width, g.top + g.height)?;
        grid.move_to(g.left, g.top + t * g.height)?;
        grid.line_to(g.left + g.width, g.top + t * g.height)?;
    }
    surface.stroke_path(&grid_pen, &grid)?;
    let mut path = supplier.new_path()?;
    for i in 0..=160 {
        let x = i as f32 / 160.;
        let p = g.screen([x, curve.evaluate(x)]);
        if i == 0 {
            path.move_to(p[0], p[1])?;
        } else {
            path.line_to(p[0], p[1])?;
        }
    }
    surface.stroke_path(&curve_pen, &path)?;
    for &point in &curve.points {
        let p = g.screen(point);
        let mut dot = supplier.new_path()?;
        dot.add_arc(&db::PointF32 { x: p[0], y: p[1] }, 4., 0., 360.)?;
        dot.close()?;
        surface.fill_path(&point_brush, &dot, db::FillType::Winding)?;
    }
    for (i, name) in [
        "Flat",
        "Bell",
        "Rise",
        "Fall",
        if curve.smooth { "Smooth" } else { "Linear" },
    ]
    .iter()
    .enumerate()
    {
        let x = g.left + i as f32 * g.width / 5.;
        surface.paint_rect(
            &color(0.23, 0.23, 0.23),
            &db::RectF32 {
                left: x,
                top: frame.top as f32 + 5.,
                width: g.width / 5. - 3.,
                height: 23.,
            },
        )?;
        label(name, x + 4., frame.top as f32 + 21.)?;
    }
    label("Open", g.left, g.top - 7.)?;
    label("1", g.left - 15., g.top + 5.)?;
    label("0", g.left - 15., g.top + g.height + 3.)?;
    label("0   Exposure time", g.left, g.top + g.height + 16.)?;
    label("1", g.left + g.width - 7., g.top + g.height + 16.)?;
    label(
        "Click: add / drag    Alt-click: delete",
        g.left,
        g.top + g.height + 33.,
    )?;
    event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
    Ok(())
}
pub fn handle(
    params: &mut ae::Parameters<Params>,
    event: &mut ae::EventExtra,
    out: &mut ae::OutData,
) -> Result<(), ae::Error> {
    // Non-input lifecycle events do not contain valid effect_win fields.
    if !matches!(
        event.event(),
        ae::Event::Draw(_) | ae::Event::Click(_) | ae::Event::Drag(_) | ae::Event::AdjustCursor(_)
    ) {
        return Ok(());
    }
    if event.window_type() != ae::WindowType::Effect
        || event.effect_area() != ae::EffectArea::Control
        || Some(event.param_index()) != params.index(Params::ExposureCurve)
    {
        return Ok(());
    }
    if matches!(event.event(), ae::Event::Draw(_)) {
        return draw(params, event);
    }
    let g = Graph::new(event.current_frame());
    let mouse = event.screen_point();
    if matches!(event.event(), ae::Event::AdjustCursor(_)) {
        if g.hit(mouse) {
            event.set_cursor(ae::CursorType::Crosshairs);
            event.set_event_out_flags(ae::EventOutFlags::HANDLED_EVENT);
        }
        return Ok(());
    }
    let mut curve = active(params)?;
    if matches!(event.event(), ae::Event::Click(_)) {
        event.set_continue_refcon(0, 0);
        if mouse.v >= event.current_frame().top + 5
            && mouse.v <= event.current_frame().top + 28
            && mouse.h as f32 >= g.left
            && (mouse.h as f32) < g.left + g.width
        {
            match ((mouse.h as f32 - g.left) * 5. / g.width) as u32 {
                0 => curve = ShutterCurve::default(),
                1 => curve = ShutterCurve::preset(2),
                2 => curve = ShutterCurve::preset(3),
                3 => curve = ShutterCurve::preset(4),
                _ => curve.smooth = !curve.smooth,
            }
        } else if g.hit(mouse) {
            let nearest = g.nearest(&curve, mouse);
            if event.modifiers().contains(ae::Modifiers::OPT_ALT_KEY) {
                if let Some(i) = nearest {
                    curve.remove(i);
                } else {
                    return Ok(());
                }
            } else {
                let Some(i) = nearest.or_else(|| curve.insert(g.normalized(mouse))) else {
                    return Ok(());
                };
                event.set_continue_refcon(0, (i + 1) as _);
                event.set_send_drag(true);
            }
        } else {
            return Ok(());
        }
    } else {
        let i = event.continue_refcon(0);
        if i <= 0 {
            return Ok(());
        }
        curve.move_point(i as usize - 1, g.normalized(mouse));
        event.set_send_drag(!event.last_time());
    }
    write(params, curve)?;
    ae::pf::suites::App::new()?
        .invalidate_rect(event.context_handle(), Some(event.current_frame()))?;
    out.set_out_flag(ae::OutFlags::RefreshUi, true);
    event.set_event_out_flags(
        ae::EventOutFlags::HANDLED_EVENT
            | ae::EventOutFlags::UPDATE_NOW
            | ae::EventOutFlags::ALWAYS_UPDATE,
    );
    Ok(())
}
