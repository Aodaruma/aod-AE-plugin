use after_effects as ae;
use std::fmt::Write;

const RESULT_OK_PREFIX: &str = "__AOD_OK__";
const RESULT_CANCEL: &str = "__AOD_CANCEL__";

pub(crate) fn edit_liquid_template(
    initial: &str,
    plugin_id: ae::aegp::PluginId,
) -> Result<Option<String>, String> {
    let utility = ae::aegp::suites::Utility::new()
        .map_err(|e| format!("Failed to acquire AEGP Utility suite: {e:?}"))?;

    let scripting_available = utility
        .is_scripting_available()
        .map_err(|e| format!("Failed to check scripting availability: {e:?}"))?;
    if !scripting_available {
        return Err("After Effects scripting is not available in this host session.".to_string());
    }

    let script = build_script(initial);
    let (result, error_text) = utility
        .execute_script(plugin_id, &script, false)
        .map_err(|e| format!("AEGP_ExecuteScript failed: {e:?}"))?;

    if !error_text.trim().is_empty() {
        return Err(format!("Script execution error: {}", error_text.trim()));
    }

    parse_script_result(&result)
}

fn build_script(initial: &str) -> String {
    let initial_lit = escape_js_single_quoted(initial);
    format!(
        r#"(function () {{
var __aodInitial = '{initial_lit}';
var normalizeNewlines = function (s) {{
  return String(s || '').replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}};

var dlg = new Window('dialog', 'AOD DatacodeEncode - Liquid Template');
dlg.orientation = 'column';
dlg.alignChildren = ['fill', 'fill'];
dlg.spacing = 10;
dlg.margins = 12;

var editor = dlg.add('edittext', undefined, normalizeNewlines(__aodInitial), {{
  multiline: true,
  scrolling: true,
  scrollable: true,
  wantReturn: true
}});
editor.preferredSize = [960, 620];

var applyMonoFont = function () {{
  var names = [
    'Consolas',
    'Cascadia Mono',
    'Cascadia Code',
    'Courier New',
    'Lucida Console',
    'Menlo',
    'Monaco'
  ];
  var styles = ['REGULAR', 'Regular'];
  for (var i = 0; i < names.length; i++) {{
    for (var j = 0; j < styles.length; j++) {{
      try {{
        var f = ScriptUI.newFont(names[i], styles[j], 12);
        if (f) {{
          dlg.graphics.font = f;
          editor.graphics.font = f;
          return;
        }}
      }} catch (e) {{}}
    }}
  }}
}};
applyMonoFont();

var buttons = dlg.add('group');
buttons.orientation = 'row';
buttons.alignment = ['right', 'center'];
buttons.spacing = 8;
buttons.add('button', undefined, 'OK', {{ name: 'ok' }});
buttons.add('button', undefined, 'Cancel', {{ name: 'cancel' }});

editor.active = true;
var ret = dlg.show();
if (ret === 1) {{
  return '{RESULT_OK_PREFIX}' + normalizeNewlines(editor.text);
}}
return '{RESULT_CANCEL}';
}})();"#
    )
}

fn parse_script_result(raw_result: &str) -> Result<Option<String>, String> {
    let result = raw_result.trim_end_matches('\0');
    if result == RESULT_CANCEL {
        return Ok(None);
    }
    if let Some(text) = result.strip_prefix(RESULT_OK_PREFIX) {
        return Ok(Some(normalize_newlines(text)));
    }
    Err(format!(
        "Unexpected script result: {}",
        preview_for_error(result)
    ))
}

fn normalize_newlines(src: &str) -> String {
    src.replace("\r\n", "\n").replace('\r', "\n")
}

fn escape_js_single_quoted(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    for ch in src.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if c <= '\u{1F}' => {
                let _ = write!(&mut out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn preview_for_error(src: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut chars = src.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}
