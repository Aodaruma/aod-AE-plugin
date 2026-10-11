// SPDX-License-Identifier: MPL-2.0
use crate::render_error::RenderError;
use crate::{codec::Job, compute_ffi::*};
use after_effects as ae;
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
};

const CLASS: &std::ffi::CStr = c"com.aodaruma.codec-map.decoded-frame.v1";

struct Suite {
    input: ae::InData,
    raw: *const AEGP_ComputeCacheSuite1,
}
impl Suite {
    fn new(input: ae::InData) -> Result<Self, ae::Error> {
        Ok(Self {
            raw: unsafe { acquire_compute_cache_suite(&input)? },
            input,
        })
    }
    fn api(&self) -> &AEGP_ComputeCacheSuite1 {
        unsafe { &*self.raw }
    }
}
impl Drop for Suite {
    fn drop(&mut self) {
        unsafe { release_compute_cache_suite(&self.input) };
    }
}
struct Receipt<'a> {
    suite: &'a Suite,
    raw: AEGP_CCCheckoutReceiptP,
}
impl Drop for Receipt<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null()
            && let Some(checkin) = self.suite.api().AEGP_CheckinComputeReceipt
        {
            unsafe { checkin(self.raw) };
        }
    }
}
struct Options<'a> {
    key: [u8; 16],
    job: &'a Job,
    abort: &'a mut dyn FnMut() -> Result<(), RenderError>,
    error: Option<RenderError>,
}
unsafe extern "C" fn key(options: *mut c_void, output: AEGP_CCComputeKeyP) -> ae::sys::A_Err {
    if options.is_null() || output.is_null() {
        return 1;
    }
    let options = unsafe { &*(options as *const Options<'_>) };
    for (i, bytes) in options.key.chunks_exact(4).enumerate() {
        unsafe {
            (*output).bytes[i] = i32::from_le_bytes(bytes.try_into().unwrap());
        }
    }
    0
}
unsafe extern "C" fn compute(options: *mut c_void, output: *mut *mut c_void) -> ae::sys::A_Err {
    if options.is_null() || output.is_null() {
        return 1;
    }
    unsafe {
        *output = std::ptr::null_mut();
    }
    let options = unsafe { &mut *(options as *mut Options<'_>) };
    match catch_unwind(AssertUnwindSafe(|| options.job.render(&mut options.abort))) {
        Ok(Ok(value)) => {
            unsafe {
                *output = Box::into_raw(Box::new(value)).cast();
            }
            0
        }
        Ok(Err(error)) => {
            let code = match &error {
                RenderError::Host(error) => (*error).into(),
                RenderError::Message(_) => 1,
            };
            options.error = Some(error);
            code
        }
        Err(_) => {
            options.error = Some("CodecMap computation panicked".into());
            1
        }
    }
}
unsafe extern "C" fn size(value: *mut c_void) -> usize {
    if value.is_null() {
        0
    } else {
        unsafe { (&*(value as *const Vec<u8>)).capacity() + std::mem::size_of::<Vec<u8>>() }
    }
}
unsafe extern "C" fn delete(value: *mut c_void) {
    if !value.is_null() {
        unsafe {
            drop(Box::from_raw(value as *mut Vec<u8>));
        }
    }
}
static CALLBACKS: AEGP_ComputeCacheCallbacks = AEGP_ComputeCacheCallbacks {
    generate_key: Some(key),
    compute: Some(compute),
    approx_size_value: Some(size),
    delete_compute_value: Some(delete),
};

pub fn register(input: ae::InData) -> Result<bool, ae::Error> {
    let suite = match Suite::new(input) {
        Ok(s) => s,
        Err(ae::Error::MissingSuite) => return Ok(false),
        Err(e) => return Err(e),
    };
    let register = suite
        .api()
        .AEGP_ClassRegister
        .ok_or(ae::Error::MissingSuite)?;
    let error = unsafe { register(CLASS.as_ptr(), &CALLBACKS) };
    if error != 0 {
        return Err(ae::Error::from(error));
    }
    Ok(true)
}
pub fn unregister(input: ae::InData) {
    if let Ok(suite) = Suite::new(input)
        && let Some(unregister) = suite.api().AEGP_ClassUnregister
    {
        unsafe {
            unregister(CLASS.as_ptr());
        }
    }
}

pub fn render(
    input: ae::InData,
    registered: bool,
    job: &Job,
    salt: &[u8],
    mut abort: impl FnMut() -> Result<(), RenderError>,
) -> Result<Vec<u8>, RenderError> {
    abort()?;
    if !registered {
        return job.render(abort);
    }
    let suite = Suite::new(input)?;
    let api = suite.api();
    let checkout = api
        .AEGP_ComputeIfNeededAndCheckout
        .ok_or("Missing cache checkout callback")?;
    let get = api
        .AEGP_GetReceiptComputeValue
        .ok_or("Missing cache value callback")?;
    api.AEGP_CheckinComputeReceipt
        .ok_or("Missing cache checkin callback")?;
    let mut options = Options {
        key: job.key(salt),
        job,
        abort: &mut abort,
        error: None,
    };
    let mut raw = std::ptr::null_mut();
    // Checkout is synchronous. Options and all borrowed input live through its return.
    let error = unsafe {
        checkout(
            CLASS.as_ptr(),
            (&mut options as *mut Options<'_>).cast(),
            true,
            &mut raw,
        )
    };
    let receipt = Receipt { suite: &suite, raw };
    if error != 0 {
        return Err(options
            .error
            .unwrap_or_else(|| ae::Error::from(error).into()));
    }
    if receipt.raw.is_null() {
        return Err("Empty cache receipt".into());
    }
    let mut value = std::ptr::null_mut();
    let error = unsafe { get(receipt.raw, &mut value) };
    if error != 0 {
        return Err(ae::Error::from(error).into());
    }
    if value.is_null() {
        return Err("Empty cache value".into());
    }
    // Never retain a borrowed cache pointer after checking the receipt in.
    let result = unsafe { (&*(value as *const Vec<u8>)).clone() };
    abort()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_keeps_one_shot_cancellation_and_publishes_no_value() {
        let job = Job {
            width: 2,
            height: 2,
            time_scale: 24,
            time_step: 1,
            crf: 28.0,
            frames: vec![],
        };
        let mut calls = 0;
        let mut abort = || {
            calls += 1;
            if calls == 1 {
                Err(ae::Error::InterruptCancel.into())
            } else {
                Ok(())
            }
        };
        let mut options = Options {
            key: [0; 16],
            job: &job,
            abort: &mut abort,
            error: None,
        };
        let mut value = std::ptr::dangling_mut::<c_void>();
        let code = unsafe { compute((&mut options as *mut Options<'_>).cast(), &mut value) };
        assert_eq!(code, ae::sys::PF_Interrupt_CANCEL);
        assert!(value.is_null());
        assert!(matches!(
            options.error,
            Some(RenderError::Host(ae::Error::InterruptCancel))
        ));
        let mut output: ae::sys::PF_OutData = unsafe { std::mem::zeroed() };
        let result = crate::render_error::finish(
            Err(options.error.take().unwrap()),
            &mut ae::OutData::from_raw(&mut output),
        );
        assert!(matches!(result, Err(ae::Error::InterruptCancel)));
        assert_eq!(output.out_flags, 0);
        assert_eq!(calls, 1);
    }
}
