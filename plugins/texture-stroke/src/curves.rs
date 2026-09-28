use super::*;
use serde::{Deserialize, Serialize};

const MAX_POINTS: usize = 8;
const WIDTH: u16 = 300;
const HEIGHT: u16 = 218;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub(super) struct Knot {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub(super) struct Curve {
    // Fixed storage keeps arbitrary handles trivially movable. Version this
    // schema before changing its serialized layout.
    version: u32,
    count: usize,
    points: [Knot; MAX_POINTS],
}

impl Default for Curve {
    fn default() -> Self {
        let mut points = [Knot { x: 0.0, y: 0.0 }; MAX_POINTS];
        points[1] = Knot { x: 1.0, y: 1.0 };
        Self {
            version: 1,
            count: 2,
            points,
        }
    }
}

impl Curve {
    pub fn read(params: &Parameters<Params>, id: Params) -> Result<Self, Error> {
        let curve = *params.get(id)?.as_arbitrary()?.value::<Self>()?;
        if curve.version != 1 || !(2..=MAX_POINTS).contains(&curve.count) {
            return Err(Error::InternalStructDamaged);
        }
        let p = &curve.points[..curve.count];
        if p[0].x != 0.0
            || p[curve.count - 1].x != 1.0
            || p.iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite() || !(0.0..=1.0).contains(&p.y))
            || p.windows(2).any(|p| p[1].x - p[0].x < 0.0009)
        {
            return Err(Error::InternalStructDamaged);
        }
        Ok(curve)
    }
    pub fn flat(y: f32) -> Self {
        let mut result = Self::default();
        result.points[0].y = y;
        result.points[1].y = y;
        result
    }

    pub fn evaluate(&self, x: f32) -> f32 {
        let count = self.count.clamp(2, MAX_POINTS);
        let p = &self.points[..count];
        let x = x.clamp(0.0, 1.0);
        let i = p.partition_point(|p| p.x < x).clamp(1, count - 1) - 1;
        let width = (p[i + 1].x - p[i].x).max(1e-6);
        let t = ((x - p[i].x) / width).clamp(0.0, 1.0);
        let slope = |j: usize| (p[j + 1].y - p[j].y) / (p[j + 1].x - p[j].x).max(1e-6);
        // Monotone cubic Hermite slopes: smooth without ringing past the knots.
        let tangent = |j: usize| {
            if j == 0 {
                return slope(0);
            }
            if j + 1 == count {
                return slope(j - 1);
            }
            let a = slope(j - 1);
            let b = slope(j);
            if a * b <= 0.0 {
                return 0.0;
            }
            let h0 = p[j].x - p[j - 1].x;
            let h1 = p[j + 1].x - p[j].x;
            let w0 = 2.0 * h1 + h0;
            let w1 = h1 + 2.0 * h0;
            (w0 + w1) / (w0 / a + w1 / b)
        };
        let y = (2.0 * t * t * t - 3.0 * t * t + 1.0) * p[i].y
            + (t * t * t - 2.0 * t * t + t) * width * tangent(i)
            + (-2.0 * t * t * t + 3.0 * t * t) * p[i + 1].y
            + (t * t * t - t * t) * width * tangent(i + 1);
        if y.is_finite() {
            y.clamp(0.0, 1.0)
        } else {
            0.5
        }
    }

    fn insert(&mut self, x: f32, y: f32) -> Option<usize> {
        if self.count >= MAX_POINTS {
            return None;
        }
        let i = self.points[..self.count]
            .partition_point(|p| p.x < x)
            .clamp(1, self.count - 1);
        if self.points[i].x - self.points[i - 1].x < 0.0021 {
            return None;
        }
        self.points.copy_within(i..self.count, i + 1);
        self.count += 1;
        self.move_point(i, x, y);
        Some(i)
    }

    fn move_point(&mut self, i: usize, x: f32, y: f32) {
        if i >= self.count {
            return;
        }
        self.points[i] = Knot {
            x: if i == 0 {
                0.0
            } else if i + 1 == self.count {
                1.0
            } else {
                x.clamp(self.points[i - 1].x + 0.001, self.points[i + 1].x - 0.001)
            },
            y: y.clamp(0.0, 1.0),
        };
    }
}

