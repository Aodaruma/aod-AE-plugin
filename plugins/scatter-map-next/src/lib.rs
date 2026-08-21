#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::ToPixel;

const MAX_GATHER_ATTEMPTS: u32 = 24;
const MAX_SWAP_ATTEMPTS: u32 = 64;
const MAX_DISK_REJECTIONS: u32 = 32;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ScatterMode,
    Amount,
    Radius,
    GrainSize,
    GatherSamples,
    MapGroupStart,
    UseAmountMap,
    AmountMapLayer,
    AmountMapChannel,
    UseRadiusMap,
    RadiusMapLayer,
    RadiusMapChannel,
    MapGroupEnd,
    OutputGroupStart,
    Seed,
    TemporalMode,
    EdgeMode,
    BlendMode,
    BlendOpacity,
    PreserveAlpha,
    Clamp32,
    OutputGroupEnd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScatterMode {
    Gather,
    Swap,
}

#[derive(Clone, Copy)]
enum MapChannel {
    Luma,
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Clone, Copy)]
enum TemporalMode {
    Static,
    Frame,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeMode {
    Reject,
    Clamp,
    Tile,
    Mirror,
    Transparent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputBlendMode {
    None,
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
}

#[derive(Clone, Copy)]
struct RenderSettings {
    scatter_mode: ScatterMode,
    amount: f32,
    radius: i32,
    grain_size: usize,
    gather_samples: u32,
    use_amount_map: bool,
    amount_map_channel: MapChannel,
    use_radius_map: bool,
    radius_map_channel: MapChannel,
    seed: u32,
    temporal_mode: TemporalMode,
    edge_mode: EdgeMode,
    blend_mode: OutputBlendMode,
    blend_opacity: f32,
    preserve_alpha: bool,
    clamp_32: bool,
}

#[derive(Clone, Debug)]
struct LayerBuffer {
    width: usize,
    height: usize,
    pixels: Vec<PixelF32>,
}

#[derive(Clone, Copy)]
struct RenderMaps<'a> {
    amount: Option<&'a LayerBuffer>,
    radius: Option<&'a LayerBuffer>,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    grid_x: usize,
    grid_y: usize,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Applies map-driven gather and swap scatter to layers.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_with_flags(
            Params::ScatterMode,
            "Scatter Mode",
            PopupDef::setup(|d| {
                d.set_options(&["Gather", "Swap"]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;
        params.add(
            Params::Amount,
            "Amount (%)",
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
            Params::Radius,
            "Radius (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(4096);
                d.set_slider_min(0);
                d.set_slider_max(256);
                d.set_default(8);
            }),
        )?;
        params.add(
            Params::GrainSize,
            "Grain Size (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(1024);
                d.set_slider_min(1);
                d.set_slider_max(128);
                d.set_default(1);
            }),
        )?;
        params.add(
            Params::GatherSamples,
            "Gather Samples",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(32);
                d.set_slider_min(1);
                d.set_slider_max(16);
                d.set_default(1);
            }),
        )?;

        params.add_group(
            Params::MapGroupStart,
            Params::MapGroupEnd,
            "Maps",
            true,
            |params| {
                params.add_with_flags(
                    Params::UseAmountMap,
                    "Map Amount",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::AmountMapLayer,
                    "Amount Map Layer (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::AmountMapChannel,
                    "Amount Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::UseRadiusMap,
                    "Map Radius",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::RadiusMapLayer,
                    "Radius Map Layer (None = Input)",
                    LayerDef::new(),
                )?;
                params.add(
                    Params::RadiusMapChannel,
                    "Radius Map Channel",
                    PopupDef::setup(|d| {
                        d.set_options(&["Luma", "Red", "Green", "Blue", "Alpha"]);
                        d.set_default(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::OutputGroupStart,
            Params::OutputGroupEnd,
            "Output",
            true,
            |params| {
                params.add(
                    Params::Seed,
                    "Seed",
                    SliderDef::setup(|d| {
                        d.set_valid_min(0);
                        d.set_valid_max(100000);
                        d.set_slider_min(0);
                        d.set_slider_max(10000);
                        d.set_default(0);
                    }),
                )?;
                params.add(
                    Params::TemporalMode,
                    "Temporal Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&["Static", "Randomize Every Frame"]);
                        d.set_default(1);
                    }),
                )?;
                params.add(
                    Params::EdgeMode,
                    "Edge Mode",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Reject and Retry",
                            "Clamp",
                            "Tile",
                            "Mirror",
                            "Transparent",
                        ]);
                        d.set_default(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::BlendMode,
                    "Blend With Original",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "None",
                            "Normal",
                            "Add",
                            "Multiply",
                            "Screen",
                            "Overlay",
                            "Difference",
                        ]);
                        d.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::BlendOpacity,
                    "Blend Opacity (%)",
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
                    "Preserve Original Alpha",
                    CheckBoxDef::setup(|d| {
                        d.set_default(false);
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
                        "AOD_ScatterMapNext - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
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
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ScatterMapNext")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => self.do_render(in_data, in_layer, out_data, out_layer, params)?,
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
                    self.do_render(in_data, in_layer, out_data, out_layer, params)?;
                }

                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index }
                if param_affects_ui(params.type_at(param_index)) =>
            {
                out_data.set_out_flag(OutFlags::RefreshUi, true);
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
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let mode = scatter_mode_from_popup(params.get(Params::ScatterMode)?.as_popup()?.value());
        let gather = matches!(mode, ScatterMode::Gather);
        self.set_param_visible(in_data, params, Params::GatherSamples, gather)?;
        self.set_param_visible(in_data, params, Params::EdgeMode, gather)?;

        let use_amount_map = params.get(Params::UseAmountMap)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::AmountMapLayer, use_amount_map)?;
        self.set_param_visible(in_data, params, Params::AmountMapChannel, use_amount_map)?;
        let use_radius_map = params.get(Params::UseRadiusMap)?.as_checkbox()?.value();
        self.set_param_visible(in_data, params, Params::RadiusMapLayer, use_radius_map)?;
        self.set_param_visible(in_data, params, Params::RadiusMapChannel, use_radius_map)?;

        let blend_mode =
            output_blend_mode_from_popup(params.get(Params::BlendMode)?.as_popup()?.value());
        self.set_param_visible(
            in_data,
            params,
            Params::BlendOpacity,
            !matches!(blend_mode, OutputBlendMode::None),
        )?;
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
        let flag_bits = flag.bits();
        let current_status = (params.get(id)?.ui_flags().bits() & flag_bits) != 0;
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
        _out_data: OutData,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let out_w = out_layer.width();
        let out_h = out_layer.height();
        if out_w == 0 || out_h == 0 || in_layer.width() == 0 || in_layer.height() == 0 {
            return Ok(());
        }

        let settings = read_render_settings(params)?;
        let source = read_layer_buffer(&in_layer);
        let working_source = resample_nearest_buffer(&source, out_w, out_h);
        let amount_map_layer =
            checkout_layer_buffer(params, Params::AmountMapLayer, settings.use_amount_map)?;
        let radius_map_layer =
            checkout_layer_buffer(params, Params::RadiusMapLayer, settings.use_radius_map)?;
        let maps = RenderMaps {
            amount: map_buffer_ref(
                amount_map_layer.as_ref(),
                &working_source,
                settings.use_amount_map,
            ),
            radius: map_buffer_ref(
                radius_map_layer.as_ref(),
                &working_source,
                settings.use_radius_map,
            ),
        };

        let frame = in_data.current_frame() as i32;
        let render_seed = temporal_seed(settings.seed, settings.temporal_mode, frame);
        let mut rendered = render_scatter(&working_source, maps, &settings, render_seed);
        let out_world_type = out_layer.world_type();
        let out_is_f32 = matches!(
            out_world_type,
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None
        );
        for (index, px) in rendered.iter_mut().enumerate() {
            let original = working_source.pixels[index];
            if settings.preserve_alpha {
                px.alpha = original.alpha;
            }
            *px = blend_with_original(*px, original, settings.blend_mode, settings.blend_opacity);
            *px = sanitize_pixel_for_output(*px, out_is_f32, settings.clamp_32);
        }

        out_layer.iterate(0, out_h as i32, None, |x, y, mut dst| {
            let px = rendered[y as usize * out_w + x as usize];
            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(px.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(px.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(px),
            }
            Ok(())
        })?;
        Ok(())
    }
}

fn param_affects_ui(param: Params) -> bool {
    matches!(
        param,
        Params::ScatterMode | Params::UseAmountMap | Params::UseRadiusMap | Params::BlendMode
    )
}

fn read_render_settings(params: &mut Parameters<Params>) -> Result<RenderSettings, Error> {
    Ok(RenderSettings {
        scatter_mode: scatter_mode_from_popup(params.get(Params::ScatterMode)?.as_popup()?.value()),
        amount: (params.get(Params::Amount)?.as_float_slider()?.value() as f32 / 100.0)
            .clamp(0.0, 1.0),
        radius: params
            .get(Params::Radius)?
            .as_slider()?
            .value()
            .clamp(0, 4096),
        grain_size: params
            .get(Params::GrainSize)?
            .as_slider()?
            .value()
            .clamp(1, 1024) as usize,
        gather_samples: params
            .get(Params::GatherSamples)?
            .as_slider()?
            .value()
            .clamp(1, 32) as u32,
        use_amount_map: params.get(Params::UseAmountMap)?.as_checkbox()?.value(),
        amount_map_channel: map_channel_from_popup(
            params.get(Params::AmountMapChannel)?.as_popup()?.value(),
        ),
        use_radius_map: params.get(Params::UseRadiusMap)?.as_checkbox()?.value(),
        radius_map_channel: map_channel_from_popup(
            params.get(Params::RadiusMapChannel)?.as_popup()?.value(),
        ),
        seed: params.get(Params::Seed)?.as_slider()?.value() as u32,
        temporal_mode: temporal_mode_from_popup(
            params.get(Params::TemporalMode)?.as_popup()?.value(),
        ),
        edge_mode: edge_mode_from_popup(params.get(Params::EdgeMode)?.as_popup()?.value()),
        blend_mode: output_blend_mode_from_popup(
            params.get(Params::BlendMode)?.as_popup()?.value(),
        ),
        blend_opacity: (params.get(Params::BlendOpacity)?.as_float_slider()?.value() as f32
            / 100.0)
            .clamp(0.0, 1.0),
        preserve_alpha: params.get(Params::PreserveAlpha)?.as_checkbox()?.value(),
        clamp_32: params.get(Params::Clamp32)?.as_checkbox()?.value(),
    })
}

fn scatter_mode_from_popup(value: i32) -> ScatterMode {
    match value {
        2 => ScatterMode::Swap,
        _ => ScatterMode::Gather,
    }
}

fn map_channel_from_popup(value: i32) -> MapChannel {
    match value {
        2 => MapChannel::Red,
        3 => MapChannel::Green,
        4 => MapChannel::Blue,
        5 => MapChannel::Alpha,
        _ => MapChannel::Luma,
    }
}

fn temporal_mode_from_popup(value: i32) -> TemporalMode {
    match value {
        2 => TemporalMode::Frame,
        _ => TemporalMode::Static,
    }
}

fn edge_mode_from_popup(value: i32) -> EdgeMode {
    match value {
        2 => EdgeMode::Clamp,
        3 => EdgeMode::Tile,
        4 => EdgeMode::Mirror,
        5 => EdgeMode::Transparent,
        _ => EdgeMode::Reject,
    }
}

fn output_blend_mode_from_popup(value: i32) -> OutputBlendMode {
    match value {
        2 => OutputBlendMode::Normal,
        3 => OutputBlendMode::Add,
        4 => OutputBlendMode::Multiply,
        5 => OutputBlendMode::Screen,
        6 => OutputBlendMode::Overlay,
        7 => OutputBlendMode::Difference,
        _ => OutputBlendMode::None,
    }
}

fn checkout_layer_buffer(
    params: &mut Parameters<Params>,
    id: Params,
    enabled: bool,
) -> Result<Option<LayerBuffer>, Error> {
    if !enabled {
        return Ok(None);
    }
    let checkout = params.checkout_at(id, None, None, None)?;
    let layer = checkout.as_layer()?.value();
    Ok(layer.as_ref().map(read_layer_buffer))
}

fn map_buffer_ref<'a>(
    layer_map: Option<&'a LayerBuffer>,
    input_layer: &'a LayerBuffer,
    enabled: bool,
) -> Option<&'a LayerBuffer> {
    if enabled {
        layer_map.or(Some(input_layer))
    } else {
        None
    }
}

