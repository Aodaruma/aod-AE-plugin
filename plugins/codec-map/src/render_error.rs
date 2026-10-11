// SPDX-License-Identifier: MPL-2.0
use after_effects as ae;

#[derive(Debug)]
pub enum RenderError {
    Host(ae::Error),
    Message(String),
}

impl From<ae::Error> for RenderError {
    fn from(error: ae::Error) -> Self {
        Self::Host(error)
    }
}
impl From<String> for RenderError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}
impl From<&str> for RenderError {
    fn from(message: &str) -> Self {
        Self::Message(message.into())
    }
}

pub fn finish(result: Result<(), RenderError>, output: &mut ae::OutData) -> Result<(), ae::Error> {
    match result {
        Ok(()) => Ok(()),
        // Preserve the first callback result. Cancellation is not sticky, and
        // polling abort again can replace it with a different result.
        Err(RenderError::Host(error)) => Err(error),
        Err(RenderError::Message(message)) => {
            crate::report_error(output, &message);
            Err(ae::Error::BadCallbackParameter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_cancellation_does_not_set_an_error_message() {
        let mut raw: ae::sys::PF_OutData = unsafe { std::mem::zeroed() };
        let mut output = ae::OutData::from_raw(&mut raw);
        let error = finish(Err(ae::Error::InterruptCancel.into()), &mut output);
        assert!(matches!(error, Err(ae::Error::InterruptCancel)));
        assert_eq!(raw.out_flags & ae::sys::PF_OutFlag_DISPLAY_ERROR_MESSAGE, 0);
        assert_eq!(raw.return_msg[0], 0);
        let error = finish(Err("Codec unavailable".into()), &mut output);
        assert!(matches!(error, Err(ae::Error::BadCallbackParameter)));
        assert_ne!(raw.out_flags & ae::sys::PF_OutFlag_DISPLAY_ERROR_MESSAGE, 0);
    }
}
