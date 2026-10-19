// Template material: MPL-2.0 with the additional permission in TEMPLATE-LICENSE.txt.
use chrono::Datelike;
use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

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
    let properties = vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("AOD_ImageBevel"),
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
            OutFlags::CustomUI
            | OutFlags::SendUpdateParamsUI
            | OutFlags::UseOutputExtent
            | OutFlags::DeepColorAware
            | OutFlags::WideTimeInput
            ,
        ),
        Property::AE_Effect_Global_OutFlags_2(
            // set up from https://docs.rs/pipl/latest/pipl/struct.OutFlags2.html
            OutFlags2::FloatColorAware
            | OutFlags2::ParamGroupStartCollapsedFlag
            | OutFlags2::RevealsZeroAlpha
            | OutFlags2::SupportsThreadedRendering
            | OutFlags2::SupportsGetFlattenedSequenceData
            | OutFlags2::AutomaticWideTimeInput
            | OutFlags2::SupportsSmartRender
            // | OutFlags2::SupportsGpuRenderF32
            ,
        ),
        Property::AE_Effect_Match_Name("ImageBevel"),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL("https://github.com/Aodaruma/aod-AE-plugin"),
    ];
    #[cfg(not(target_os = "windows"))]
    pipl::plugin_build(properties);
    #[cfg(target_os = "windows")]
    build_windows(properties);
}

#[cfg(target_os = "windows")]
fn build_windows(properties: Vec<Property>) {
    // The pinned pipl crate embeds bytes in an RC string. rc.exe on Japanese
    // Windows recodes bytes such as 0x88, corrupting the following properties.
    // A binary file resource preserves every byte regardless of the ANSI code page.
    for property in &properties {
        match property {
            Property::Kind(kind) => println!(
                "cargo:rustc-env=PIPL_KIND={}",
                u32::from_le_bytes(kind.as_bytes())
            ),
            Property::Name(value) => println!("cargo:rustc-env=PIPL_NAME={value}"),
            Property::Category(value) => println!("cargo:rustc-env=PIPL_CATEGORY={value}"),
            Property::CodeWin64X86(value) => println!("cargo:rustc-env=PIPL_ENTRYPOINT={value}"),
            Property::AE_Effect_Match_Name(value) => {
                println!("cargo:rustc-env=PIPL_MATCH_NAME={value}")
            }
            Property::AE_Effect_Support_URL(value) => {
                println!("cargo:rustc-env=PIPL_SUPPORT_URL={value}")
            }
            Property::AE_Reserved_Info(value) => {
                println!("cargo:rustc-env=PIPL_AE_RESERVED={value}")
            }
            Property::AE_Effect_Spec_Version { major, minor } => {
                println!("cargo:rustc-env=PIPL_AE_SPEC_VER_MAJOR={major}");
                println!("cargo:rustc-env=PIPL_AE_SPEC_VER_MINOR={minor}");
            }
            Property::AE_Effect_Version {
                version,
                subversion,
                bugversion,
                stage,
                build,
            } => {
                println!(
                    "cargo:rustc-env=PIPL_VERSION={}",
                    pf_version(*version, *subversion, *bugversion, *stage, *build)
                );
            }
            Property::AE_Effect_Global_OutFlags(flags) => {
                println!("cargo:rustc-env=PIPL_OUTFLAGS={}", flags.bits())
            }
            Property::AE_Effect_Global_OutFlags_2(flags) => {
                println!("cargo:rustc-env=PIPL_OUTFLAGS2={}", flags.bits())
            }
            _ => {}
        }
    }
    for flag in [
        "sends_update_params_ui",
        "smart_render",
        "threaded_rendering",
    ] {
        println!("cargo:rustc-check-cfg=cfg({flag})");
        println!("cargo:rustc-cfg={flag}");
    }
    for flag in [
        "does_dialog",
        "uses_audio",
        "gpu_render",
        "with_premiere",
        "catch_panics",
    ] {
        println!("cargo:rustc-check-cfg=cfg({flag})");
    }
    let path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("image_bevel.pipl");
    std::fs::write(&path, build_pipl(properties).unwrap()).unwrap();
    winres::WindowsResource::new()
        .append_rc_content(&format!(
            "16000 PiPL DISCARDABLE \"{}\"",
            path.display().to_string().replace('\\', "/")
        ))
        .compile()
        .unwrap();
}