fn read_layer_buffer(layer: &Layer) -> LayerBuffer {
    let width = layer.width();
    let height = layer.height();
    let world_type = layer.world_type();
    let mut pixels = vec![transparent_pixel(); width * height];
    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = match world_type {
                ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
                ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
            };
        }
    }
    LayerBuffer {
        width,
        height,
        pixels,
    }
}

fn resample_nearest_buffer(source: &LayerBuffer, width: usize, height: usize) -> LayerBuffer {
    if source.width == width && source.height == height {
        return source.clone();
    }
    let mut pixels = vec![transparent_pixel(); width * height];
    for y in 0..height {
        for x in 0..width {
            let source_x = remap_coord_nearest(x, width, source.width);
            let source_y = remap_coord_nearest(y, height, source.height);
            pixels[y * width + x] = source.pixels[source_y * source.width + source_x];
        }
    }
    LayerBuffer {
        width,
        height,
        pixels,
    }
}

fn remap_coord_nearest(coord: usize, out_len: usize, source_len: usize) -> usize {
    if out_len == 0 || source_len <= 1 {
        return 0;
    }
    let mapped = ((coord as f32 + 0.5) * source_len as f32 / out_len as f32 - 0.5).round();
    (mapped as i64).clamp(0, source_len as i64 - 1) as usize
}

