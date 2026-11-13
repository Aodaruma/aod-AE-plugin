// SPDX-License-Identifier: MPL-2.0
use crate::{
    codec::{Controls, Frame, Session, Settings},
    render_error::RenderError,
};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

struct Reusable {
    settings: Settings,
    prefix: [u8; 16],
    count: usize,
    session: Session,
}
static POOL: Mutex<Vec<Reusable>> = Mutex::new(Vec::new());
static EPOCH: AtomicU64 = AtomicU64::new(0);
pub fn clear() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
    POOL.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

pub fn render(
    settings: Settings,
    prefixes: &[[u8; 16]],
    controls: &[Controls],
    target: usize,
    mut frame: impl FnMut(usize) -> Result<Frame, RenderError>,
    mut abort: impl FnMut() -> Result<(), RenderError>,
) -> Result<(Vec<u8>, usize), RenderError> {
    let profile = std::env::var_os("CODECMAP_PROFILE").map(|_| std::time::Instant::now());
    abort()?;
    if prefixes.is_empty()
        || prefixes.len() > 6003
        || controls.len() != prefixes.len()
        || target >= prefixes.len()
    {
        return Err("Invalid stream plan".into());
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    let reusable = !settings.use_b && target + 1 == prefixes.len();
    let cached = if reusable {
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        pool.iter()
            .rposition(|p| {
                p.settings == settings
                    && p.count <= prefixes.len()
                    && prefixes[p.count - 1] == p.prefix
            })
            .map(|i| pool.remove(i))
    } else {
        None
    };
    let mut stream = match cached {
        Some(s) => s,
        None => Reusable {
            settings,
            prefix: [0; 16],
            count: 0,
            session: Session::open(
                settings,
                if reusable { 6003 } else { prefixes.len() },
                if reusable { 6002 } else { target },
            )?,
        },
    };
    let mut pushed = 0;
    for (i, control) in controls.iter().enumerate().skip(stream.count) {
        abort()?;
        let mut input = frame(i)?;
        input.controls = *control;
        stream.session.push(&input)?;
        pushed += 1;
    }
    abort()?;
    let result = if reusable {
        stream.session.snapshot()?
    } else {
        stream.session.finish()?.0
    };
    abort()?;
    if let Some(start) = profile {
        eprintln!(
            "CODECMAP_STREAM,{},{},{},{},{},{}",
            settings.codec as i32,
            settings.width,
            settings.height,
            prefixes.len(),
            pushed,
            start.elapsed().as_micros()
        );
    }
    if reusable && epoch == EPOCH.load(Ordering::SeqCst) {
        stream.count = prefixes.len();
        stream.prefix = *prefixes.last().unwrap();
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() == 2 {
            pool.remove(0);
        }
        pool.push(stream);
    }
    Ok((result, pushed))
}
