// Opt-in instrumentation for local AE benchmarks. Normal builds contain no logger.
use std::{fs::OpenOptions, io::Write, sync::Mutex, time::Duration};

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn record(
    width: usize,
    height: usize,
    depth: after_effects::aegp::WorldType,
    read: Duration,
    mask: Duration,
    total: Duration,
) {
    let Some(path) = std::env::var_os("AOD_EYEDROPPER_PROFILE_PATH") else {
        return;
    };
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "{},{width},{height},{depth:?},{:.6},{:.6},{:.6},{:.6}",
            env!("CARGO_PKG_VERSION"),
            read.as_secs_f64() * 1000.0,
            (mask - read).as_secs_f64() * 1000.0,
            (total - mask).as_secs_f64() * 1000.0,
            total.as_secs_f64() * 1000.0,
        );
    }
}
