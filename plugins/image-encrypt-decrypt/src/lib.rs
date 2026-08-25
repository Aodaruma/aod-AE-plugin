#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use std::env;

use ae::pf::*;
use utils::image::{TRANSPARENT, finite_or, read_layer};

const COORDINATE_SALT: u64 = 0x3f84_d5b5_b547_0917;
const BLOCK_SALT: u64 = 0x9216_d5d9_8979_fb1b;
const CHANNEL_SALT: u64 = 0xd1b5_4a32_d192_ed03;
const LINEAR_SALT: u64 = 0x6a09_e667_f3bc_c909;
const BIT_PLANE_SALT: u64 = 0xbb67_ae85_84ca_a73b;
const RECOVERY_SALT: u64 = 0x3c6e_f372_fe94_f82b;
const MAX_REDUNDANCY: usize = 15;
const MAX_RECOVERY_SAMPLES: usize = MAX_REDUNDANCY * 2 + 2;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Operation,
    Algorithm,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    Rounds,
    BlockSize,
    Precision,
    Channels,
    // Appended after the 0.1.0 layout to preserve all existing parameter IDs.
    RecoveryRedundancy,
    RecoveryInterleave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Encode,
    Decode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Algorithm {
    Coordinate,
    Blocks,
    Channels,
    Combined,
    LinearInterleave,
    BitPlane,
    InterleaveBitPlane,
    ResilientReplicas,
}

impl Algorithm {
    fn uses_blocks(self) -> bool {
        matches!(self, Self::Blocks | Self::Combined)
    }

    fn uses_affine_cipher(self) -> bool {
        matches!(self, Self::Channels | Self::Combined)
    }

    fn uses_bit_plane_cipher(self) -> bool {
        matches!(self, Self::BitPlane | Self::InterleaveBitPlane)
    }

    fn uses_quantization(self) -> bool {
        self.uses_affine_cipher()
            || self.uses_bit_plane_cipher()
            || matches!(self, Self::ResilientReplicas)
    }