fn render_scatter(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    match settings.scatter_mode {
        ScatterMode::Gather => render_gather(source, maps, settings, seed),
        ScatterMode::Swap => render_swap(source, maps, settings, seed),
    }
}

fn render_gather(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    let mut output = source.pixels.clone();
    if settings.amount <= 0.0 || settings.radius <= 0 {
        return output;
    }

    let grain = settings.grain_size.max(1);
    for y in 0..source.height {
        for x in 0..source.width {
            let index = y * source.width + x;
            let amount = settings.amount
                * map_value(
                    maps.amount,
                    x,
                    y,
                    source.width,
                    source.height,
                    settings.amount_map_channel,
                    1.0,
                );
            let cell_x = x / grain;
            let cell_y = y / grain;
            let event = rand01(hash_coords(seed, cell_x as i32, cell_y as i32, 0, 0xA1));
            if event >= amount.clamp(0.0, 1.0) {
                continue;
            }

            let radius_factor = map_value(
                maps.radius,
                x,
                y,
                source.width,
                source.height,
                settings.radius_map_channel,
                1.0,
            );
            let radius = ((settings.radius as f32) * radius_factor.clamp(0.0, 1.0)).floor() as i32;
            if radius <= 0 {
                continue;
            }

            let mut sum = transparent_pixel();
            for tap in 0..settings.gather_samples {
                let sampled = gather_sample(
                    source,
                    x,
                    y,
                    cell_x as i32,
                    cell_y as i32,
                    radius,
                    tap,
                    seed,
                    settings.edge_mode,
                );
                sum.alpha += sampled.alpha;
                sum.red += sampled.red;
                sum.green += sampled.green;
                sum.blue += sampled.blue;
            }
            let inv = (settings.gather_samples as f32).recip();
            output[index] = PixelF32 {
                alpha: sum.alpha * inv,
                red: sum.red * inv,
                green: sum.green * inv,
                blue: sum.blue * inv,
            };
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn gather_sample(
    source: &LayerBuffer,
    x: usize,
    y: usize,
    cell_x: i32,
    cell_y: i32,
    radius: i32,
    tap: u32,
    seed: u32,
    edge_mode: EdgeMode,
) -> PixelF32 {
    for attempt in 0..MAX_GATHER_ATTEMPTS {
        let (dx, dy) = discrete_disk_offset(cell_x, cell_y, tap, attempt, radius, seed);
        let sample_x = x as i32 + dx;
        let sample_y = y as i32 + dy;
        if matches!(edge_mode, EdgeMode::Reject) {
            if let (Some(sample_x), Some(sample_y)) = (
                in_bounds_coord(sample_x, source.width),
                in_bounds_coord(sample_y, source.height),
            ) {
                return source.pixels[sample_y * source.width + sample_x];
            }
            continue;
        }
        let sample_x = resolve_coord(sample_x, source.width, edge_mode);
        let sample_y = resolve_coord(sample_y, source.height, edge_mode);
        if let (Some(sample_x), Some(sample_y)) = (sample_x, sample_y) {
            return source.pixels[sample_y * source.width + sample_x];
        }
        return transparent_pixel();
    }
    source.pixels[y * source.width + x]
}

fn render_swap(
    source: &LayerBuffer,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<PixelF32> {
    build_swap_permutation(source.width, source.height, maps, settings, seed)
        .into_iter()
        .map(|source_index| source.pixels[source_index])
        .collect()
}

fn build_swap_permutation(
    width: usize,
    height: usize,
    maps: RenderMaps<'_>,
    settings: &RenderSettings,
    seed: u32,
) -> Vec<usize> {
    let mut permutation: Vec<usize> = (0..width * height).collect();
    if width == 0 || height == 0 || settings.amount <= 0.0 || settings.radius <= 0 {
        return permutation;
    }

    let grain = settings.grain_size.max(1);
    let columns = width.div_ceil(grain);
    let rows = height.div_ceil(grain);
    let mut blocks = Vec::with_capacity(columns * rows);
    for grid_y in 0..rows {
        for grid_x in 0..columns {
            let x = grid_x * grain;
            let y = grid_y * grain;
            blocks.push(Block {
                x,
                y,
                width: grain.min(width - x),
                height: grain.min(height - y),
                grid_x,
                grid_y,
            });
        }
    }

    let mut active = vec![false; blocks.len()];
    let mut radii = vec![0_i32; blocks.len()];
    for (index, block) in blocks.iter().enumerate() {
        let center_x = (block.x + block.width / 2).min(width - 1);
        let center_y = (block.y + block.height / 2).min(height - 1);
        let amount = settings.amount
            * map_value(
                maps.amount,
                center_x,
                center_y,
                width,
                height,
                settings.amount_map_channel,
                1.0,
            );
        active[index] = rand01(hash_coords(
            seed ^ 0x6C8E_9CF5,
            block.grid_x as i32,
            block.grid_y as i32,
            0,
            0xB1,
        )) < amount.clamp(0.0, 1.0);
        let radius_factor = map_value(
            maps.radius,
            center_x,
            center_y,
            width,
            height,
            settings.radius_map_channel,
            1.0,
        );
        radii[index] = ((settings.radius as f32) * radius_factor.clamp(0.0, 1.0)).floor() as i32;
    }

    let mut order: Vec<usize> = (0..blocks.len()).collect();
    shuffle_indices(&mut order, seed ^ 0xD1B5_4A35);
    let mut used = vec![false; blocks.len()];
    for block_index in order {
        if used[block_index] || !active[block_index] || radii[block_index] <= 0 {
            continue;
        }
        let block = blocks[block_index];
        let cell_radius = (radii[block_index] as usize).div_ceil(grain).max(1) as i32;
        let mut partner = None;
        for attempt in 0..MAX_SWAP_ATTEMPTS {
            let (offset_x, offset_y) = discrete_disk_offset(
                block.grid_x as i32,
                block.grid_y as i32,
                0,
                attempt,
                cell_radius,
                seed ^ 0x94D0_49BB,
            );
            let candidate_x = block.grid_x as i32 + offset_x;
            let candidate_y = block.grid_y as i32 + offset_y;
            if candidate_x < 0
                || candidate_y < 0
                || candidate_x >= columns as i32
                || candidate_y >= rows as i32
            {
                continue;
            }
            let candidate_index = candidate_y as usize * columns + candidate_x as usize;
            let candidate = blocks[candidate_index];
            if candidate_index == block_index
                || used[candidate_index]
                || !active[candidate_index]
                || candidate.width != block.width
                || candidate.height != block.height
            {
                continue;
            }
            let dx = candidate.x as i64 - block.x as i64;
            let dy = candidate.y as i64 - block.y as i64;
            let allowed_radius = radii[block_index].min(radii[candidate_index]) as i64;
            if dx * dx + dy * dy > allowed_radius * allowed_radius {
                continue;
            }
            partner = Some(candidate_index);
            break;
        }
        if let Some(partner_index) = partner {
            swap_blocks(&mut permutation, width, block, blocks[partner_index]);
            used[block_index] = true;
            used[partner_index] = true;
        }
    }
    permutation
}

fn swap_blocks(permutation: &mut [usize], image_width: usize, a: Block, b: Block) {
    for y in 0..a.height {
        for x in 0..a.width {
            let destination_a = (a.y + y) * image_width + a.x + x;
            let destination_b = (b.y + y) * image_width + b.x + x;
            permutation[destination_a] = destination_b;
            permutation[destination_b] = destination_a;
        }
    }
}

fn discrete_disk_offset(
    cell_x: i32,
    cell_y: i32,
    tap: u32,
    attempt: u32,
    radius: i32,
    seed: u32,
) -> (i32, i32) {
    if radius <= 0 {
        return (0, 0);
    }
    let span = (radius * 2 + 1) as u32;
    for rejection in 0..MAX_DISK_REJECTIONS {
        let channel = attempt
            .wrapping_mul(MAX_DISK_REJECTIONS)
            .wrapping_add(rejection)
            .wrapping_mul(2);
        let x_hash = hash_coords(seed, cell_x, cell_y, tap as i32, channel);
        let y_hash = hash_coords(seed, cell_x, cell_y, tap as i32, channel + 1);
        let dx = (x_hash % span) as i32 - radius;
        let dy = (y_hash % span) as i32 - radius;
        let distance_squared = dx * dx + dy * dy;
        if distance_squared > 0 && distance_squared <= radius * radius {
            return (dx, dy);
        }
    }

    match hash_coords(seed ^ 0x7F4A_7C15, cell_x, cell_y, tap as i32, attempt) % 4 {
        0 => (1, 0),
        1 => (-1, 0),
        2 => (0, 1),
        _ => (0, -1),
    }
}

fn shuffle_indices(values: &mut [usize], seed: u32) {
    let mut state = hash_u32(seed ^ values.len() as u32);
    for i in (1..values.len()).rev() {
        state = hash_u32(state.wrapping_add(i as u32).wrapping_add(0x9E37_79B9));
        let j = state as usize % (i + 1);
        values.swap(i, j);
    }
}

fn map_value(
    map: Option<&LayerBuffer>,
    x: usize,
    y: usize,
    out_width: usize,
    out_height: usize,
    channel: MapChannel,
    default_value: f32,
) -> f32 {
    let Some(map) = map else {
        return default_value;
    };
    if map.width == 0 || map.height == 0 {
        return default_value;
    }
    let map_x = remap_coord_float(x, out_width, map.width);
    let map_y = remap_coord_float(y, out_height, map.height);
    scalar_from_pixel(sample_map_bilinear(map, map_x, map_y), channel).clamp(0.0, 1.0)
}

fn remap_coord_float(coord: usize, out_len: usize, source_len: usize) -> f32 {
    if out_len == 0 || source_len == 0 {
        return 0.0;
    }
    (coord as f32 + 0.5) * source_len as f32 / out_len as f32 - 0.5
}

fn sample_map_bilinear(map: &LayerBuffer, x: f32, y: f32) -> PixelF32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let sample = |sample_x: i32, sample_y: i32| {
        let sample_x = sample_x.clamp(0, map.width as i32 - 1) as usize;
        let sample_y = sample_y.clamp(0, map.height as i32 - 1) as usize;
        map.pixels[sample_y * map.width + sample_x]
    };
    let top = lerp_pixel(sample(x0, y0), sample(x1, y0), tx);
    let bottom = lerp_pixel(sample(x0, y1), sample(x1, y1), tx);
    lerp_pixel(top, bottom, ty)
}

fn lerp_pixel(a: PixelF32, b: PixelF32, t: f32) -> PixelF32 {
    PixelF32 {
        alpha: lerp(a.alpha, b.alpha, t),
        red: lerp(a.red, b.red, t),
        green: lerp(a.green, b.green, t),
        blue: lerp(a.blue, b.blue, t),
    }
}

fn scalar_from_pixel(px: PixelF32, channel: MapChannel) -> f32 {
    match channel {
        MapChannel::Luma => {
            0.2126 * sanitize_non_finite(px.red)
                + 0.7152 * sanitize_non_finite(px.green)
                + 0.0722 * sanitize_non_finite(px.blue)
        }
        MapChannel::Red => sanitize_non_finite(px.red),
        MapChannel::Green => sanitize_non_finite(px.green),
        MapChannel::Blue => sanitize_non_finite(px.blue),
        MapChannel::Alpha => sanitize_non_finite(px.alpha),
    }
}

fn in_bounds_coord(coord: i32, len: usize) -> Option<usize> {
    if coord >= 0 && coord < len as i32 {
        Some(coord as usize)
    } else {
        None
    }
}

fn resolve_coord(coord: i32, len: usize, edge_mode: EdgeMode) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let len = len as i32;
    match edge_mode {
        EdgeMode::Reject | EdgeMode::Transparent => in_bounds_coord(coord, len as usize),
        EdgeMode::Clamp => Some(coord.clamp(0, len - 1) as usize),
        EdgeMode::Tile => Some(coord.rem_euclid(len) as usize),
        EdgeMode::Mirror => Some(mirror_index(coord, len) as usize),
    }
}

fn mirror_index(coord: i32, len: i32) -> i32 {
    if len <= 1 {
        return 0;
    }
    let period = 2 * len - 2;
    let value = coord.rem_euclid(period);
    if value < len { value } else { period - value }
}

fn temporal_seed(seed: u32, mode: TemporalMode, frame: i32) -> u32 {
    match mode {
        TemporalMode::Static => seed,
        TemporalMode::Frame => hash_u32(seed ^ (frame as u32).wrapping_mul(0x9E37_79B9)),
    }
}

fn hash_coords(seed: u32, x: i32, y: i32, z: i32, channel: u32) -> u32 {
    let mut hash = seed ^ 0xA511_E9B3;
    hash = hash.wrapping_add((x as u32).wrapping_mul(0x85EB_CA6B));
    hash = hash.wrapping_add((y as u32).wrapping_mul(0xC2B2_AE35));
    hash = hash.wrapping_add((z as u32).wrapping_mul(0x27D4_EB2D));
    hash = hash.wrapping_add(channel.wrapping_mul(0x1656_67B1));
    hash_u32(hash)
}

fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    value
}

fn rand01(value: u32) -> f32 {
    value as f32 / u32::MAX as f32
}

fn blend_with_original(
    scatter: PixelF32,
    original: PixelF32,
    mode: OutputBlendMode,
    opacity: f32,
) -> PixelF32 {
    if matches!(mode, OutputBlendMode::None) {
        return scatter;
    }
    let opacity = opacity.clamp(0.0, 1.0);
    let red = blend_channel(original.red, scatter.red, mode);
    let green = blend_channel(original.green, scatter.green, mode);
    let blue = blend_channel(original.blue, scatter.blue, mode);
    PixelF32 {
        alpha: lerp(original.alpha, scatter.alpha, opacity),
        red: lerp(original.red, red, opacity),
        green: lerp(original.green, green, opacity),
        blue: lerp(original.blue, blue, opacity),
    }
}

fn blend_channel(base: f32, source: f32, mode: OutputBlendMode) -> f32 {
    match mode {
        OutputBlendMode::None | OutputBlendMode::Normal => source,
        OutputBlendMode::Add => base + source,
        OutputBlendMode::Multiply => base * source,
        OutputBlendMode::Screen => 1.0 - (1.0 - base) * (1.0 - source),
        OutputBlendMode::Overlay => {
            if base <= 0.5 {
                2.0 * base * source
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - source)
            }
        }
        OutputBlendMode::Difference => (base - source).abs(),
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn transparent_pixel() -> PixelF32 {
    PixelF32 {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    }
}

fn sanitize_pixel_for_output(mut px: PixelF32, out_is_f32: bool, clamp_32: bool) -> PixelF32 {
    px.alpha = sanitize_non_finite(px.alpha);
    px.red = sanitize_non_finite(px.red);
    px.green = sanitize_non_finite(px.green);
    px.blue = sanitize_non_finite(px.blue);
    if !out_is_f32 || clamp_32 {
        px.alpha = px.alpha.clamp(0.0, 1.0);
        px.red = px.red.clamp(0.0, 1.0);
        px.green = px.green.clamp(0.0, 1.0);
        px.blue = px.blue.clamp(0.0, 1.0);
    }
    px
}

fn sanitize_non_finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(value: f32) -> PixelF32 {
        PixelF32 {
            alpha: 1.0,
            red: value,
            green: value * 0.5,
            blue: 1.0 - value,
        }
    }

    fn source(width: usize, height: usize) -> LayerBuffer {
        let count = width * height;
        LayerBuffer {
            width,
            height,
            pixels: (0..count)
                .map(|index| pixel((index + 1) as f32 / (count + 1) as f32))
                .collect(),
        }
    }

    fn settings(mode: ScatterMode) -> RenderSettings {
        RenderSettings {
            scatter_mode: mode,
            amount: 1.0,
            radius: 4,
            grain_size: 1,
            gather_samples: 1,
            use_amount_map: false,
            amount_map_channel: MapChannel::Luma,
            use_radius_map: false,
            radius_map_channel: MapChannel::Luma,
            seed: 42,
            temporal_mode: TemporalMode::Static,
            edge_mode: EdgeMode::Reject,
            blend_mode: OutputBlendMode::None,
            blend_opacity: 1.0,
            preserve_alpha: false,
            clamp_32: false,
        }
    }

    fn no_maps() -> RenderMaps<'static> {
        RenderMaps {
            amount: None,
            radius: None,
        }
    }

    #[test]
    fn gather_single_sample_only_copies_existing_pixels() {
        let source = source(8, 8);
        let rendered = render_gather(&source, no_maps(), &settings(ScatterMode::Gather), 42);
        for output in rendered {
            assert!(source.pixels.iter().any(|input| {
                input.alpha.to_bits() == output.alpha.to_bits()
                    && input.red.to_bits() == output.red.to_bits()
                    && input.green.to_bits() == output.green.to_bits()
                    && input.blue.to_bits() == output.blue.to_bits()
            }));
        }
    }

    #[test]
    fn gather_multiple_samples_can_create_mixed_pixels() {
        let source = source(8, 8);
        let mut settings = settings(ScatterMode::Gather);
        settings.gather_samples = 4;
        let rendered = render_gather(&source, no_maps(), &settings, 42);
        assert!(rendered.iter().any(|output| {
            !source
                .pixels
                .iter()
                .any(|input| input.red.to_bits() == output.red.to_bits())
        }));
    }

    #[test]
    fn zero_amount_is_identity() {
        let source = source(8, 8);
        let mut settings = settings(ScatterMode::Gather);
        settings.amount = 0.0;
        let rendered = render_gather(&source, no_maps(), &settings, 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn black_amount_map_is_identity() {
        let source = source(8, 8);
        let black_map = LayerBuffer {
            width: 8,
            height: 8,
            pixels: vec![transparent_pixel(); 64],
        };
        let maps = RenderMaps {
            amount: Some(&black_map),
            radius: None,
        };
        let rendered = render_gather(&source, maps, &settings(ScatterMode::Gather), 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn swap_is_a_strict_permutation() {
        let source = source(16, 16);
        let settings = settings(ScatterMode::Swap);
        let rendered = render_swap(&source, no_maps(), &settings, 42);
        let mut input_values: Vec<u32> = source.pixels.iter().map(|px| px.red.to_bits()).collect();
        let mut output_values: Vec<u32> = rendered.iter().map(|px| px.red.to_bits()).collect();
        input_values.sort_unstable();
        output_values.sort_unstable();
        assert_eq!(input_values, output_values);
        assert!(
            source
                .pixels
                .iter()
                .zip(rendered.iter())
                .any(|(input, output)| input.red.to_bits() != output.red.to_bits())
        );
    }

    #[test]
    fn swap_respects_radius() {
        let source = source(16, 16);
        let settings = settings(ScatterMode::Swap);
        let permutation =
            build_swap_permutation(source.width, source.height, no_maps(), &settings, 42);
        for (destination, source_index) in permutation.into_iter().enumerate() {
            let destination_x = destination % source.width;
            let destination_y = destination / source.width;
            let source_x = source_index % source.width;
            let source_y = source_index / source.width;
            let dx = destination_x as i32 - source_x as i32;
            let dy = destination_y as i32 - source_y as i32;
            assert!(dx * dx + dy * dy <= settings.radius * settings.radius);
        }
    }

    #[test]
    fn swap_with_black_amount_map_is_identity() {
        let source = source(8, 8);
        let black_map = LayerBuffer {
            width: 8,
            height: 8,
            pixels: vec![transparent_pixel(); 64],
        };
        let maps = RenderMaps {
            amount: Some(&black_map),
            radius: None,
        };
        let rendered = render_swap(&source, maps, &settings(ScatterMode::Swap), 42);
        assert_eq!(
            source
                .pixels
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>(),
            rendered
                .iter()
                .map(|px| px.red.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn temporal_frame_mode_changes_seed() {
        let seed_a = temporal_seed(10, TemporalMode::Frame, 1);
        let seed_b = temporal_seed(10, TemporalMode::Frame, 2);
        assert_ne!(seed_a, seed_b);
        assert_eq!(
            temporal_seed(10, TemporalMode::Static, 1),
            temporal_seed(10, TemporalMode::Static, 2)
        );
    }
}