impl ae::ArbitraryData<Curve> for Curve {
    fn interpolate(&self, other: &Self, t: f64) -> Self {
        let t = (t as f32).clamp(0.0, 1.0);
        if t == 0.0 {
            return *self;
        }
        if t == 1.0 {
            return *other;
        }
        let mut out = Self {
            count: MAX_POINTS,
            ..Self::default()
        };
        // Different point counts can be keyframed without indexing mismatches.
        for (i, p) in out.points.iter_mut().enumerate() {
            p.x = i as f32 / (MAX_POINTS - 1) as f32;
            p.y = lerp(self.evaluate(p.x), other.evaluate(p.x), t);
        }
        out
    }
}

pub(super) fn add(params: &mut Parameters<Params>, id: Params, name: &str) -> Result<(), Error> {
    let mut def = ArbitraryDef::new();
    def.set_default(Curve::default())?;
    params.add_customized(id, name, def, |p| {
        p.set_flags(ae::ParamFlag::SUPERVISE);
        p.set_ui_flags(ae::ParamUIFlags::CONTROL);
        p.set_ui_width(WIDTH);
        p.set_ui_height(HEIGHT);
        -1
    })
}

// Coordinates are relative to current_frame, not the ECW or OS window origin.
fn graph(event: &ae::EventExtra) -> [f32; 4] {
    let r = event.current_frame();
    [
        r.left as f32 + 38.0,
        r.top as f32 + 12.0,
        (r.width() as f32 - 52.0).max(40.0),
        146.0,
    ]
}

