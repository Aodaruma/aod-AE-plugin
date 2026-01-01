//! {{AE_PLUGIN_NAME}}
//!
//! Entry points are AE/PR SDK specific.
//! Replace this scaffold with your actual after-effects crate integration.

use anyhow::Result;

pub fn hello() -> Result<()> {
    // TODO: wire AE entrypoint / params / render
    Ok(())
}

#[cfg(feature = "opencv")]
pub fn uses_opencv() {
    // TODO: call algo::opencv stuff
}

#[cfg(feature = "fft")]
pub fn uses_fft() {
    // TODO: call algo::fft stuff
}

#[cfg(feature = "gpu")]
pub fn uses_gpu() {
    // TODO: call algo::gpu stuff
}
