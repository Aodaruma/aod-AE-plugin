// SPDX-License-Identifier: MPL-2.0
use super::*;

pub fn strength_quality(codec: codec::Codec, strength: f64) -> f64 {
    let s = strength.clamp(0.0, 100.0) / 100.0;
    match codec {
        codec::Codec::H264 => 1.0 + 50.0 * s,
        codec::Codec::Vp9 => 63.0 * s,
        codec::Codec::Mpeg4 => 1.0 + 30.0 * s,
    }
}

pub fn setup(params: &mut Parameters<Params>) -> Result<(), Error> {
    use ae::{ParamFlag as F, ParamUIFlags as U};
    let fixed = || F::CANNOT_TIME_VARY | F::CANNOT_INTERP;
    let supervised = || fixed() | F::SUPERVISE;
    // Enum variant names determine disk IDs in the pinned SDK. Keep legacy names.
    params.add_with_flags(
        Params::Style,
        "Style",
        ae::PopupDef::setup(|p| {
            p.set_options(&["Compression", "Blocks", "Datamosh", "Custom"]);
            p.set_default(1).set_value(4);
        }),
        supervised() | F::USE_VALUE_FOR_OLD_PROJECTS,
        U::NONE,
    )?;
    params.add_with_flags(
        Params::Strength,
        "Compression",
        slider(54.0, 0.0, 100.0),
        supervised(),
        U::NONE,
    )?;
    params.add_with_flags(
        Params::DropI,
        "Datamosh",
        ae::CheckBoxDef::setup(|p| {
            p.set_default(false).set_value(false);
        }),
        F::SUPERVISE,
        U::NONE,
    )?;
    params.add_with_flags(
        Params::MapSource,
        "Area",
        popup(&["Whole Image", "Map Layer"]),
        supervised(),
        U::NONE,
    )?;
    params.add(Params::MapLayer, "Map Layer", ae::LayerDef::new())?;
    params.add(Params::Mix, "Mix", slider(100.0, 0.0, 100.0))?;
    params.add_group(
        Params::AdvancedStart,
        Params::AdvancedEnd,
        "Advanced",
        true,
        |p| {
            p.add_group(Params::CodecStart, Params::CodecEnd, "Codec", false, |p| {
                p.add_with_flags(
                    Params::Codec,
                    "Codec",
                    popup(&["H.264", "VP9", "MPEG-4 Part 2"]),
                    supervised(),
                    U::NONE,
                )?;
                p.add_with_flags(
                    Params::QualityMode,
                    "Quality Control",
                    ae::PopupDef::setup(|p| {
                        p.set_options(&["Compression", "Native"]);
                        p.set_default(1).set_value(2);
                    }),
                    supervised() | F::USE_VALUE_FOR_OLD_PROJECTS,
                    U::NONE,
                )?;
                for (id, value, min, max) in [
                    (Params::Crf, 28.0, 1.0, 51.0),
                    (Params::Vp9Quality, 35.0, 0.0, 63.0),
                    (Params::Mpeg4Quality, 8.0, 1.0, 31.0),
                ] {
                    p.add_with_flags(
                        id,
                        "Codec Quality",
                        slider(value, min, max),
                        supervised(),
                        U::NONE,
                    )?;
                }
                Ok(())
            })?;
            p.add_group(
                Params::MotionStart,
                Params::MotionEnd,
                "Motion",
                false,
                |p| {
                    p.add_with_flags(
                        Params::Mode,
                        "Frame Mode",
                        popup(&["Past Frames", "Independent Frames"]),
                        supervised(),
                        U::NONE,
                    )?;
                    p.add_with_flags(
                        Params::Gop,
                        "Refresh Interval",
                        integer(12, 1, 60),
                        supervised(),
                        U::NONE,
                    )?;
                    p.add_with_flags(
                        Params::Future,
                        "Allow Future Frames",
                        ae::CheckBoxDef::setup(|p| {
                            p.set_default(false).set_value(false);
                        }),
                        supervised(),
                        U::NONE,
                    )?;
                    p.add_with_flags(
                        Params::DropB,
                        "Drop B-Frames",
                        ae::CheckBoxDef::setup(|p| {
                            p.set_default(false).set_value(false);
                        }),
                        F::SUPERVISE,
                        U::NONE,
                    )?;
                    Ok(())
                },
            )?;
            p.add_group(Params::MapStart, Params::MapEnd, "Map", false, |p| {
                p.add(
                    Params::Channel,
                    "Channel",
                    popup(&["Luma", "Red", "Green", "Blue", "Alpha"]),
                )?;
                p.add(
                    Params::Invert,
                    "Invert",
                    ae::CheckBoxDef::setup(|p| {
                        p.set_default(false).set_value(false);
                    }),
                )?;
                p.add(Params::Gamma, "Gamma", slider(1.0, 0.1, 10.0))?;
                p.add(Params::Black, "Dark Area Bias", slider(0.0, -24.0, 24.0))?;
                p.add(Params::White, "Light Area Bias", slider(12.0, -24.0, 24.0))?;
                p.add(
                    Params::Output,
                    "Map Behavior",
                    popup(&["Restrict Effect", "Full Codec Frame"]),
                )?;
                Ok(())
            })?;
            p.add_group(
                Params::DiagnosticsStart,
                Params::DiagnosticsEnd,
                "Diagnostics",
                false,
                |p| {
                    p.add(
                        Params::Preview,
                        "Display",
                        popup(&["Result", "Map", "Compression Bias", "Difference"]),
                    )?;
                    p.add_with_flags(
                        Params::ReplayMemory,
                        "Working Memory (MiB)",
                        integer(1024, 64, 8192),
                        fixed(),
                        U::NONE,
                    )?;
                    p.add_with_flags(
                        Params::Reset,
                        "Cache",
                        ae::ButtonDef::setup(|p| {
                            p.set_label("Reset Cache");
                        }),
                        supervised(),
                        U::NONE,
                    )?;
                    p.add_with_flags(
                        Params::Generation,
                        "Cache Generation",
                        integer(0, 0, i32::MAX),
                        fixed(),
                        U::INVISIBLE,
                    )?;
                    Ok(())
                },
            )?;
            Ok(())
        },
    )
}

