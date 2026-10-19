// SPDX-License-Identifier: MPL-2.0
use super::{Params, Plugin, ae};
use ae::Error;
use ae::pf::*;

const SCALE_PARAMS: [(Params, &str); 6] = [
    (Params::MinScale, "Min Scale (%)"),
    (Params::MaxScale, "Max Scale (%)"),
    (Params::MinX, "Min Scale X (%)"),
    (Params::MaxX, "Max Scale X (%)"),
    (Params::MinY, "Min Scale Y (%)"),
    (Params::MaxY, "Max Scale Y (%)"),
];

pub fn add_parameters(params: &mut Parameters<Params>) -> Result<(), Error> {
    // Append all IDs, preserving the existing projects' parameter ordering.
    params.add_with_flags(
        Params::UseMap,
        "Use Map Layer",
        CheckBoxDef::setup(|d| {
            d.set_default(false);
        }),
        ParamFlag::SUPERVISE,
        ParamUIFlags::empty(),
    )?;
    params.add(Params::MapLayer, "Scale Map Layer", LayerDef::new())?;
    params.add(
        Params::MapChannel,
        "Map Channel",
        PopupDef::setup(|d| {
            d.set_options(&["Lightness", "R", "G", "B", "A"]);
            d.set_default(1);
        }),
    )?;
    for (id, name) in SCALE_PARAMS {
        params.add(
            id,
            name,
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-400.0);
                d.set_valid_max(400.0);
                d.set_slider_min(-400.0);
                d.set_slider_max(400.0);
                d.set_default(100.0);
                d.set_precision(2);
            }),
        )?;
    }
    for (id, name, default, max) in [
        (Params::Anchor, "Anchor Map Value (%)", 50.0, 100.0),
        (Params::Tolerance, "Anchor Tolerance (+/- %)", 0.2, 50.0),
    ] {
        params.add(
            id,
            name,
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(max);
                d.set_slider_min(0.0);
                d.set_slider_max(max);
                d.set_default(default);
                d.set_precision(2);
            }),
        )?;
    }
    Ok(())
}

fn visible(id: Params, map: bool, separate: bool) -> bool {
    match id {
        Params::Scale => !map && !separate,
        Params::ScaleX | Params::ScaleY => !map && separate,
        Params::Output => !map,
        Params::MinScale | Params::MaxScale => map && !separate,
        Params::MinX | Params::MaxX | Params::MinY | Params::MaxY => map && separate,
        Params::MapLayer | Params::MapChannel | Params::Anchor | Params::Tolerance => map,
        _ => true,
    }
}

pub fn update(
    plugin: &Plugin,
    in_data: InData,
    params: &mut Parameters<Params>,
) -> Result<(), Error> {
    let map = params.get(Params::UseMap)?.as_checkbox()?.value();
    let separate = params.get(Params::SeparateXY)?.as_checkbox()?.value();
    for id in [
        Params::Scale,
        Params::ScaleX,
        Params::ScaleY,
        Params::Output,
        Params::MapLayer,
        Params::MapChannel,
        Params::MinScale,
        Params::MaxScale,
        Params::MinX,
        Params::MaxX,
        Params::MinY,
        Params::MaxY,
        Params::Anchor,
        Params::Tolerance,
    ] {
        let show = visible(id, map, separate);
        set_visible(plugin, in_data, params, id, show)?;
        set_ui_flag(params, id, ParamUIFlags::DISABLED, !show)?;
    }
    Ok(())
}

fn set_visible(
    plugin: &Plugin,
    in_data: InData,
    params: &mut Parameters<Params>,
    id: Params,
    visible: bool,
) -> Result<(), Error> {
    if !in_data.is_premiere()
        && let Some(plugin_id) = plugin.aegp_id
        && let Some(index) = params.index(id)
        && let Ok(effect) = in_data.effect().aegp_effect(plugin_id)
        && let Ok(stream) = effect.new_stream_by_index(plugin_id, index as i32)
    {
        // The pinned SDK exposes the combined mask as an enum; its getter
        // panics for zero/combined flags. Set individual flags as the SDK requires.
        stream.set_dynamic_stream_flag(ae::aegp::DynamicStreamFlags::Hidden, false, !visible)?;
        return Ok(());
    }
    set_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible)
}

fn set_ui_flag(
    params: &mut Parameters<Params>,
    id: Params,
    flag: ParamUIFlags,
    value: bool,
) -> Result<(), Error> {
    if (params.get(id)?.ui_flags().bits() & flag.bits() != 0) != value {
        let mut param = params.get_mut(id)?;
        param.set_ui_flag(flag, value);
        param.update_param_ui()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_and_axis_switches_expose_only_the_relevant_scales() {
        let all = [
            Params::Scale,
            Params::ScaleX,
            Params::ScaleY,
            Params::MinScale,
            Params::MaxScale,
            Params::MinX,
            Params::MaxX,
            Params::MinY,
            Params::MaxY,
        ];
        for (map, separate, expected) in [
            (false, false, vec![Params::Scale]),
            (false, true, vec![Params::ScaleX, Params::ScaleY]),
            (true, false, vec![Params::MinScale, Params::MaxScale]),
            (
                true,
                true,
                vec![Params::MinX, Params::MaxX, Params::MinY, Params::MaxY],
            ),
        ] {
            assert_eq!(
                all.into_iter()
                    .filter(|&p| visible(p, map, separate))
                    .collect::<Vec<_>>(),
                expected
            );
            for id in [
                Params::MapLayer,
                Params::MapChannel,
                Params::Anchor,
                Params::Tolerance,
            ] {
                assert_eq!(visible(id, map, separate), map);
            }
        }
    }
}
