// SPDX-License-Identifier: MPL-2.0
use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::NonNull;

unsafe extern "C" {
    fn cm_create(
        directory: *const c_char,
        width: i32,
        height: i32,
        scale: i32,
        step: i32,
        crf: f64,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn cm_destroy(session: *mut c_void);
    fn cm_frame(
        session: *mut c_void,
        input: *const u8,
        input_len: usize,
        offsets: *const f32,
        count: usize,
        output: *mut u8,
        output_len: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}

pub struct Frame {
    pub yuv: Vec<u8>,
    pub offsets: Vec<f32>,
}
pub struct Job {
    pub width: usize,
    pub height: usize,
    pub time_scale: i32,
    pub time_step: i32,
    pub crf: f64,
    pub frames: Vec<Frame>,
}

impl Job {
    pub fn key(&self, salt: &[u8]) -> [u8; 16] {
        let mut h = blake3::Hasher::new();
        h.update(b"com.aodaruma.codec-map.h264.v1.ffmpeg8.1.yuv709limited");
        h.update(salt);
        for v in [
            self.width as u64,
            self.height as u64,
            self.time_scale as u64,
            self.time_step as u64,
            self.crf.to_bits(),
            self.frames.len() as u64,
        ] {
            h.update(&v.to_le_bytes());
        }
        for frame in &self.frames {
            h.update(&frame.yuv);
            for value in &frame.offsets {
                h.update(&value.to_bits().to_le_bytes());
            }
        }
        h.finalize().as_bytes()[..16].try_into().unwrap()
    }

    pub fn render(&self, mut abort: impl FnMut() -> Result<(), String>) -> Result<Vec<u8>, String> {
        let mut session = Session::new(self)?;
        let mut result = Vec::new();
        for frame in &self.frames {
            abort()?;
            result = session.frame(frame)?;
        }
        if result.is_empty() {
            return Err("No codec input frames".into());
        }
        Ok(result)
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
                job.crf,
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
    fn frame(&mut self, frame: &Frame) -> Result<Vec<u8>, String> {
        if frame.yuv.len() != self.size || frame.offsets.len() != self.blocks {
            return Err("Invalid codec buffer length".into());
        }
        let mut output = vec![0; self.size];
        let mut error = [0 as c_char; 1024];
        let ok = unsafe {
            cm_frame(
                self.raw.as_ptr(),
                frame.yuv.as_ptr(),
                frame.yuv.len(),
                frame.offsets.as_ptr(),
                frame.offsets.len(),
                output.as_mut_ptr(),
                output.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if ok == 0 {
            return Err(error_text(&error));
        }
        Ok(output)
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
            crf: 30.0,
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