pub(super) fn event(
    params: &mut Parameters<Params>,
    event: &mut ae::EventExtra,
) -> Result<(), Error> {
    if event.window_type() != ae::WindowType::Effect
        || event.effect_area() != ae::EffectArea::Control
    {
        return Ok(());
    }
    let Some(channel) = dynamics::CHANNELS
        .iter()
        .position(|c| params.index(c.curve) == Some(event.param_index()))
    else {
        return Ok(());
    };
    let id = dynamics::CHANNELS[channel].curve;
    let mut curve = Curve::read(params, id)?;
    let [left, top, width, height] = graph(event);
    let mut changed = false;
    match event.event() {
        ae::Event::Draw(_) => {
            let drawing =
                ae::suites::EffectCustomUI::new()?.drawing_reference(event.context_handle())?;
            let supplier = drawing.supplier()?;
            let surface = drawing.surface()?;
            let color = |r, g, b| ae::drawbot::ColorRgba {
                red: r,
                green: g,
                blue: b,
                alpha: 1.0,
            };
            let r = event.current_frame();
            surface.paint_rect(
                &color(0.13, 0.13, 0.13),
                &ae::drawbot::RectF32 {
                    left: r.left as f32,
                    top: r.top as f32,
                    width: r.width() as f32,
                    height: r.height() as f32,
                },
            )?;
            let grid_pen = supplier.new_pen(&color(0.28, 0.28, 0.28), 1.0)?;
            let mut grid = supplier.new_path()?;
            for i in 0..=4 {
                let f = i as f32 / 4.0;
                grid.move_to(left + width * f, top)?;
                grid.line_to(left + width * f, top + height)?;
                grid.move_to(left, top + height * f)?;
                grid.line_to(left + width, top + height * f)?;
            }
            surface.stroke_path(&grid_pen, &grid)?;
            let pen = supplier.new_pen(&color(0.22, 0.72, 1.0), 2.0)?;
            let mut line = supplier.new_path()?;
            for i in 0..=128 {
                let x = i as f32 / 128.0;
                let (px, py) = (left + x * width, top + (1.0 - curve.evaluate(x)) * height);
                if i == 0 {
                    line.move_to(px, py)?;
                } else {
                    line.line_to(px, py)?;
                }
            }
            surface.stroke_path(&pen, &line)?;
            for p in &curve.points[..curve.count.clamp(2, MAX_POINTS)] {
                surface.paint_rect(
                    &color(0.85, 0.93, 1.0),
                    &ae::drawbot::RectF32 {
                        left: left + p.x * width - 3.0,
                        top: top + (1.0 - p.y) * height - 3.0,
                        width: 6.0,
                        height: 6.0,
                    },
                )?;
            }
            let font = supplier.new_default_font(11.0)?;
            let brush = supplier.new_brush(&color(0.83, 0.83, 0.83))?;
            let label = |s: &str, x, y| {
                surface.draw_string(
                    &brush,
                    &font,
                    s,
                    &ae::drawbot::PointF32 { x, y },
                    ae::drawbot::TextAlignment::Left,
                    ae::drawbot::TextTruncation::None,
                    0.0,
                )
            };
            let rotation = channel == dynamics::ROTATION;
            label(
                if rotation { "+180" } else { "200%" },
                left - 36.0,
                top + 7.0,
            )?;
            label(
                if rotation { "0" } else { "100%" },
                left - 36.0,
                top + height * 0.5 + 4.0,
            )?;
            label(
                if rotation { "-180" } else { "0%" },
                left - 36.0,
                top + height,
            )?;
            label("0", left, top + height + 15.0)?;
            label("Input", left + width * 0.4, top + height + 15.0)?;
            label("1", left + width - 5.0, top + height + 15.0)?;
            label("Reset", left, top + height + 36.0)?;
            label("Flat", left + 75.0, top + height + 36.0)?;
            label("Invert", left + 145.0, top + height + 36.0)?;
            label(
                "Click: add   Drag: move   Alt-click: remove",
                left - 30.0,
                top + height + 53.0,
            )?;
        }
        ae::Event::Click(click) => {
            event.set_continue_refcon(0, -1);
            let p = click.screen_point();
            let (mx, my) = (p.h as f32, p.v as f32);
            if my >= top + height + 21.0 && my <= top + height + 41.0 {
                if mx < left + 63.0 {
                    curve = Curve::default();
                } else if mx < left + 135.0 {
                    curve = Curve::flat(0.5);
                } else {
                    for p in &mut curve.points[..curve.count] {
                        p.y = 1.0 - p.y;
                    }
                }
                changed = true;
            } else if mx >= left - 8.0
                && mx <= left + width + 8.0
                && my >= top - 8.0
                && my <= top + height + 8.0
            {
                let x = ((mx - left) / width).clamp(0.0, 1.0);
                let y = (1.0 - (my - top) / height).clamp(0.0, 1.0);
                let near = curve.points[..curve.count]
                    .iter()
                    .position(|p| ((p.x - x) * width).hypot((p.y - y) * height) < 9.0);
                if event.modifiers().contains(ae::Modifiers::OPT_ALT_KEY) {
                    if let Some(i) = near.filter(|i| *i > 0 && *i + 1 < curve.count) {
                        curve.points.copy_within(i + 1..curve.count, i);
                        curve.count -= 1;
                        changed = true;
                    }
                } else if let Some(i) = near.or_else(|| curve.insert(x, y)) {
                    curve.move_point(i, x, y);
                    event.set_continue_refcon(0, i as _);
                    event.set_send_drag(true);
                    changed = true;
                }
            }
        }
        ae::Event::Drag(_) => {
            // AE resets send_drag between callbacks; request the next event
            // until mouse-up, otherwise a drag stops at its initial point.
            event.set_send_drag(!event.last_time());
            let i = event.continue_refcon(0);
            if i >= 0 {
                // AE 2025 can retain the initial click's screen_point during
                // DRAG. PF_GetMouse returns the current ECW-local position.
                let p = ae::suites::App::new()?.mouse_position()?;
                curve.move_point(
                    i as usize,
                    (p.h as f32 - left) / width,
                    1.0 - (p.v as f32 - top) / height,
                );
                changed = true;
            }
        }
        ae::Event::AdjustCursor(_) => event.set_cursor(ae::CursorType::Crosshairs),
        _ => return Ok(()),
    }
    if changed {
        params.get_mut(id)?.as_arbitrary_mut()?.set_value(curve)?;
        ae::suites::App::new()?
            .invalidate_rect(event.context_handle(), Some(event.current_frame()))?;
    }
    event.set_event_out_flags(
        ae::EventOutFlags::HANDLED_EVENT
            | if changed {
                ae::EventOutFlags::ALWAYS_UPDATE | ae::EventOutFlags::UPDATE_NOW
            } else {
                ae::EventOutFlags::NONE
            },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smooth_curve_stays_bounded_and_passes_through_knots() {
        let mut c = Curve::default();
        c.insert(0.25, 0.8);
        c.insert(0.7, 0.2);
        for p in &c.points[..c.count] {
            assert!((c.evaluate(p.x) - p.y).abs() < 1e-5);
        }
        for i in 0..1000 {
            assert!((0.0..=1.0).contains(&c.evaluate(i as f32 / 999.0)));
        }
    }
    #[test]
    fn default_is_linear_and_flat_is_neutral() {
        for i in 0..101 {
            let x = i as f32 / 100.0;
            assert!((Curve::default().evaluate(x) - x).abs() < 1e-5);
            assert!((Curve::flat(0.5).evaluate(x) - 0.5).abs() < 1e-5);
        }
    }
}
