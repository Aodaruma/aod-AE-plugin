use chrono::Datelike;
use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[cfg(target_os = "windows")]
fn embed_binary_safe_pipl(pipl: &[u8]) {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));
    let pipl_path = out_dir.join("image_transform.pipl");
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
    println!("cargo:rustc-env=BUILD_YEAR={}", chrono::Local::now().year());

    let version: Vec<u32> = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|part| part.parse().expect("numeric package version"))
        .collect();
    assert_eq!(version.len(), 3, "CARGO_PKG_VERSION must be major.minor.patch");

    let properties = || vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("AOD_ImageTransform"),
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
            version: version[0],
            subversion: version[1],
            bugversion: version[2],
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::UseOutputExtent
                | OutFlags::DeepColorAware
                | OutFlags::SendUpdateParamsUI,
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::FloatColorAware
                | OutFlags2::SupportsSmartRender
                | OutFlags2::ParamGroupStartCollapsedFlag
                | OutFlags2::RevealsZeroAlpha,
        ),
        Property::AE_Effect_Match_Name("ImageTransform"),
        Property::AE_Reserved_Info(8),
        Property::AE_Effect_Support_URL("https://github.com/Aodaruma/aod-AE-plugin"),
    ];

    pipl::plugin_build(properties());
    #[cfg(target_os = "windows")]
    embed_binary_safe_pipl(&pipl::build_pipl(properties()).expect("build PiPL"));
}
