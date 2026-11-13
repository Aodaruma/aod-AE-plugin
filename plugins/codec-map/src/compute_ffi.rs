// SPDX-License-Identifier: MPL-2.0
// Adapted from reaction-diffusion-rs, crates/aeplugin/src/compute_cache.rs,
// revision 8b107de652598a8f22003d1e4da74f753eadfa7d (MPL-2.0).
#![allow(non_camel_case_types, non_snake_case)]

use ae::sys;
use after_effects as ae;
use std::ffi::c_void;
use std::os::raw::c_char;

pub type AEGP_CCComputeClassIdP = *const c_char;
pub type AEGP_CCComputeOptionsRefconP = *mut c_void;
pub type AEGP_CCComputeValueRefconP = *mut c_void;
pub type AEGP_CCCheckoutReceiptP = *mut c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AEGP_GUID {
    pub bytes: [sys::A_long; 4],
}

pub type AEGP_CCComputeKey = AEGP_GUID;
pub type AEGP_CCComputeKeyP = *mut AEGP_CCComputeKey;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct AEGP_ComputeCacheCallbacks {
    pub generate_key: Option<
        unsafe extern "C" fn(AEGP_CCComputeOptionsRefconP, AEGP_CCComputeKeyP) -> sys::A_Err,
    >,
    pub compute: Option<
        unsafe extern "C" fn(
            AEGP_CCComputeOptionsRefconP,
            *mut AEGP_CCComputeValueRefconP,
        ) -> sys::A_Err,
    >,
    pub approx_size_value: Option<unsafe extern "C" fn(AEGP_CCComputeValueRefconP) -> usize>,
    pub delete_compute_value: Option<unsafe extern "C" fn(AEGP_CCComputeValueRefconP)>,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct AEGP_ComputeCacheSuite1 {
    pub AEGP_ClassRegister: Option<
        unsafe extern "C" fn(
            AEGP_CCComputeClassIdP,
            *const AEGP_ComputeCacheCallbacks,
        ) -> sys::A_Err,
    >,
    pub AEGP_ClassUnregister: Option<unsafe extern "C" fn(AEGP_CCComputeClassIdP) -> sys::A_Err>,
    pub AEGP_ComputeIfNeededAndCheckout: Option<
        unsafe extern "C" fn(
            AEGP_CCComputeClassIdP,
            AEGP_CCComputeOptionsRefconP,
            bool,
            *mut AEGP_CCCheckoutReceiptP,
        ) -> sys::A_Err,
    >,
    pub AEGP_CheckoutCached: Option<
        unsafe extern "C" fn(
            AEGP_CCComputeClassIdP,
            AEGP_CCComputeOptionsRefconP,
            *mut AEGP_CCCheckoutReceiptP,
        ) -> sys::A_Err,
    >,
    pub AEGP_GetReceiptComputeValue: Option<
        unsafe extern "C" fn(
            AEGP_CCCheckoutReceiptP,
            *mut AEGP_CCComputeValueRefconP,
        ) -> sys::A_Err,
    >,
    pub AEGP_CheckinComputeReceipt:
        Option<unsafe extern "C" fn(AEGP_CCCheckoutReceiptP) -> sys::A_Err>,
}

pub const K_AEGP_COMPUTE_CACHE_SUITE: &[u8] = b"AEGP Compute Cache\0";
pub const K_AEGP_COMPUTE_CACHE_SUITE_VERSION1: i32 = 1;

pub unsafe fn acquire_compute_cache_suite(
    in_data: &ae::InData,
) -> Result<*const AEGP_ComputeCacheSuite1, ae::Error> {
    let sp_basic = in_data.pica_basic_suite_ptr();
    if sp_basic.is_null() {
        return Err(ae::Error::MissingSuite);
    }

    let mut suite_ptr: *const AEGP_ComputeCacheSuite1 = std::ptr::null();
    let acquire = unsafe { (*sp_basic).AcquireSuite }.ok_or(ae::Error::MissingSuite)?;
    let err = unsafe {
        acquire(
            K_AEGP_COMPUTE_CACHE_SUITE.as_ptr() as *const c_char,
            K_AEGP_COMPUTE_CACHE_SUITE_VERSION1,
            &mut suite_ptr as *mut _ as *mut *const c_void,
        )
    };

    if err != sys::kSPNoError as i32 || suite_ptr.is_null() {
        return Err(ae::Error::MissingSuite);
    }

    Ok(suite_ptr)
}

pub unsafe fn release_compute_cache_suite(in_data: &ae::InData) {
    let sp_basic = in_data.pica_basic_suite_ptr();
    if sp_basic.is_null() {
        return;
    }

    if let Some(release) = unsafe { (*sp_basic).ReleaseSuite } {
        let _ = unsafe {
            release(
                K_AEGP_COMPUTE_CACHE_SUITE.as_ptr() as *const c_char,
                K_AEGP_COMPUTE_CACHE_SUITE_VERSION1,
            )
        };
    }
}
