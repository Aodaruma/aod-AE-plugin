use ae::Error;
use ae::pf::*;
use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub(crate) enum Params {
    HeightStart,
    HeightSource,
    Shape,
    MapLayer,
    MapChannel,
    InvertMap,
    MapBlack,
    MapWhite,
    HeightEnd,
    GeometryStart,
    Center,
    Size,
    Aspect,
    Rotation,
    Roundness,
    RingWidth,
    FacetSize,
    FacetAmount,
    FacetJitter,
    CrackWidth,
    Seed,
    GeometryEnd,
    RefractionStart,
    HeightStrength,
    NormalRadius,
    Refraction,
    Dispersion,
    Sampling,
    Edge,
    RefractionEnd,
    OutputStart,
    Mix,
    PreserveAlpha,
    Debug,
    Clamp32,
    OutputEnd,
    // New controls are intentionally appended after every v0.1 parameter.
    // AE persists effects by parameter index, so inserting them above would
    // reinterpret existing projects.
    FractureStart,
    CrackDepth,
    RadialCracks,
    CrackBranching,
    CrackJitter,
    StressRings,
    RingJitter,
    ImpactFalloff,
    FractureEnd,
    SpectralStart,
    DispersionSteps,
    // Appended after the v0.2 spectral step control to preserve its index.
    AutoSpectralSteps,
    SpectralEnd,
}

