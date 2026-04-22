use super::*;

pub(crate) const PAYLOAD_UI_WIDTH: u16 = 320;
pub(crate) const PAYLOAD_UI_HEIGHT: u16 = 88;

const KEYCODE_FLAG_PRINTABLE: u32 = ae::sys::PF_KEYCODE_FLAG_Printable as u32;
const KEYCODE_VALUE_MASK: u32 = 0xFFFF;
const MAX_UTF8_CHARS: usize = 1024;
const MAX_HEX_CHARS: usize = 2048;

pub(crate) fn handle_payload_event(
    _in_data: &InData,
    params: &mut Parameters<Params>,
    extra: &mut EventExtra,
) -> Result<bool, Error> {
    if extra.window_type() != WindowType::Effect {
        return Ok(false);
    }
    if extra.effect_area() != EffectArea::Control {
        return Ok(false);
    }
    if params.index(Params::Payload) != Some(extra.param_index()) {
        return Ok(false);
    }

    match extra.event() {
        Event::Draw(_) => {
            draw_payload_editor(params, extra)?;
            extra.set_event_out_flags(EventOutFlags::HANDLED_EVENT);
            Ok(true)
        }
        Event::Click(_) => {
            extra.set_event_out_flags(EventOutFlags::HANDLED_EVENT | EventOutFlags::UPDATE_NOW);
            Ok(true)
        }
        Event::Keydown(_) => {
            let changed = apply_key_input(params, extra)?;
            if changed {
                extra.set_event_out_flags(EventOutFlags::HANDLED_EVENT | EventOutFlags::UPDATE_NOW);
            } else {
                extra.set_event_out_flags(EventOutFlags::HANDLED_EVENT);
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn draw_payload_editor(params: &Parameters<Params>, extra: &mut EventExtra) -> Result<(), Error> {
    let input_mode = read_input_mode(params)?;
    let payload = read_payload(params)?;
    let value_src = if matches!(input_mode, InputMode::Utf8) {
        payload.utf8.as_str()
    } else {
        payload.hex.as_str()
    };

    let drawbot = extra.context_handle().drawing_reference()?;
    let supplier = drawbot.supplier()?;
    let surface = drawbot.surface()?;
    let frame = extra.current_frame();

    let width = (frame.right - frame.left).max(1) as f32;
    let height = (frame.bottom - frame.top).max(1) as f32;
    let rect = ae::drawbot::RectF32 {
        left: frame.left as f32 + 0.5,
        top: frame.top as f32 + 0.5,
        width,
        height,
    };

    let mut path = supplier.new_path()?;
    path.add_rect(&rect)?;

    let bg_brush = supplier.new_brush(&ae::drawbot::ColorRgba {
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
    })?;
    surface.fill_path(&bg_brush, &path, ae::drawbot::FillType::Winding)?;

    let border_pen = supplier.new_pen(
        &ae::drawbot::ColorRgba {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        },
        1.0,
    )?;
    surface.stroke_path(&border_pen, &path)?;

    if !supplier.supports_text()? {
        return Ok(());
    }

    let default_font_size = supplier.default_font_size()?.max(9.0);
    let title_font = supplier.new_default_font(default_font_size)?;
    let body_font = supplier.new_default_font((default_font_size - 1.0).max(8.0))?;

    let text_brush = supplier.new_brush(&ae::drawbot::ColorRgba {
        red: 0.05,
        green: 0.05,
        blue: 0.05,
        alpha: 1.0,
    })?;
    let hint_brush = supplier.new_brush(&ae::drawbot::ColorRgba {
        red: 0.25,
        green: 0.25,
        blue: 0.25,
        alpha: 1.0,
    })?;

    let mode_label = if matches!(input_mode, InputMode::Utf8) {
        "UTF-8"
    } else {
        "HEX"
    };
    let title = format!("Payload Editor [{}]", mode_label);
    let value_line = if value_src.is_empty() {
        "<Type payload here.>".to_string()
    } else {
        truncate_for_display(value_src, 96)
    };
    let hint_line = if matches!(input_mode, InputMode::Utf8) {
        "Printable keys are accepted."
    } else {
        "HEX mode: 0-9 A-F and separators(space,_,-)."
    };

    let left = frame.left as f32 + 6.0;
    surface.draw_string(
        &text_brush,
        &title_font,
        &title,
        &ae::drawbot::PointF32 {
            x: left,
            y: frame.top as f32 + 16.0,
        },
        ae::drawbot::TextAlignment::Left,
        ae::drawbot::TextTruncation::EndEllipsis,
        (width - 12.0).max(32.0),
    )?;
    surface.draw_string(
        &text_brush,
        &body_font,
        &value_line,
        &ae::drawbot::PointF32 {
            x: left,
            y: frame.top as f32 + 38.0,
        },
        ae::drawbot::TextAlignment::Left,
        ae::drawbot::TextTruncation::EndEllipsis,
        (width - 12.0).max(32.0),
    )?;
    surface.draw_string(
        &hint_brush,
        &body_font,
        &hint_line,
        &ae::drawbot::PointF32 {
            x: left,
            y: frame.top as f32 + 58.0,
        },
        ae::drawbot::TextAlignment::Left,
        ae::drawbot::TextTruncation::EndEllipsis,
        (width - 12.0).max(32.0),
    )?;

    Ok(())
}

fn apply_key_input(params: &mut Parameters<Params>, extra: &EventExtra) -> Result<bool, Error> {
    let input_mode = read_input_mode(params)?;
    let mut payload = read_payload(params)?;
    let keycode = unsafe { extra.as_ref().u.key_down.keycode as u32 };

    let changed = if matches!(input_mode, InputMode::Utf8) {
        apply_utf8_key(keycode, &mut payload.utf8)
    } else {
        apply_hex_key(keycode, &mut payload.hex)
    };

    if changed {
        write_payload(params, payload)?;
    }
    Ok(changed)
}

fn apply_utf8_key(keycode: u32, value: &mut String) -> bool {
    match parse_key_input(keycode) {
        KeyInput::Printable(ch) => {
            if ch.is_control() || value.chars().count() >= MAX_UTF8_CHARS {
                return false;
            }
            value.push(ch);
            true
        }
        KeyInput::Backspace | KeyInput::Delete => value.pop().is_some(),
        KeyInput::Ignore => false,
    }
}

fn apply_hex_key(keycode: u32, value: &mut String) -> bool {
    match parse_key_input(keycode) {
        KeyInput::Printable(ch) => {
            if value.chars().count() >= MAX_HEX_CHARS {
                return false;
            }
            if ch.is_ascii_hexdigit() {
                value.push(ch.to_ascii_uppercase());
                return true;
            }
            if ch.is_ascii_whitespace() || ch == '_' || ch == '-' {
                value.push(ch);
                return true;
            }
            false
        }
        KeyInput::Backspace | KeyInput::Delete => value.pop().is_some(),
        KeyInput::Ignore => false,
    }
}

fn read_input_mode(params: &Parameters<Params>) -> Result<InputMode, Error> {
    Ok(if params.get(Params::InputMode)?.as_popup()?.value() == 2 {
        InputMode::Hex
    } else {
        InputMode::Utf8
    })
}

fn read_payload(params: &Parameters<Params>) -> Result<DataPayload, Error> {
    let payload = params
        .get(Params::Payload)?
        .as_arbitrary()?
        .value::<DataPayload>()?;
    Ok((*payload).clone())
}

fn write_payload(params: &mut Parameters<Params>, payload: DataPayload) -> Result<(), Error> {
    params
        .get_mut(Params::Payload)?
        .as_arbitrary_mut()?
        .set_value(payload)?;
    Ok(())
}

fn truncate_for_display(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let keep_head = max_chars / 2;
    let keep_tail = max_chars - keep_head - 3;
    let head: String = text.chars().take(keep_head).collect();
    let tail: String = text
        .chars()
        .skip(char_count.saturating_sub(keep_tail))
        .collect();
    format!("{head}...{tail}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyInput {
    Printable(char),
    Backspace,
    Delete,
    Ignore,
}

fn parse_key_input(keycode: u32) -> KeyInput {
    if keycode & KEYCODE_FLAG_PRINTABLE != 0 {
        let value = keycode & KEYCODE_VALUE_MASK;
        if let Some(ch) = char::from_u32(value) {
            return KeyInput::Printable(ch);
        }
        return KeyInput::Ignore;
    }

    let control = (keycode & KEYCODE_VALUE_MASK) as u16;
    if control == ae::sys::PF_ControlCode_Backspace as u16 {
        return KeyInput::Backspace;
    }
    if control == ae::sys::PF_ControlCode_Delete as u16 {
        return KeyInput::Delete;
    }
    if control == ae::sys::PF_ControlCode_Space as u16 {
        return KeyInput::Printable(' ');
    }

    KeyInput::Ignore
}
