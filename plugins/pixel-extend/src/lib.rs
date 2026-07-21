#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Direction,
    ExtendCount,
    ExtendDirection,
    StepSize,
    Offset,
    DirectionMapGroupStart,
    UseDirectionMap,
    DirectionMapLayer,
    MapChannel,
    MapVectorMode,
    MapInfluence,
    DirectionMapGroupEnd,
    ShapeGroupStart,
    Decay,
    AngleChange,
    ExpansionPerStep,
    ExpansionSamples,
    ShapeGroupEnd,
    DetectionGroupStart,
    Basis,
    SelectMode,
    ColorMode,
    InvertDetection,
    Threshold,
    Softness,
    TargetColor,
    DetectionGroupEnd,
    InterpolationGroupStart,
    Interpolation,
    MitchellB,
    MitchellC,
    InterpolationGroupEnd,
    CompositeGroupStart,
    BlendMode,
    Opacity,
    ShowSource,
    CompositeGroupEnd,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Extends pixels in adjustable directions with selectable source masks and interpolation.";
const EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug)]
enum SourceBasis {
    Alpha,
    Lightness,
    Color,
}

impl SourceBasis {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Lightness,
            3 => Self::Color,
            _ => Self::Alpha,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SelectMode {
    Above,
    Below,
    Similar,
    Different,
}

impl SelectMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Below,
            3 => Self::Similar,
            4 => Self::Different,
            _ => Self::Above,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum InterpolationMode {
    Nearest,
    Bilinear,
    Bicubic,
    Mitchell,
}

#[derive(Clone, Copy, Debug)]
enum ExtendDirection {
    Forward,
    Backward,
    Both,
}

impl ExtendDirection {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Backward,
            3 => Self::Both,
            _ => Self::Forward,
        }
    }
}

impl InterpolationMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            1 => Self::Nearest,
            3 => Self::Bicubic,
            4 => Self::Mitchell,
            _ => Self::Bilinear,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum BlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
}

impl BlendMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Add,
            3 => Self::Multiply,
            4 => Self::Screen,
            5 => Self::Overlay,
            6 => Self::Darken,
            7 => Self::Lighten,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MapChannel {
    Red,
    Green,
    Blue,
    Alpha,
    Hue,
    Saturation,
    Value,
}

impl MapChannel {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Green,
            3 => Self::Blue,
            4 => Self::Alpha,
            5 => Self::Hue,
            6 => Self::Saturation,
            7 => Self::Value,
            _ => Self::Red,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum MapVectorMode {
    Gradient,
    Divergence,
    Rotation,
}

impl MapVectorMode {
    fn from_popup_value(value: i32) -> Self {
        match value {
            2 => Self::Divergence,
            3 => Self::Rotation,
            _ => Self::Gradient,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    direction_rad: f32,
    extend_count: usize,
    extend_direction: ExtendDirection,
    step_size: f32,
    offset: f32,
    use_direction_map: bool,
    map_channel: MapChannel,
    map_vector_mode: MapVectorMode,
    map_influence: f32,
    decay: f32,
    angle_change_rad: f32,
    expansion_per_step: f32,
    expansion_samples: usize,
    basis: SourceBasis,
    select_mode: SelectMode,
    invert_detection: bool,
    threshold: f32,
    softness: f32,
    target_color: PixelF32,
    interpolation: InterpolationMode,
    mitchell_b: f32,
    mitchell_c: f32,
    blend_mode: BlendMode,
    opacity: f32,
    show_source: bool,
}

#[derive(Clone, Copy, Debug)]
struct DirectionMapData<'a> {
    pixels: &'a [PixelF32],
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, Debug)]
struct SourceImage<'a> {
    pixels: &'a [PixelF32],
    masks: &'a [f32],
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct WeightedPixel {
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    weight: f32,
}

impl WeightedPixel {
    fn add(&mut self, px: PixelF32, weight: f32) {
        if weight == 0.0 || !weight.is_finite() {
            return;
        }

        self.red += px.red * weight;
        self.green += px.green * weight;
        self.blue += px.blue * weight;
        self.alpha += px.alpha * weight;
        self.weight += weight;
    }

    fn finish(self) -> PixelF32 {
        if self.weight.abs() <= EPSILON {
            return transparent_pixel();
        }

        let inv = 1.0 / self.weight;
        PixelF32 {
            alpha: clamp01(self.alpha * inv),
            red: clamp01(self.red * inv),
            green: clamp01(self.green * inv),
            blue: clamp01(self.blue * inv),
        }
    }
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::Direction,
            "Direction",
            AngleDef::setup(|d| {
                d.set_default(45.0);
                d.set_value(d.default());
            }),
        )?;

