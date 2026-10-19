use crate::{Params, curve::Curve};
use ae::drawbot::{ColorRgba, PointF32, RectF32};
use after_effects as ae;

pub const WIDTH: u16 = 340;
pub const HEIGHT: u16 = 224;

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
            height: (frame.height() as f32 - 58.0).max(16.0),
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
            let mouse = event.screen_point();
            let [x, y] = plot.value(mouse);
            let selected = (0..curve.len()).find(|&k| {
                let p = plot.point(curve.points[k]);
                (p[0] - mouse.h as f32).powi(2) + (p[1] - mouse.v as f32).powi(2) <= 81.0
            });
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
                let [x, y] = plot.value(event.screen_point());
                curve.move_point(selection as usize - 1, x, y);
                event.set_send_drag(!event.last_time());
                commit(params, event, curve)?;
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
    let smooth = params.get(Params::Interpolation)?.as_popup()?.value() == 1;
    let mut path = supplier.new_path()?;
    for i in 0..=256 {
        let x = i as f32 / 256.0;
        let p = plot.point([x, curve.sample(x, smooth)]);
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
