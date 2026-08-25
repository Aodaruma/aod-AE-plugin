use chrono::Datelike;
use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[cfg(target_os = "windows")]
fn embed_binary_safe_pipl(pipl: &[u8]) {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));
    let pipl_path = out_dir.join("glass_displace.pipl");
    std::fs::write(&pipl_path, pipl).expect("write binary PiPL resource");

    let escaped_path = pipl_path.to_string_lossy().replace('\\', "\\\\");
    let mut resource = winres::WindowsResource::new();
    resource.append_rc_content(&format!("16000 PiPL DISCARDABLE \"{escaped_path}\""));
    resource.compile().expect("compile binary PiPL resource");
}

#[rustfmt::skip]
fn main() {
    println!("cargo::rustc-check-cfg=cfg(does_dialog)");
    println!("cargo::rustc-check-cfg=cfg(threaded_rendering)");

    let current_year = chrono::Local::now().year();
    println!("cargo:rustc-env=BUILD_YEAR={}", current_year);

    let pkg_version = env!("CARGO_PKG_VERSION");
    let version_parts: Vec<&str> = pkg_version.split('.').collect();
    if version_parts.len() != 3 {
        panic!("CARGO_PKG_VERSION must be in the format 'major.minor.patch'");
    }
    let major: u32 = version_parts[0].parse().expect("Invalid major version");
    let minor: u32 = version_parts[1].parse().expect("Invalid minor version");
    let patch: u32 = version_parts[2].parse().expect("Invalid patch version");

    // Determine the stage based on building whether debug or release
    /*
    // pipl load error occured when stage = Stage::Release in pipl == v0.1.1, so temporarily fixed to Develop
    let stage = if cfg!(debug_assertions) {
        Stage::Develop
    } else {
        Stage::Release
    };
    */
    let stage = Stage::Develop;

    // --------------------------------------------------
    // Build the plugin with PiPL
    let properties = || vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("AOD_GlassDisplace"),
        Property::Category("Aodaruma"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: major,
            subversion: minor,
            bugversion: patch,
            stage,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            // set up from https://docs.rs/pipl/latest/pipl/struct.OutFlags.html
            OutFlags::UseOutputExtent
            | OutFlags::DeepColorAware
            | OutFlags::WideTimeInput
            | OutFlags::SendUpdateParamsUI
            ,
        ),
        Property::AE_Effect_Global_OutFlags_2(
            // set up from https://docs.rs/pipl/latest/pipl/struct.OutFlags2.html
            OutFlags2::FloatColorAware
            | OutFlags2::SupportsThreadedRendering
            | OutFlags2::AutomaticWideTimeInput
            | OutFlags2::SupportsSmartRender
            | OutFlags2::RevealsZeroAlpha
            | OutFlags2::ParamGroupStartCollapsedFlag
            // | OutFlags2::SupportsGpuRenderF32
            ,
        ),
        Property::AE_Effect_Match_Name("GlassDisplace"),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL("https://github.com/Aodaruma/aod-AE-plugin"),
    ];

    pipl::plugin_build(properties());
    #[cfg(target_os = "windows")]
    embed_binary_safe_pipl(&pipl::build_pipl(properties()).expect("build PiPL"));
}
