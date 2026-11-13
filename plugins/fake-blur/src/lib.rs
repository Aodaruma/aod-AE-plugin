#![allow(clippy::drop_non_drop, clippy::question_mark)]
use after_effects as ae;
use utils::blur::host::{self, Kind, Params};
#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);
const PLUGIN_DESCRIPTION: &str =
    "Creates map-controlled mipmap, displacement, and interpolated blur effects.";
impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _: InData,
        _: OutData,
    ) -> Result<(), Error> {
        host::setup(params, Kind::Fake)
    }
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        data: InData,
        mut out: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        if matches!(cmd, ae::Command::About) {
            out.set_return_msg(concat!(
                "AOD_FakeBlur ",
                env!("CARGO_PKG_VERSION"),
                "\r\r",
                "Creates map-controlled mipmap, displacement, and interpolated blur effects."
            ));
            return Ok(());
        }
        let _ = PLUGIN_DESCRIPTION;
        host::command(cmd, data, out, params, Kind::Fake)
    }
}
