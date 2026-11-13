// SPDX-License-Identifier: MPL-2.0
use crate::{codec::Controls, render_error::RenderError};

pub struct Replay {
    pub times: Vec<i32>,
    pub controls: Vec<Controls>,
    pub target: usize,
}

// Restart at the last retained GOP boundary, so request order cannot alter the mosh.
pub fn plan(
    now: i32,
    step: i32,
    gop: i32,
    independent: bool,
    use_b: bool,
    mut at: impl FnMut(i32) -> Result<(bool, bool), RenderError>,
) -> Result<Replay, RenderError> {
    let base = crate::core::times(now, step, gop, independent)?;
    if independent {
        return Ok(Replay {
            times: base,
            controls: vec![Controls {
                force_i: true,
                ..Default::default()
            }],
            target: 0,
        });
    }
    let step = i64::from(
        step.checked_abs()
            .filter(|s| *s > 0)
            .ok_or("Invalid frame duration")?,
    );
    let frame = i64::from(now).div_euclid(step);
    let phase = i64::from(now).rem_euclid(step);
    let gop = i64::from(gop);
    let boundary = frame.div_euclid(gop) * gop;
    let time = |n: i64| {
        i32::try_from(n * step + phase).map_err(|_| RenderError::from("Frame time overflow"))
    };
    let mut first = boundary;
    // Negative source time has no nonnegative seed. Retain that GOP's first frame.
    while first > 0 && at(time(first)?)?.0 {
        first -= gop;
        if frame - first >= 6000 {
            return Err("Datamosh history exceeds 6000 frames. Keep an I-frame by turning Drop I-Frames off at a GOP boundary.".into());
        }
    }
    let last = if use_b {
        (frame + 2).min(boundary + gop - 1)
    } else {
        frame
    };
    let mut times = Vec::new();
    let mut controls = Vec::new();
    for n in first..=last {
        let t = time(n)?;
        let (drop_i, drop_b) = at(t)?;
        times.push(t);
        controls.push(Controls {
            force_i: n.rem_euclid(gop) == 0,
            drop_i: n != first && drop_i,
            drop_b: use_b && drop_b,
        });
    }
    Ok(Replay {
        times,
        controls,
        target: (frame - first) as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_follows_animated_drops_and_explicit_future_opt_in() {
        let flags = |t| Ok((t >= 12, t % 2 == 1));
        let past = plan(17, 1, 6, false, false, flags).unwrap();
        assert_eq!(past.times, (6..=17).collect::<Vec<_>>());
        assert!(past.controls[6].force_i && past.controls[6].drop_i);
        assert!(!past.controls.iter().any(|c| c.drop_b));
        let future = plan(13, 1, 6, false, true, flags).unwrap();
        assert_eq!(future.times.last(), Some(&15));
        assert_eq!(future.times[future.target], 13);
        assert!(future.controls[future.target].drop_b);
        let reset = plan(24, 1, 6, false, false, |t| Ok((t < 24, false))).unwrap();
        assert_eq!(reset.times, [24]);
        let gop_one = plan(4, 1, 1, false, false, |_| Ok((true, false))).unwrap();
        assert_eq!(gop_one.times, [0, 1, 2, 3, 4]);
        assert!(!gop_one.controls[0].drop_i);
        assert!(gop_one.controls[1..].iter().all(|c| c.force_i && c.drop_i));
        let subframe = plan(131, 10, 6, false, true, |t| Ok((t >= 121, false))).unwrap();
        assert_eq!(
            subframe.times,
            (6..=15).map(|n| n * 10 + 1).collect::<Vec<_>>()
        );
        assert!(plan(6000, 1, 1, false, false, |_| Ok((true, false))).is_err());
        assert_eq!(
            plan(-1, 10, 6, false, false, |_| Ok((true, false)))
                .unwrap()
                .times,
            [-51, -41, -31, -21, -11, -1]
        );
    }
}
