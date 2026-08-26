use ae::Error;
use ae::pf::*;
use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub(crate) enum Params {
    GeometryStart,
    Shape,
    CoordinateMode,
    PointA,
    PointB,
    Center,
    Angle,
    Length,
    Aspect,
    ShapeExponent,
    SpiralTurns,
    RayCount,
    GeometryEnd,
    RainbowStart,
    ColorModel,
    SplitRangeEnds,
    HueScale,
    HueOffset,
    Component2Scale,
    Component2Offset,
    Component3Scale,
    Component3Offset,
    StartHue,
    StartComponent2,
    StartComponent3,
    EndHue,
    EndComponent2,
    EndComponent3,
    Extend,
    ClampGamut,
    RainbowEnd,
    EasingStart,
    Easing,
    BezierX1,
    BezierY1,
    BezierX2,
    BezierY2,
    EasingEnd,
    OutputStart,
    Mix,
    PreserveInputAlpha,
    OutputEnd,
    GenerationStart,
    GenerationMode,
    TwoColorSpace,
    StartColor,
    EndColor,
    GenerationEnd,
    TransformStart,
    Skew,
    TransformEnd,
}

pub(crate) fn setup(params: &mut ae::Parameters<Params>) -> Result<(), Error> {
    params.add_group(
        Params::GeometryStart,
        Params::GeometryEnd,
        "Geometry",
        false,
        |params| {
            params.add_with_flags(
                Params::Shape,
                "Shape",
                PopupDef::setup(|d| {
                    d.set_options(&[
                        "Linear",
                        "Radial (L2)",
                        "Diamond (L1)",
                        "Conic",
                        "Box (L-infinity)",
                        "Minkowski (Lp)",
                        "Reflected Linear",
                        "Spiral",
                        "Starburst",
                    ]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add_with_flags(
                Params::CoordinateMode,
                "Coordinates",
                PopupDef::setup(|d| {
                    d.set_options(&["Two Points", "Parametric"]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add(
                Params::PointA,
                "Start Point",
                PointDef::setup(|d| {
                    d.set_default((25.0, 50.0));
                }),
            )?;
            params.add(
                Params::PointB,
                "End Point",
                PointDef::setup(|d| {
                    d.set_default((75.0, 50.0));
                }),
            )?;
            params.add_with_flags(
                Params::Center,
                "Center",
                PointDef::setup(|d| {
                    d.set_default((50.0, 50.0));
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::Angle,
                "Angle",
                AngleDef::setup(|d| {
                    d.set_default(0.0);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::Length,
                "Length / Radius (px)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.01);
                    d.set_valid_max(32768.0);
                    d.set_slider_min(1.0);
                    d.set_slider_max(2048.0);
                    d.set_default(500.0);
                    d.set_precision(2);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::Aspect,
                "Aspect (-Vertical/+Horizontal)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-1.0);
                    d.set_valid_max(1.0);
                    d.set_slider_min(-1.0);
                    d.set_slider_max(1.0);
                    d.set_default(0.0);
                    d.set_precision(3);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::ShapeExponent,
                "Minkowski Exponent (p)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(0.25);
                    d.set_valid_max(64.0);
                    d.set_slider_min(0.5);
                    d.set_slider_max(16.0);
                    d.set_default(2.0);
                    d.set_precision(3);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::SpiralTurns,
                "Spiral Turns",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-128.0);
                    d.set_valid_max(128.0);
                    d.set_slider_min(-16.0);
                    d.set_slider_max(16.0);
                    d.set_default(3.0);
                    d.set_precision(2);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::RayCount,
                "Ray Count",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(1.0);
                    d.set_valid_max(512.0);
                    d.set_slider_min(1.0);
                    d.set_slider_max(64.0);
                    d.set_default(12.0);
                    d.set_precision(0);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            Ok(())
        },
    )?;

    params.add_group(
        Params::RainbowStart,
        Params::RainbowEnd,
        "Gradient Mapping",
        false,
        |params| {
            params.add_with_flags(
                Params::ColorModel,
                "Color Model",
                PopupDef::setup(|d| {
                    // Keep the first three entries stable for existing projects.
                    d.set_options(&[
                        "OKLCH",
                        "HSV",
                        "HSL",
                        "CIELCh(ab)",
                        "CIELCh(uv)",
                        "JzCzHz",
                        "IPT ICh",
                        "OkHSL",
                        "OkHSV",
                        "CAM16-UCS J'M'h'",
                    ]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add_with_flags(
                Params::SplitRangeEnds,
                "Split Range Start / End",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;

            for (id, name, valid_min, valid_max, slider_min, slider_max, default) in [
                (
                    Params::HueScale,
                    "Hue Scale (%)",
                    -1600.0,
                    1600.0,
                    -400.0,
                    400.0,
                    100.0,
                ),
                (
                    Params::HueOffset,
                    "Hue Offset (deg)",
                    -3600.0,
                    3600.0,
                    -720.0,
                    720.0,
                    0.0,
                ),
                (
                    Params::Component2Scale,
                    "Chroma Scale (%)",
                    -400.0,
                    400.0,
                    -200.0,
                    200.0,
                    0.0,
                ),
                (
                    Params::Component2Offset,
                    "Chroma Offset (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    100.0,
                ),
                (
                    Params::Component3Scale,
                    "Lightness Scale (%)",
                    -400.0,
                    400.0,
                    -200.0,
                    200.0,
                    0.0,
                ),
                (
                    Params::Component3Offset,
                    "Lightness Offset (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    75.0,
                ),
            ] {
                params.add(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(valid_min);
                        d.set_valid_max(valid_max);
                        d.set_slider_min(slider_min);
                        d.set_slider_max(slider_max);
                        d.set_default(default);
                        d.set_precision(2);
                    }),
                )?;
            }

            for (id, name, valid_min, valid_max, slider_min, slider_max, default) in [
                (
                    Params::StartHue,
                    "Start Hue (deg)",
                    -3600.0,
                    3600.0,
                    -720.0,
                    720.0,
                    0.0,
                ),
                (
                    Params::StartComponent2,
                    "Start Chroma (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    100.0,
                ),
                (
                    Params::StartComponent3,
                    "Start Lightness (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    75.0,
                ),
                (
                    Params::EndHue,
                    "End Hue (deg)",
                    -3600.0,
                    3600.0,
                    -720.0,
                    720.0,
                    360.0,
                ),
                (
                    Params::EndComponent2,
                    "End Chroma (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    100.0,
                ),
                (
                    Params::EndComponent3,
                    "End Lightness (%)",
                    -400.0,
                    400.0,
                    0.0,
                    200.0,
                    75.0,
                ),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(valid_min);
                        d.set_valid_max(valid_max);
                        d.set_slider_min(slider_min);
                        d.set_slider_max(slider_max);
                        d.set_default(default);
                        d.set_precision(2);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
                )?;
            }

            params.add(
                Params::Extend,
                "Extend",
                PopupDef::setup(|d| {
                    d.set_options(&["Clamp", "Repeat", "Mirror"]);
                    d.set_default(1);
                }),
            )?;
            params.add(
                Params::ClampGamut,
                "Clamp to Display Gamut",
                CheckBoxDef::setup(|d| {
                    d.set_default(true);
                }),
            )?;
            Ok(())
        },
    )?;

    params.add_group(
        Params::EasingStart,
        Params::EasingEnd,
        "Easing",
        false,
        |params| {
            params.add_with_flags(
                Params::Easing,
                "Preset",
                PopupDef::setup(|d| {
                    d.set_options(&[
                        "Linear",
                        "Ease In",
                        "Ease Out",
                        "Ease In-Out",
                        "Smoothstep",
                        "Smootherstep",
                        "Custom Cubic Bezier",
                    ]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            for (id, name, default) in [
                (Params::BezierX1, "Bezier X1", 0.25),
                (Params::BezierY1, "Bezier Y1", 0.1),
                (Params::BezierX2, "Bezier X2", 0.25),
                (Params::BezierY2, "Bezier Y2", 1.0),
            ] {
                params.add_with_flags(
                    id,
                    name,
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(if matches!(id, Params::BezierX1 | Params::BezierX2) {
                            0.0
                        } else {
                            -4.0
                        });
                        d.set_valid_max(if matches!(id, Params::BezierX1 | Params::BezierX2) {
                            1.0
                        } else {
                            4.0
                        });
                        d.set_slider_min(if matches!(id, Params::BezierX1 | Params::BezierX2) {
                            0.0
                        } else {
                            -2.0
                        });
                        d.set_slider_max(if matches!(id, Params::BezierX1 | Params::BezierX2) {
                            1.0
                        } else {
                            2.0
                        });
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
                Params::PreserveInputAlpha,
                "Preserve Input Alpha",
                CheckBoxDef::setup(|d| {
                    d.set_default(false);
                }),
            )?;
            Ok(())
        },
    )?;

    // Keep all parameters above append-only for project compatibility. Generation
    // mode and the two-color controls were introduced after the v0.3 parameter set.
    params.add_group(
        Params::GenerationStart,
        Params::GenerationEnd,
        "Generation",
        false,
        |params| {
            params.add_with_flags(
                Params::GenerationMode,
                "Mode",
                PopupDef::setup(|d| {
                    d.set_options(&["Parametric", "Two Color"]);
                    d.set_default(1);
                }),
                ae::ParamFlag::SUPERVISE,
                ae::ParamUIFlags::empty(),
            )?;
            params.add_with_flags(
                Params::TwoColorSpace,
                "Interpolation Color Space",
                PopupDef::setup(|d| {
                    d.set_options(&[
                        "OKLab",
                        "OKLCH (Shortest Hue)",
                        "CAM16-UCS J'a'b'",
                        "CAM16-UCS J'M'h' (Shortest Hue)",
                    ]);
                    d.set_default(1);
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::StartColor,
                "Start Color",
                ColorDef::setup(|d| {
                    d.set_default(Pixel8 {
                        alpha: 255,
                        red: 0,
                        green: 0,
                        blue: 0,
                    });
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            params.add_with_flags(
                Params::EndColor,
                "End Color",
                ColorDef::setup(|d| {
                    d.set_default(Pixel8 {
                        alpha: 255,
                        red: 255,
                        green: 255,
                        blue: 255,
                    });
                }),
                ae::ParamFlag::empty(),
                ae::ParamUIFlags::DISABLED | ae::ParamUIFlags::INVISIBLE,
            )?;
            Ok(())
        },
    )?;

    params.add_group(
        Params::TransformStart,
        Params::TransformEnd,
        "Transform",
        false,
        |params| {
            params.add(
                Params::Skew,
                "Skew (%)",
                FloatSliderDef::setup(|d| {
                    d.set_valid_min(-1000.0);
                    d.set_valid_max(1000.0);
                    d.set_slider_min(-100.0);
                    d.set_slider_max(100.0);
                    d.set_default(0.0);
                    d.set_precision(2);
                }),
            )?;
            Ok(())
        },
    )?;
    Ok(())
}
