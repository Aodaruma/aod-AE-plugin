// SPDX-License-Identifier: MPL-2.0
use crate::render_error::RenderError;
use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::NonNull;

unsafe extern "C" {
    fn cm_create(
        directory: *const c_char,
        width: i32,
        height: i32,
        scale: i32,
        step: i32,
        kind: i32,
        quality: f64,
        use_b: i32,
        count: i32,
        target: i32,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn cm_destroy(session: *mut c_void);
    fn cm_push(
        session: *mut c_void,
        input: *const u8,
        input_len: usize,
        offsets: *const f32,
        count: usize,
        force_i: i32,
        drop_flags: i32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn cm_finish(
        session: *mut c_void,
        output: *mut u8,
        output_len: usize,
        stats: *mut i32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264 = 1,
    Vp9 = 2,
    Mpeg4 = 3,
}
impl Codec {
    pub fn from_choice(value: i32) -> Result<Self, String> {
        match value {
            1 => Ok(Self::H264),
            2 => Ok(Self::Vp9),
            3 => Ok(Self::Mpeg4),
            _ => Err("Unknown codec".into()),
        }
    }
    pub fn supports_b(self) -> bool {
        self != Self::Vp9
    }
    pub fn roi_offset(self, value: f32) -> f32 {
        match self {
            Self::H264 => value,
            Self::Vp9 => (value / 8.0).round() * 8.0,
            Self::Mpeg4 => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub force_i: bool,
    pub drop_i: bool,
    pub drop_b: bool,
}
pub struct Frame {
    pub yuv: Vec<u8>,
    pub offsets: Vec<f32>,
    pub controls: Controls,
}
pub struct Job {
    pub width: usize,
    pub height: usize,
    pub time_scale: i32,
    pub time_step: i32,
    pub codec: Codec,
    pub quality: f64,
    pub use_b: bool,
    pub target: usize,
    pub frames: Vec<Frame>,
}

impl Job {
    pub fn key(&self, salt: &[u8]) -> [u8; 16] {
        let mut h = blake3::Hasher::new();
        h.update(b"com.aodaruma.codec-map.v2.ffmpeg8.1.yuv709limited");
        h.update(salt);
        for v in [
            self.width as u64,
            self.height as u64,
            self.time_scale as u64,
            self.time_step as u64,
            self.codec as u64,
            self.quality.to_bits(),
            self.use_b as u64,
            self.target as u64,
            self.frames.len() as u64,
        ] {
            h.update(&v.to_le_bytes());
        }
        for frame in &self.frames {
            h.update(&[
                frame.controls.force_i as u8,
                frame.controls.drop_i as u8,
                frame.controls.drop_b as u8,
            ]);
            h.update(&frame.yuv);
            for value in &frame.offsets {
                h.update(&value.to_bits().to_le_bytes());
            }
        }
        h.finalize().as_bytes()[..16].try_into().unwrap()
    }

    pub fn render(
        &self,
        mut abort: impl FnMut() -> Result<(), RenderError>,
    ) -> Result<Vec<u8>, RenderError> {
        abort()?;
        let mut session = Session::new(self)?;
        for frame in &self.frames {
            abort()?;
            session.push(frame)?;
        }
        abort()?;
        Ok(session.finish()?.0)
    }
}

struct Session {
    raw: NonNull<c_void>,
    size: usize,
    blocks: usize,
}
impl Session {
    fn new(job: &Job) -> Result<Self, String> {
        if job.width < 2
            || job.height < 2
            || job.width > 16384
            || job.height > 16384
            || !job.width.is_multiple_of(2)
            || !job.height.is_multiple_of(2)
        {
            return Err("Invalid padded dimensions".into());
        }
        if job.frames.is_empty() || job.frames.len() > 6003 || job.target >= job.frames.len() {
            return Err("Invalid codec replay range".into());
        }
        let directory = std::env::var("CODECMAP_CODEC_DIR").unwrap_or_default();
        if !directory.is_empty() && !std::path::Path::new(&directory).is_absolute() {
            return Err("CODECMAP_CODEC_DIR must be an absolute path".into());
        }
        let directory = CString::new(directory).map_err(|_| "Invalid runtime path")?;
        let mut error = [0 as c_char; 1024];
        let raw = unsafe {
            cm_create(
                directory.as_ptr(),
                job.width as i32,
                job.height as i32,
                job.time_scale,
                job.time_step,
                job.codec as i32,
                job.quality,
                i32::from(job.use_b),
                job.frames.len() as i32,
                job.target as i32,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        Ok(Self {
            raw: NonNull::new(raw).ok_or_else(|| error_text(&error))?,
            size: job.width * job.height * 3 / 2,
            blocks: job.width.div_ceil(16) * job.height.div_ceil(16),
        })
    }
    fn push(&mut self, frame: &Frame) -> Result<(), String> {
        if frame.yuv.len() != self.size || frame.offsets.len() != self.blocks {
            return Err("Invalid codec buffer length".into());
        }
        let mut error = [0 as c_char; 1024];
        let ok = unsafe {
            cm_push(
                self.raw.as_ptr(),
                frame.yuv.as_ptr(),
                frame.yuv.len(),
                frame.offsets.as_ptr(),
                frame.offsets.len(),
                i32::from(frame.controls.force_i),
                i32::from(frame.controls.drop_i) | (i32::from(frame.controls.drop_b) << 1),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if ok == 0 {
            return Err(error_text(&error));
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<(Vec<u8>, [i32; 6]), String> {
        let mut output = vec![0; self.size];
        let mut stats = [0; 6];
        let mut error = [0 as c_char; 1024];
        let ok = unsafe {
            cm_finish(
                self.raw.as_ptr(),
                output.as_mut_ptr(),
                output.len(),
                stats.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if ok == 0 {
            return Err(error_text(&error));
        }
        Ok((output, stats))
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe { cm_destroy(self.raw.as_ptr()) };
    }
}
fn error_text(error: &[c_char]) -> String {
    unsafe { CStr::from_ptr(error.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: usize, invert: bool) -> Job {
        let (w, h) = (64, 48);
        let frames = (0..n)
            .map(|t| {
                let pixels = (0..w * h)
                    .map(|i| {
                        let x = i % w % 32;
                        let y = i / w;
                        let v = ((x * 17 + y * 31 + t * 7) % 251) as f32 / 250.0;
                        [v, v, v, 1.0]
                    })
                    .collect::<Vec<_>>();
                Frame {
                    controls: Controls::default(),
                    yuv: crate::core::to_yuv(&pixels, w, h).unwrap(),
                    offsets: (0..12)
                        .map(|i| if (i % 4 < 2) ^ invert { -12.0 } else { 18.0 })
                        .collect(),
                }
            })
            .collect();
        Job {
            width: w,
            height: h,
            time_scale: 24,
            time_step: 1,
            codec: Codec::H264,
            quality: 30.0,
            use_b: false,
            target: n - 1,
            frames,
        }
    }
    #[test]
    fn cache_identity_includes_past_map_and_source() {
        let mut job = fixture(3, false);
        let key = job.key(&[]);
        job.frames[0].offsets[0] += 1.0;
        assert_ne!(key, job.key(&[]));
        let key = job.key(&[]);
        job.frames[0].yuv[0] ^= 1;
        assert_ne!(key, job.key(&[]));
        assert_ne!(job.key(&[]), job.key(&[1]));
        let key = job.key(&[]);
        job.frames[0].controls.drop_i = true;
        assert_ne!(key, job.key(&[]));
        let key = job.key(&[]);
        job.target = 1;
        assert_ne!(key, job.key(&[]));
    }
    fn run_stats(job: &Job) -> (Vec<u8>, [i32; 6]) {
        let mut session = Session::new(job).unwrap();
        for frame in &job.frames {
            session.push(frame).unwrap();
        }
        session.finish().unwrap()
    }
    fn for_codec(kind: Codec, n: usize) -> Job {
        let mut job = fixture(n, false);
        job.codec = kind;
        job.quality = if kind == Codec::Mpeg4 { 8.0 } else { 35.0 };
        job
    }
    #[test]
    #[ignore = "requires the bundled FFmpeg runtime"]
    fn real_codecs_drop_encoded_pictures_and_hold_presentation_time() {
        for kind in [Codec::H264, Codec::Vp9, Codec::Mpeg4] {
            let mut job = for_codec(kind, 10);
            let (_, stats) = run_stats(&job);
            assert_eq!(stats, [1, 9, 0, 0, 0, 10], "{kind:?}");
            // All-I + drop must retain the initialization frame and hold it.
            for f in &mut job.frames {
                f.controls.force_i = true;
                f.controls.drop_i = true;
            }
            let (dropped, stats) = run_stats(&job);
            assert_eq!(stats, [10, 0, 0, 9, 0, 1], "{kind:?}");
            let seed = for_codec(kind, 1).render(|| Ok(())).unwrap();
            assert_eq!(seed, dropped, "{kind:?}");
            job.frames[6].controls.drop_i = false;
            let (reset, stats) = run_stats(&job);
            assert_eq!(stats[3], 8);
            assert_ne!(reset, seed, "{kind:?}");
            // Dropped GOP I leaves actual predicted packets in the damaged stream.
            let mut job = for_codec(kind, 10);
            job.frames[6].controls.force_i = true;
            let normal = job.render(|| Ok(())).unwrap();
            job.frames[6].controls.drop_i = true;
            let (moshed, stats) = run_stats(&job);
            assert_eq!(stats[3], 1, "{kind:?}");
            assert_ne!(moshed, normal, "{kind:?}");
            assert_eq!(moshed, job.render(|| Ok(())).unwrap(), "{kind:?}");
            if kind.supports_b() {
                let mut job = for_codec(kind, 7);
                job.use_b = true;
                job.target = 2;
                let (normal, stats) = run_stats(&job);
                assert!(stats[2] > 0, "{kind:?} {stats:?}");
                for f in &mut job.frames {
                    f.controls.drop_b = true;
                }
                let (dropped, stats) = run_stats(&job);
                assert_eq!(stats[2], stats[4], "{kind:?} {stats:?}");
                assert_ne!(normal, dropped, "{kind:?}");
                job.target = 0;
                assert_eq!(dropped, job.render(|| Ok(())).unwrap(), "{kind:?}");
            }
        }
    }
    #[test]
    #[ignore = "requires the bundled FFmpeg runtime"]
    fn real_vp9_roi_and_codec_quality_are_effective() {
        let mut job = for_codec(Codec::Vp9, 5);
        let normal = job.render(|| Ok(())).unwrap();
        for f in &mut job.frames {
            for q in &mut f.offsets {
                *q = -*q;
            }
        }
        assert_ne!(normal, job.render(|| Ok(())).unwrap());
        for kind in [Codec::H264, Codec::Vp9, Codec::Mpeg4] {
            let mut job = for_codec(kind, 3);
            let normal = job.render(|| Ok(())).unwrap();
            job.quality = 2.0;
            assert_ne!(normal, job.render(|| Ok(())).unwrap(), "{kind:?}");
        }
    }
    #[test]
    #[ignore = "requires the bundled FFmpeg runtime"]
    fn b_replay_prefix_matches_full_stream_at_gop_edges() {
        for kind in [Codec::H264, Codec::Mpeg4] {
            for drop_i in [false, true] {
                for target in 0..12 {
                    let setup = |count| {
                        let mut job = for_codec(kind, count);
                        job.use_b = true;
                        job.target = target;
                        for (i, f) in job.frames.iter_mut().enumerate() {
                            f.controls.force_i = i % 6 == 0;
                            f.controls.drop_i = drop_i && i >= 6;
                            f.controls.drop_b = i % 2 == 1;
                        }
                        job
                    };
                    let end = (target + 2).min((target / 6 + 1) * 6 - 1);
                    let full = setup(12).render(|| Ok(())).unwrap();
                    let prefix = setup(end + 1).render(|| Ok(())).unwrap();
                    assert_eq!(full, prefix, "{kind:?} drop_i={drop_i} target={target}");
                }
            }
        }
    }
    #[test]
    #[ignore = "requires the pinned FFmpeg runtime; run with --ignored"]
    fn real_h264_roi_replay_and_cancellation() {
        for count in [1, 3] {
            let job = fixture(count, false);
            let output = job.render(|| Ok(())).unwrap();
            assert_eq!(output, job.render(|| Ok(())).unwrap());
            let reversed = fixture(count, true).render(|| Ok(())).unwrap();
            assert_ne!(output, reversed);
            let input = &job.frames.last().unwrap().yuv;
            let error = |bytes: &[u8], left: bool| -> u64 {
                (0..64 * 48)
                    .filter(|i| (i % 64 < 32) == left)
                    .map(|i| (bytes[i] as i64 - input[i] as i64).unsigned_abs().pow(2))
                    .sum()
            };
            assert!(error(&output, true) < error(&output, false));
            assert!(error(&reversed, false) < error(&reversed, true));
        }
        let order = [5, 1, 3, 5, 2];
        let baseline = fixture(5, false).render(|| Ok(())).unwrap();
        for count in order {
            let out = fixture(count, false).render(|| Ok(())).unwrap();
            if count == 5 {
                assert_eq!(out, baseline);
            }
        }
        assert!(fixture(3, false).render(|| Err("cancel".into())).is_err());
        let mut changed = fixture(5, false);
        changed.frames[0].offsets.fill(24.0);
        assert_ne!(baseline, changed.render(|| Ok(())).unwrap());
        let mut changed = fixture(5, false);
        changed.frames[0].yuv[..64 * 48].fill(16);
        assert_ne!(baseline, changed.render(|| Ok(())).unwrap());
        let workers = (0..8)
            .map(|_| std::thread::spawn(|| fixture(5, false).render(|| Ok(())).unwrap()))
            .collect::<Vec<_>>();
        for worker in workers {
            assert_eq!(baseline, worker.join().unwrap());
        }
    }
}
