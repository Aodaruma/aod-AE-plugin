use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn log(message: impl AsRef<str>) {
    if let Ok(path) = log_file_path() {
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "[{}] {}", timestamp_secs(), message.as_ref());
        }
    }
}

pub(crate) fn session_start() {
    log("----- session start -----");
}

fn log_file_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA")
            && !local_app_data.is_empty()
        {
            return Ok(PathBuf::from(local_app_data)
                .join("Aodaruma")
                .join("DatacodeEncode")
                .join("debug.log"));
        }
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home)
            .join(".config")
            .join("aodaruma")
            .join("datacode-encode")
            .join("debug.log"));
    }
    std::env::current_dir()
        .map(|p| {
            p.join(".aodaruma")
                .join("datacode-encode")
                .join("debug.log")
        })
        .map_err(|e| format!("FAILED TO RESOLVE LOG FILE PATH: {e}"))
}

fn timestamp_secs() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_secs(),
        Err(_) => 0,
    }
}