        params.add(
            Params::ExtendCount,
            "Extend Count",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(4096);
                d.set_slider_min(0);
                d.set_slider_max(512);
                d.set_default(64);
            }),
        )?;

        params.add(
            Params::ExtendDirection,
            "Extend Direction",
            PopupDef::setup(|d| {
                d.set_options(&["Forward", "Backward", "Both"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::StepSize,
            "Step Size (px)",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0);
                d.set_valid_max(64.0);
                d.set_slider_min(0.0);
                d.set_slider_max(16.0);
                d.set_default(1.0);
                d.set_precision(3);
            }),
        )?;

        params.add(
            Params::Offset,
            "Offset",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(-4096.0);
                d.set_valid_max(4096.0);
                d.set_slider_min(-512.0);
                d.set_slider_max(512.0);
                d.set_default(0.0);
                d.set_precision(3);
            }),
        )?;

        params.add_group(
            Params::DirectionMapGroupStart,
            Params::DirectionMapGroupEnd,
            "Direction Map",
            true,
            |params| {
                params.add_with_flags(
                    Params::UseDirectionMap,
                    "Use Direction Map",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add_with_flags(
                    Params::DirectionMapLayer,
                    "Direction Map Layer",
                    LayerDef::new(),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                params.add_with_flags(
                    Params::MapChannel,
                    "Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Red",
                            "Green",
                            "Blue",
                            "Alpha",
                            "Hue",
                            "Saturation",
                            "Value",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                params.add_with_flags(
                    Params::MapVectorMode,
                    "Map Vector Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["Gradient", "Divergence", "Rotation"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                params.add_with_flags(
                    Params::MapInfluence,
                    "Map Influence",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(100.0);
                        d.set_precision(2);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::ShapeGroupStart,
            Params::ShapeGroupEnd,
            "Falloff / Curve",
            true,
            |params| {
                params.add(
                    Params::Decay,
                    "Decay",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::AngleChange,
                    "Angle Change",
                    AngleDef::setup(|d| {
                        d.set_default(0.0);
                        d.set_value(d.default());
                    }),
                )?;

                params.add(
                    Params::ExpansionPerStep,
                    "Expansion / Step",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(64.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(8.0);
                        d.set_default(0.0);
                        d.set_precision(3);
                    }),
                )?;

                params.add(
                    Params::ExpansionSamples,
                    "Expansion Samples",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(65);
                        d.set_slider_min(1);
                        d.set_slider_max(17);
                        d.set_default(5);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::DetectionGroupStart,
            Params::DetectionGroupEnd,
            "Pixel Detection",
            false,
            |params| {
                params.add_with_flags(
                    Params::Basis,
                    "Source Basis",
                    PopupDef::setup(|d| {
                        d.set_options(&["Alpha", "Lightness", "Color"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::SelectMode,
                    "Threshold Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["Above / Brighter", "Below / Darker"]);
                        d.set_default(1);
                    }),
                )?;

                params.add_with_flags(
                    Params::ColorMode,
                    "Color Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["Similar Color", "Different Color"]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                params.add(
                    Params::InvertDetection,
                    "Invert Detection",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                )?;

                params.add(
                    Params::Threshold,
                    "Threshold",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.5);
                        d.set_precision(4);
                    }),
                )?;

                params.add(
                    Params::Softness,
                    "Softness",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(0.0);
                        d.set_precision(4);
                    }),
                )?;

                params.add_with_flags(
                    Params::TargetColor,
                    "Target Color",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::InterpolationGroupStart,
            Params::InterpolationGroupEnd,
            "Interpolation",
            true,
            |params| {
                params.add_with_flags(
                    Params::Interpolation,
                    "Interpolation",
                    PopupDef::setup(|d| {
                        d.set_options(&["Nearest", "Bilinear", "Bicubic", "Mitchell"]);
                        d.set_default(2);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add_with_flags(
                    Params::MitchellB,
                    "Mitchell B",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0 / 3.0);
                        d.set_precision(3);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                params.add_with_flags(
                    Params::MitchellC,
                    "Mitchell C",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(1.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(1.0);
                        d.set_default(1.0 / 3.0);
                        d.set_precision(3);
                    }),
                    ae::ParamFlag::empty(),
                    ae::ParamUIFlags::INVISIBLE,
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::CompositeGroupStart,
            Params::CompositeGroupEnd,
            "Composite",
            false,
            |params| {
                params.add_with_flags(
                    Params::ShowSource,
                    "Show Source",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add(
                    Params::BlendMode,
                    "Blend Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Normal", "Add", "Multiply", "Screen", "Overlay", "Darken", "Lighten",
                        ]);
                        d.set_default(1);
                    }),
                )?;

                params.add(
                    Params::Opacity,
                    "Opacity",
                    FloatSliderDef::setup(|d| {
                        d.set_valid_min(0.0);
                        d.set_valid_max(100.0);
                        d.set_slider_min(0.0);
                        d.set_slider_max(100.0);
                        d.set_default(100.0);
                        d.set_precision(2);
                    }),
                )?;

                Ok(())
            },
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    format!(
                        "AOD_PixelExtend - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_PixelExtend")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.do_render(in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let req = extra.output_request();

                if let Ok(in_result) = extra.callbacks().checkout_layer(
                    0,
                    0,
                    &req,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                ) {
                    let _ = extra.union_result_rect(in_result.result_rect.into());
                    let _ = extra.union_max_result_rect(in_result.max_result_rect.into());
                } else {
                    return Err(Error::InterruptCancel);
                }
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let in_layer_opt = cb.checkout_layer_pixels(0)?;
                let out_layer_opt = cb.checkout_output()?;

                if let (Some(in_layer), Some(out_layer)) = (in_layer_opt, out_layer_opt) {
                    self.do_render(in_data, in_layer, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let changed = params.type_at(param_index);
                if changed == Params::Basis
                    || changed == Params::Interpolation
                    || changed == Params::ShowSource
                    || changed == Params::UseDirectionMap
                {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut params_copy = params.cloned();
                self.update_params_ui(in_data, &mut params_copy)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let basis = SourceBasis::from_popup_value(params.get(Params::Basis)?.as_popup()?.value());
        let interpolation = InterpolationMode::from_popup_value(
            params.get(Params::Interpolation)?.as_popup()?.value(),
        );
        let show_source = params.get(Params::ShowSource)?.as_checkbox()?.value();
        let use_direction_map = params.get(Params::UseDirectionMap)?.as_checkbox()?.value();

        self.set_param_visible(
            in_data,
            params,
            Params::DirectionMapLayer,
            use_direction_map,
        )?;
        self.set_param_visible(in_data, params, Params::MapChannel, use_direction_map)?;
        self.set_param_visible(in_data, params, Params::MapVectorMode, use_direction_map)?;
        self.set_param_visible(in_data, params, Params::MapInfluence, use_direction_map)?;
        self.set_param_visible(
            in_data,
            params,
            Params::SelectMode,
            !matches!(basis, SourceBasis::Color),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::ColorMode,
            matches!(basis, SourceBasis::Color),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::TargetColor,
            matches!(basis, SourceBasis::Color),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::MitchellB,
            matches!(interpolation, InterpolationMode::Mitchell),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::MitchellC,
            matches!(interpolation, InterpolationMode::Mitchell),
        )?;
        self.set_param_visible(in_data, params, Params::BlendMode, show_source)?;

        Self::set_param_name(
            params,
            Params::Threshold,
            match basis {
                SourceBasis::Alpha => "Alpha Threshold",
                SourceBasis::Lightness => "Lightness Threshold",
                SourceBasis::Color => "Color Tolerance",
            },
        )?;
        Ok(())
    }

    fn set_param_name(
        params: &mut ae::Parameters<Params>,
        id: Params,
        name: &str,
    ) -> Result<(), Error> {
        let mut p = params.get_mut(id)?;
        p.set_name(name)?;
        p.update_param_ui()?;
        Ok(())
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut ae::Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if in_data.is_premiere() {
            return Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible);
        }

        if let Some(plugin_id) = self.aegp_id {
            let effect = in_data.effect();
            if let Some(index) = params.index(id)
                && let Ok(effect_ref) = effect.aegp_effect(plugin_id)
                && let Ok(stream) = effect_ref.new_stream_by_index(plugin_id, index as i32)
            {
                return stream.set_dynamic_stream_flag(
                    ae::aegp::DynamicStreamFlags::Hidden,
                    false,
                    !visible,
                );
            }
        }

        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_ui_flag(
        params: &mut ae::Parameters<Params>,
        id: Params,
        flag: ae::pf::ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let current_status = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current_status == status {
            return Ok(());
        }

        let mut p = params.get_mut(id)?;
        p.set_ui_flag(flag, status);
        p.update_param_ui()?;
        Ok(())
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = in_layer.width();
        let height = in_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }

        let settings = read_settings(in_data, params)?;
        let source = capture_source(&in_layer);
        let masks = build_mask_map(&source, &settings);
        let has_extension_source = masks.iter().any(|&mask| mask > EPSILON);
        let source_image = SourceImage {
            pixels: &source,
            masks: &masks,
            width,
            height,
        };
        let map_checkout = if settings.use_direction_map {
            Some(params.checkout_at(Params::DirectionMapLayer, None, None, None)?)
        } else {
            None
        };
        let map_layer = match map_checkout.as_ref() {
            Some(checkout) => checkout.as_layer()?.value(),
            None => None,
        };
        let map_source = map_layer.as_ref().map(capture_source);
        let direction_map = match (map_layer.as_ref(), map_source.as_ref()) {
            (Some(layer), Some(pixels)) if layer.width() > 0 && layer.height() > 0 => {
                Some(DirectionMapData {
                    pixels,
                    width: layer.width(),
                    height: layer.height(),
                })
            }
            _ => None,
        };
        let progress_final = out_layer.height() as i32;
        let should_render_extension = has_extension_source
            && settings.opacity > EPSILON
            && settings.extend_count > 0
            && settings.step_size > EPSILON;

        out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
            let source_px = source[y as usize * width + x as usize];
            let extend_px = if should_render_extension {
                scale_pixel_opacity(
                    extend_pixel(
                        &source_image,
                        x as f32,
                        y as f32,
                        &settings,
                        direction_map.as_ref(),
                    ),
                    settings.opacity,
                )
            } else {
                transparent_pixel()
            };
            let out_px = if settings.show_source {
                blend_behind_source(source_px, extend_px, settings.blend_mode)
            } else {
                extend_px
            };

            write_output_pixel(&mut dst, sanitize_pixel(out_px));
            Ok(())
        })?;

        Ok(())
    }
}

fn read_settings(in_data: InData, params: &mut Parameters<Params>) -> Result<Settings, Error> {
    let direction_rad = (angle_degrees(in_data, params, Params::Direction)? - 90.0).to_radians();
    let extend_count = params.get(Params::ExtendCount)?.as_slider()?.value().max(0) as usize;
    let extend_direction =
        ExtendDirection::from_popup_value(params.get(Params::ExtendDirection)?.as_popup()?.value());
    let step_size = (params.get(Params::StepSize)?.as_float_slider()?.value() as f32).max(0.0);
    let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f32;
    let use_direction_map = params.get(Params::UseDirectionMap)?.as_checkbox()?.value();
    let map_channel =
        MapChannel::from_popup_value(params.get(Params::MapChannel)?.as_popup()?.value());
    let map_vector_mode =
        MapVectorMode::from_popup_value(params.get(Params::MapVectorMode)?.as_popup()?.value());
    let map_influence = ((params.get(Params::MapInfluence)?.as_float_slider()?.value() as f32)
        / 100.0)
        .clamp(0.0, 1.0);
    let decay = (params.get(Params::Decay)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let angle_change_rad = angle_degrees(in_data, params, Params::AngleChange)?.to_radians();
    let expansion_per_step = (params
        .get(Params::ExpansionPerStep)?
        .as_float_slider()?
        .value() as f32)
        .max(0.0);
    let expansion_samples = odd_sample_count(
        params
            .get(Params::ExpansionSamples)?
            .as_slider()?
            .value()
            .max(1) as usize,
    );
    let basis = SourceBasis::from_popup_value(params.get(Params::Basis)?.as_popup()?.value());
    let select_mode = if matches!(basis, SourceBasis::Color) {
        match params.get(Params::ColorMode)?.as_popup()?.value() {
            2 => SelectMode::Different,
            _ => SelectMode::Similar,
        }
    } else {
        SelectMode::from_popup_value(params.get(Params::SelectMode)?.as_popup()?.value())
    };
    let invert_detection = params.get(Params::InvertDetection)?.as_checkbox()?.value();
    let threshold =
        (params.get(Params::Threshold)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let softness =
        (params.get(Params::Softness)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let target_color = params
        .get(Params::TargetColor)?
        .as_color()?
        .value()
        .to_pixel32();
    let interpolation =
        InterpolationMode::from_popup_value(params.get(Params::Interpolation)?.as_popup()?.value());
    let mitchell_b =
        (params.get(Params::MitchellB)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let mitchell_c =
        (params.get(Params::MitchellC)?.as_float_slider()?.value() as f32).clamp(0.0, 1.0);
    let blend_mode =
        BlendMode::from_popup_value(params.get(Params::BlendMode)?.as_popup()?.value());
    let opacity =
        ((params.get(Params::Opacity)?.as_float_slider()?.value() as f32) / 100.0).clamp(0.0, 1.0);
    let show_source = params.get(Params::ShowSource)?.as_checkbox()?.value();

    Ok(Settings {
        direction_rad,
        extend_count,
        extend_direction,
        step_size,
        offset,
        use_direction_map,
        map_channel,
        map_vector_mode,
        map_influence,
        decay,
        angle_change_rad,
        expansion_per_step,
        expansion_samples,
        basis,
        select_mode,
        invert_detection,
        threshold,
        softness,
        target_color,
        interpolation,
        mitchell_b,
        mitchell_c,
        blend_mode,
        opacity,
        show_source,
    })
}

fn angle_degrees(
    _in_data: InData,
    params: &mut Parameters<Params>,
    id: Params,
) -> Result<f32, Error> {
    Ok(params.get(id)?.as_angle()?.float_value()? as f32)
}

fn extend_pixel(
    source: &SourceImage<'_>,
    x: f32,
    y: f32,
    settings: &Settings,
    direction_map: Option<&DirectionMapData<'_>>,
) -> PixelF32 {
    if settings.extend_count == 0 || settings.step_size <= EPSILON {
        return transparent_pixel();
    }

    let direction_rad =
        mapped_direction_rad(x, y, source.width, source.height, settings, direction_map);

    match settings.extend_direction {
        ExtendDirection::Forward => {
            extend_pixel_one_direction(source, x, y, settings, direction_rad, 1.0)
        }
        ExtendDirection::Backward => {
            extend_pixel_one_direction(source, x, y, settings, direction_rad, -1.0)
        }
        ExtendDirection::Both => alpha_over(
            extend_pixel_one_direction(source, x, y, settings, direction_rad, 1.0),
            extend_pixel_one_direction(source, x, y, settings, direction_rad, -1.0),
        ),
    }
}

fn extend_pixel_one_direction(
    source: &SourceImage<'_>,
    x: f32,
    y: f32,
    settings: &Settings,
    direction_rad: f32,
    direction_sign: f32,
) -> PixelF32 {
    let mut out = transparent_pixel();
    let base_angle = direction_rad
        + if direction_sign < 0.0 {
            std::f32::consts::PI
        } else {
            0.0
        };
    let mut path_x = base_angle.cos() * settings.offset;
    let mut path_y = base_angle.sin() * settings.offset;
    let mut falloff = 1.0f32;

    for step in 0..=settings.extend_count {
        let t = if settings.extend_count == 0 {
            0.0
        } else {
            step as f32 / settings.extend_count as f32
        };
        let sx = x - path_x;
        let sy = y - path_y;
        let expansion_radius = step as f32 * settings.expansion_per_step;
        let step_px = if expansion_radius <= EPSILON || settings.expansion_samples <= 1 {
            sample_extension_point(source, sx, sy, settings, falloff)
        } else {
            sample_expanded_extension(
                source,
                sx,
                sy,
                base_angle,
                expansion_radius,
                settings,
                falloff,
            )
        };

        if step_px.alpha > EPSILON {
            out = alpha_over(out, step_px);
            if out.alpha >= 0.999 {
                break;
            }
        }

        let angle = base_angle + settings.angle_change_rad * t * direction_sign;
        path_x += angle.cos() * settings.step_size;
        path_y += angle.sin() * settings.step_size;
        falloff *= settings.decay;
        if falloff <= EPSILON && settings.decay < 1.0 {
            break;
        }
    }

    out
}

fn sample_extension_point(
    source: &SourceImage<'_>,
    x: f32,
    y: f32,
    settings: &Settings,
    falloff: f32,
) -> PixelF32 {
    let mask = sample_mask(source.masks, source.width, source.height, x, y, settings);
    let weight = mask * falloff;
    if weight <= EPSILON {
        return transparent_pixel();
    }

    scale_pixel_opacity(
        sample_pixel(source.pixels, source.width, source.height, x, y, settings),
        weight,
    )
}

fn sample_expanded_extension(
    source: &SourceImage<'_>,
    x: f32,
    y: f32,
    angle: f32,
    radius: f32,
    settings: &Settings,
    falloff: f32,
) -> PixelF32 {
    let sample_count = settings.expansion_samples.max(1);
    let denom = sample_count.saturating_sub(1).max(1) as f32;
    let normal_x = -angle.sin();
    let normal_y = angle.cos();
    let mut out = transparent_pixel();

    for sample in 0..sample_count {
        let u = if sample_count == 1 {
            0.0
        } else {
            (sample as f32 / denom) * 2.0 - 1.0
        };
        let offset = u * radius;
        let edge_weight = 1.0 - u.abs() * 0.5;
        let px = sample_extension_point(
            source,
            x + normal_x * offset,
            y + normal_y * offset,
            settings,
            falloff * edge_weight,
        );
        out = alpha_over(out, px);
        if out.alpha >= 0.999 {
            break;
        }
    }

    out
}

fn odd_sample_count(count: usize) -> usize {
    let count = count.clamp(1, 65);
    if count.is_multiple_of(2) {
        (count + 1).min(65)
    } else {
        count
    }
}

fn mapped_direction_rad(
    x: f32,
    y: f32,
    out_width: usize,
    out_height: usize,
    settings: &Settings,
    direction_map: Option<&DirectionMapData<'_>>,
) -> f32 {
    if !settings.use_direction_map || settings.map_influence <= EPSILON {
        return settings.direction_rad;
    }

    let Some(map) = direction_map else {
        return settings.direction_rad;
    };

    let map_x = remap_axis_to_map(x, out_width, map.width);
    let map_y = remap_axis_to_map(y, out_height, map.height);
    let Some((map_dx, map_dy)) = direction_map_vector(map, map_x, map_y, settings) else {
        return settings.direction_rad;
    };

    let base_x = settings.direction_rad.cos();
    let base_y = settings.direction_rad.sin();
    let influence = settings.map_influence;
    let dir_x = base_x * (1.0 - influence) + map_dx * influence;
    let dir_y = base_y * (1.0 - influence) + map_dy * influence;
    if dir_x.abs() <= EPSILON && dir_y.abs() <= EPSILON {
        settings.direction_rad
    } else {
        dir_y.atan2(dir_x)
    }
}

fn direction_map_vector(
    map: &DirectionMapData<'_>,
    x: f32,
    y: f32,
    settings: &Settings,
) -> Option<(f32, f32)> {
    let left = sample_map_scalar(map, x - 1.0, y, settings.map_channel);
    let right = sample_map_scalar(map, x + 1.0, y, settings.map_channel);
    let up = sample_map_scalar(map, x, y - 1.0, settings.map_channel);
    let down = sample_map_scalar(map, x, y + 1.0, settings.map_channel);

    let dx = 0.5 * (right - left);
    let dy = 0.5 * (down - up);
    let (vx, vy) = match settings.map_vector_mode {
        MapVectorMode::Gradient => (dx, dy),
        MapVectorMode::Divergence => (-dx, -dy),
        MapVectorMode::Rotation => (-dy, dx),
    };
    let len = (vx * vx + vy * vy).sqrt();
    if !len.is_finite() || len <= EPSILON {
        None
    } else {
        Some((vx / len, vy / len))
    }
}

fn remap_axis_to_map(coord: f32, out_len: usize, map_len: usize) -> f32 {
    if out_len == 0 || map_len == 0 || out_len == map_len {
        coord
    } else {
        ((coord + 0.5) * map_len as f32 / out_len as f32) - 0.5
    }
}

fn sample_map_scalar(map: &DirectionMapData<'_>, x: f32, y: f32, channel: MapChannel) -> f32 {
    map_channel_value(sample_map_pixel(map, x, y), channel)
}

fn sample_map_pixel(map: &DirectionMapData<'_>, x: f32, y: f32) -> PixelF32 {
    if !x.is_finite() || !y.is_finite() {
        return transparent_pixel();
    }

    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let p00 = fetch_clamped(map.pixels, map.width, map.height, x0, y0);
    let p10 = fetch_clamped(map.pixels, map.width, map.height, x1, y0);
    let p01 = fetch_clamped(map.pixels, map.width, map.height, x0, y1);
    let p11 = fetch_clamped(map.pixels, map.width, map.height, x1, y1);

    lerp_pixel(lerp_pixel(p00, p10, tx), lerp_pixel(p01, p11, tx), ty)
}

fn map_channel_value(px: PixelF32, channel: MapChannel) -> f32 {
    let (r, g, b) = unpremultiplied_rgb(px);
    match channel {
        MapChannel::Red => r,
        MapChannel::Green => g,
        MapChannel::Blue => b,
        MapChannel::Alpha => sanitize_unit(px.alpha),
        MapChannel::Hue => rgb_to_hsv(r, g, b).0,
        MapChannel::Saturation => rgb_to_hsv(r, g, b).1,
        MapChannel::Value => rgb_to_hsv(r, g, b).2,
    }
}

fn build_mask_map(source: &[PixelF32], settings: &Settings) -> Vec<f32> {
    source.iter().map(|&px| source_mask(px, settings)).collect()
}

fn source_mask(px: PixelF32, settings: &Settings) -> f32 {
    let mask = match settings.basis {
        SourceBasis::Alpha => scalar_mask(px.alpha, settings),
        SourceBasis::Lightness => {
            let (r, g, b) = unpremultiplied_rgb(px);
            scalar_mask(0.2126 * r + 0.7152 * g + 0.0722 * b, settings)
        }
        SourceBasis::Color => color_mask(px, settings),
    };

    if settings.invert_detection {
        1.0 - mask
    } else {
        mask
    }
}

fn scalar_mask(value: f32, settings: &Settings) -> f32 {
    match settings.select_mode {
        SelectMode::Below => smooth_threshold(settings.threshold - value, settings.softness),
        _ => smooth_threshold(value - settings.threshold, settings.softness),
    }
}

fn color_mask(px: PixelF32, settings: &Settings) -> f32 {
    let (r, g, b) = unpremultiplied_rgb(px);
    let (tr, tg, tb) = unpremultiplied_rgb(settings.target_color);
    let dist = ((r - tr).powi(2) + (g - tg).powi(2) + (b - tb).powi(2)).sqrt() / 3.0_f32.sqrt();

    match settings.select_mode {
        SelectMode::Different => smooth_threshold(dist - settings.threshold, settings.softness),
        _ => smooth_threshold(settings.threshold - dist, settings.softness),
    }
}

fn smooth_threshold(distance: f32, softness: f32) -> f32 {
    if softness <= EPSILON {
        if distance >= 0.0 { 1.0 } else { 0.0 }
    } else {
        (distance / softness).clamp(0.0, 1.0)
    }
}

fn sample_pixel(
    source: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    settings: &Settings,
) -> PixelF32 {
    if !x.is_finite() || !y.is_finite() {
        return transparent_pixel();
    }

    match settings.interpolation {
        InterpolationMode::Nearest => sample_nearest(source, width, height, x, y),
        InterpolationMode::Bilinear => sample_bilinear(source, width, height, x, y),
        InterpolationMode::Bicubic => sample_bicubic(source, width, height, x, y),
        InterpolationMode::Mitchell => sample_mitchell(
            source,
            width,
            height,
            x,
            y,
            settings.mitchell_b,
            settings.mitchell_c,
        ),
    }
}

fn sample_mask(
    source: &[f32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    settings: &Settings,
) -> f32 {
    if !x.is_finite() || !y.is_finite() {
        return 0.0;
    }

    match settings.interpolation {
        InterpolationMode::Nearest => sample_mask_nearest(source, width, height, x, y),
        InterpolationMode::Bilinear => sample_mask_bilinear(source, width, height, x, y),
        InterpolationMode::Bicubic => sample_mask_bicubic(source, width, height, x, y),
        InterpolationMode::Mitchell => sample_mask_mitchell(
            source,
            width,
            height,
            x,
            y,
            settings.mitchell_b,
            settings.mitchell_c,
        ),
    }
    .clamp(0.0, 1.0)
}

fn sample_mask_nearest(source: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    fetch_mask_or_zero(
        source,
        width,
        height,
        x.round() as isize,
        y.round() as isize,
    )
}

fn sample_mask_bilinear(source: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = fetch_mask_or_zero(source, width, height, x0, y0);
    let p10 = fetch_mask_or_zero(source, width, height, x1, y0);
    let p01 = fetch_mask_or_zero(source, width, height, x0, y1);
    let p11 = fetch_mask_or_zero(source, width, height, x1, y1);
    let top = p00 + (p10 - p00) * tx;
    let bottom = p01 + (p11 - p01) * tx;
    top + (bottom - top) * ty
}

fn sample_mask_bicubic(source: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    sample_mask_separable(source, width, height, x, y, 2.0, |d| cubic_weight(d, -0.5))
}

fn sample_mask_mitchell(
    source: &[f32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    b: f32,
    c: f32,
) -> f32 {
    sample_mask_separable(source, width, height, x, y, 2.0, |d| {
        mitchell_weight(d, b, c)
    })
}

fn sample_mask_separable<F>(
    source: &[f32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    radius: f32,
    mut weight_fn: F,
) -> f32
where
    F: FnMut(f32) -> f32,
{
    let min_y = (y - radius).floor() as isize;
    let max_y = (y + radius).ceil() as isize;
    let min_x = (x - radius).floor() as isize;
    let max_x = (x + radius).ceil() as isize;
    let mut sum = 0.0;
    let mut weight_sum = 0.0;

    for sy in min_y..=max_y {
        let wy = weight_fn(y - sy as f32);
        if wy == 0.0 {
            continue;
        }

        for sx in min_x..=max_x {
            let wx = weight_fn(x - sx as f32);
            if wx == 0.0 {
                continue;
            }

            let weight = wx * wy;
            sum += fetch_mask_or_zero(source, width, height, sx, sy) * weight;
            weight_sum += weight;
        }
    }

    if weight_sum.abs() <= EPSILON {
        0.0
    } else {
        sum / weight_sum
    }
}

fn sample_nearest(source: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    fetch_or_transparent(
        source,
        width,
        height,
        x.round() as isize,
        y.round() as isize,
    )
}

fn sample_bilinear(source: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    let x0 = x.floor() as isize;
    let y0 = y.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = fetch_or_transparent(source, width, height, x0, y0);
    let p10 = fetch_or_transparent(source, width, height, x1, y0);
    let p01 = fetch_or_transparent(source, width, height, x0, y1);
    let p11 = fetch_or_transparent(source, width, height, x1, y1);

    lerp_pixel(lerp_pixel(p00, p10, tx), lerp_pixel(p01, p11, tx), ty)
}

fn sample_bicubic(source: &[PixelF32], width: usize, height: usize, x: f32, y: f32) -> PixelF32 {
    sample_separable(source, width, height, x, y, 2.0, |d| cubic_weight(d, -0.5))
}

fn sample_mitchell(
    source: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    b: f32,
    c: f32,
) -> PixelF32 {
    sample_separable(source, width, height, x, y, 2.0, |d| {
        mitchell_weight(d, b, c)
    })
}

fn sample_separable<F>(
    source: &[PixelF32],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
    radius: f32,
    mut weight_fn: F,
) -> PixelF32
where
    F: FnMut(f32) -> f32,
{
    let min_y = (y - radius).floor() as isize;
    let max_y = (y + radius).ceil() as isize;
    let min_x = (x - radius).floor() as isize;
    let max_x = (x + radius).ceil() as isize;
    let mut acc = WeightedPixel::default();

    for sy in min_y..=max_y {
        let wy = weight_fn(y - sy as f32);
        if wy == 0.0 {
            continue;
        }

        for sx in min_x..=max_x {
            let wx = weight_fn(x - sx as f32);
            if wx == 0.0 {
                continue;
            }

            acc.add(fetch_or_transparent(source, width, height, sx, sy), wx * wy);
        }
    }

    acc.finish()
}

fn cubic_weight(d: f32, a: f32) -> f32 {
    let x = d.abs();
    if x <= 1.0 {
        (a + 2.0) * x * x * x - (a + 3.0) * x * x + 1.0
    } else if x < 2.0 {
        a * x * x * x - 5.0 * a * x * x + 8.0 * a * x - 4.0 * a
    } else {
        0.0
    }
}

fn mitchell_weight(d: f32, b: f32, c: f32) -> f32 {
    let x = d.abs();

    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
            + (-18.0 + 12.0 * b + 6.0 * c) * x * x
            + (6.0 - 2.0 * b))
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x * x * x
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}

fn fetch_or_transparent(
    source: &[PixelF32],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
) -> PixelF32 {
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        return transparent_pixel();
    }
    source[y as usize * width + x as usize]
}

fn fetch_mask_or_zero(source: &[f32], width: usize, height: usize, x: isize, y: isize) -> f32 {
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        return 0.0;
    }
    source[y as usize * width + x as usize]
}

fn fetch_clamped(source: &[PixelF32], width: usize, height: usize, x: isize, y: isize) -> PixelF32 {
    if width == 0 || height == 0 {
        return transparent_pixel();
    }

    let x = x.clamp(0, width as isize - 1) as usize;
    let y = y.clamp(0, height as isize - 1) as usize;
    source[y * width + x]
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: a.alpha + (b.alpha - a.alpha) * t,
        red: a.red + (b.red - a.red) * t,
        green: a.green + (b.green - a.green) * t,
        blue: a.blue + (b.blue - a.blue) * t,
    }
}

fn alpha_over(top: PixelF32, bottom: PixelF32) -> PixelF32 {
    let top_a = clamp01(top.alpha);
    let bottom_a = clamp01(bottom.alpha);
    let out_a = top_a + bottom_a * (1.0 - top_a);
    if out_a <= EPSILON {
        return transparent_pixel();
    }

    PixelF32 {
        alpha: out_a,
        red: top.red + bottom.red * (1.0 - top_a),
        green: top.green + bottom.green * (1.0 - top_a),
        blue: top.blue + bottom.blue * (1.0 - top_a),
    }
}

fn blend_behind_source(source: PixelF32, extension: PixelF32, mode: BlendMode) -> PixelF32 {
    let blended_extension = blend_pixels(source, extension, mode);
    alpha_over(blended_extension, source)
}

fn blend_pixels(base: PixelF32, blend: PixelF32, mode: BlendMode) -> PixelF32 {
    let (base_r, base_g, base_b) = unpremultiplied_rgb(base);
    let (blend_r, blend_g, blend_b) = unpremultiplied_rgb(blend);
    let out_a = blend.alpha;
    let red = blend_channel(base_r, blend_r, mode);
    let green = blend_channel(base_g, blend_g, mode);
    let blue = blend_channel(base_b, blend_b, mode);

    PixelF32 {
        alpha: out_a,
        red: red * out_a,
        green: green * out_a,
        blue: blue * out_a,
    }
}

fn blend_channel(base: f32, blend: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => blend,
        BlendMode::Add => clamp01(base + blend),
        BlendMode::Multiply => base * blend,
        BlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - blend),
        BlendMode::Overlay => {
            if base <= 0.5 {
                2.0 * base * blend
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - blend)
            }
        }
        BlendMode::Darken => base.min(blend),
        BlendMode::Lighten => base.max(blend),
    }
}

fn scale_pixel_opacity(px: PixelF32, opacity: f32) -> PixelF32 {
    let opacity = clamp01(opacity);
    PixelF32 {
        alpha: px.alpha * opacity,
        red: px.red * opacity,
        green: px.green * opacity,
        blue: px.blue * opacity,
    }
}

fn capture_source(layer: &Layer) -> Vec<PixelF32> {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut source = vec![transparent_pixel(); width * height];

    for y in 0..height {
        for x in 0..width {
            source[y * width + x] = match world_type {
                ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
                ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
            };
        }
    }

    source
}

fn unpremultiplied_rgb(px: PixelF32) -> (f32, f32, f32) {
    if px.alpha > EPSILON {
        (
            sanitize_unit(px.red / px.alpha),
            sanitize_unit(px.green / px.alpha),
            sanitize_unit(px.blue / px.alpha),
        )
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let value = max;
    let saturation = if max <= EPSILON { 0.0 } else { delta / max };
    if delta <= EPSILON {
        return (0.0, saturation, value);
    }

    let hue_prime = if (max - r).abs() <= f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        ((b - r) / delta) + 2.0
    } else {
        ((r - g) / delta) + 4.0
    };

    ((hue_prime / 6.0).rem_euclid(1.0), saturation, value)
}

fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn sanitize_pixel(px: PixelF32) -> PixelF32 {
    PixelF32 {
        alpha: sanitize_unit(px.alpha),
        red: sanitize_unit(px.red),
        green: sanitize_unit(px.green),
        blue: sanitize_unit(px.blue),
    }
}

fn sanitize_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn write_output_pixel(dst: &mut GenericPixelMut<'_>, px: PixelF32) {
    match dst {
        GenericPixelMut::Pixel8(p) => {
            **p = px.to_pixel8();
        }
        GenericPixelMut::Pixel16(p) => {
            **p = px.to_pixel16();
        }
        GenericPixelMut::PixelF32(p) => {
            **p = px;
        }
        GenericPixelMut::PixelF64(p) => {
            p.alphaF = px.alpha as _;
            p.redF = px.red as _;
            p.greenF = px.green as _;
            p.blueF = px.blue as _;
        }
    }
}