    fn uses_recovery(self) -> bool {
        matches!(self, Self::ResilientReplicas)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Precision {
    Auto,
    Bpc8,
    Bpc16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Channels {
    Rgb,
    Rgba,
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    operation: Operation,
    algorithm: Algorithm,
    key: u64,
    rounds: usize,
    block_size: usize,
    precision: Precision,
    channels: Channels,
    recovery_redundancy: usize,
    recovery_interleave: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResilientLayout {
    physical_len: usize,
    logical_width: usize,
    logical_height: usize,
    logical_len: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderLayout {
    input_rect: Option<ae::Rect>,
    output_rect: Option<ae::Rect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BufferBounds {
    origin_x: i64,
    origin_y: i64,
    width: usize,
    height: usize,
}

impl BufferBounds {
    const fn new(origin: (i64, i64), width: usize, height: usize) -> Self {
        Self {
            origin_x: origin.0,
            origin_y: origin.1,
            width,
            height,
        }
    }

    fn layer_coordinate(self, local_x: usize, local_y: usize) -> (i64, i64) {
        (
            self.origin_x + local_x as i64,
            self.origin_y + local_y as i64,
        )
    }

    fn local_coordinate(self, layer_x: i64, layer_y: i64) -> Option<(usize, usize)> {
        let local_x = layer_x.checked_sub(self.origin_x)?;
        let local_y = layer_y.checked_sub(self.origin_y)?;
        if local_x < 0
            || local_y < 0
            || local_x >= self.width as i64
            || local_y >= self.height as i64
        {
            return None;
        }
        Some((local_x as usize, local_y as usize))
    }
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str =
    "Applies reversible permutations and ciphers, plus a corruption-tolerant visual format.";

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::Operation,
            "Operation",
            PopupDef::setup(|d| {
                d.set_options(&["Encode", "Decode"]);
                d.set_default(1);
            }),
        )?;

        params.add_with_flags(
            Params::Algorithm,
            "Algorithm",
            PopupDef::setup(|d| {
                d.set_options(&[
                    "Coordinate Shear",
                    "Block Permutation",
                    "Affine Channel Cipher",
                    "Combined Legacy",
                    "Linear Interleave",
                    "Bit-Plane Cipher",
                    "Interleave + Bit-Plane",
                    "Resilient Replicas (Lossy)",
                ]);
                d.set_default(1);
            }),
            ae::ParamFlag::SUPERVISE | ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::empty(),
        )?;

        for (id, name, default) in [
            (Params::KeyA, "Key A", 0x1357),
            (Params::KeyB, "Key B", 0x2468),
            (Params::KeyC, "Key C", 0x9abc),
            (Params::KeyD, "Key D", 0xdef0),
        ] {
            params.add(
                id,
                name,
                SliderDef::setup(|d| {
                    d.set_valid_min(0);
                    d.set_valid_max(65_535);
                    d.set_slider_min(0);
                    d.set_slider_max(65_535);
                    d.set_default(default);
                }),
            )?;
        }

        params.add(
            Params::Rounds,
            "Rounds",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(16);
                d.set_slider_min(1);
                d.set_slider_max(16);
                d.set_default(5);
            }),
        )?;

        params.add(
            Params::BlockSize,
            "Block Size (px)",
            SliderDef::setup(|d| {
                d.set_valid_min(2);
                d.set_valid_max(512);
                d.set_slider_min(2);
                d.set_slider_max(128);
                d.set_default(16);
            }),
        )?;

        params.add(
            Params::Precision,
            "Cipher Precision",
            PopupDef::setup(|d| {
                d.set_options(&["Auto", "8 bpc", "16 bpc"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::Channels,
            "Channels",
            PopupDef::setup(|d| {
                d.set_options(&["RGB", "RGBA"]);
                d.set_default(1);
            }),
        )?;

        params.add(
            Params::RecoveryRedundancy,
            "Recovery Redundancy",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(MAX_REDUNDANCY as i32);
                d.set_slider_min(1);
                d.set_slider_max(MAX_REDUNDANCY as i32);
                d.set_default(5);
            }),
        )?;

        params.add(
            Params::RecoveryInterleave,
            "Recovery Interleave",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(16);
                d.set_slider_min(0);
                d.set_slider_max(16);
                d.set_default(4);
            }),
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
                        "AOD_ImageCrypt - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImageCrypt")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => self.do_render(
                in_data,
                in_layer,
                out_layer,
                params,
                RenderLayout::default(),
            )?,
            ae::Command::SmartPreRender { mut extra } => {
                let mut request = extra.output_request();
                request.preserve_rgb_of_zero_alpha = 1;
                let callbacks = extra.callbacks();
                let query = callbacks.checkout_layer(
                    0,
                    1000,
                    &request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let mut full_request = request;
                full_request.rect = query.max_result_rect;
                let input = callbacks.checkout_layer(
                    0,
                    0,
                    &full_request,
                    in_data.current_time(),
                    in_data.time_step(),
                    in_data.time_scale(),
                )?;
                let full_rect: ae::Rect = input.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);
                extra.set_pre_render_data(RenderLayout {
                    input_rect: Some(full_rect),
                    output_rect: Some(full_rect),
                });
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let layout = extra
                    .pre_render_data::<RenderLayout>()
                    .copied()
                    .unwrap_or_default();
                let in_layer = cb.checkout_layer_pixels(0)?;
                let render_result: Result<(), Error> = (|| {
                    let out_layer = cb.checkout_output()?;
                    if let (Some(in_layer), Some(out_layer)) = (in_layer, out_layer) {
                        self.do_render(in_data, in_layer, out_layer, params, layout)?;
                    }
                    Ok(())
                })();
                let checkin_result = cb.checkin_layer_pixels(0);
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if params.type_at(param_index) == Params::Algorithm {
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
        let algorithm = algorithm_from_popup(params.get(Params::Algorithm)?.as_popup()?.value());
        self.set_param_visible(in_data, params, Params::BlockSize, algorithm.uses_blocks())?;
        for id in [Params::Precision, Params::Channels] {
            self.set_param_visible(in_data, params, id, algorithm.uses_quantization())?;
        }
        for id in [Params::RecoveryRedundancy, Params::RecoveryInterleave] {
            self.set_param_visible(in_data, params, id, algorithm.uses_recovery())?;
        }
        Ok(())
    }

    fn set_param_visible(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if in_data.is_premiere() {
            return Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible);
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
        Self::set_param_ui_flag(params, id, ParamUIFlags::INVISIBLE, !visible)
    }

    fn set_param_ui_flag(
        params: &mut Parameters<Params>,
        id: Params,
        flag: ParamUIFlags,
        status: bool,
    ) -> Result<(), Error> {
        let current = (params.get(id)?.ui_flags().bits() & flag.bits()) != 0;
        if current == status {
            return Ok(());
        }
        let mut param = params.get_mut(id)?;
        param.set_ui_flag(flag, status);
        param.update_param_ui()?;
        Ok(())
    }

    fn read_settings(params: &mut Parameters<Params>) -> Result<Settings, Error> {
        let words = [
            params.get(Params::KeyA)?.as_slider()?.value(),
            params.get(Params::KeyB)?.as_slider()?.value(),
            params.get(Params::KeyC)?.as_slider()?.value(),
            params.get(Params::KeyD)?.as_slider()?.value(),
        ];
        Ok(Settings {
            operation: operation_from_popup(params.get(Params::Operation)?.as_popup()?.value()),
            algorithm: algorithm_from_popup(params.get(Params::Algorithm)?.as_popup()?.value()),
            key: key_from_words(words),
            rounds: params
                .get(Params::Rounds)?
                .as_slider()?
                .value()
                .clamp(1, 16) as usize,
            block_size: params
                .get(Params::BlockSize)?
                .as_slider()?
                .value()
                .clamp(2, 512) as usize,
            precision: precision_from_popup(params.get(Params::Precision)?.as_popup()?.value()),
            channels: channels_from_popup(params.get(Params::Channels)?.as_popup()?.value()),
            recovery_redundancy: params
                .get(Params::RecoveryRedundancy)?
                .as_slider()?
                .value()
                .clamp(1, MAX_REDUNDANCY as i32) as usize,
            recovery_interleave: params
                .get(Params::RecoveryInterleave)?
                .as_slider()?
                .value()
                .clamp(0, 16) as usize,
        })
    }

    fn do_render(
        &self,
        in_data: InData,
        in_layer: Layer,
        mut out_layer: Layer,
        params: &mut Parameters<Params>,
        layout: RenderLayout,
    ) -> Result<(), Error> {
        let source_width = in_layer.width();
        let source_height = in_layer.height();
        let output_width = out_layer.width();
        let output_height = out_layer.height();
        if source_width == 0 || source_height == 0 || output_width == 0 || output_height == 0 {
            return Ok(());
        }

        let settings = Self::read_settings(params)?;
        let source = read_layer(&in_layer);
        let out_world_type = out_layer.world_type();
        let modulus = cipher_modulus(settings.precision, out_world_type);
        let bit_precision = bit_precision_bits(settings.precision, out_world_type);
        let (source_bounds, output_bounds) =
            render_buffer_bounds(in_data, &in_layer, &out_layer, layout);
        let active_channels = if matches!(settings.channels, Channels::Rgba) {
            4
        } else {
            3
        };

        out_layer.iterate(0, output_height as i32, None, |x, y, mut destination| {
            let layer_coordinate = output_bounds.layer_coordinate(x as usize, y as usize);
            let Some(output_coord) =
                source_bounds.local_coordinate(layer_coordinate.0, layer_coordinate.1)
            else {
                write_pixel(&mut destination, out_world_type, TRANSPARENT);
                return Ok(());
            };

            if settings.algorithm.uses_recovery() {
                let pixel = match settings.operation {
                    Operation::Encode => resilient_encode_pixel(
                        &source,
                        source_width,
                        source_height,
                        output_coord,
                        settings,
                        modulus,
                        active_channels,
                    ),
                    Operation::Decode => resilient_decode_pixel(
                        &source,
                        source_width,
                        source_height,
                        output_coord,
                        settings,
                        modulus,
                        active_channels,
                    ),
                };
                write_pixel(&mut destination, out_world_type, pixel);
                return Ok(());
            }

            let source_coord = match settings.operation {
                Operation::Encode => map_inverse(
                    output_coord,
                    source_width,
                    source_height,
                    settings.algorithm,
                    settings.block_size,
                    settings.key,
                    settings.rounds,
                ),
                Operation::Decode => map_forward(
                    output_coord,
                    source_width,
                    source_height,
                    settings.algorithm,
                    settings.block_size,
                    settings.key,
                    settings.rounds,
                ),
            };
            let mut pixel = source[source_coord.1 * source_width + source_coord.0];

            if settings.algorithm.uses_affine_cipher() {
                let cipher_coord = match settings.operation {
                    Operation::Encode => output_coord,
                    Operation::Decode => source_coord,
                };
                pixel = cipher_pixel(
                    pixel,
                    settings.operation,
                    cipher_coord,
                    settings.key,
                    settings.rounds,
                    modulus,
                    active_channels,
                );
            } else if settings.algorithm.uses_bit_plane_cipher() {
                let cipher_coord = match settings.operation {
                    Operation::Encode => output_coord,
                    Operation::Decode => source_coord,
                };
                pixel = bit_plane_cipher_pixel(
                    pixel,
                    settings.operation,
                    cipher_coord,
                    settings.key,
                    settings.rounds,
                    bit_precision,
                    active_channels,
                );
            }

            write_pixel(&mut destination, out_world_type, pixel);
            Ok(())
        })?;

        Ok(())
    }
}

fn render_buffer_bounds(
    in_data: InData,
    in_layer: &Layer,
    out_layer: &Layer,
    layout: RenderLayout,
) -> (BufferBounds, BufferBounds) {
    let pre_effect_origin = in_data.pre_effect_source_origin();
    let source_fallback = (-(pre_effect_origin.h as i64), -(pre_effect_origin.v as i64));
    let source_origin = layer_buffer_origin(in_layer, layout.input_rect, source_fallback);

    // output_origin is the position of input buffer (0, 0) inside the output
    // buffer. Therefore the output buffer's layer-space top-left is shifted in
    // the opposite direction.
    let output_shift = in_data.output_origin();
    let output_fallback = (
        source_origin.0 - output_shift.h as i64,
        source_origin.1 - output_shift.v as i64,
    );
    let output_origin = layer_buffer_origin(out_layer, layout.output_rect, output_fallback);

    (
        BufferBounds::new(source_origin, in_layer.width(), in_layer.height()),
        BufferBounds::new(output_origin, out_layer.width(), out_layer.height()),
    )
}

fn layer_buffer_origin(
    layer: &Layer,
    requested_rect: Option<ae::Rect>,
    fallback: (i64, i64),
) -> (i64, i64) {
    let native = layer.origin();
    if native.h != 0 || native.v != 0 {
        return (native.h as i64, native.v as i64);
    }
    if let Some(rect) = requested_rect
        && rect.width() == layer.width() as i32
        && rect.height() == layer.height() as i32
    {
        return (rect.left as i64, rect.top as i64);
    }
    fallback
}

fn operation_from_popup(value: i32) -> Operation {
    if value == 2 {
        Operation::Decode
    } else {
        Operation::Encode
    }
}

fn algorithm_from_popup(value: i32) -> Algorithm {
    match value {
        2 => Algorithm::Blocks,
        3 => Algorithm::Channels,
        4 => Algorithm::Combined,
        5 => Algorithm::LinearInterleave,
        6 => Algorithm::BitPlane,
        7 => Algorithm::InterleaveBitPlane,
        8 => Algorithm::ResilientReplicas,
        _ => Algorithm::Coordinate,
    }
}

fn precision_from_popup(value: i32) -> Precision {
    match value {
        2 => Precision::Bpc8,
        3 => Precision::Bpc16,
        _ => Precision::Auto,
    }
}

fn channels_from_popup(value: i32) -> Channels {
    if value == 2 {
        Channels::Rgba
    } else {
        Channels::Rgb
    }
}

fn key_from_words(words: [i32; 4]) -> u64 {
    words.into_iter().enumerate().fold(0_u64, |key, (i, word)| {
        key | ((word.clamp(0, 65_535) as u64) << (i * 16))
    })
}

fn map_forward(
    coord: (usize, usize),
    width: usize,
    height: usize,
    algorithm: Algorithm,
    block_size: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    match algorithm {
        Algorithm::Coordinate => {
            coordinate_forward(coord, width, height, key, rounds, COORDINATE_SALT)
        }
        Algorithm::Blocks => block_forward(coord, width, height, block_size, key, rounds),
        Algorithm::Channels => coord,
        Algorithm::Combined => {
            let coord = coordinate_forward(coord, width, height, key, rounds, COORDINATE_SALT);
            block_forward(coord, width, height, block_size, key, rounds)
        }
        Algorithm::LinearInterleave | Algorithm::InterleaveBitPlane => {
            linear_coordinate_forward(coord, width, height, key, rounds)
        }
        Algorithm::BitPlane | Algorithm::ResilientReplicas => coord,
    }
}

fn map_inverse(
    coord: (usize, usize),
    width: usize,
    height: usize,
    algorithm: Algorithm,
    block_size: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    match algorithm {
        Algorithm::Coordinate => {
            coordinate_inverse(coord, width, height, key, rounds, COORDINATE_SALT)
        }
        Algorithm::Blocks => block_inverse(coord, width, height, block_size, key, rounds),
        Algorithm::Channels => coord,
        Algorithm::Combined => {
            let coord = block_inverse(coord, width, height, block_size, key, rounds);
            coordinate_inverse(coord, width, height, key, rounds, COORDINATE_SALT)
        }
        Algorithm::LinearInterleave | Algorithm::InterleaveBitPlane => {
            linear_coordinate_inverse(coord, width, height, key, rounds)
        }
        Algorithm::BitPlane | Algorithm::ResilientReplicas => coord,
    }
}

/// Permutes the flattened raster with keyed affine maps. Unlike the 2-D
/// coordinate shear this deliberately crosses scanline boundaries, producing a
/// long-range interleave while remaining exactly invertible for every frame
/// dimension (including prime-sized buffers).
fn linear_coordinate_forward(
    coord: (usize, usize),
    width: usize,
    height: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    let len = width.saturating_mul(height);
    if width == 0 || height == 0 || len == 0 {
        return (0, 0);
    }
    let index = linear_index_forward(coord.1 * width + coord.0, len, key, rounds, LINEAR_SALT);
    (index % width, index / width)
}

fn linear_coordinate_inverse(
    coord: (usize, usize),
    width: usize,
    height: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    let len = width.saturating_mul(height);
    if width == 0 || height == 0 || len == 0 {
        return (0, 0);
    }
    let index = linear_index_inverse(coord.1 * width + coord.0, len, key, rounds, LINEAR_SALT);
    (index % width, index / width)
}

fn linear_index_forward(mut index: usize, len: usize, key: u64, rounds: usize, salt: u64) -> usize {
    if len <= 1 {
        return 0;
    }
    for round in 0..rounds {
        let (multiplier, offset) = linear_permutation_parameters(len, key, round, salt);
        index = ((index as u128 * multiplier as u128 + offset as u128) % len as u128) as usize;
    }
    index
}

fn linear_index_inverse(mut index: usize, len: usize, key: u64, rounds: usize, salt: u64) -> usize {
    if len <= 1 {
        return 0;
    }
    for round in (0..rounds).rev() {
        let (multiplier, offset) = linear_permutation_parameters(len, key, round, salt);
        let shifted = (index + len - offset) % len;
        let inverse = modular_inverse_usize(multiplier, len);
        index = ((shifted as u128 * inverse as u128) % len as u128) as usize;
    }
    index
}

fn linear_permutation_parameters(len: usize, key: u64, round: usize, salt: u64) -> (usize, usize) {
    let hash = mix64(key ^ salt ^ (round as u64).wrapping_mul(0xd6e8_feb8_6659_fd93));
    let mut multiplier = (hash as usize) % len;
    if multiplier == 0 {
        multiplier = 1;
    }
    while greatest_common_divisor_usize(multiplier, len) != 1 {
        multiplier += 1;
        if multiplier >= len {
            multiplier = 1;
        }
    }
    let offset = (mix64(hash ^ 0xa076_1d64_78bd_642f) as usize) % len;
    (multiplier, offset)
}

fn modular_inverse_usize(value: usize, modulus: usize) -> usize {
    let (mut old_r, mut r) = (value as i128, modulus as i128);
    let (mut old_s, mut s) = (1_i128, 0_i128);
    while r != 0 {
        let quotient = old_r / r;
        (old_r, r) = (r, old_r - quotient * r);
        (old_s, s) = (s, old_s - quotient * s);
    }
    old_s.rem_euclid(modulus as i128) as usize
}

fn greatest_common_divisor_usize(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn coordinate_forward(
    mut coord: (usize, usize),
    width: usize,
    height: usize,
    key: u64,
    rounds: usize,
    salt: u64,
) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    for round in 0..rounds {
        let (ax, ay, bx, by) = shear_parameters(width, height, key, round, salt);
        coord.0 = modular_sum(coord.0, ax, coord.1, bx, width);
        coord.1 = modular_sum(coord.1, ay, coord.0, by, height);
    }
    coord
}

fn coordinate_inverse(
    mut coord: (usize, usize),
    width: usize,
    height: usize,
    key: u64,
    rounds: usize,
    salt: u64,
) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    for round in (0..rounds).rev() {
        let (ax, ay, bx, by) = shear_parameters(width, height, key, round, salt);
        coord.1 = modular_difference(coord.1, ay, coord.0, by, height);
        coord.0 = modular_difference(coord.0, ax, coord.1, bx, width);
    }
    coord
}

fn block_forward(
    coord: (usize, usize),
    width: usize,
    height: usize,
    block_size: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    block_map(
        coord,
        width,
        height,
        block_size,
        |tile, tiles_x, tiles_y| {
            coordinate_forward(tile, tiles_x, tiles_y, key, rounds, BLOCK_SALT)
        },
    )
}

fn block_inverse(
    coord: (usize, usize),
    width: usize,
    height: usize,
    block_size: usize,
    key: u64,
    rounds: usize,
) -> (usize, usize) {
    block_map(
        coord,
        width,
        height,
        block_size,
        |tile, tiles_x, tiles_y| {
            coordinate_inverse(tile, tiles_x, tiles_y, key, rounds, BLOCK_SALT)
        },
    )
}

fn block_map(
    coord: (usize, usize),
    width: usize,
    height: usize,
    block_size: usize,
    transform: impl FnOnce((usize, usize), usize, usize) -> (usize, usize),
) -> (usize, usize) {
    let block_size = block_size.max(1);
    let tiles_x = width / block_size;
    let tiles_y = height / block_size;
    let covered_width = tiles_x * block_size;
    let covered_height = tiles_y * block_size;
    if tiles_x == 0 || tiles_y == 0 || coord.0 >= covered_width || coord.1 >= covered_height {
        return coord;
    }
    let local = (coord.0 % block_size, coord.1 % block_size);
    let tile = (coord.0 / block_size, coord.1 / block_size);
    let mapped = transform(tile, tiles_x, tiles_y);
    (
        mapped.0 * block_size + local.0,
        mapped.1 * block_size + local.1,
    )
}

fn shear_parameters(
    width: usize,
    height: usize,
    key: u64,
    round: usize,
    salt: u64,
) -> (usize, usize, usize, usize) {
    let first = mix64(key ^ salt ^ (round as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let second = mix64(first ^ 0xa076_1d64_78bd_642f);
    (
        (first as usize) % width.max(1),
        ((first >> 32) as usize) % height.max(1),
        (second as usize) % width.max(1),
        ((second >> 32) as usize) % height.max(1),
    )
}

fn modular_sum(value: usize, factor: usize, other: usize, shift: usize, modulus: usize) -> usize {
    if modulus == 0 {
        return 0;
    }
    ((value as u128 + factor as u128 * other as u128 + shift as u128) % modulus as u128) as usize
}

fn modular_difference(
    value: usize,
    factor: usize,
    other: usize,
    shift: usize,
    modulus: usize,
) -> usize {
    if modulus == 0 {
        return 0;
    }
    let modulus = modulus as i128;
    (value as i128 - factor as i128 * other as i128 - shift as i128).rem_euclid(modulus) as usize
}

fn cipher_modulus(precision: Precision, world_type: ae::aegp::WorldType) -> u32 {
    match world_type {
        ae::aegp::WorldType::U8 => ae::MAX_CHANNEL8 + 1,
        ae::aegp::WorldType::U15 => match precision {
            Precision::Bpc8 => ae::MAX_CHANNEL8 + 1,
            Precision::Auto | Precision::Bpc16 => ae::MAX_CHANNEL16 + 1,
        },
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => match precision {
            Precision::Bpc8 => ae::MAX_CHANNEL8 + 1,
            Precision::Auto | Precision::Bpc16 => ae::MAX_CHANNEL16 + 1,
        },
    }
}

fn bit_precision_bits(precision: Precision, world_type: ae::aegp::WorldType) -> u32 {
    match world_type {
        ae::aegp::WorldType::U8 => 8,
        // AE's integer 16-bpc world uses a 0..32768 range. A 15-bit cipher
        // therefore quantizes to 0..32767 so every rotated bit pattern remains
        // representable and the transform stays bijective through host storage.
        ae::aegp::WorldType::U15 => match precision {
            Precision::Bpc8 => 8,
            Precision::Auto | Precision::Bpc16 => 15,
        },
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => match precision {
            Precision::Bpc8 => 8,
            Precision::Auto | Precision::Bpc16 => 16,
        },
    }
}

/// Rotates and masks the selected integer bit planes with a keyed stream. This
/// differs from the affine channel cipher: no arithmetic carries cross bit
/// planes, which creates a sharper digital/bit-plane look while remaining
/// exactly reversible at the selected quantization precision.
fn bit_plane_cipher_pixel(
    pixel: ae::PixelF32,
    operation: Operation,
    coord: (usize, usize),
    key: u64,
    rounds: usize,
    bits: u32,
    active_channels: usize,
) -> ae::PixelF32 {
    let bits = bits.clamp(1, 16);
    let maximum = (1_u32 << bits) - 1;
    let mut values = [
        quantize(pixel.red, maximum),
        quantize(pixel.green, maximum),
        quantize(pixel.blue, maximum),
        quantize(pixel.alpha, maximum),
    ];
    for round in match operation {
        Operation::Encode => EitherRounds::Forward(0..rounds),
        Operation::Decode => EitherRounds::Reverse((0..rounds).rev()),
    } {
        for (channel, value) in values.iter_mut().enumerate().take(active_channels) {
            let hash = bit_plane_hash(key, coord, round, channel);
            let mask = hash as u32 & maximum;
            let rotation = ((hash >> 32) as u32 % bits).max(1) % bits;
            *value = match operation {
                Operation::Encode => rotate_left_width(*value ^ mask, rotation, bits),
                Operation::Decode => rotate_right_width(*value, rotation, bits) ^ mask,
            };
        }
    }
    ae::PixelF32 {
        red: dequantize(values[0], maximum),
        green: dequantize(values[1], maximum),
        blue: dequantize(values[2], maximum),
        alpha: if active_channels == 4 {
            dequantize(values[3], maximum)
        } else {
            pixel.alpha
        },
    }
}

enum EitherRounds {
    Forward(std::ops::Range<usize>),
    Reverse(std::iter::Rev<std::ops::Range<usize>>),
}

impl Iterator for EitherRounds {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Forward(iter) => iter.next(),
            Self::Reverse(iter) => iter.next(),
        }
    }
}

fn bit_plane_hash(key: u64, coord: (usize, usize), round: usize, channel: usize) -> u64 {
    let position =
        (coord.0 as u64).wrapping_mul(0x9e37_79b1_85eb_ca87) ^ (coord.1 as u64).rotate_left(29);
    mix64(
        key ^ BIT_PLANE_SALT
            ^ position
            ^ (round as u64).wrapping_mul(0xa5a3_564e_27f8_8671)
            ^ (channel as u64).wrapping_mul(0xd6e8_feb8_6659_fd93),
    )
}

fn rotate_left_width(value: u32, rotation: u32, bits: u32) -> u32 {
    let mask = (1_u32 << bits) - 1;
    if rotation == 0 {
        return value & mask;
    }
    ((value << rotation) | (value >> (bits - rotation))) & mask
}

fn rotate_right_width(value: u32, rotation: u32, bits: u32) -> u32 {
    let mask = (1_u32 << bits) - 1;
    if rotation == 0 {
        return value & mask;
    }
    ((value >> rotation) | (value << (bits - rotation))) & mask
}

/// Builds a lower-resolution logical raster whose samples are repeated across
/// the full encoded frame. Interleaving scatters each repetition so a localized
/// scratch or a handful of edited pixels are unlikely to damage the same
/// logical sample. This necessarily trades spatial resolution for redundancy;
/// lossless same-size error correction is impossible without spare capacity.
fn resilient_layout(width: usize, height: usize, redundancy: usize) -> ResilientLayout {
    let physical_len = width.saturating_mul(height).max(1);
    let redundancy = redundancy.clamp(1, MAX_REDUNDANCY).min(physical_len);
    if redundancy == 1 {
        return ResilientLayout {
            physical_len,
            logical_width: width.max(1),
            logical_height: height.max(1),
            logical_len: physical_len,
        };
    }

    let capacity = (physical_len / redundancy).max(1);
    let aspect = width.max(1) as f64 / height.max(1) as f64;
    let logical_width =
        ((capacity as f64 * aspect).sqrt().floor() as usize).clamp(1, width.max(1).min(capacity));
    let logical_height = (capacity / logical_width).clamp(1, height.max(1));
    let logical_len = logical_width * logical_height;
    ResilientLayout {
        physical_len,
        logical_width,
        logical_height,
        logical_len,
    }
}

fn logical_coordinate(index: usize, layout: ResilientLayout) -> (usize, usize) {
    (index % layout.logical_width, index / layout.logical_width)
}

fn logical_source_coordinate(
    index: usize,
    layout: ResilientLayout,
    width: usize,
    height: usize,
) -> (usize, usize) {
    let (x, y) = logical_coordinate(index, layout);
    (
        ((2 * x + 1) * width / (2 * layout.logical_width)).min(width - 1),
        ((2 * y + 1) * height / (2 * layout.logical_height)).min(height - 1),
    )
}

fn output_logical_index(
    coord: (usize, usize),
    layout: ResilientLayout,
    width: usize,
    height: usize,
) -> usize {
    let x = (coord.0 * layout.logical_width / width).min(layout.logical_width - 1);
    let y = (coord.1 * layout.logical_height / height).min(layout.logical_height - 1);
    y * layout.logical_width + x
}

fn resilient_encode_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    destination_coord: (usize, usize),
    settings: Settings,
    modulus: u32,
    active_channels: usize,
) -> ae::PixelF32 {
    let layout = resilient_layout(width, height, settings.recovery_redundancy);
    let destination_index = destination_coord.1 * width + destination_coord.0;
    let slot = linear_index_inverse(
        destination_index,
        layout.physical_len,
        settings.key,
        settings.recovery_interleave,
        RECOVERY_SALT,
    );
    let logical_index = slot % layout.logical_len;
    let source_coord = logical_source_coordinate(logical_index, layout, width, height);
    cipher_pixel(
        source[source_coord.1 * width + source_coord.0],
        Operation::Encode,
        logical_coordinate(logical_index, layout),
        settings.key,
        settings.rounds,
        modulus,
        active_channels,
    )
}

fn resilient_decode_pixel(
    source: &[ae::PixelF32],
    width: usize,
    height: usize,
    destination_coord: (usize, usize),
    settings: Settings,
    modulus: u32,
    active_channels: usize,
) -> ae::PixelF32 {
    let layout = resilient_layout(width, height, settings.recovery_redundancy);
    let logical_index = output_logical_index(destination_coord, layout, width, height);
    let logical_coord = logical_coordinate(logical_index, layout);
    let mut samples = [TRANSPARENT; MAX_RECOVERY_SAMPLES];
    let mut count = 0;
    let mut slot = logical_index;
    while slot < layout.physical_len && count < samples.len() {
        let physical_index = linear_index_forward(
            slot,
            layout.physical_len,
            settings.key,
            settings.recovery_interleave,
            RECOVERY_SALT,
        );
        samples[count] = cipher_pixel(
            source[physical_index],
            Operation::Decode,
            logical_coord,
            settings.key,
            settings.rounds,
            modulus,
            active_channels,
        );
        count += 1;
        slot += layout.logical_len;
    }
    debug_assert!(count > 0);
    median_pixel(&samples[..count])
}

fn median_pixel(samples: &[ae::PixelF32]) -> ae::PixelF32 {
    ae::PixelF32 {
        red: median_channel(samples, |pixel| pixel.red),
        green: median_channel(samples, |pixel| pixel.green),
        blue: median_channel(samples, |pixel| pixel.blue),
        alpha: median_channel(samples, |pixel| pixel.alpha),
    }
}

fn median_channel(samples: &[ae::PixelF32], channel: impl Fn(ae::PixelF32) -> f32) -> f32 {
    let mut values = [0.0_f32; MAX_RECOVERY_SAMPLES];
    for (destination, sample) in values.iter_mut().zip(samples) {
        *destination = finite_or(channel(*sample), 0.0);
    }
    let values = &mut values[..samples.len()];
    values.sort_unstable_by(f32::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
}

fn cipher_pixel(
    pixel: ae::PixelF32,
    operation: Operation,
    coord: (usize, usize),
    key: u64,
    rounds: usize,
    modulus: u32,
    active_channels: usize,
) -> ae::PixelF32 {
    let max_value = modulus - 1;
    let mut values = [
        quantize(pixel.red, max_value),
        quantize(pixel.green, max_value),
        quantize(pixel.blue, max_value),
        quantize(pixel.alpha, max_value),
    ];
    match operation {
        Operation::Encode => {
            channel_encode(&mut values, active_channels, modulus, key, coord, rounds)
        }
        Operation::Decode => {
            channel_decode(&mut values, active_channels, modulus, key, coord, rounds)
        }
    }
    ae::PixelF32 {
        red: dequantize(values[0], max_value),
        green: dequantize(values[1], max_value),
        blue: dequantize(values[2], max_value),
        alpha: if active_channels == 4 {
            dequantize(values[3], max_value)
        } else {
            pixel.alpha
        },
    }
}

fn channel_encode(
    values: &mut [u32; 4],
    active_channels: usize,
    modulus: u32,
    key: u64,
    coord: (usize, usize),
    rounds: usize,
) {
    for round in 0..rounds {
        for channel in 0..active_channels {
            let value = if channel == 0 {
                values[channel]
            } else {
                (values[channel] + values[channel - 1]) % modulus
            };
            let (multiplier, offset) = affine_parameters(key, coord, round, channel, modulus);
            values[channel] = affine_encode(value, multiplier, offset, modulus);
        }
    }
}

fn channel_decode(
    values: &mut [u32; 4],
    active_channels: usize,
    modulus: u32,
    key: u64,
    coord: (usize, usize),
    rounds: usize,
) {
    for round in (0..rounds).rev() {
        for channel in (0..active_channels).rev() {
            let (multiplier, offset) = affine_parameters(key, coord, round, channel, modulus);
            let value = affine_decode(values[channel], multiplier, offset, modulus);
            values[channel] = if channel == 0 {
                value
            } else {
                (value + modulus - values[channel - 1]) % modulus
            };
        }
    }
}

fn affine_parameters(
    key: u64,
    coord: (usize, usize),
    round: usize,
    channel: usize,
    modulus: u32,
) -> (u32, u32) {
    let position =
        (coord.0 as u64).wrapping_mul(0x9e37_79b1_85eb_ca87) ^ (coord.1 as u64).rotate_left(29);
    let hash = mix64(
        key ^ CHANNEL_SALT
            ^ position
            ^ (round as u64).wrapping_mul(0xd6e8_feb8_6659_fd93)
            ^ (channel as u64).wrapping_mul(0xa5a3_564e_27f8_8671),
    );
    let mut multiplier = (hash as u32 % (modulus - 1)) + 1;
    while greatest_common_divisor(multiplier, modulus) != 1 {
        multiplier += 1;
        if multiplier >= modulus {
            multiplier = 1;
        }
    }
    let offset = (mix64(hash ^ 0xe703_7ed1_a0b4_28db) % modulus as u64) as u32;
    (multiplier, offset)
}

fn affine_encode(value: u32, multiplier: u32, offset: u32, modulus: u32) -> u32 {
    ((value as u64 * multiplier as u64 + offset as u64) % modulus as u64) as u32
}

fn affine_decode(value: u32, multiplier: u32, offset: u32, modulus: u32) -> u32 {
    let shifted = (value + modulus - offset) % modulus;
    ((shifted as u64 * modular_inverse(multiplier, modulus) as u64) % modulus as u64) as u32
}

fn modular_inverse(value: u32, modulus: u32) -> u32 {
    let (mut old_r, mut r) = (value as i64, modulus as i64);
    let (mut old_s, mut s) = (1_i64, 0_i64);
    while r != 0 {
        let quotient = old_r / r;
        (old_r, r) = (r, old_r - quotient * r);
        (old_s, s) = (s, old_s - quotient * s);
    }
    old_s.rem_euclid(modulus as i64) as u32
}

fn greatest_common_divisor(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn quantize(value: f32, max_value: u32) -> u32 {
    (finite_or(value, 0.0).clamp(0.0, 1.0) * max_value as f32).round() as u32
}

fn dequantize(value: u32, max_value: u32) -> f32 {
    value as f32 / max_value as f32
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn write_pixel(
    destination: &mut ae::GenericPixelMut<'_>,
    world_type: ae::aegp::WorldType,
    pixel: ae::PixelF32,
) {
    match world_type {
        ae::aegp::WorldType::U8 => destination.set_from_u8(ae::Pixel8 {
            red: to_integer_channel(pixel.red, ae::MAX_CHANNEL8) as u8,
            green: to_integer_channel(pixel.green, ae::MAX_CHANNEL8) as u8,
            blue: to_integer_channel(pixel.blue, ae::MAX_CHANNEL8) as u8,
            alpha: to_integer_channel(pixel.alpha, ae::MAX_CHANNEL8) as u8,
        }),
        ae::aegp::WorldType::U15 => destination.set_from_u16(ae::Pixel16 {
            red: to_integer_channel(pixel.red, ae::MAX_CHANNEL16) as u16,
            green: to_integer_channel(pixel.green, ae::MAX_CHANNEL16) as u16,
            blue: to_integer_channel(pixel.blue, ae::MAX_CHANNEL16) as u16,
            alpha: to_integer_channel(pixel.alpha, ae::MAX_CHANNEL16) as u16,
        }),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => destination.set_from_f32(pixel),
    }
}

fn to_integer_channel(value: f32, maximum: u32) -> u32 {
    (finite_or(value, 0.0).clamp(0.0, 1.0) * maximum as f32).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_bounds_align_negative_and_nonzero_origins() {
        let source = BufferBounds::new((-12, 7), 100, 80);
        let output = BufferBounds::new((-20, 4), 120, 90);

        let layer_coordinate = output.layer_coordinate(8, 3);
        assert_eq!(layer_coordinate, (-12, 7));
        assert_eq!(source.local_coordinate(-12, 7), Some((0, 0)));
        assert_eq!(source.local_coordinate(87, 86), Some((99, 79)));
        assert_eq!(source.local_coordinate(-13, 7), None);
        assert_eq!(source.local_coordinate(88, 7), None);
    }

    fn store_in_world(pixel: ae::PixelF32, world_type: ae::aegp::WorldType) -> ae::PixelF32 {
        let maximum = match world_type {
            ae::aegp::WorldType::U8 => Some(ae::MAX_CHANNEL8),
            ae::aegp::WorldType::U15 => Some(ae::MAX_CHANNEL16),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => None,
        };
        let Some(maximum) = maximum else {
            return pixel;
        };
        ae::PixelF32 {
            red: dequantize(to_integer_channel(pixel.red, maximum), maximum),
            green: dequantize(to_integer_channel(pixel.green, maximum), maximum),
            blue: dequantize(to_integer_channel(pixel.blue, maximum), maximum),
            alpha: dequantize(to_integer_channel(pixel.alpha, maximum), maximum),
        }
    }

    fn cipher_quantized(pixel: ae::PixelF32, modulus: u32, active_channels: usize) -> ae::PixelF32 {
        let maximum = modulus - 1;
        ae::PixelF32 {
            red: dequantize(quantize(pixel.red, maximum), maximum),
            green: dequantize(quantize(pixel.green, maximum), maximum),
            blue: dequantize(quantize(pixel.blue, maximum), maximum),
            alpha: if active_channels == 4 {
                dequantize(quantize(pixel.alpha, maximum), maximum)
            } else {
                pixel.alpha
            },
        }
    }

    fn bit_quantized(pixel: ae::PixelF32, bits: u32, active_channels: usize) -> ae::PixelF32 {
        cipher_quantized(pixel, 1_u32 << bits, active_channels)
    }

    fn assert_pixel_near(actual: ae::PixelF32, expected: ae::PixelF32) {
        for (actual, expected) in [
            (actual.red, expected.red),
            (actual.green, expected.green),
            (actual.blue, expected.blue),
            (actual.alpha, expected.alpha),
        ] {
            assert!(
                (actual - expected).abs() <= 1.0e-6,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn coordinate_scramble_round_trips_arbitrary_dimensions() {
        for (width, height) in [(1, 1), (2, 3), (17, 11), (64, 48)] {
            for y in 0..height {
                for x in 0..width {
                    let encoded = coordinate_forward(
                        (x, y),
                        width,
                        height,
                        0x0123_4567_89ab_cdef,
                        7,
                        COORDINATE_SALT,
                    );
                    let decoded = coordinate_inverse(
                        encoded,
                        width,
                        height,
                        0x0123_4567_89ab_cdef,
                        7,
                        COORDINATE_SALT,
                    );
                    assert_eq!(decoded, (x, y));
                }
            }
        }
    }

    #[test]
    fn block_scramble_round_trips_complete_and_remainder_regions() {
        let (width, height, block_size) = (37, 29, 8);
        for y in 0..height {
            for x in 0..width {
                let encoded =
                    block_forward((x, y), width, height, block_size, 0xfedc_ba98_7654_3210, 5);
                let decoded =
                    block_inverse(encoded, width, height, block_size, 0xfedc_ba98_7654_3210, 5);
                assert_eq!(decoded, (x, y));
            }
        }
    }

    #[test]
    fn linear_interleave_round_trips_prime_and_composite_lengths() {
        for (width, height) in [(1, 1), (17, 1), (13, 11), (64, 48)] {
            for y in 0..height {
                for x in 0..width {
                    let encoded =
                        linear_coordinate_forward((x, y), width, height, 0x0123_4567_89ab_cdef, 9);
                    let decoded =
                        linear_coordinate_inverse(encoded, width, height, 0x0123_4567_89ab_cdef, 9);
                    assert_eq!(decoded, (x, y));
                }
            }
        }
    }

    #[test]
    fn channel_cipher_round_trips_host_precisions() {
        for modulus in [ae::MAX_CHANNEL8 + 1, ae::MAX_CHANNEL16 + 1] {
            let maximum = modulus - 1;
            for active_channels in [3, 4] {
                for value in [0, 1, maximum / 3, maximum / 2, maximum - 1, maximum] {
                    let mut channels = [value, maximum - value, value / 2, maximum];
                    let original = channels;
                    channel_encode(
                        &mut channels,
                        active_channels,
                        modulus,
                        0x0ddc_0ffe_e15e_beef,
                        (123, 456),
                        6,
                    );
                    channel_decode(
                        &mut channels,
                        active_channels,
                        modulus,
                        0x0ddc_0ffe_e15e_beef,
                        (123, 456),
                        6,
                    );
                    assert_eq!(channels, original);
                }
            }
        }
    }

    #[test]
    fn cipher_precision_never_exceeds_integer_output_capacity() {
        assert_eq!(
            cipher_modulus(Precision::Bpc16, ae::aegp::WorldType::U8),
            ae::MAX_CHANNEL8 + 1
        );
        assert_eq!(
            cipher_modulus(Precision::Bpc16, ae::aegp::WorldType::U15),
            ae::MAX_CHANNEL16 + 1
        );
        assert_eq!(
            cipher_modulus(Precision::Bpc8, ae::aegp::WorldType::F32),
            ae::MAX_CHANNEL8 + 1
        );
    }

    #[test]
    fn channel_cipher_round_trips_through_host_storage() {
        let base = ae::PixelF32 {
            red: 0.145_098_05,
            green: 0.623_529_43,
            blue: 0.901_960_8,
            alpha: 0.741_176_5,
        };
        for world_type in [
            ae::aegp::WorldType::U8,
            ae::aegp::WorldType::U15,
            ae::aegp::WorldType::F32,
        ] {
            let original = store_in_world(base, world_type);
            for precision in [Precision::Auto, Precision::Bpc8, Precision::Bpc16] {
                let modulus = cipher_modulus(precision, world_type);
                for active_channels in [3, 4] {
                    let encoded = cipher_pixel(
                        original,
                        Operation::Encode,
                        (37, 91),
                        0x0ddc_0ffe_e15e_beef,
                        6,
                        modulus,
                        active_channels,
                    );
                    let encoded = store_in_world(encoded, world_type);
                    let decoded = cipher_pixel(
                        encoded,
                        Operation::Decode,
                        (37, 91),
                        0x0ddc_0ffe_e15e_beef,
                        6,
                        modulus,
                        active_channels,
                    );
                    let decoded = store_in_world(decoded, world_type);
                    let expected = store_in_world(
                        cipher_quantized(original, modulus, active_channels),
                        world_type,
                    );
                    assert_pixel_near(decoded, expected);
                }
            }
        }
    }

    #[test]
    fn bit_plane_cipher_round_trips_selected_precision() {
        let base = ae::PixelF32 {
            red: 0.145_098_05,
            green: 0.623_529_43,
            blue: 0.901_960_8,
            alpha: 0.741_176_5,
        };
        for bits in [8, 15, 16] {
            for active_channels in [3, 4] {
                let encoded = bit_plane_cipher_pixel(
                    base,
                    Operation::Encode,
                    (37, 91),
                    0x0ddc_0ffe_e15e_beef,
                    7,
                    bits,
                    active_channels,
                );
                let decoded = bit_plane_cipher_pixel(
                    encoded,
                    Operation::Decode,
                    (37, 91),
                    0x0ddc_0ffe_e15e_beef,
                    7,
                    bits,
                    active_channels,
                );
                assert_pixel_near(decoded, bit_quantized(base, bits, active_channels));
            }
        }
    }

    #[test]
    fn bit_plane_cipher_round_trips_through_host_storage() {
        let base = ae::PixelF32 {
            red: 0.145_098_05,
            green: 0.623_529_43,
            blue: 0.901_960_8,
            alpha: 0.741_176_5,
        };
        for world_type in [
            ae::aegp::WorldType::U8,
            ae::aegp::WorldType::U15,
            ae::aegp::WorldType::F32,
        ] {
            let original = store_in_world(base, world_type);
            for precision in [Precision::Auto, Precision::Bpc8, Precision::Bpc16] {
                let bits = bit_precision_bits(precision, world_type);
                for active_channels in [3, 4] {
                    let encoded = bit_plane_cipher_pixel(
                        original,
                        Operation::Encode,
                        (37, 91),
                        0x0ddc_0ffe_e15e_beef,
                        7,
                        bits,
                        active_channels,
                    );
                    let encoded = store_in_world(encoded, world_type);
                    let decoded = bit_plane_cipher_pixel(
                        encoded,
                        Operation::Decode,
                        (37, 91),
                        0x0ddc_0ffe_e15e_beef,
                        7,
                        bits,
                        active_channels,
                    );
                    let decoded = store_in_world(decoded, world_type);
                    let expected =
                        store_in_world(bit_quantized(original, bits, active_channels), world_type);
                    assert_pixel_near(decoded, expected);
                }
            }
        }
    }

    fn recovery_settings() -> Settings {
        Settings {
            operation: Operation::Encode,
            algorithm: Algorithm::ResilientReplicas,
            key: 0x0123_4567_89ab_cdef,
            rounds: 5,
            block_size: 16,
            precision: Precision::Bpc8,
            channels: Channels::Rgba,
            recovery_redundancy: 5,
            recovery_interleave: 4,
        }
    }

    #[test]
    fn resilient_layout_never_exceeds_fixed_sample_storage() {
        for width in 1..40 {
            for height in 1..40 {
                for redundancy in 1..=MAX_REDUNDANCY {
                    let layout = resilient_layout(width, height, redundancy);
                    let repetitions = layout.physical_len.div_ceil(layout.logical_len);
                    assert!(repetitions <= MAX_RECOVERY_SAMPLES);
                }
            }
        }
    }

    #[test]
    fn resilient_replicas_recover_two_corrupted_copies() {
        let (width, height) = (23, 17);
        let settings = recovery_settings();
        let modulus = ae::MAX_CHANNEL8 + 1;
        let source: Vec<_> = (0..width * height)
            .map(|index| {
                let x = index % width;
                let y = index / width;
                ae::PixelF32 {
                    red: ((x * 11 + y * 3) % 256) as f32 / 255.0,
                    green: ((x * 5 + y * 17) % 256) as f32 / 255.0,
                    blue: ((x * 19 + y * 7) % 256) as f32 / 255.0,
                    alpha: ((x * 13 + y * 23) % 256) as f32 / 255.0,
                }
            })
            .collect();
        let mut encoded = vec![TRANSPARENT; width * height];
        for y in 0..height {
            for x in 0..width {
                encoded[y * width + x] =
                    resilient_encode_pixel(&source, width, height, (x, y), settings, modulus, 4);
            }
        }

        // Damage two replicas belonging to the same logical sample. With the
        // default five-or-more copies, median decoding still selects an intact
        // value for every channel.
        let layout = resilient_layout(width, height, settings.recovery_redundancy);
        let damaged_logical_index = layout.logical_len / 2;
        for repetition in 0..2 {
            let slot = damaged_logical_index + repetition * layout.logical_len;
            let physical_index = linear_index_forward(
                slot,
                layout.physical_len,
                settings.key,
                settings.recovery_interleave,
                RECOVERY_SALT,
            );
            encoded[physical_index] = ae::PixelF32 {
                red: 1.0,
                green: 0.0,
                blue: 1.0,
                alpha: 0.0,
            };
        }

        for y in 0..height {
            for x in 0..width {
                let decoded =
                    resilient_decode_pixel(&encoded, width, height, (x, y), settings, modulus, 4);
                let logical_index = output_logical_index((x, y), layout, width, height);
                let source_coord = logical_source_coordinate(logical_index, layout, width, height);
                let expected =
                    cipher_quantized(source[source_coord.1 * width + source_coord.0], modulus, 4);
                assert_pixel_near(decoded, expected);
            }
        }
    }

    #[test]
    fn original_popup_values_remain_project_compatible() {
        assert_eq!(algorithm_from_popup(1), Algorithm::Coordinate);
        assert_eq!(algorithm_from_popup(2), Algorithm::Blocks);
        assert_eq!(algorithm_from_popup(3), Algorithm::Channels);
        assert_eq!(algorithm_from_popup(4), Algorithm::Combined);
    }

    #[test]
    fn combined_coordinate_map_round_trips() {
        let algorithm = Algorithm::Combined;
        for y in 0..31 {
            for x in 0..43 {
                let encoded = map_forward((x, y), 43, 31, algorithm, 6, 0x55aa_1234_5678_cdef, 4);
                let decoded = map_inverse(encoded, 43, 31, algorithm, 6, 0x55aa_1234_5678_cdef, 4);
                assert_eq!(decoded, (x, y));
            }
        }
    }
}
