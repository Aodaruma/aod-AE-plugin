use super::*;
use crate::codec::{bytes_to_hex_upper, parse_hex_binary};
use liquid::{ParserBuilder, object};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

const TEMPLATE_FILE_NAME: &str = "payload.liquid";
const DEFAULT_TEMPLATE: &str = concat!(
    "{% comment %} DatacodeEncode Liquid Template {% endcomment %}\n",
    "{% if file_utf8 != blank %}\n",
    "    {{ file_utf8 }}\n",
    "{% elsif file_hex != blank %}\n",
    "    {{ file_hex }}\n",
    "{% elsif file_bin_hex != blank %}\n",
    "    {{ file_bin_hex }}\n",
    "{% elsif input_mode == 'hex' %}\n",
    "    48454C4C4F20574F524C4421\n",
    "{% else %}\n",
    "HELLO WORLD!\n",
    "current_time={{ current_time }}\n",
    "time_seconds={{ time_seconds }}\n",
    "frame={{ frame }}\n",
    "origin=({{ origin_x }}, {{ origin_y }})\n",
    "origin_direction={{ origin_direction }}\n",
    "cell={{ cell_width }}x{{ cell_height }} modules @ {{ cell_pixel_size }}px\n",
    "{% endif %}\n"
);

#[derive(Clone, Debug)]
pub(crate) struct TemplateTransformContext {
    pub origin_x: i32,
    pub origin_y: i32,
    pub origin_direction: &'static str,
    pub cell_pixel_size: usize,
    pub cell_width: usize,
    pub cell_height: usize,
}

pub(crate) fn default_template() -> &'static str {
    DEFAULT_TEMPLATE
}

pub(crate) fn render_payload_from_template(
    input_mode: InputMode,
    template_src: &str,
    current_time: i32,
    time_step: i32,
    time_scale: u32,
    transform: &TemplateTransformContext,
) -> Result<(Vec<u8>, String), String> {
    let template_dir = template_file_path()
        .map_err(|e| format!("FAILED TO RESOLVE TEMPLATE PATH: {e}"))?
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let template_path_display = template_dir.join(TEMPLATE_FILE_NAME).display().to_string();
    let source_utf8 = read_optional_utf8(template_dir.join("payload.txt"))?;
    let source_hex = read_optional_utf8(template_dir.join("payload.hex"))?;
    let source_bin_hex = read_optional_binary_hex(template_dir.join("payload.bin"))?;
    let payload_utf8 = source_utf8.clone();
    let payload_hex = if !source_hex.is_empty() {
        source_hex.clone()
    } else {
        source_bin_hex.clone()
    };

    let parser = ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| format!("LIQUID PARSER BUILD FAILED: {e}"))?;
    let template = parser
        .parse(template_src)
        .map_err(|e| format!("LIQUID TEMPLATE PARSE ERROR: {e}"))?;

    let seconds = if time_scale == 0 {
        0.0
    } else {
        current_time as f64 / time_scale as f64
    };
    let frame = if time_step == 0 {
        0.0
    } else {
        current_time as f64 / time_step as f64
    };
    let input_mode_name = match input_mode {
        InputMode::Utf8 => "utf8",
        InputMode::Hex => "hex",
    };

    let globals = object!({
        "input_mode": input_mode_name,
        "current_time": current_time,
        "time_step": time_step,
        "time_scale": time_scale as i64,
        "time_seconds": seconds,
        "frame": frame,
        "frame_index": current_time as i64,
        "origin_x": transform.origin_x,
        "origin_y": transform.origin_y,
        "origin_direction": transform.origin_direction,
        "cell_pixel_size": transform.cell_pixel_size as i64,
        "cell_width": transform.cell_width as i64,
        "cell_height": transform.cell_height as i64,
        "newline": "\n",
        "tab": "\t",
        "template_path": template_path_display,
        "template_dir": template_dir.display().to_string(),
        "source_utf8": source_utf8.clone(),
        "source_hex": source_hex.clone(),
        "source_bin_hex": source_bin_hex.clone(),
        "file_utf8": source_utf8,
        "file_hex": source_hex,
        "file_bin_hex": source_bin_hex,
        "payload_utf8": payload_utf8,
        "payload_hex": payload_hex,
    });

    let rendered = template
        .render(&globals)
        .map_err(|e| format!("LIQUID TEMPLATE RENDER ERROR: {e}"))?;

    let payload_bytes = match input_mode {
        InputMode::Utf8 => rendered.as_bytes().to_vec(),
        InputMode::Hex => parse_hex_binary(&rendered)?,
    };
    if payload_bytes.is_empty() {
        return Err("PAYLOAD IS EMPTY".into());
    }
    let payload_text = if matches!(input_mode, InputMode::Utf8) {
        rendered
    } else {
        bytes_to_hex_upper(&payload_bytes)
    };

    Ok((payload_bytes, payload_text))
}

pub(crate) fn template_file_path() -> Result<PathBuf, String> {
    let base = template_base_dir()?;
    Ok(base.join(TEMPLATE_FILE_NAME))
}

fn template_base_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA")
            && !local_app_data.is_empty()
        {
            return Ok(PathBuf::from(local_app_data)
                .join("Aodaruma")
                .join("DatacodeEncode"));
        }
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home)
            .join(".config")
            .join("aodaruma")
            .join("datacode-encode"));
    }
    std::env::current_dir()
        .map(|p| p.join(".aodaruma").join("datacode-encode"))
        .map_err(|e| format!("FAILED TO RESOLVE TEMPLATE BASE DIR: {e}"))
}

fn read_optional_utf8(path: PathBuf) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path)
        .map(|s| s.trim_end_matches(&['\r', '\n'][..]).to_string())
        .map_err(|e| format!("FAILED TO READ {}: {e}", path.display()))
}

fn read_optional_binary_hex(path: PathBuf) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read(&path)
        .map(|bytes| bytes_to_hex_upper(&bytes))
        .map_err(|e| format!("FAILED TO READ {}: {e}", path.display()))
}