pub(crate) fn setup(params: &mut ae::Parameters<Params>) -> Result<(), Error> {
    params.add_group(
        Params::HeightStart,
        Params::HeightEnd,
        "Height Map",
        false,
        |params| {
            params.add_with_flags(
                Params::HeightSource,
                "Height Source",
                PopupDef::setup(|d| {
                    d.set_options(&["Procedural", "Custom Map", "Input Luma", "Input Alpha"]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add_with_flags(
                Params::Shape,
                "Shape",
                PopupDef::setup(|d| {
                    d.set_options(&[
                        "Sphere",
                        "Rounded Rectangle",
                        "Diamond",
                        "Ring",
                        "Facet Field",
                        "Fractured Field",
                        "Impact Glass",
                    ]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add_with_flags(
                Params::MapLayer,
                "Custom Map Layer",
                LayerDef::new(),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::MapChannel,
                "Map Channel",
                PopupDef::setup(|d| {
                    d.set_options(&["Luma", "Alpha", "Red", "Green", "Blue"]);
                    d.set_default(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::InvertMap,
                "Invert Map",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            for (id, name, default) in [
                (Params::MapBlack, "Map Black", 0.0),
                (Params::MapWhite, "Map White", 1.0),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(-16.0);
                        d.set_valid_max(16.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(default);
                        d.set_precision(3);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;
            }
            Ok(())
        },
    )?;

    params.add_group(
        Params::GeometryStart,
        Params::GeometryEnd,
        "Procedural Geometry",
        false,
        |params| {
            params.add(
                Params::Center,
                "Center",
                PointDef::setup(|d| {
                    d.set_default((50.0, 50.0));
                }),
            )?;
            params.add(
                Params::Size,
                "Size / Impact Diameter (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.01);
                    d.set_valid_max(32768.0);
                    d.set_slider_min(1.0);
                    d.set_slider_max(2048.0);
                    d.set_default(500.0);
                    d.set_precision(2);
                }),
            )?;
            params.add(
                Params::Aspect,
                "Aspect",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.01);
                    d.set_valid_max(100.0);
                    d.set_slider_min(0.1);
                    d.set_slider_max(10.0);
                    d.set_default(1.0);
                    d.set_precision(3);
                }),
            )?;
            params.add(
                Params::Rotation,
                "Rotation",
                AngleDef::setup(|d| {
                    d.set_default(0.0);
                }),
            )?;
            params.add_with_flags(
                Params::Roundness,
                "Corner Roundness (%)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(100.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(100.0);
                    d.set_default(50.0);
                    d.set_precision(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::RingWidth,
                "Ring Width (%)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(1.0);
                    d.set_valid_max(100.0);
                    d.set_slider_min(1.0);
                    d.set_slider_max(100.0);
                    d.set_default(25.0);
                    d.set_precision(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::FacetSize,
                "Cell Size (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(1.0);
                    d.set_valid_max(4096.0);
                    d.set_slider_min(2.0);
                    d.set_slider_max(256.0);
                    d.set_default(48.0);
                    d.set_precision(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            for (id, name, default) in [
                (Params::FacetAmount, "Facet Relief (%)", 100.0),
                (Params::FacetJitter, "Cell Irregularity (%)", 75.0),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(default);
                        d.set_precision(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;
            }
            params.add_with_flags(
                Params::CrackWidth,
                "Crack Width (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(512.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(32.0);
                    d.set_default(2.0);
                    d.set_precision(2);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::Seed,
                "Seed",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(16_777_215.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(10000.0);
                    d.set_default(1.0);
                    d.set_precision(0);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            Ok(())
        },
    )?;

    params.add_group(
        Params::RefractionStart,
        Params::RefractionEnd,
        "Refraction",
        false,
        |params| {
            params.add(
                Params::HeightStrength,
                "Normal Strength",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-1000.0);
                    d.set_valid_max(1000.0);
                    d.set_slider_min(-250.0);
                    d.set_slider_max(250.0);
                    d.set_default(100.0);
                    d.set_precision(2);
                }),
            )?;
            params.add(
                Params::NormalRadius,
                "Normal Radius (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.25);
                    d.set_valid_max(512.0);
                    d.set_slider_min(0.25);
                    d.set_slider_max(32.0);
                    d.set_default(1.0);
                    d.set_precision(2);
                }),
            )?;
            params.add(
                Params::Refraction,
                "Refraction (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-32768.0);
                    d.set_valid_max(32768.0);
                    d.set_slider_min(-256.0);
                    d.set_slider_max(256.0);
                    d.set_default(40.0);
                    d.set_precision(2);
                }),
            )?;
            params.add_with_flags(
                Params::Dispersion,
                "Chromatic Dispersion (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-4096.0);
                    d.set_valid_max(4096.0);
                    d.set_slider_min(-64.0);
                    d.set_slider_max(64.0);
                    d.set_default(2.0);
                    d.set_precision(2);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add(
                Params::Sampling,
                "Sampling",
                PopupDef::setup(|d| {
                    d.set_options(&["Nearest", "Bilinear"]);
                    d.set_default(2);
                }),
            )?;
            params.add(
                Params::Edge,
                "Edge",
                PopupDef::setup(|d| {
                    d.set_options(&["Transparent", "Clamp", "Tile", "Mirror"]);
                    d.set_default(2);
                }),
            )?;
            Ok(())
        },
    )?;

    params.add_group(
        Params::OutputStart,
        Params::OutputEnd,
        "Output",
        false,
        |params| {
            params.add(
                Params::Mix,
                "Mix (%)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(100.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(100.0);
                    d.set_default(100.0);
                    d.set_precision(1);
                }),
            )?;
            params.add(
                Params::PreserveAlpha,
                "Preserve Input Alpha",
                CheckBoxDef::setup(|d| {
                    d.set_default(true);
                }),
            )?;
            params.add(
                Params::Debug,
                "View",
                PopupDef::setup(|d| {
                    d.set_options(&["Final", "Height", "Normal", "Displacement", "Facet ID"]);
                    d.set_default(1);
                }),
            )?;
            params.add(
                Params::Clamp32,
                "Clamp (32bpc)",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
            )?;
            Ok(())
        },
    )?;

    // Appended groups keep all legacy parameter indices stable. Their
    // controls are hidden dynamically unless the selected fracture mode uses
    // them.
    params.add_group(
        Params::FractureStart,
        Params::FractureEnd,
        "Fracture Detail",
        true,
        |params| {
            params.add_with_flags(
                Params::CrackDepth,
                "Crack Depth (%)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.0);
                    d.set_valid_max(100.0);
                    d.set_slider_min(0.0);
                    d.set_slider_max(100.0);
                    d.set_default(90.0);
                    d.set_precision(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::RadialCracks,
                "Radial Cracks",
                SliderDef::setup(|d| {
                    d.set_valid_min(3);
                    d.set_valid_max(96);
                    d.set_slider_min(3);
                    d.set_slider_max(48);
                    d.set_default(18);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            for (id, name, default) in [
                (Params::CrackBranching, "Branching (%)", 55.0),
                (Params::CrackJitter, "Crack Jitter (%)", 45.0),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(default);
                        d.set_precision(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;
            }
            params.add_with_flags(
                Params::StressRings,
                "Stress Rings",
                SliderDef::setup(|d| {
                    d.set_valid_min(0);
                    d.set_valid_max(32);
                    d.set_slider_min(0);
                    d.set_slider_max(16);
                    d.set_default(6);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            for (id, name, default) in [
                (Params::RingJitter, "Ring Irregularity (%)", 35.0),
                (Params::ImpactFalloff, "Impact Edge Falloff (%)", 20.0),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(default);
                        d.set_precision(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;
            }
            Ok(())
        },
    )?;

    params.add_group(
        Params::SpectralStart,
        Params::SpectralEnd,
        "Spectral Dispersion",
        true,
        |params| {
            params.add(
                Params::DispersionSteps,
                "Spectral Steps (Manual)",
                SliderDef::setup(|d| {
                    d.set_valid_min(3);
                    d.set_valid_max(32);
                    d.set_slider_min(3);
                    d.set_slider_max(16);
                    d.set_default(8);
                }),
            )?;
            params.add_with_flags(
                Params::AutoSpectralSteps,
                "Auto Spectral Steps",
                CheckBoxDef::setup(|d| {
                    d.set_default(true);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            Ok(())
        },
    )?;
    Ok(())
}
