// Template material: MPL-2.0 with the additional permission in TEMPLATE-LICENSE.txt.
use chrono::Datelike;
use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;
const NAME: &str = "AOD_SeamScale";
const MATCH_NAME: &str = "SeamScale";
const CATEGORY: &str = "Aodaruma";
const SUPPORT_URL: &str = "https://github.com/Aodaruma/aod-AE-plugin";

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
    let flags = OutFlags::SendUpdateParamsUI | OutFlags::DeepColorAware;
    let flags2 = OutFlags2::FloatColorAware | OutFlags2::SupportsThreadedRendering
        | OutFlags2::SupportsGetFlattenedSequenceData | OutFlags2::SupportsSmartRender;
    #[cfg(target_os = "windows")]
    let flag_bits = (flags.bits(), flags2.bits());

    // --------------------------------------------------
    // Build the plugin with PiPL
    let properties = vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name(NAME),
        Property::Category(CATEGORY),

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
        Property::AE_Effect_Global_OutFlags(flags),
        Property::AE_Effect_Global_OutFlags_2(flags2),
        Property::AE_Effect_Match_Name(MATCH_NAME),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL(SUPPORT_URL),
    ];
    #[cfg(target_os = "windows")]
    build_windows(properties, pf_version(major, minor, patch, stage, 1), flag_bits.0, flag_bits.1);
    #[cfg(not(target_os = "windows"))]
    pipl::plugin_build(properties);
}

#[cfg(target_os = "windows")]
fn build_windows(properties: Vec<Property>, version: u32, flags: u32, flags2: u32) {
    // Keep the entry point metadata identical to the PiPL without sending its
    // bytes through an RC string/code-page conversion (notably version 0.1.2+).
    for (name, value) in [
        ("PIPL_NAME", NAME.to_owned()),
        ("PIPL_MATCH_NAME", MATCH_NAME.to_owned()),
        ("PIPL_CATEGORY", CATEGORY.to_owned()),
        ("PIPL_SUPPORT_URL", SUPPORT_URL.to_owned()),
        ("PIPL_ENTRYPOINT", "EffectMain".to_owned()),
        (
            "PIPL_KIND",
            u32::from_le_bytes(PIPLType::AEEffect.as_bytes()).to_string(),
        ),
        ("PIPL_AE_SPEC_VER_MAJOR", PF_PLUG_IN_VERSION.to_string()),
        ("PIPL_AE_SPEC_VER_MINOR", PF_PLUG_IN_SUBVERS.to_string()),
        ("PIPL_VERSION", version.to_string()),
        ("PIPL_OUTFLAGS", flags.to_string()),
        ("PIPL_OUTFLAGS2", flags2.to_string()),
        ("PIPL_AE_RESERVED", "8".to_owned()),
    ] {
        println!("cargo:rustc-env={name}={value}");
    }
    for cfg in [
        "with_premiere",
        "catch_panics",
        "does_dialog",
        "uses_audio",
        "gpu_render",
        "threaded_rendering",
        "smart_render",
        "sends_update_params_ui",
    ] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
    }
    for cfg in [
        "threaded_rendering",
        "smart_render",
        "sends_update_params_ui",
    ] {
        println!("cargo:rustc-cfg={cfg}");
    }
    let bytes = pipl::build_pipl(properties).expect("Build PiPL");
    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("seam-scale.pipl");
    std::fs::write(&path, bytes).expect("Write binary PiPL");
    let resource_path = path
        .to_str()
        .expect("UTF-8 resource path")
        .replace('\\', "/")
        .replace('"', "\\\"");
    winres::WindowsResource::new()
        .append_rc_content(&format!("16000 PiPL \"{resource_path}\""))
        .compile()
        .expect("Embed binary PiPL");
}