fn integer(value: i32, min: i32, max: i32) -> ae::SliderDef<'static> {
    ae::SliderDef::setup(|p| {
        p.set_default(value)
            .set_value(value)
            .set_valid_min(min)
            .set_valid_max(max)
            .set_slider_min(min)
            .set_slider_max(max);
    })
}

pub fn changed(index: usize, params: &mut Parameters<Params>) -> Result<(), Error> {
    if Some(index) == params.index(Params::Style) {
        let style = choice(params, Params::Style)?;
        if style <= 3 {
            set_choice(params, Params::Codec, if style == 1 { 1 } else { 3 })?;
            set_float(
                params,
                Params::Strength,
                match style {
                    1 => 54.0,
                    2 => 80.0,
                    _ => 50.0,
                },
            )?;
            set_choice(params, Params::QualityMode, 1)?;
            set_choice(params, Params::Mode, if style == 2 { 2 } else { 1 })?;
            {
                let mut p = params.get_mut(Params::Gop)?;
                p.as_slider_mut()?
                    .set_value(if style == 2 { 1 } else { 12 });
                p.set_value_changed();
            }
            set_bool(params, Params::DropI, style == 3)?;
            set_bool(params, Params::Future, false)?;
            set_bool(params, Params::DropB, false)?;
        }
    } else if [Params::Crf, Params::Vp9Quality, Params::Mpeg4Quality]
        .into_iter()
        .any(|id| params.index(id) == Some(index))
    {
        set_choice(params, Params::QualityMode, 2)?;
        set_choice(params, Params::Style, 4)?;
    } else if [
        Params::Codec,
        Params::Mode,
        Params::Gop,
        Params::Future,
        Params::DropB,
        Params::QualityMode,
    ]
    .into_iter()
    .any(|id| params.index(id) == Some(index))
    {
        set_choice(params, Params::Style, 4)?;
    }
    if choice(params, Params::QualityMode)? == 1 {
        let strength = float(params, Params::Strength)?;
        for (id, codec) in [
            (Params::Crf, codec::Codec::H264),
            (Params::Vp9Quality, codec::Codec::Vp9),
            (Params::Mpeg4Quality, codec::Codec::Mpeg4),
        ] {
            set_float(params, id, strength_quality(codec, strength))?;
        }
    }
    Ok(())
}
fn set_choice(p: &mut Parameters<Params>, id: Params, value: i32) -> Result<(), Error> {
    let mut param = p.get_mut(id)?;
    param.as_popup_mut()?.set_value(value);
    param.set_value_changed();
    Ok(())
}
fn set_float(p: &mut Parameters<Params>, id: Params, value: f64) -> Result<(), Error> {
    let mut param = p.get_mut(id)?;
    param.as_float_slider_mut()?.set_value(value);
    param.set_value_changed();
    Ok(())
}
fn set_bool(p: &mut Parameters<Params>, id: Params, value: bool) -> Result<(), Error> {
    let mut param = p.get_mut(id)?;
    param.as_checkbox_mut()?.set_value(value);
    param.set_value_changed();
    Ok(())
}
