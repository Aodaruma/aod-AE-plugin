#![allow(clippy::drop_non_drop, clippy::question_mark)]

#[cfg(feature = "gpu_wgpu")]
mod gpu;

use after_effects as ae;
use std::collections::VecDeque;
use std::env;
#[cfg(feature = "gpu_wgpu")]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

use ae::pf::*;
#[cfg(feature = "gpu_wgpu")]
use gpu::{GpuBoundaryCondition, SolveParams, WgpuContext};
use utils::ToPixel;
use utils::image::{
    SampleEdge, luma, read_layer, sample_bilinear, sanitize_pixel, unpremultiplied_rgb,
};

const PLUGIN_DESCRIPTION: &str = "Relights images using color-region, channel-generated, or supplied normal maps with configurable material and light controls.";
const NORMAL_LAYER_CHECKOUT_ID: u32 = 1;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    NormalStart,
    NormalSource,
    GeneratedStart,
    GenerationMethod,
    ColorRegionsStart,
    ColorTolerance,
    AlphaThreshold,
    RegionRadius,
    RegionShape,
    EdgeSoftness,
    ColorRegionsEnd,
    HeightChannelStart,
    HeightChannel,
    HeightBlur,
    HeightChannelEnd,
    NormalStrength,
    InvertHeight,
    GeneratedEnd,
    ExternalStart,
    NormalLayer,
    NormalScale,
    NormalY,
    ExternalEnd,
    NormalEnd,
    LightStart,
    LightType,
    LightColor,
    LightIntensity,
    DirectionAzimuth,
    DirectionElevation,
    PointPosition,
    PointHeight,
    PointRadius,
    LightEnd,
    MaterialStart,
    Ambient,
    Diffuse,
    Specular,
    Shininess,
    Exposure,
    ClampOutput,
    MaterialEnd,
    OutputMode,
    // Appended after the v0.2 parameter sequence to preserve existing project indices.
    RegionSolverStart,
    RegionSolver,
    BoundaryCondition,
    PoissonIterations,
    PoissonCurvature,
    ScreenedDamping,
    EdgeFeather,
    RegionSolverEnd,
    // Appended after the v0.4 parameter sequence to preserve existing project indices.
    PrincipledStart,
    MaterialModel,
    BaseTint,
    Metallic,
    Roughness,
    Ior,
    SpecularIorLevel,
    EnvironmentStrength,
    PrincipledEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NormalSource {
    Generated,
    Layer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GenerationMethod {
    ColorRegions,
    HeightChannel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegionSolver {
    DistanceField,
    Poisson,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundaryCondition {
    Dirichlet,
    Neumann,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeightChannel {
    Luminance,
    Alpha,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NormalY {
    OpenGl,
    DirectX,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LightType {
    Directional,
    Point,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaterialModel {
    Principled,
    LegacyBlinnPhong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    Relit,
    Normal,
    Height,
    Diffuse,
    Specular,
    Lighting,
}

impl OutputMode {
    fn uses_lighting(self) -> bool {
        !matches!(self, Self::Normal | Self::Height)
    }
}

#[derive(Clone, Copy, Debug)]
struct Settings {
    normal_source: NormalSource,
    generation_method: GenerationMethod,
    color_tolerance: f32,
    alpha_threshold: f32,
    region_solver: RegionSolver,
    region_radius: f32,
    region_shape: f32,
    boundary_condition: BoundaryCondition,
    poisson_iterations: usize,
    poisson_curvature: f32,
    screened_damping: f32,
    edge_feather: f32,
    edge_softness: f32,
    height_channel: HeightChannel,
    height_blur: f32,
    normal_strength: f32,
    invert_height: bool,
    normal_scale: f32,
    normal_y: NormalY,
    light_type: LightType,
    light_color: [f32; 3],
    light_intensity: f32,
    direction_azimuth: f32,
    direction_elevation: f32,
    point_position: (f32, f32),
    point_height: f32,
    point_radius: f32,
    ambient: f32,
    diffuse: f32,
    specular: f32,
    shininess: f32,
    material_model: MaterialModel,
    base_tint: [f32; 3],
    metallic: f32,
    roughness: f32,
    ior: f32,
    specular_ior_level: f32,
    environment_strength: f32,
    exposure: f32,
    clamp_output: bool,
    output_mode: OutputMode,
    pixel_scale: (f32, f32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SurfaceSettingsKey {
    color_tolerance: u32,
    alpha_threshold: u32,
    region_solver: RegionSolver,
    region_radius: u32,
    region_shape: u32,
    boundary_condition: BoundaryCondition,
    poisson_iterations: usize,
    poisson_curvature: u32,
    screened_damping: u32,
    edge_feather: u32,
    edge_softness: u32,
    normal_strength: u32,
    invert_height: bool,
    pixel_scale_x: u32,
    pixel_scale_y: u32,
}

impl From<Settings> for SurfaceSettingsKey {
    fn from(settings: Settings) -> Self {
        Self {
            color_tolerance: settings.color_tolerance.to_bits(),
            alpha_threshold: settings.alpha_threshold.to_bits(),
            region_solver: settings.region_solver,
            region_radius: settings.region_radius.to_bits(),
            region_shape: settings.region_shape.to_bits(),
            boundary_condition: settings.boundary_condition,
            poisson_iterations: settings.poisson_iterations,
            poisson_curvature: settings.poisson_curvature.to_bits(),
            screened_damping: settings.screened_damping.to_bits(),
            edge_feather: settings.edge_feather.to_bits(),
            edge_softness: settings.edge_softness.to_bits(),
            normal_strength: settings.normal_strength.to_bits(),
            invert_height: settings.invert_height,
            pixel_scale_x: settings.pixel_scale.0.to_bits(),
            pixel_scale_y: settings.pixel_scale.1.to_bits(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SurfaceCacheKey {
    source_hash: [u64; 4],
    width: usize,
    height: usize,
    source_origin_h: i32,
    source_origin_v: i32,
    settings: SurfaceSettingsKey,
}

impl SurfaceCacheKey {
    fn new(
        source: &[PixelF32],
        width: usize,
        height: usize,
        source_origin: ae::Point,
        settings: Settings,
    ) -> Self {
        Self {
            source_hash: source_content_hash(source),
            width,
            height,
            source_origin_h: source_origin.h,
            source_origin_v: source_origin.v,
            settings: settings.into(),
        }
    }
}

#[derive(Clone)]
struct SurfaceMaps {
    heights: Arc<[f32]>,
    normals: Arc<[[f32; 3]]>,
}

impl SurfaceMaps {
    fn new(heights: Vec<f32>, normals: Vec<[f32; 3]>) -> Self {
        Self {
            heights: heights.into(),
            normals: normals.into(),
        }
    }
}

struct SurfaceCacheEntry {
    key: SurfaceCacheKey,
    maps: SurfaceMaps,
}

#[derive(Default)]
struct SurfaceCache {
    entry: Mutex<Option<SurfaceCacheEntry>>,
}

impl SurfaceCache {
    fn get_or_build(
        &self,
        key: SurfaceCacheKey,
        build: impl FnOnce() -> SurfaceMaps,
    ) -> SurfaceMaps {
        // The lock intentionally covers construction. It guarantees that competing Smart Render
        // tiles for one frame execute the expensive full-frame solver exactly once.
        let mut entry = self.entry.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(cached) = entry.as_ref()
            && cached.key == key
        {
            return cached.maps.clone();
        }
        // Drop an unused previous frame before the new full-frame solve to avoid
        // retaining two large surface maps at the peak of a cache miss.
        *entry = None;
        let maps = build();
        *entry = Some(SurfaceCacheEntry {
            key,
            maps: maps.clone(),
        });
        maps
    }

    fn clear(&self) {
        let mut entry = self.entry.lock().unwrap_or_else(|error| error.into_inner());
        *entry = None;
    }
}

fn source_content_hash(source: &[PixelF32]) -> [u64; 4] {
    let mut state = [
        0xA0D1_6A2B_9374_C5E8,
        0x6C8E_9CF5_701A_42D3,
        0xD1B5_4A32_D192_ED03,
        0x94D0_49BB_1331_11EB,
    ];
    for (index, pixel) in source.iter().enumerate() {
        let alpha_red = ((pixel.alpha.to_bits() as u64) << 32) | pixel.red.to_bits() as u64;
        let green_blue = ((pixel.green.to_bits() as u64) << 32) | pixel.blue.to_bits() as u64;
        let position = index as u64;
        state[0] = mix_fingerprint_lane(state[0], alpha_red ^ position);
        state[1] = mix_fingerprint_lane(
            state[1],
            green_blue ^ position.wrapping_mul(0x9E37_79B1_85EB_CA87),
        );
        state[2] =
            mix_fingerprint_lane(state[2], alpha_red.rotate_left(23) ^ green_blue ^ !position);
        state[3] = mix_fingerprint_lane(
            state[3],
            green_blue.rotate_right(17) ^ alpha_red ^ position.rotate_left(31),
        );
    }
    let length = source.len() as u64;
    for (lane, value) in state.iter_mut().enumerate() {
        *value =
            finalize_fingerprint(*value ^ length.wrapping_mul(0xD6E8_FEB8_6659_FD93 ^ lane as u64));
    }
    state
}

fn mix_fingerprint_lane(state: u64, value: u64) -> u64 {
    state
        .rotate_left(27)
        .wrapping_add(finalize_fingerprint(value))
        .wrapping_mul(0x9E37_79B1_85EB_CA87)
}

fn finalize_fingerprint(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
    surface_cache: SurfaceCache,
}

struct LayerCheckoutGuard<const N: usize> {
    callbacks: SmartRenderCallbacks,
    checkout_ids: [u32; N],
    count: usize,
}

impl<const N: usize> LayerCheckoutGuard<N> {
    fn new(callbacks: SmartRenderCallbacks) -> Self {
        Self {
            callbacks,
            checkout_ids: [0; N],
            count: 0,
        }
    }

    fn checkout(&mut self, checkout_id: u32) -> Result<Option<Layer>, Error> {
        assert!(self.count < N, "layer checkout guard capacity exceeded");
        let layer = self.callbacks.checkout_layer_pixels(checkout_id)?;
        self.checkout_ids[self.count] = checkout_id;
        self.count += 1;
        Ok(layer)
    }

    fn checkin_all(&mut self) -> Result<(), Error> {
        let mut first_error = None;
        while self.count > 0 {
            self.count -= 1;
            if let Err(error) = self
                .callbacks
                .checkin_layer_pixels(self.checkout_ids[self.count])
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<const N: usize> Drop for LayerCheckoutGuard<N> {
    fn drop(&mut self) {
        let _ = self.checkin_all();
    }
}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let supervise = || {
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP
        };

        params.add_group(
            Params::NormalStart,
            Params::NormalEnd,
            "Normal Source",
            false,
            |params| {
                params.add_with_flags(
                    Params::NormalSource,
                    "Source",
                    PopupDef::setup(|d| {
                        d.set_options(&["Generated", "Normal Layer"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ParamUIFlags::empty(),
                )?;
                params.add_group(
                    Params::GeneratedStart,
                    Params::GeneratedEnd,
                    "Generated Normal",
                    false,
                    |params| {
                        params.add_with_flags(
                            Params::GenerationMethod,
                            "Method",
                            PopupDef::setup(|d| {
                                d.set_options(&["Color Regions", "Height Channel"]);
                                d.set_default(1);
                            }),
                            supervise(),
                            ParamUIFlags::empty(),
                        )?;
                        params.add_group(
                            Params::ColorRegionsStart,
                            Params::ColorRegionsEnd,
                            "Color Regions",
                            false,
                            |params| {
                                add_float_slider(
                                    params,
                                    Params::ColorTolerance,
                                    "Color Tolerance",
                                    0.0,
                                    1.0,
                                    0.0,
                                    0.2,
                                    0.02,
                                    3,
                                )?;
                                add_float_slider(
                                    params,
                                    Params::AlphaThreshold,
                                    "Alpha Threshold",
                                    0.0,
                                    1.0,
                                    0.0,
                                    1.0,
                                    0.01,
                                    3,
                                )?;
                                add_float_slider(
                                    params,
                                    Params::RegionRadius,
                                    "Region Radius (px)",
                                    0.01,
                                    4096.0,
                                    1.0,
                                    256.0,
                                    32.0,
                                    1,
                                )?;
                                add_float_slider(
                                    params,
                                    Params::RegionShape,
                                    "Height Shape",
                                    0.05,
                                    16.0,
                                    0.1,
                                    8.0,
                                    2.0,
                                    2,
                                )?;
                                add_float_slider(
                                    params,
                                    Params::EdgeSoftness,
                                    "Edge Softness (px)",
                                    0.0,
                                    1024.0,
                                    0.0,
                                    128.0,
                                    1.0,
                                    1,
                                )?;
                                Ok(())
                            },
                        )?;
                        params.add_group(
                            Params::HeightChannelStart,
                            Params::HeightChannelEnd,
                            "Height Channel",
                            false,
                            |params| {
                                params.add(
                                    Params::HeightChannel,
                                    "Channel",
                                    PopupDef::setup(|d| {
                                        d.set_options(&[
                                            "Luminance",
                                            "Alpha",
                                            "Red",
                                            "Green",
                                            "Blue",
                                        ]);
                                        d.set_default(1);
                                    }),
                                )?;
                                params.add(
                                    Params::HeightBlur,
                                    "Height Blur (px)",
                                    FloatSliderDef::setup(|d| {
                                        d.set_valid_min(0.0);
                                        d.set_valid_max(256.0);
                                        d.set_slider_min(0.0);
                                        d.set_slider_max(64.0);
                                        d.set_default(1.0);
                                        d.set_precision(1);
                                    }),
                                )?;
                                Ok(())
                            },
                        )?;
                        params.add(
                            Params::NormalStrength,
                            "Normal Strength",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(0.0);
                                d.set_valid_max(100.0);
                                d.set_slider_min(0.0);
                                d.set_slider_max(20.0);
                                d.set_default(5.0);
                                d.set_precision(2);
                            }),
                        )?;
                        params.add(
                            Params::InvertHeight,
                            "Invert Height",
                            CheckBoxDef::setup(|d| {
                                d.set_default(false);
                            }),
                        )?;
                        Ok(())
                    },
                )?;
                params.add_group(
                    Params::ExternalStart,
                    Params::ExternalEnd,
                    "Normal Layer",
                    false,
                    |params| {
                        params.add(Params::NormalLayer, "Layer", LayerDef::new())?;
                        params.add(
                            Params::NormalScale,
                            "XY Scale",
                            FloatSliderDef::setup(|d| {
                                d.set_valid_min(-20.0);
                                d.set_valid_max(20.0);
                                d.set_slider_min(-4.0);
                                d.set_slider_max(4.0);
                                d.set_default(1.0);
                                d.set_precision(3);
                            }),
                        )?;
                        params.add(
                            Params::NormalY,
                            "Y Convention",
                            PopupDef::setup(|d| {
                                d.set_options(&["OpenGL (+Y)", "DirectX (-Y)"]);
                                d.set_default(1);
                            }),
                        )?;
                        Ok(())
                    },
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::LightStart,
            Params::LightEnd,
            "Light",
            false,
            |params| {
                params.add_with_flags(
                    Params::LightType,
                    "Type",
                    PopupDef::setup(|d| {
                        d.set_options(&["Directional", "Point"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::LightColor,
                    "Color",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                )?;
                add_float_slider(
                    params,
                    Params::LightIntensity,
                    "Intensity",
                    0.0,
                    100.0,
                    0.0,
                    8.0,
                    1.0,
                    3,
                )?;
                params.add(
                    Params::DirectionAzimuth,
                    "Azimuth",
                    AngleDef::setup(|d| {
                        d.set_default(-45.0);
                    }),
                )?;
                add_float_slider(
                    params,
                    Params::DirectionElevation,
                    "Elevation",
                    -89.9,
                    89.9,
                    -89.9,
                    89.9,
                    45.0,
                    2,
                )?;
                params.add(
                    Params::PointPosition,
                    "Position",
                    PointDef::setup(|d| {
                        d.set_default((50.0, 50.0));
                    }),
                )?;
                add_float_slider(
                    params,
                    Params::PointHeight,
                    "Height (px)",
                    0.01,
                    100000.0,
                    1.0,
                    4000.0,
                    500.0,
                    2,
                )?;
                add_float_slider(
                    params,
                    Params::PointRadius,
                    "Falloff Radius (px)",
                    0.01,
                    100000.0,
                    1.0,
                    4000.0,
                    1000.0,
                    2,
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::MaterialStart,
            Params::MaterialEnd,
            "Output / Legacy Material",
            true,
            |params| {
                add_float_slider(
                    params,
                    Params::Ambient,
                    "Ambient",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    0.1,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::Diffuse,
                    "Diffuse",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    1.0,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::Specular,
                    "Specular",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    0.3,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::Shininess,
                    "Shininess",
                    1.0,
                    2048.0,
                    1.0,
                    256.0,
                    32.0,
                    1,
                )?;
                add_float_slider(
                    params,
                    Params::Exposure,
                    "Exposure (EV)",
                    -20.0,
                    20.0,
                    -5.0,
                    5.0,
                    0.0,
                    2,
                )?;
                params.add(
                    Params::ClampOutput,
                    "Clamp Output",
                    CheckBoxDef::setup(|d| {
                        d.set_default(true);
                    }),
                )?;
                params.add_with_flags(
                    Params::OutputMode,
                    "Output",
                    PopupDef::setup(|d| {
                        d.set_options(&[
                            "Relit Image",
                            "Normal Map",
                            "Height",
                            "Diffuse",
                            "Specular",
                            "Lighting",
                        ]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ParamUIFlags::empty(),
                )?;
                Ok(())
            },
        )?;

        // Keep this group appended so v0.2 project parameter indices remain stable.
        params.add_group(
            Params::RegionSolverStart,
            Params::RegionSolverEnd,
            "Color Region Solver",
            false,
            |params| {
                params.add_with_flags(
                    Params::RegionSolver,
                    "Surface Solver",
                    PopupDef::setup(|d| {
                        d.set_options(&["Distance Field (SDF)", "Poisson / Neumann (Smooth)"]);
                        d.set_default(2);
                    }),
                    supervise(),
                    ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::BoundaryCondition,
                    "Boundary Condition",
                    PopupDef::setup(|d| {
                        d.set_options(&["Fixed Height (Dirichlet)", "Normal Continuity (Neumann)"]);
                        d.set_default(2);
                    }),
                )?;
                params.add(
                    Params::PoissonIterations,
                    "Iterations",
                    SliderDef::setup(|d| {
                        d.set_valid_min(1);
                        d.set_valid_max(2000);
                        d.set_slider_min(1);
                        d.set_slider_max(400);
                        d.set_default(160);
                    }),
                )?;
                add_float_slider(
                    params,
                    Params::PoissonCurvature,
                    "Divergence / Curvature",
                    0.0,
                    50.0,
                    0.0,
                    10.0,
                    1.5,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::ScreenedDamping,
                    "Screened Damping",
                    0.0,
                    4.0,
                    0.0,
                    2.0,
                    0.02,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::EdgeFeather,
                    "Edge Feather (px)",
                    0.0,
                    1024.0,
                    0.0,
                    128.0,
                    8.0,
                    1,
                )?;
                Ok(())
            },
        )?;

        // Keep Principled controls appended so all v0.4 and earlier indices remain stable.
        params.add_group(
            Params::PrincipledStart,
            Params::PrincipledEnd,
            "Material",
            true,
            |params| {
                params.add_with_flags(
                    Params::MaterialModel,
                    "Surface",
                    PopupDef::setup(|d| {
                        d.set_options(&["Principled (GGX)", "Legacy Blinn-Phong"]);
                        d.set_default(1);
                    }),
                    supervise(),
                    ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::BaseTint,
                    "Base Tint",
                    ColorDef::setup(|d| {
                        d.set_default(Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                )?;
                add_float_slider(
                    params,
                    Params::Metallic,
                    "Metallic",
                    0.0,
                    1.0,
                    0.0,
                    1.0,
                    0.0,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::Roughness,
                    "Roughness",
                    0.0,
                    1.0,
                    0.0,
                    1.0,
                    0.5,
                    3,
                )?;
                add_float_slider(params, Params::Ior, "IOR", 1.0, 4.0, 1.0, 3.0, 1.5, 3)?;
                add_float_slider(
                    params,
                    Params::SpecularIorLevel,
                    "Specular IOR Level",
                    0.0,
                    1.0,
                    0.0,
                    1.0,
                    0.5,
                    3,
                )?;
                add_float_slider(
                    params,
                    Params::EnvironmentStrength,
                    "Environment Strength",
                    0.0,
                    10.0,
                    0.0,
                    2.0,
                    0.1,
                    3,
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
            ae::Command::About => out_data.set_return_msg(
                format!(
                    "AOD_ImageRelight - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{year} Aodaruma",
                    version = env!("CARGO_PKG_VERSION"),
                    year = env!("BUILD_YEAR")
                )
                .as_str(),
            ),
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::UseOutputExtent, true);
                out_data.set_out_flag(OutFlags::DeepColorAware, true);
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::FloatColorAware, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out_data.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ImageRelight")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render { in_layer, out_layer } => {
                self.render_legacy(in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                let request = extra.output_request();
                let callbacks = extra.callbacks();
                let input_result = checkout_full_layer(callbacks, 0, 1000, 0, &request, in_data)?;
                let full_rect: ae::Rect = input_result.max_result_rect.into();
                let _ = extra.union_result_rect(full_rect);
                let _ = extra.union_max_result_rect(full_rect);
                extra.set_returns_extra_pixels(true);

                if normal_source(params)? == NormalSource::Layer {
                    let param_index = params.index(Params::NormalLayer).ok_or(Error::BadCallbackParameter)?;
                    checkout_full_layer(
                        callbacks,
                        param_index as i32,
                        1001,
                        NORMAL_LAYER_CHECKOUT_ID,
                        &request,
                        in_data,
                    )?;
                }
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let settings = read_settings(params, in_data)?;
                let mut checkouts = LayerCheckoutGuard::<2>::new(cb);
                let render_result = (|| -> Result<(), Error> {
                    let normal_layer = if settings.normal_source == NormalSource::Layer {
                        checkouts.checkout(NORMAL_LAYER_CHECKOUT_ID)?
                    } else {
                        None
                    };
                    let input = checkouts.checkout(0)?;
                    let output = cb.checkout_output()?;
                    if let (Some(input), Some(output)) = (input, output) {
                        self.do_render(input, output, normal_layer.as_ref(), settings)
                    } else {
                        Ok(())
                    }
                })();
                let checkin_result = checkouts.checkin_all();
                render_result?;
                checkin_result?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if matches!(
                    params.type_at(param_index),
                    Params::NormalSource
                        | Params::GenerationMethod
                        | Params::RegionSolver
                        | Params::LightType
                        | Params::MaterialModel
                        | Params::OutputMode
                ) {
                    out_data.set_out_flag(OutFlags::RefreshUi, true);
                }
            }
            ae::Command::UpdateParamsUi => {
                let mut cloned = params.cloned();
                self.update_params_ui(in_data, &mut cloned)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn render_legacy(
        &self,
        in_data: InData,
        in_layer: Layer,
        out_layer: Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let settings = read_settings(params, in_data)?;
        if settings.normal_source == NormalSource::Layer {
            let checkout = params.checkout_at(Params::NormalLayer, None, None, None)?;
            let normal_layer = checkout.as_layer()?.value();
            self.do_render(in_layer, out_layer, normal_layer.as_ref(), settings)
        } else {
            self.do_render(in_layer, out_layer, None, settings)
        }
    }

    fn update_params_ui(
        &self,
        in_data: InData,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let generated = normal_source(params)? == NormalSource::Generated;
        for id in [
            Params::GeneratedStart,
            Params::GenerationMethod,
            Params::NormalStrength,
            Params::InvertHeight,
            Params::GeneratedEnd,
        ] {
            self.set_param_visible(in_data, params, id, generated)?;
        }
        for id in [
            Params::ExternalStart,
            Params::NormalLayer,
            Params::NormalScale,
            Params::NormalY,
            Params::ExternalEnd,
        ] {
            self.set_param_visible(in_data, params, id, !generated)?;
        }

        let color_regions =
            generated && generation_method(params)? == GenerationMethod::ColorRegions;
        for id in [
            Params::ColorRegionsStart,
            Params::ColorTolerance,
            Params::AlphaThreshold,
            Params::EdgeSoftness,
            Params::ColorRegionsEnd,
        ] {
            self.set_param_visible(in_data, params, id, color_regions)?;
        }
        for id in [
            Params::RegionSolverStart,
            Params::RegionSolver,
            Params::RegionSolverEnd,
        ] {
            self.set_param_visible(in_data, params, id, color_regions)?;
        }
        let sdf = color_regions && region_solver(params)? == RegionSolver::DistanceField;
        for id in [Params::RegionRadius, Params::RegionShape] {
            self.set_param_visible(in_data, params, id, sdf)?;
        }
        let poisson = color_regions && region_solver(params)? == RegionSolver::Poisson;
        for id in [
            Params::BoundaryCondition,
            Params::PoissonIterations,
            Params::PoissonCurvature,
            Params::ScreenedDamping,
            Params::EdgeFeather,
        ] {
            self.set_param_visible(in_data, params, id, poisson)?;
        }
        let height_channel =
            generated && generation_method(params)? == GenerationMethod::HeightChannel;
        for id in [
            Params::HeightChannelStart,
            Params::HeightChannel,
            Params::HeightBlur,
            Params::HeightChannelEnd,
        ] {
            self.set_param_visible(in_data, params, id, height_channel)?;
        }

        let light_type = light_type(params)?;
        let output = output_mode(params)?;
        let lighting = output.uses_lighting();
        for id in [
            Params::LightStart,
            Params::LightType,
            Params::LightColor,
            Params::LightIntensity,
            Params::LightEnd,
        ] {
            self.set_param_visible(in_data, params, id, lighting)?;
        }
        let directional = lighting && light_type == LightType::Directional;
        self.set_param_visible(in_data, params, Params::DirectionAzimuth, directional)?;
        self.set_param_visible(in_data, params, Params::DirectionElevation, directional)?;
        let point = lighting && light_type == LightType::Point;
        self.set_param_visible(in_data, params, Params::PointPosition, point)?;
        self.set_param_visible(in_data, params, Params::PointHeight, point)?;
        self.set_param_visible(in_data, params, Params::PointRadius, point)?;

        let principled = lighting && material_model(params)? == MaterialModel::Principled;
        let legacy = lighting && !principled;
        self.set_param_visible(
            in_data,
            params,
            Params::Ambient,
            legacy && matches!(output, OutputMode::Relit | OutputMode::Lighting),
        )?;
        self.set_param_visible(
            in_data,
            params,
            Params::Diffuse,
            legacy
                && matches!(
                    output,
                    OutputMode::Relit | OutputMode::Diffuse | OutputMode::Lighting
                ),
        )?;
        let show_specular = matches!(
            output,
            OutputMode::Relit | OutputMode::Specular | OutputMode::Lighting
        );
        self.set_param_visible(in_data, params, Params::Specular, legacy && show_specular)?;
        self.set_param_visible(in_data, params, Params::Shininess, legacy && show_specular)?;
        for id in [
            Params::PrincipledStart,
            Params::MaterialModel,
            Params::PrincipledEnd,
        ] {
            self.set_param_visible(in_data, params, id, lighting)?;
        }
        for id in [
            Params::BaseTint,
            Params::Metallic,
            Params::Roughness,
            Params::Ior,
            Params::SpecularIorLevel,
        ] {
            self.set_param_visible(in_data, params, id, principled)?;
        }
        self.set_param_visible(
            in_data,
            params,
            Params::EnvironmentStrength,
            principled && matches!(output, OutputMode::Relit | OutputMode::Lighting),
        )?;
        self.set_param_visible(in_data, params, Params::Exposure, lighting)?;
        self.set_param_visible(in_data, params, Params::ClampOutput, lighting)?;
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

    fn do_render(
        &self,
        in_layer: Layer,
        mut out_layer: Layer,
        normal_layer: Option<&Layer>,
        mut settings: Settings,
    ) -> Result<(), Error> {
        let src_width = in_layer.width();
        let src_height = in_layer.height();
        let out_width = out_layer.width();
        let out_height = out_layer.height();
        if src_width == 0 || src_height == 0 || out_width == 0 || out_height == 0 {
            return Ok(());
        }
        settings.point_position =
            point_in_checkout_world(settings.point_position, in_layer.origin());
        let source = read_layer(&in_layer);
        let source_origin = in_layer.origin();
        let output_origin = out_layer.origin();
        let surface_maps = if settings.normal_source == NormalSource::Generated
            && settings.generation_method == GenerationMethod::ColorRegions
        {
            let key = SurfaceCacheKey::new(&source, src_width, src_height, source_origin, settings);
            self.surface_cache.get_or_build(key, || {
                let (heights, normals) = build_surface_maps(
                    &source,
                    src_width,
                    src_height,
                    source_origin,
                    None,
                    settings,
                );
                SurfaceMaps::new(heights, normals)
            })
        } else {
            self.surface_cache.clear();
            let (heights, normals) = build_surface_maps(
                &source,
                src_width,
                src_height,
                source_origin,
                normal_layer,
                settings,
            );
            SurfaceMaps::new(heights, normals)
        };
        let shading = PreparedShading::new(settings);
        let origin_delta_x = output_origin.h - source_origin.h;
        let origin_delta_y = output_origin.v - source_origin.v;
        let out_world_type = out_layer.world_type();
        out_layer.iterate(0, out_height as i32, None, |x, y, mut dst| {
            let source_x = x + origin_delta_x;
            let source_y = y + origin_delta_y;
            let (albedo, height, normal) = if source_x >= 0
                && source_y >= 0
                && (source_x as usize) < src_width
                && (source_y as usize) < src_height
            {
                let index = source_y as usize * src_width + source_x as usize;
                (
                    source[index],
                    surface_maps.heights[index],
                    normalize3(surface_maps.normals[index]),
                )
            } else {
                let sx = source_x as f32;
                let sy = source_y as f32;
                (
                    sample_bilinear(
                        &source,
                        src_width,
                        src_height,
                        sx,
                        sy,
                        SampleEdge::Transparent,
                    ),
                    sample_scalar_bilinear(
                        surface_maps.heights.as_ref(),
                        src_width,
                        src_height,
                        sx,
                        sy,
                    ),
                    sample_normal_bilinear(
                        surface_maps.normals.as_ref(),
                        src_width,
                        src_height,
                        sx,
                        sy,
                    ),
                )
            };
            let pixel = shade_pixel(
                albedo,
                height,
                normal,
                source_x as f32,
                source_y as f32,
                shading,
            );
            match out_world_type {
                ae::aegp::WorldType::U8 => dst.set_from_u8(pixel.to_pixel8()),
                ae::aegp::WorldType::U15 => dst.set_from_u16(pixel.to_pixel16()),
                ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(pixel),
            }
            Ok(())
        })?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn add_float_slider(
    params: &mut Parameters<Params>,
    id: Params,
    name: &str,
    valid_min: f32,
    valid_max: f32,
    slider_min: f32,
    slider_max: f32,
    default: f64,
    precision: i16,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(valid_min);
            d.set_valid_max(valid_max);
            d.set_slider_min(slider_min);
            d.set_slider_max(slider_max);
            d.set_default(default);
            d.set_precision(precision);
        }),
    )?;
    Ok(())
}

fn normal_source(params: &Parameters<Params>) -> Result<NormalSource, Error> {
    Ok(
        if params.get(Params::NormalSource)?.as_popup()?.value() == 2 {
            NormalSource::Layer
        } else {
            NormalSource::Generated
        },
    )
}

fn generation_method(params: &Parameters<Params>) -> Result<GenerationMethod, Error> {
    Ok(
        if params.get(Params::GenerationMethod)?.as_popup()?.value() == 2 {
            GenerationMethod::HeightChannel
        } else {
            GenerationMethod::ColorRegions
        },
    )
}

fn region_solver(params: &Parameters<Params>) -> Result<RegionSolver, Error> {
    Ok(
        if params.get(Params::RegionSolver)?.as_popup()?.value() == 2 {
            RegionSolver::Poisson
        } else {
            RegionSolver::DistanceField
        },
    )
}

fn light_type(params: &Parameters<Params>) -> Result<LightType, Error> {
    Ok(if params.get(Params::LightType)?.as_popup()?.value() == 2 {
        LightType::Point
    } else {
        LightType::Directional
    })
}

fn material_model(params: &Parameters<Params>) -> Result<MaterialModel, Error> {
    Ok(
        if params.get(Params::MaterialModel)?.as_popup()?.value() == 2 {
            MaterialModel::LegacyBlinnPhong
        } else {
            MaterialModel::Principled
        },
    )
}

fn output_mode(params: &Parameters<Params>) -> Result<OutputMode, Error> {
    Ok(match params.get(Params::OutputMode)?.as_popup()?.value() {
        2 => OutputMode::Normal,
        3 => OutputMode::Height,
        4 => OutputMode::Diffuse,
        5 => OutputMode::Specular,
        6 => OutputMode::Lighting,
        _ => OutputMode::Relit,
    })
}

fn read_settings(params: &Parameters<Params>, in_data: InData) -> Result<Settings, Error> {
    let popup = |id| params.get(id)?.as_popup().map(|p| p.value());
    let slider = |id| params.get(id)?.as_float_slider().map(|p| p.value() as f32);
    let light_color = params.get(Params::LightColor)?.as_color()?.float_value()?;
    let base_tint = params.get(Params::BaseTint)?.as_color()?.float_value()?;
    Ok(Settings {
        normal_source: normal_source(params)?,
        generation_method: generation_method(params)?,
        color_tolerance: slider(Params::ColorTolerance)?.clamp(0.0, 1.0),
        alpha_threshold: slider(Params::AlphaThreshold)?.clamp(0.0, 1.0),
        region_solver: region_solver(params)?,
        region_radius: slider(Params::RegionRadius)?.max(0.01),
        region_shape: slider(Params::RegionShape)?.max(0.05),
        boundary_condition: if popup(Params::BoundaryCondition)? == 2 {
            BoundaryCondition::Neumann
        } else {
            BoundaryCondition::Dirichlet
        },
        poisson_iterations: params
            .get(Params::PoissonIterations)?
            .as_slider()?
            .value()
            .clamp(1, 2000) as usize,
        poisson_curvature: slider(Params::PoissonCurvature)?.clamp(0.0, 50.0),
        screened_damping: slider(Params::ScreenedDamping)?.clamp(0.0, 4.0),
        edge_feather: slider(Params::EdgeFeather)?.clamp(0.0, 1024.0),
        edge_softness: slider(Params::EdgeSoftness)?.max(0.0),
        height_channel: match popup(Params::HeightChannel)? {
            2 => HeightChannel::Alpha,
            3 => HeightChannel::Red,
            4 => HeightChannel::Green,
            5 => HeightChannel::Blue,
            _ => HeightChannel::Luminance,
        },
        height_blur: slider(Params::HeightBlur)?.round().clamp(0.0, 256.0),
        normal_strength: slider(Params::NormalStrength)?.max(0.0),
        invert_height: params.get(Params::InvertHeight)?.as_checkbox()?.value(),
        normal_scale: slider(Params::NormalScale)?,
        normal_y: if popup(Params::NormalY)? == 2 {
            NormalY::DirectX
        } else {
            NormalY::OpenGl
        },
        light_type: light_type(params)?,
        light_color: [light_color.red, light_color.green, light_color.blue],
        light_intensity: slider(Params::LightIntensity)?.max(0.0),
        direction_azimuth: (params
            .get(Params::DirectionAzimuth)?
            .as_angle()?
            .float_value()? as f32)
            .to_radians(),
        direction_elevation: slider(Params::DirectionElevation)?.to_radians(),
        point_position: params.get(Params::PointPosition)?.as_point()?.value(),
        point_height: slider(Params::PointHeight)?.max(0.01),
        point_radius: slider(Params::PointRadius)?.max(0.01),
        ambient: slider(Params::Ambient)?.max(0.0),
        diffuse: slider(Params::Diffuse)?.max(0.0),
        specular: slider(Params::Specular)?.max(0.0),
        shininess: slider(Params::Shininess)?.max(1.0),
        material_model: material_model(params)?,
        base_tint: [base_tint.red, base_tint.green, base_tint.blue],
        metallic: slider(Params::Metallic)?.clamp(0.0, 1.0),
        roughness: slider(Params::Roughness)?.clamp(0.0, 1.0),
        ior: slider(Params::Ior)?.clamp(1.0, 4.0),
        specular_ior_level: slider(Params::SpecularIorLevel)?.clamp(0.0, 1.0),
        environment_strength: slider(Params::EnvironmentStrength)?.max(0.0),
        exposure: slider(Params::Exposure)?,
        clamp_output: params.get(Params::ClampOutput)?.as_checkbox()?.value(),
        output_mode: output_mode(params)?,
        pixel_scale: render_pixel_scale(in_data),
    })
}

fn render_pixel_scale(in_data: InData) -> (f32, f32) {
    square_pixel_scale(
        rational_or_one(in_data.pixel_aspect_ratio()),
        rational_or_one(in_data.downsample_x()),
        rational_or_one(in_data.downsample_y()),
    )
}

fn rational_or_one(value: RationalScale) -> f32 {
    if value.num > 0 && value.den > 0 {
        let ratio = value.num as f32 / value.den as f32;
        if ratio.is_finite() && ratio > 0.0 {
            return ratio;
        }
    }
    1.0
}

fn square_pixel_scale(pixel_aspect: f32, downsample_x: f32, downsample_y: f32) -> (f32, f32) {
    (
        pixel_aspect.max(1.0e-6) / downsample_x.max(1.0e-6),
        1.0 / downsample_y.max(1.0e-6),
    )
}

fn point_in_checkout_world(point: (f32, f32), origin: ae::Point) -> (f32, f32) {
    local_point_in_target(point, ae::Point { h: 0, v: 0 }, origin)
}

#[cfg(test)]
fn source_local_from_output_local(
    x: f32,
    y: f32,
    output_origin: ae::Point,
    source_origin: ae::Point,
) -> (f32, f32) {
    local_point_in_target((x, y), output_origin, source_origin)
}

fn local_point_in_target(
    point: (f32, f32),
    point_origin: ae::Point,
    target_origin: ae::Point,
) -> (f32, f32) {
    (
        point.0 + (point_origin.h - target_origin.h) as f32,
        point.1 + (point_origin.v - target_origin.v) as f32,
    )
}

fn checkout_full_layer(
    callbacks: PreRenderCallbacks,
    param_index: i32,
    query_id: i32,
    checkout_id: u32,
    request: &ae::sys::PF_RenderRequest,
    in_data: InData,
) -> Result<ae::sys::PF_CheckoutResult, Error> {
    let query = callbacks.checkout_layer(
        param_index,
        query_id,
        request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;
    let mut full_request = *request;
    full_request.rect = query.max_result_rect;
    callbacks.checkout_layer(
        param_index,
        checkout_id as i32,
        &full_request,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )
}

fn build_surface_maps(
    source: &[PixelF32],
    width: usize,
    height: usize,
    source_origin: ae::Point,
    normal_layer: Option<&Layer>,
    settings: Settings,
) -> (Vec<f32>, Vec<[f32; 3]>) {
    match settings.normal_source {
        NormalSource::Generated => match settings.generation_method {
            GenerationMethod::ColorRegions => {
                color_region_surface_maps(source, width, height, settings)
            }
            GenerationMethod::HeightChannel => {
                let mut heights: Vec<f32> = source
                    .iter()
                    .copied()
                    .map(|pixel| {
                        height_value(pixel, settings.height_channel, settings.invert_height)
                    })
                    .collect();
                let blur_x = buffer_radius(settings.height_blur, settings.pixel_scale.0, width);
                let blur_y = buffer_radius(settings.height_blur, settings.pixel_scale.1, height);
                if blur_x > 0 || blur_y > 0 {
                    heights = box_blur(&heights, width, height, blur_x, blur_y);
                }
                let normals = normals_from_height(
                    &heights,
                    width,
                    height,
                    settings.normal_strength,
                    settings.pixel_scale,
                );
                (heights, normals)
            }
        },
        NormalSource::Layer => {
            let heights = vec![0.5; width * height];
            let mut normals = vec![[0.0, 0.0, 1.0]; width * height];
            if let Some(layer) = normal_layer {
                let layer_width = layer.width();
                let layer_height = layer.height();
                if layer_width > 0 && layer_height > 0 {
                    let normal_origin = layer.origin();
                    let pixels = read_layer(layer);
                    for y in 0..height {
                        for x in 0..width {
                            let (sx, sy) =
                                aligned_layer_coordinate(x, y, source_origin, normal_origin);
                            let pixel = sample_bilinear(
                                &pixels,
                                layer_width,
                                layer_height,
                                sx,
                                sy,
                                SampleEdge::Transparent,
                            );
                            normals[y * width + x] =
                                decode_normal(pixel, settings.normal_scale, settings.normal_y);
                        }
                    }
                }
            }
            (heights, normals)
        }
    }
}

const BACKGROUND_LABEL: u32 = u32::MAX;

fn color_region_surface_maps(
    source: &[PixelF32],
    width: usize,
    height: usize,
    settings: Settings,
) -> (Vec<f32>, Vec<[f32; 3]>) {
    let labels: Vec<u32> = source
        .iter()
        .copied()
        .map(|pixel| pack_region_label(pixel, settings.alpha_threshold, settings.color_tolerance))
        .collect();
    let boundary = region_boundary_mask(&labels, width, height);
    let distance =
        region_boundary_distance(&labels, &boundary, width, height, settings.pixel_scale);
    let (heights, neumann_boundary) = match settings.region_solver {
        RegionSolver::DistanceField => {
            let radius = settings.region_radius.max(1.0e-6);
            let shape = settings.region_shape.max(0.05);
            let mut heights = vec![0.0; width * height];
            for (index, value) in heights.iter_mut().enumerate() {
                if labels[index] == BACKGROUND_LABEL {
                    continue;
                }
                let shaped = (distance[index] / radius).clamp(0.0, 1.0).powf(shape);
                *value = if settings.invert_height {
                    1.0 - shaped
                } else {
                    shaped
                };
            }
            (heights, false)
        }
        RegionSolver::Poisson => (
            poisson_region_heights(
                &labels,
                &boundary,
                &distance,
                width,
                height,
                settings.boundary_condition,
                settings.poisson_iterations,
                settings.poisson_curvature,
                settings.screened_damping,
                settings.edge_feather,
                settings.invert_height,
                settings.pixel_scale,
            ),
            settings.boundary_condition == BoundaryCondition::Neumann,
        ),
    };
    let normals = normals_from_regions(
        &heights,
        &labels,
        &distance,
        width,
        height,
        settings.normal_strength,
        settings.edge_softness,
        settings.invert_height,
        neumann_boundary,
        settings.pixel_scale,
    );
    (heights, normals)
}

fn pack_region_label(pixel: PixelF32, alpha_threshold: f32, tolerance: f32) -> u32 {
    if !pixel.alpha.is_finite()
        || pixel.alpha <= 0.0
        || pixel.alpha < alpha_threshold.clamp(0.0, 1.0)
    {
        return BACKGROUND_LABEL;
    }
    let rgb = unpremultiplied_rgb(pixel);
    let scale = ae::MAX_CHANNEL8 as f32;
    let step = (tolerance.clamp(0.0, 1.0) * scale).round().max(1.0) as i32;
    let quantize = |value: f32| -> u32 {
        let raw = (value.clamp(0.0, 1.0) * scale + 0.5) as i32;
        if step <= 1 {
            raw as u32
        } else {
            (((raw + step / 2) / step) * step).clamp(0, ae::MAX_CHANNEL8 as i32) as u32
        }
    };
    (quantize(rgb[0]) << 16) | (quantize(rgb[1]) << 8) | quantize(rgb[2])
}

fn region_boundary_mask(labels: &[u32], width: usize, height: usize) -> Vec<bool> {
    let mut boundary = vec![false; width * height];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let label = labels[index];
            if label == BACKGROUND_LABEL {
                continue;
            }
            boundary[index] = x == 0
                || y == 0
                || x + 1 == width
                || y + 1 == height
                || labels[index - 1] != label
                || labels[index + 1] != label
                || labels[index - width] != label
                || labels[index + width] != label;
        }
    }
    boundary
}

fn region_boundary_distance(
    labels: &[u32],
    boundary: &[bool],
    width: usize,
    height: usize,
    pixel_scale: (f32, f32),
) -> Vec<f32> {
    let mut distance = vec![f32::INFINITY; width * height];
    for (index, is_boundary) in boundary.iter().copied().enumerate() {
        if is_boundary {
            distance[index] = 0.0;
        }
    }
    let step_x = pixel_scale.0.max(1.0e-6);
    let step_y = pixel_scale.1.max(1.0e-6);
    let step_diagonal = step_x.hypot(step_y);

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let label = labels[index];
            if label == BACKGROUND_LABEL || distance[index] == 0.0 {
                continue;
            }
            let mut best = distance[index];
            if x > 0 && labels[index - 1] == label {
                best = best.min(distance[index - 1] + step_x);
            }
            if y > 0 {
                if labels[index - width] == label {
                    best = best.min(distance[index - width] + step_y);
                }
                if x > 0 && labels[index - width - 1] == label {
                    best = best.min(distance[index - width - 1] + step_diagonal);
                }
                if x + 1 < width && labels[index - width + 1] == label {
                    best = best.min(distance[index - width + 1] + step_diagonal);
                }
            }
            distance[index] = best;
        }
    }

    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            let label = labels[index];
            if label == BACKGROUND_LABEL || distance[index] == 0.0 {
                continue;
            }
            let mut best = distance[index];
            if x + 1 < width && labels[index + 1] == label {
                best = best.min(distance[index + 1] + step_x);
            }
            if y + 1 < height {
                if labels[index + width] == label {
                    best = best.min(distance[index + width] + step_y);
                }
                if x + 1 < width && labels[index + width + 1] == label {
                    best = best.min(distance[index + width + 1] + step_diagonal);
                }
                if x > 0 && labels[index + width - 1] == label {
                    best = best.min(distance[index + width - 1] + step_diagonal);
                }
            }
            distance[index] = best;
        }
    }
    distance
}

fn region_components(labels: &[u32], width: usize, height: usize) -> Vec<Vec<usize>> {
    let mut components = Vec::new();
    let mut visited = vec![false; labels.len()];
    let mut queue = VecDeque::new();
    for seed in 0..labels.len() {
        if visited[seed] || labels[seed] == BACKGROUND_LABEL {
            continue;
        }
        let label = labels[seed];
        let mut component = Vec::new();
        visited[seed] = true;
        queue.push_back(seed);
        while let Some(index) = queue.pop_front() {
            component.push(index);
            let x = index % width;
            let y = index / width;
            let neighbors = [
                (x > 0).then_some(index.saturating_sub(1)),
                (x + 1 < width).then_some(index + 1),
                (y > 0).then_some(index.saturating_sub(width)),
                (y + 1 < height).then_some(index + width),
            ];
            for neighbor in neighbors.into_iter().flatten() {
                if !visited[neighbor] && labels[neighbor] == label {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

fn poisson_profiles(distance: &[f32], components: &[Vec<usize>], edge_feather: f32) -> Vec<f32> {
    let mut profile = vec![0.0; distance.len()];
    for component in components {
        let max_distance = component
            .iter()
            .map(|&index| distance[index])
            .filter(|value| value.is_finite())
            .fold(0.0_f32, f32::max);
        if max_distance <= 1.0e-8 {
            continue;
        }
        for &index in component {
            let base = (distance[index] / max_distance).clamp(0.0, 1.0);
            let feather = if edge_feather > 0.0 {
                smoothstep(0.0, edge_feather, distance[index])
            } else {
                1.0
            };
            profile[index] = base * feather;
        }
    }
    profile
}

fn poisson_rhs(
    profile: &[f32],
    boundary: &[bool],
    components: &[Vec<usize>],
    condition: BoundaryCondition,
    curvature: f32,
    lambda_squared: f32,
) -> Vec<f32> {
    let mut rhs = vec![0.0; profile.len()];
    for component in components {
        let mean = if condition == BoundaryCondition::Neumann && !component.is_empty() {
            component.iter().map(|&index| profile[index]).sum::<f32>() / component.len() as f32
        } else {
            0.0
        };
        for &index in component {
            if condition == BoundaryCondition::Dirichlet && boundary[index] {
                continue;
            }
            let forcing = if condition == BoundaryCondition::Neumann {
                // Removing the component mean makes the unscreened Neumann system compatible.
                curvature * (profile[index] - mean)
            } else {
                curvature * profile[index]
            };
            rhs[index] = forcing + lambda_squared * profile[index];
        }
    }
    rhs
}

fn poisson_sor_omega(width: usize, height: usize) -> f32 {
    let extent = width.max(height);
    if extent <= 2 {
        return 1.0;
    }
    let rho = (std::f32::consts::PI / extent as f32).cos();
    (2.0 / (1.0 + (1.0 - rho * rho).max(0.0).sqrt())).clamp(1.0, 1.8)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn poisson_candidate(
    index: usize,
    heights: &[f32],
    labels: &[u32],
    rhs: &[f32],
    width: usize,
    height: usize,
    condition: BoundaryCondition,
    lambda_squared: f32,
    weight_x: f32,
    weight_y: f32,
) -> f32 {
    let x = index % width;
    let y = index / width;
    let label = labels[index];
    let mut sum = 0.0;
    let mut diagonal = lambda_squared;
    let mut add_neighbor = |neighbor: Option<usize>, weight: f32| {
        if let Some(neighbor) = neighbor
            && labels[neighbor] == label
        {
            sum += weight * heights[neighbor];
            diagonal += weight;
        } else if condition == BoundaryCondition::Dirichlet {
            // A missing or differently labelled neighbor has fixed zero height.
            diagonal += weight;
        }
    };
    add_neighbor((x > 0).then_some(index.saturating_sub(1)), weight_x);
    add_neighbor((x + 1 < width).then_some(index + 1), weight_x);
    add_neighbor((y > 0).then_some(index.saturating_sub(width)), weight_y);
    add_neighbor((y + 1 < height).then_some(index + width), weight_y);
    if diagonal <= 1.0e-12 {
        0.0
    } else {
        (rhs[index] + sum) / diagonal
    }
}

const POISSON_LEFT: u8 = 1 << 0;
const POISSON_RIGHT: u8 = 1 << 1;
const POISSON_TOP: u8 = 1 << 2;
const POISSON_BOTTOM: u8 = 1 << 3;

#[derive(Clone, Copy, Debug)]
struct PoissonStencil {
    index: u32,
    neighbor_mask: u8,
    diagonal: f32,
}

#[allow(clippy::too_many_arguments)]
fn prepare_poisson_stencil(
    index: usize,
    labels: &[u32],
    width: usize,
    height: usize,
    condition: BoundaryCondition,
    lambda_squared: f32,
    weight_x: f32,
    weight_y: f32,
) -> PoissonStencil {
    let x = index % width;
    let y = index / width;
    let label = labels[index];
    let mut neighbor_mask = 0;
    let mut diagonal = lambda_squared;
    let mut add_neighbor = |neighbor: Option<usize>, bit: u8, weight: f32| {
        if let Some(neighbor) = neighbor
            && labels[neighbor] == label
        {
            neighbor_mask |= bit;
            diagonal += weight;
        } else if condition == BoundaryCondition::Dirichlet {
            diagonal += weight;
        }
    };
    add_neighbor(
        (x > 0).then_some(index.saturating_sub(1)),
        POISSON_LEFT,
        weight_x,
    );
    add_neighbor(
        (x + 1 < width).then_some(index + 1),
        POISSON_RIGHT,
        weight_x,
    );
    add_neighbor(
        (y > 0).then_some(index.saturating_sub(width)),
        POISSON_TOP,
        weight_y,
    );
    add_neighbor(
        (y + 1 < height).then_some(index + width),
        POISSON_BOTTOM,
        weight_y,
    );
    PoissonStencil {
        index: index as u32,
        neighbor_mask,
        diagonal,
    }
}

fn poisson_candidate_from_stencil(
    stencil: PoissonStencil,
    heights: &[f32],
    rhs: &[f32],
    width: usize,
    weight_x: f32,
    weight_y: f32,
) -> f32 {
    if stencil.diagonal <= 1.0e-12 {
        return 0.0;
    }
    let index = stencil.index as usize;
    let mut sum = 0.0;
    if stencil.neighbor_mask & POISSON_LEFT != 0 {
        sum += weight_x * heights[index - 1];
    }
    if stencil.neighbor_mask & POISSON_RIGHT != 0 {
        sum += weight_x * heights[index + 1];
    }
    if stencil.neighbor_mask & POISSON_TOP != 0 {
        sum += weight_y * heights[index - width];
    }
    if stencil.neighbor_mask & POISSON_BOTTOM != 0 {
        sum += weight_y * heights[index + width];
    }
    (rhs[index] + sum) / stencil.diagonal
}

fn remove_component_means(values: &mut [f32], components: &[Vec<usize>]) {
    for component in components {
        if component.is_empty() {
            continue;
        }
        let mean =
            component.iter().map(|&index| values[index]).sum::<f32>() / component.len() as f32;
        for &index in component {
            values[index] -= mean;
        }
    }
}

fn normalize_component_heights(
    heights: &mut [f32],
    components: &[Vec<usize>],
    condition: BoundaryCondition,
    relief_scale: f32,
    invert: bool,
) {
    let relief_scale = relief_scale.clamp(0.0, 1.0);
    for component in components {
        let mut minimum = f32::INFINITY;
        let mut maximum = f32::NEG_INFINITY;
        for &index in component {
            let value = heights[index];
            if value.is_finite() {
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
        }
        if condition == BoundaryCondition::Dirichlet {
            minimum = 0.0;
        }
        let range = maximum - minimum;
        for &index in component {
            let mut value = if range.is_finite() && range > 1.0e-8 {
                ((heights[index] - minimum) / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            value *= relief_scale;
            if invert {
                value = 1.0 - value;
            }
            heights[index] = value;
        }
    }
}

fn poisson_relief_scale(curvature: f32, screened_damping: f32) -> f32 {
    let total_drive = curvature.max(0.0) + screened_damping.max(0.0);
    if !total_drive.is_finite() || total_drive <= 0.0 {
        0.0
    } else {
        1.0 - (-total_drive).exp()
    }
}

#[cfg(feature = "gpu_wgpu")]
static POISSON_GPU: OnceLock<Result<Arc<WgpuContext>, String>> = OnceLock::new();

#[cfg(feature = "gpu_wgpu")]
#[allow(clippy::too_many_arguments)]
fn try_poisson_gpu(
    labels: &[u32],
    boundary: &[bool],
    rhs: &[f32],
    width: usize,
    height: usize,
    condition: BoundaryCondition,
    iterations: usize,
    lambda_squared: f32,
    weight_x: f32,
    weight_y: f32,
    omega: f32,
) -> Option<Vec<f32>> {
    // Pure Neumann systems need a per-component mean projection after every iteration. Keep that
    // uncommon configuration on the CPU until the projection also lives on the GPU.
    if condition == BoundaryCondition::Neumann && lambda_squared <= 1.0e-12 {
        return None;
    }

    let updates = labels.len().saturating_mul(iterations.max(1));
    let preference = env::var("AOD_IMAGE_RELIGHT_GPU")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let enabled = match preference.as_str() {
        "0" | "off" | "false" | "cpu" => false,
        "1" | "on" | "true" | "gpu" => true,
        _ => updates >= 8 * 1024 * 1024,
    };
    if !enabled {
        return None;
    }

    let context = POISSON_GPU
        .get_or_init(|| WgpuContext::new().map(Arc::new))
        .as_ref()
        .ok()?;
    let heights = context
        .solve(
            labels,
            boundary,
            rhs,
            SolveParams {
                width,
                height,
                condition: match condition {
                    BoundaryCondition::Dirichlet => GpuBoundaryCondition::Dirichlet,
                    BoundaryCondition::Neumann => GpuBoundaryCondition::Neumann,
                },
                iterations,
                lambda_squared,
                weight_x,
                weight_y,
                omega,
            },
        )
        .ok()?;
    heights
        .iter()
        .all(|value| value.is_finite())
        .then_some(heights)
}

#[cfg(not(feature = "gpu_wgpu"))]
#[allow(clippy::too_many_arguments)]
fn try_poisson_gpu(
    _labels: &[u32],
    _boundary: &[bool],
    _rhs: &[f32],
    _width: usize,
    _height: usize,
    _condition: BoundaryCondition,
    _iterations: usize,
    _lambda_squared: f32,
    _weight_x: f32,
    _weight_y: f32,
    _omega: f32,
) -> Option<Vec<f32>> {
    None
}

#[allow(clippy::too_many_arguments)]
fn solve_poisson_cpu(
    labels: &[u32],
    boundary: &[bool],
    rhs: &[f32],
    components: &[Vec<usize>],
    width: usize,
    height: usize,
    condition: BoundaryCondition,
    iterations: usize,
    lambda_squared: f32,
    weight_x: f32,
    weight_y: f32,
    omega: f32,
) -> Vec<f32> {
    // A zero initialization avoids turning a finite-iteration seed residue into false relief.
    let mut heights = vec![0.0; labels.len()];
    let mut red = Vec::new();
    let mut black = Vec::new();
    for component in components {
        for &index in component {
            if condition == BoundaryCondition::Dirichlet && boundary[index] {
                continue;
            }
            let x = index % width;
            let y = index / width;
            let stencil = prepare_poisson_stencil(
                index,
                labels,
                width,
                height,
                condition,
                lambda_squared,
                weight_x,
                weight_y,
            );
            if ((x ^ y) & 1) == 0 {
                red.push(stencil);
            } else {
                black.push(stencil);
            }
        }
    }

    for _ in 0..iterations.max(1) {
        let mut max_delta = 0.0_f32;
        for indices in [&red, &black] {
            for &stencil in indices {
                let index = stencil.index as usize;
                let candidate = poisson_candidate_from_stencil(
                    stencil, &heights, rhs, width, weight_x, weight_y,
                );
                let old = heights[index];
                let updated = old + omega * (candidate - old);
                heights[index] = if updated.is_finite() { updated } else { old };
                max_delta = max_delta.max((heights[index] - old).abs());
            }
        }
        if condition == BoundaryCondition::Neumann && lambda_squared <= 1.0e-12 {
            // Pure Neumann solutions are defined up to a constant; pin each component mean.
            remove_component_means(&mut heights, components);
        }
        if max_delta <= 1.0e-5 {
            break;
        }
    }
    heights
}

#[allow(clippy::too_many_arguments)]
fn poisson_region_heights(
    labels: &[u32],
    boundary: &[bool],
    distance: &[f32],
    width: usize,
    height: usize,
    condition: BoundaryCondition,
    iterations: usize,
    curvature: f32,
    screened_damping: f32,
    edge_feather: f32,
    invert: bool,
    pixel_scale: (f32, f32),
) -> Vec<f32> {
    let relief_scale = poisson_relief_scale(curvature, screened_damping);
    if relief_scale <= 0.0 {
        return labels
            .iter()
            .map(|&label| {
                if invert && label != BACKGROUND_LABEL {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
    }
    let components = region_components(labels, width, height);
    let profile = poisson_profiles(distance, &components, edge_feather.max(0.0));
    let lambda_squared = screened_damping.max(0.0).powi(2);
    let rhs = poisson_rhs(
        &profile,
        boundary,
        &components,
        condition,
        curvature,
        lambda_squared,
    );
    let weight_x = 1.0 / pixel_scale.0.max(1.0e-6).powi(2);
    let weight_y = 1.0 / pixel_scale.1.max(1.0e-6).powi(2);
    let omega = poisson_sor_omega(width, height);
    let mut heights = try_poisson_gpu(
        labels,
        boundary,
        &rhs,
        width,
        height,
        condition,
        iterations.max(1),
        lambda_squared,
        weight_x,
        weight_y,
        omega,
    )
    .unwrap_or_else(|| {
        solve_poisson_cpu(
            labels,
            boundary,
            &rhs,
            &components,
            width,
            height,
            condition,
            iterations,
            lambda_squared,
            weight_x,
            weight_y,
            omega,
        )
    });

    normalize_component_heights(&mut heights, &components, condition, relief_scale, invert);
    heights
}

#[allow(clippy::too_many_arguments)]
fn normals_from_regions(
    heights: &[f32],
    labels: &[u32],
    distance: &[f32],
    width: usize,
    height: usize,
    strength: f32,
    edge_softness: f32,
    inverted: bool,
    neumann_boundary: bool,
    pixel_scale: (f32, f32),
) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0, 0.0, 1.0]; width * height];
    let boundary_height = if inverted { 1.0 } else { 0.0 };
    let step_x = pixel_scale.0.max(1.0e-6);
    let step_y = pixel_scale.1.max(1.0e-6);
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let label = labels[index];
            if label == BACKGROUND_LABEL {
                continue;
            }
            let boundary_height = if neumann_boundary {
                heights[index]
            } else {
                boundary_height
            };
            let sample = |sample_x: usize, sample_y: usize| {
                let sample_index = sample_y * width + sample_x;
                if labels[sample_index] == label {
                    heights[sample_index]
                } else {
                    boundary_height
                }
            };
            let left = if x > 0 {
                sample(x - 1, y)
            } else {
                boundary_height
            };
            let right = if x + 1 < width {
                sample(x + 1, y)
            } else {
                boundary_height
            };
            let top = if y > 0 {
                sample(x, y - 1)
            } else {
                boundary_height
            };
            let bottom = if y + 1 < height {
                sample(x, y + 1)
            } else {
                boundary_height
            };
            let edge_weight = if edge_softness > 0.0 {
                smoothstep(0.0, edge_softness, distance[index])
            } else {
                1.0
            };
            let dx = (right - left) * 0.5 / step_x;
            let dy = (bottom - top) * 0.5 / step_y;
            normals[index] = normalize3([
                -dx * strength * edge_weight,
                -dy * strength * edge_weight,
                1.0,
            ]);
        }
    }
    normals
}

fn buffer_radius(distance: f32, square_per_pixel: f32, extent: usize) -> usize {
    ((distance.max(0.0) / square_per_pixel.max(1.0e-6)).round() as usize)
        .min(extent.saturating_sub(1))
}

fn aligned_layer_coordinate(
    x: usize,
    y: usize,
    source_origin: ae::Point,
    target_origin: ae::Point,
) -> (f32, f32) {
    (
        x as f32 + (source_origin.h - target_origin.h) as f32,
        y as f32 + (source_origin.v - target_origin.v) as f32,
    )
}

fn height_value(pixel: PixelF32, channel: HeightChannel, invert: bool) -> f32 {
    let rgb = unpremultiplied_rgb(pixel);
    let value = match channel {
        HeightChannel::Luminance => luma(pixel),
        HeightChannel::Alpha => pixel.alpha,
        HeightChannel::Red => rgb[0],
        HeightChannel::Green => rgb[1],
        HeightChannel::Blue => rgb[2],
    }
    .clamp(0.0, 1.0);
    if invert { 1.0 - value } else { value }
}

fn box_blur(
    source: &[f32],
    width: usize,
    height: usize,
    radius_x: usize,
    radius_y: usize,
) -> Vec<f32> {
    if (radius_x == 0 && radius_y == 0) || width == 0 || height == 0 {
        return source.to_vec();
    }
    let mut horizontal = vec![0.0; source.len()];
    let mut prefix = vec![0.0; width + 1];
    for y in 0..height {
        prefix[0] = 0.0;
        for x in 0..width {
            prefix[x + 1] = prefix[x] + source[y * width + x];
        }
        for x in 0..width {
            let left = x.saturating_sub(radius_x);
            let right = (x + radius_x + 1).min(width);
            horizontal[y * width + x] = (prefix[right] - prefix[left]) / (right - left) as f32;
        }
    }
    let mut output = vec![0.0; source.len()];
    prefix.resize(height + 1, 0.0);
    for x in 0..width {
        prefix[0] = 0.0;
        for y in 0..height {
            prefix[y + 1] = prefix[y] + horizontal[y * width + x];
        }
        for y in 0..height {
            let top = y.saturating_sub(radius_y);
            let bottom = (y + radius_y + 1).min(height);
            output[y * width + x] = (prefix[bottom] - prefix[top]) / (bottom - top) as f32;
        }
    }
    output
}

fn normals_from_height(
    heights: &[f32],
    width: usize,
    height: usize,
    strength: f32,
    pixel_scale: (f32, f32),
) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0, 0.0, 1.0]; width * height];
    for y in 0..height {
        let top = y.saturating_sub(1);
        let bottom = (y + 1).min(height - 1);
        for x in 0..width {
            let left = x.saturating_sub(1);
            let right = (x + 1).min(width - 1);
            let dx_span = (right - left).max(1) as f32 * pixel_scale.0;
            let dy_span = (bottom - top).max(1) as f32 * pixel_scale.1;
            let dx = (heights[y * width + right] - heights[y * width + left]) / dx_span;
            let dy = (heights[bottom * width + x] - heights[top * width + x]) / dy_span;
            normals[y * width + x] = normalize3([-dx * strength, -dy * strength, 1.0]);
        }
    }
    normals
}

fn decode_normal(pixel: PixelF32, scale: f32, convention: NormalY) -> [f32; 3] {
    if pixel.alpha <= 1.0e-6 {
        return [0.0, 0.0, 1.0];
    }
    let rgb = unpremultiplied_rgb(pixel);
    let mut y = rgb[1] * 2.0 - 1.0;
    if convention == NormalY::DirectX {
        y = -y;
    }
    normalize3([(rgb[0] * 2.0 - 1.0) * scale, y * scale, rgb[2] * 2.0 - 1.0])
}

#[derive(Clone, Copy)]
struct PreparedShading {
    settings: Settings,
    exposure: f32,
    dielectric_f0: f32,
    roughness_alpha_squared: f32,
    directional: Option<([f32; 3], [f32; 3])>,
}

impl PreparedShading {
    fn new(settings: Settings) -> Self {
        let directional = if settings.light_type == LightType::Directional {
            let (light, _) = light_vector(0.0, 0.0, settings);
            let light = normalize3(light);
            let halfway = normalize3([light[0], light[1], light[2] + 1.0]);
            Some((light, halfway))
        } else {
            None
        };
        Self {
            settings,
            exposure: 2.0_f32.powf(settings.exposure),
            dielectric_f0: dielectric_f0(settings.ior, settings.specular_ior_level),
            roughness_alpha_squared: settings.roughness.max(0.045).powi(4),
            directional,
        }
    }
}

fn shade_pixel(
    albedo: PixelF32,
    height: f32,
    normal: [f32; 3],
    x: f32,
    y: f32,
    shading: PreparedShading,
) -> PixelF32 {
    let settings = shading.settings;
    let alpha = albedo.alpha.clamp(0.0, 1.0);
    if settings.output_mode == OutputMode::Normal {
        return PixelF32 {
            alpha,
            red: (normal[0] * 0.5 + 0.5) * alpha,
            green: (normal[1] * 0.5 + 0.5) * alpha,
            blue: (normal[2] * 0.5 + 0.5) * alpha,
        };
    }
    if settings.output_mode == OutputMode::Height {
        return gray_with_alpha(height, alpha);
    }

    let (light, halfway, attenuation) = if let Some((light, halfway)) = shading.directional {
        (light, halfway, 1.0)
    } else {
        let (light, attenuation) = light_vector(x, y, settings);
        let light = normalize3(light);
        let halfway = normalize3([light[0], light[1], light[2] + 1.0]);
        (light, halfway, attenuation)
    };
    let rgb = unpremultiplied_rgb(albedo);
    let mut output_rgb = match settings.material_model {
        MaterialModel::Principled => {
            shade_principled(rgb, normal, light, halfway, attenuation, shading)
        }
        MaterialModel::LegacyBlinnPhong => {
            shade_legacy(rgb, normal, light, halfway, attenuation, settings)
        }
    };
    for channel in &mut output_rgb {
        *channel *= shading.exposure;
    }
    sanitize_pixel(
        PixelF32 {
            alpha,
            red: output_rgb[0] * alpha,
            green: output_rgb[1] * alpha,
            blue: output_rgb[2] * alpha,
        },
        settings.clamp_output,
    )
}

fn shade_legacy(
    albedo: [f32; 3],
    normal: [f32; 3],
    light: [f32; 3],
    halfway: [f32; 3],
    attenuation: f32,
    settings: Settings,
) -> [f32; 3] {
    let terms = blinn_terms_prepared(normal, light, halfway, settings.shininess);
    let diffuse = terms.0 * settings.diffuse * settings.light_intensity * attenuation;
    let specular = terms.1 * settings.specular * settings.light_intensity * attenuation;
    match settings.output_mode {
        OutputMode::Diffuse => [diffuse; 3],
        OutputMode::Specular => [specular; 3],
        OutputMode::Lighting => [
            settings.ambient + settings.light_color[0] * (diffuse + specular),
            settings.ambient + settings.light_color[1] * (diffuse + specular),
            settings.ambient + settings.light_color[2] * (diffuse + specular),
        ],
        OutputMode::Relit => [
            albedo[0] * (settings.ambient + settings.light_color[0] * diffuse)
                + settings.light_color[0] * specular,
            albedo[1] * (settings.ambient + settings.light_color[1] * diffuse)
                + settings.light_color[1] * specular,
            albedo[2] * (settings.ambient + settings.light_color[2] * diffuse)
                + settings.light_color[2] * specular,
        ],
        _ => [0.0; 3],
    }
}

fn shade_principled(
    source_color: [f32; 3],
    normal: [f32; 3],
    light: [f32; 3],
    halfway: [f32; 3],
    attenuation: f32,
    shading: PreparedShading,
) -> [f32; 3] {
    let settings = shading.settings;
    let base_color = [
        source_color[0] * settings.base_tint[0],
        source_color[1] * settings.base_tint[1],
        source_color[2] * settings.base_tint[2],
    ];
    let (diffuse, specular) = principled_terms_prepared(
        normal,
        light,
        halfway,
        base_color,
        settings.metallic,
        shading.dielectric_f0,
        shading.roughness_alpha_squared,
    );
    let direct_scale = settings.light_intensity * attenuation;
    let direct_diffuse = multiply_light(diffuse, settings.light_color, direct_scale);
    let direct_specular = multiply_light(specular, settings.light_color, direct_scale);
    let environment = [
        base_color[0] * settings.environment_strength,
        base_color[1] * settings.environment_strength,
        base_color[2] * settings.environment_strength,
    ];
    match settings.output_mode {
        OutputMode::Diffuse => direct_diffuse,
        OutputMode::Specular => direct_specular,
        OutputMode::Lighting | OutputMode::Relit => [
            environment[0] + direct_diffuse[0] + direct_specular[0],
            environment[1] + direct_diffuse[1] + direct_specular[1],
            environment[2] + direct_diffuse[2] + direct_specular[2],
        ],
        _ => [0.0; 3],
    }
}

fn multiply_light(value: [f32; 3], light_color: [f32; 3], scale: f32) -> [f32; 3] {
    [
        value[0] * light_color[0] * scale,
        value[1] * light_color[1] * scale,
        value[2] * light_color[2] * scale,
    ]
}

fn dielectric_f0(ior: f32, specular_ior_level: f32) -> f32 {
    let ratio = (ior.clamp(1.0, 4.0) - 1.0) / (ior.clamp(1.0, 4.0) + 1.0);
    (ratio * ratio * specular_ior_level.clamp(0.0, 1.0) * 2.0).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn principled_terms_prepared(
    normal: [f32; 3],
    light: [f32; 3],
    halfway: [f32; 3],
    base_color: [f32; 3],
    metallic: f32,
    dielectric_f0: f32,
    alpha_squared: f32,
) -> ([f32; 3], [f32; 3]) {
    let normal = normalize3(normal);
    let no_l = dot3(normal, light).max(0.0);
    let no_v = normal[2].max(0.0);
    if no_l <= 0.0 || no_v <= 0.0 {
        return ([0.0; 3], [0.0; 3]);
    }
    let no_h = dot3(normal, halfway).max(0.0);
    let vo_h = halfway[2].max(0.0);
    let alpha_squared = alpha_squared.max(1.0e-8);
    let denominator = no_h * no_h * (alpha_squared - 1.0) + 1.0;
    let distribution =
        alpha_squared / (std::f32::consts::PI * denominator * denominator).max(1.0e-8);
    let geometry = smith_ggx_g1(no_l, alpha_squared) * smith_ggx_g1(no_v, alpha_squared);
    let metallic = metallic.clamp(0.0, 1.0);
    let one_minus_cosine_fifth = (1.0 - vo_h).clamp(0.0, 1.0).powi(5);
    let mut diffuse = [0.0; 3];
    let mut specular = [0.0; 3];
    for channel in 0..3 {
        let conductor_f0 = base_color[channel].clamp(0.0, 1.0);
        let f0 = dielectric_f0 * (1.0 - metallic) + conductor_f0 * metallic;
        let fresnel = f0 + (1.0 - f0) * one_minus_cosine_fifth;
        let diffuse_brdf =
            (1.0 - fresnel) * (1.0 - metallic) * base_color[channel] / std::f32::consts::PI;
        let specular_brdf = distribution * geometry * fresnel / (4.0 * no_v * no_l).max(1.0e-8);
        diffuse[channel] = diffuse_brdf * no_l;
        specular[channel] = specular_brdf * no_l;
    }
    (diffuse, specular)
}

fn smith_ggx_g1(no_x: f32, alpha_squared: f32) -> f32 {
    let no_x = no_x.clamp(0.0, 1.0);
    if no_x <= 0.0 {
        return 0.0;
    }
    2.0 * no_x / (no_x + (alpha_squared + (1.0 - alpha_squared) * no_x * no_x).sqrt())
}

fn light_vector(x: f32, y: f32, settings: Settings) -> ([f32; 3], f32) {
    match settings.light_type {
        LightType::Directional => {
            let planar = settings.direction_elevation.cos();
            (
                normalize3([
                    planar * settings.direction_azimuth.cos(),
                    planar * settings.direction_azimuth.sin(),
                    settings.direction_elevation.sin(),
                ]),
                1.0,
            )
        }
        LightType::Point => {
            let vector = [
                (settings.point_position.0 - x) * settings.pixel_scale.0,
                (y - settings.point_position.1) * settings.pixel_scale.1,
                settings.point_height,
            ];
            let distance = length3(vector);
            let ratio = distance / settings.point_radius;
            (normalize3(vector), 1.0 / (1.0 + ratio * ratio))
        }
    }
}

#[cfg(test)]
fn blinn_terms(normal: [f32; 3], light: [f32; 3], shininess: f32) -> (f32, f32) {
    let normal = normalize3(normal);
    let light = normalize3(light);
    let halfway = normalize3([light[0], light[1], light[2] + 1.0]);
    blinn_terms_prepared(normal, light, halfway, shininess)
}

fn blinn_terms_prepared(
    normal: [f32; 3],
    light: [f32; 3],
    halfway: [f32; 3],
    shininess: f32,
) -> (f32, f32) {
    let normal = normalize3(normal);
    let diffuse = dot3(normal, light).max(0.0);
    if diffuse <= 0.0 {
        return (0.0, 0.0);
    }
    let specular = dot3(normal, halfway).max(0.0).powf(shininess.max(1.0));
    (diffuse, specular)
}

fn sample_normal_bilinear(
    values: &[[f32; 3]],
    width: usize,
    height: usize,
    x: f32,
    y: f32,
) -> [f32; 3] {
    let x = x.clamp(0.0, width.saturating_sub(1) as f32);
    let y = y.clamp(0.0, height.saturating_sub(1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let mut value = [0.0; 3];
    for channel in 0..3 {
        let top =
            values[y0 * width + x0][channel] * (1.0 - tx) + values[y0 * width + x1][channel] * tx;
        let bottom =
            values[y1 * width + x0][channel] * (1.0 - tx) + values[y1 * width + x1][channel] * tx;
        value[channel] = top * (1.0 - ty) + bottom * ty;
    }
    normalize3(value)
}

fn sample_scalar_bilinear(values: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, width.saturating_sub(1) as f32);
    let y = y.clamp(0.0, height.saturating_sub(1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let top = values[y0 * width + x0] * (1.0 - tx) + values[y0 * width + x1] * tx;
    let bottom = values[y1 * width + x0] * (1.0 - tx) + values[y1 * width + x1] * tx;
    top * (1.0 - ty) + bottom * ty
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn normalize3(vector: [f32; 3]) -> [f32; 3] {
    let length = length3(vector);
    if !length.is_finite() || length <= 1.0e-8 {
        [0.0, 0.0, 1.0]
    } else {
        [vector[0] / length, vector[1] / length, vector[2] / length]
    }
}

fn length3(vector: [f32; 3]) -> f32 {
    dot3(vector, vector).sqrt()
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn gray_with_alpha(value: f32, alpha: f32) -> PixelF32 {
    PixelF32 {
        alpha,
        red: value * alpha,
        green: value * alpha,
        blue: value * alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings() -> Settings {
        Settings {
            normal_source: NormalSource::Generated,
            generation_method: GenerationMethod::ColorRegions,
            color_tolerance: 0.02,
            alpha_threshold: 0.01,
            region_solver: RegionSolver::DistanceField,
            region_radius: 32.0,
            region_shape: 2.0,
            boundary_condition: BoundaryCondition::Neumann,
            poisson_iterations: 160,
            poisson_curvature: 1.5,
            screened_damping: 0.02,
            edge_feather: 8.0,
            edge_softness: 1.0,
            height_channel: HeightChannel::Luminance,
            height_blur: 0.0,
            normal_strength: 1.0,
            invert_height: false,
            normal_scale: 1.0,
            normal_y: NormalY::OpenGl,
            light_type: LightType::Point,
            light_color: [1.0; 3],
            light_intensity: 1.0,
            direction_azimuth: 0.0,
            direction_elevation: 0.0,
            point_position: (10.0, 10.0),
            point_height: 10.0,
            point_radius: 20.0,
            ambient: 0.0,
            diffuse: 1.0,
            specular: 0.0,
            shininess: 32.0,
            material_model: MaterialModel::LegacyBlinnPhong,
            base_tint: [1.0; 3],
            metallic: 0.0,
            roughness: 0.5,
            ior: 1.5,
            specular_ior_level: 0.5,
            environment_strength: 0.1,
            exposure: 0.0,
            clamp_output: false,
            output_mode: OutputMode::Relit,
            pixel_scale: (1.0, 1.0),
        }
    }

    #[test]
    fn flat_height_produces_flat_normals() {
        let normals = normals_from_height(&[0.5; 25], 5, 5, 10.0, (1.0, 1.0));
        for normal in normals {
            assert!((normal[0]).abs() < 1.0e-6);
            assert!((normal[1]).abs() < 1.0e-6);
            assert!((normal[2] - 1.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn neutral_normal_map_decodes_to_surface_normal() {
        let pixel = PixelF32 {
            alpha: 1.0,
            red: 0.5,
            green: 0.5,
            blue: 1.0,
        };
        let normal = decode_normal(pixel, 1.0, NormalY::OpenGl);
        assert!(normal[0].abs() < 1.0e-6);
        assert!(normal[1].abs() < 1.0e-6);
        assert!((normal[2] - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn front_light_is_brighter_than_back_light() {
        let front = blinn_terms([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 32.0);
        let back = blinn_terms([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], 32.0);
        assert!(front.0 > back.0);
        assert!(front.1 > back.1);
    }

    #[test]
    fn principled_default_ior_has_four_percent_reflectance() {
        assert!((dielectric_f0(1.5, 0.5) - 0.04).abs() < 1.0e-6);
    }

    #[test]
    fn smoother_ggx_has_a_stronger_normal_incidence_highlight() {
        let evaluate = |roughness: f32| {
            principled_terms_prepared(
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.5; 3],
                0.0,
                0.04,
                roughness.max(0.045).powi(4),
            )
        };
        let smooth = evaluate(0.2);
        let rough = evaluate(0.8);
        assert!(smooth.1[0] > rough.1[0]);
        assert!(smooth.0[0].is_finite() && smooth.1[0].is_finite());
    }

    #[test]
    fn metallic_surface_has_no_diffuse_lobe_and_tints_specular() {
        let (diffuse, specular) = principled_terms_prepared(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.8, 0.2, 0.05],
            1.0,
            0.04,
            0.5_f32.powi(4),
        );
        assert_eq!(diffuse, [0.0; 3]);
        assert!(specular[0] > specular[1]);
        assert!(specular[1] > specular[2]);
    }

    #[test]
    fn ggx_terms_stay_finite_at_grazing_angles() {
        let (diffuse, specular) = principled_terms_prepared(
            normalize3([1.0, 0.0, 1.0e-4]),
            normalize3([1.0, 0.0, 1.0e-4]),
            normalize3([1.0, 0.0, 1.0001]),
            [1.0; 3],
            0.35,
            0.04,
            0.3_f32.powi(4),
        );
        assert!(diffuse.into_iter().chain(specular).all(f32::is_finite));
    }

    #[test]
    fn box_blur_preserves_constant_height() {
        let blurred = box_blur(&[0.25; 35], 7, 5, 3, 1);
        assert!(blurred.iter().all(|value| (*value - 0.25).abs() < 1.0e-6));
    }

    #[test]
    fn pixel_geometry_accounts_for_par_and_downsample() {
        assert_eq!(square_pixel_scale(2.0, 0.5, 0.25), (4.0, 4.0));
        assert_eq!(buffer_radius(20.0, 2.0, 100), 10);
    }

    #[test]
    fn extended_bounds_origin_localizes_point_light() {
        let local = point_in_checkout_world((50.0, 25.0), ae::Point { h: -20, v: 10 });
        assert_eq!(local, (70.0, 15.0));
        let local_positive = point_in_checkout_world((50.0, 25.0), ae::Point { h: 12, v: 8 });
        assert_eq!(local_positive, (38.0, 17.0));
    }

    #[test]
    fn legacy_full_frame_with_matching_origins_keeps_identity_coordinates() {
        let origin = ae::Point { h: 17, v: -9 };
        assert_eq!(
            source_local_from_output_local(3.0, 4.0, origin, origin),
            (3.0, 4.0)
        );
    }

    #[test]
    fn smart_render_tile_origin_translates_without_resizing() {
        let source_origin = ae::Point { h: -40, v: 25 };
        let tile_origin = ae::Point { h: 120, v: 80 };
        let first = source_local_from_output_local(3.0, 4.0, tile_origin, source_origin);
        let adjacent = source_local_from_output_local(4.0, 4.0, tile_origin, source_origin);
        assert_eq!(first, (163.0, 59.0));
        assert_eq!(adjacent, (164.0, 59.0));
    }

    #[test]
    fn point_light_and_tile_pixel_share_source_local_coordinates() {
        let source_origin = ae::Point { h: -20, v: 10 };
        let tile_origin = ae::Point { h: 30, v: 12 };
        let sampled = source_local_from_output_local(20.0, 3.0, tile_origin, source_origin);
        let point_light = point_in_checkout_world((50.0, 15.0), source_origin);
        assert_eq!(sampled, (70.0, 5.0));
        assert_eq!(sampled, point_light);
    }

    #[test]
    fn generated_normal_gradient_is_resolution_invariant() {
        let width = 7;
        let height = 3;
        let full_resolution: Vec<f32> = (0..width * height)
            .map(|index| (index % width) as f32 * 0.1)
            .collect();
        let half_resolution: Vec<f32> = (0..width * height)
            .map(|index| (index % width) as f32 * 0.2)
            .collect();
        let full = normals_from_height(&full_resolution, width, height, 1.0, (1.0, 1.0));
        let half = normals_from_height(&half_resolution, width, height, 1.0, (2.0, 2.0));
        for channel in 0..3 {
            assert!((full[width + 3][channel] - half[width + 3][channel]).abs() < 1.0e-6);
        }
    }

    #[test]
    fn point_falloff_is_resolution_invariant() {
        let full_settings = test_settings();
        let mut half_settings = full_settings;
        half_settings.point_position = (5.0, 5.0);
        half_settings.pixel_scale = (2.0, 2.0);
        let full = light_vector(20.0, 10.0, full_settings);
        let half = light_vector(10.0, 5.0, half_settings);
        for channel in 0..3 {
            assert!((full.0[channel] - half.0[channel]).abs() < 1.0e-6);
        }
        assert!((full.1 - half.1).abs() < 1.0e-6);
    }

    #[test]
    fn checked_out_normal_layer_origins_are_aligned() {
        let source_origin = ae::Point { h: 120, v: 80 };
        let normal_origin = ae::Point { h: 100, v: 60 };
        assert_eq!(
            aligned_layer_coordinate(3, 4, source_origin, normal_origin),
            (23.0, 24.0)
        );
    }

    #[test]
    fn region_labels_use_straight_color_and_keep_opaque_black() {
        let half_alpha = PixelF32 {
            alpha: 0.5,
            red: 0.1,
            green: 0.2,
            blue: 0.3,
        };
        let opaque = PixelF32 {
            alpha: 1.0,
            red: 0.2,
            green: 0.4,
            blue: 0.6,
        };
        assert_eq!(
            pack_region_label(half_alpha, 0.01, 0.0),
            pack_region_label(opaque, 0.01, 0.0)
        );
        assert_ne!(
            pack_region_label(
                PixelF32 {
                    alpha: 1.0,
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                },
                0.01,
                0.0,
            ),
            BACKGROUND_LABEL
        );
        assert_eq!(
            pack_region_label(
                PixelF32 {
                    alpha: 0.0,
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                },
                0.01,
                0.0,
            ),
            BACKGROUND_LABEL
        );
        assert_eq!(
            pack_region_label(
                PixelF32 {
                    alpha: 0.0,
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                },
                0.0,
                0.0,
            ),
            BACKGROUND_LABEL
        );
    }

    #[test]
    fn color_tolerance_groups_nearby_region_colors() {
        let pixel = |red: f32| PixelF32 {
            alpha: 1.0,
            red,
            green: 0.25,
            blue: 0.75,
        };
        assert_ne!(
            pack_region_label(pixel(0.500), 0.01, 0.0),
            pack_region_label(pixel(0.507), 0.01, 0.0)
        );
        assert_eq!(
            pack_region_label(pixel(0.500), 0.01, 0.02),
            pack_region_label(pixel(0.507), 0.01, 0.02)
        );
    }

    #[test]
    fn disconnected_color_regions_form_independent_surfaces() {
        let width = 9;
        let height = 5;
        let transparent = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let red = PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        };
        let blue = PixelF32 {
            alpha: 1.0,
            red: 0.0,
            green: 0.0,
            blue: 1.0,
        };
        let mut source = vec![transparent; width * height];
        for y in 0..height {
            for x in 0..4 {
                source[y * width + x] = red;
            }
            for x in 5..width {
                source[y * width + x] = blue;
            }
        }
        let mut settings = test_settings();
        settings.region_radius = 2.0;
        settings.region_shape = 2.0;
        settings.edge_softness = 0.0;
        let (heights, normals) = color_region_surface_maps(&source, width, height, settings);
        assert_eq!(heights[2 * width], 0.0);
        assert_eq!(heights[2 * width + 4], 0.0);
        assert_eq!(heights[2 * width + 8], 0.0);
        assert!((heights[2 * width + 1] - 0.25).abs() < 1.0e-6);
        assert!((heights[2 * width + 6] - 0.25).abs() < 1.0e-6);
        assert!(normals[2 * width + 1][0] < 0.0);
        assert!(normals[2 * width + 2][0] > 0.0);
        assert!(normals[2 * width + 6][0] < 0.0);
        assert!(normals[2 * width + 7][0] > 0.0);
    }

    #[test]
    fn region_distance_uses_full_resolution_square_pixel_units() {
        let width = 7;
        let height = 3;
        let labels = vec![0; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let horizontal_is_shorter =
            region_boundary_distance(&labels, &boundary, width, height, (1.0, 4.0));
        let vertical_is_shorter =
            region_boundary_distance(&labels, &boundary, width, height, (2.0, 1.0));
        let center = width + 3;
        assert!((horizontal_is_shorter[center] - 3.0).abs() < 1.0e-6);
        assert!((vertical_is_shorter[center] - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn region_height_inversion_reverses_the_surface_slope() {
        let width = 5;
        let height = 3;
        let source = vec![
            PixelF32 {
                alpha: 1.0,
                red: 0.25,
                green: 0.5,
                blue: 0.75,
            };
            width * height
        ];
        let mut settings = test_settings();
        settings.region_radius = 4.0;
        settings.region_shape = 1.0;
        settings.edge_softness = 0.0;
        let (raised_height, raised_normal) =
            color_region_surface_maps(&source, width, height, settings);
        settings.invert_height = true;
        let (recessed_height, recessed_normal) =
            color_region_surface_maps(&source, width, height, settings);
        let sample = width + 1;
        assert!((raised_height[sample] + recessed_height[sample] - 1.0).abs() < 1.0e-6);
        assert!((raised_normal[sample][0] + recessed_normal[sample][0]).abs() < 1.0e-6);
    }

    #[test]
    fn pure_neumann_rhs_is_compatible_per_connected_component() {
        let width = 7;
        let height = 3;
        let mut labels = vec![BACKGROUND_LABEL; width * height];
        for y in 0..height {
            for x in 0..3 {
                labels[y * width + x] = 0x11_22_33;
                labels[y * width + x + 4] = 0x11_22_33;
            }
        }
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let components = region_components(&labels, width, height);
        let profile = poisson_profiles(&distance, &components, 0.0);
        let rhs = poisson_rhs(
            &profile,
            &boundary,
            &components,
            BoundaryCondition::Neumann,
            3.0,
            0.0,
        );
        assert_eq!(components.len(), 2);
        for component in components {
            let sum = component.iter().map(|&index| rhs[index]).sum::<f32>();
            assert!(sum.abs() < 1.0e-5);
        }
    }

    #[test]
    fn neumann_poisson_height_is_finite_and_normalized() {
        let width = 9;
        let height = 9;
        let labels = vec![0x44_55_66; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let heights = poisson_region_heights(
            &labels,
            &boundary,
            &distance,
            width,
            height,
            BoundaryCondition::Neumann,
            300,
            1.5,
            0.0,
            2.0,
            false,
            (1.0, 1.0),
        );
        assert!(heights.iter().all(|value| value.is_finite()));
        assert!(heights.iter().all(|value| (0.0..=1.0).contains(value)));
        assert!((heights.iter().copied().fold(f32::INFINITY, f32::min)).abs() < 1.0e-6);
        let maximum = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!((maximum - poisson_relief_scale(1.5, 0.0)).abs() < 1.0e-6);
        assert!(heights[4 * width + 4] > heights[0]);
    }

    #[test]
    fn dirichlet_poisson_keeps_region_boundary_at_zero() {
        let width = 9;
        let height = 9;
        let labels = vec![0xAA_BB_CC; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let heights = poisson_region_heights(
            &labels,
            &boundary,
            &distance,
            width,
            height,
            BoundaryCondition::Dirichlet,
            300,
            1.5,
            0.02,
            2.0,
            false,
            (1.0, 1.0),
        );
        for (index, is_boundary) in boundary.iter().copied().enumerate() {
            if is_boundary {
                assert!(heights[index].abs() < 1.0e-6);
            }
        }
        assert!(heights[4 * width + 4] > 0.7);
    }

    #[test]
    fn disconnected_same_color_poisson_regions_normalize_independently() {
        let width = 15;
        let height = 7;
        let mut labels = vec![BACKGROUND_LABEL; width * height];
        for y in 0..height {
            for x in 0..5 {
                labels[y * width + x] = 0x12_34_56;
            }
            for x in 8..width {
                labels[y * width + x] = 0x12_34_56;
            }
        }
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let heights = poisson_region_heights(
            &labels,
            &boundary,
            &distance,
            width,
            height,
            BoundaryCondition::Neumann,
            240,
            1.5,
            0.02,
            1.0,
            false,
            (1.0, 1.0),
        );
        let components = region_components(&labels, width, height);
        assert_eq!(components.len(), 2);
        for component in components {
            let minimum = component
                .iter()
                .map(|&index| heights[index])
                .fold(f32::INFINITY, f32::min);
            let maximum = component
                .iter()
                .map(|&index| heights[index])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(minimum.abs() < 1.0e-6);
            assert!((maximum - poisson_relief_scale(1.5, 0.02)).abs() < 1.0e-6);
        }
    }

    #[test]
    fn one_pixel_neumann_region_stays_finite() {
        let labels = [0x01_02_03];
        let boundary = [true];
        let distance = [0.0];
        let raised = poisson_region_heights(
            &labels,
            &boundary,
            &distance,
            1,
            1,
            BoundaryCondition::Neumann,
            200,
            10.0,
            0.0,
            8.0,
            false,
            (1.0, 1.0),
        );
        let inverted = poisson_region_heights(
            &labels,
            &boundary,
            &distance,
            1,
            1,
            BoundaryCondition::Neumann,
            200,
            10.0,
            0.0,
            8.0,
            true,
            (1.0, 1.0),
        );
        assert_eq!(raised, vec![0.0]);
        assert_eq!(inverted, vec![1.0]);
    }

    #[test]
    fn poisson_stencil_uses_physical_pixel_scale() {
        let labels = vec![1; 9];
        let rhs = vec![0.0; 9];
        let heights = vec![0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let isotropic = poisson_candidate(
            4,
            &heights,
            &labels,
            &rhs,
            3,
            3,
            BoundaryCondition::Neumann,
            0.0,
            1.0,
            1.0,
        );
        let wide_pixels = poisson_candidate(
            4,
            &heights,
            &labels,
            &rhs,
            3,
            3,
            BoundaryCondition::Neumann,
            0.0,
            0.25,
            1.0,
        );
        assert!((isotropic - 0.5).abs() < 1.0e-6);
        assert!((wide_pixels - 0.2).abs() < 1.0e-6);
    }

    #[test]
    fn prepared_poisson_stencil_is_bitwise_identical_to_reference_candidate() {
        let labels = [1, 1, BACKGROUND_LABEL, 1, 1, 2, BACKGROUND_LABEL, 2, 2];
        let heights = [0.1, 0.2, 0.0, 0.3, 0.4, 0.5, 0.0, 0.6, 0.7];
        let rhs = [0.9, -0.2, 0.0, 0.4, 0.8, -0.7, 0.0, 0.3, 0.6];
        let weight_x = 0.37;
        let weight_y = 1.41;
        let lambda_squared = 0.0625;

        for condition in [BoundaryCondition::Dirichlet, BoundaryCondition::Neumann] {
            for index in [0, 1, 3, 4, 5, 7, 8] {
                let reference = poisson_candidate(
                    index,
                    &heights,
                    &labels,
                    &rhs,
                    3,
                    3,
                    condition,
                    lambda_squared,
                    weight_x,
                    weight_y,
                );
                let prepared = poisson_candidate_from_stencil(
                    prepare_poisson_stencil(
                        index,
                        &labels,
                        3,
                        3,
                        condition,
                        lambda_squared,
                        weight_x,
                        weight_y,
                    ),
                    &heights,
                    &rhs,
                    3,
                    weight_x,
                    weight_y,
                );
                assert_eq!(prepared.to_bits(), reference.to_bits(), "index {index}");
            }
        }
    }

    #[cfg(feature = "gpu_wgpu")]
    #[test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "requires an interactive Windows GPU driver; run explicitly with --ignored"
    )]
    fn gpu_screened_poisson_tracks_the_cpu_reference() {
        let Ok(context) = WgpuContext::new() else {
            eprintln!("Skipping ImageRelight CPU/GPU comparison because wgpu is unavailable");
            return;
        };
        let width = 9;
        let height = 7;
        let labels = vec![3; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let components = region_components(&labels, width, height);
        let profile = poisson_profiles(&distance, &components, 3.0);
        let lambda_squared = 0.02_f32.powi(2);
        let rhs = poisson_rhs(
            &profile,
            &boundary,
            &components,
            BoundaryCondition::Neumann,
            1.5,
            lambda_squared,
        );
        let omega = poisson_sor_omega(width, height);
        let iterations = 80;
        let cpu = solve_poisson_cpu(
            &labels,
            &boundary,
            &rhs,
            &components,
            width,
            height,
            BoundaryCondition::Neumann,
            iterations,
            lambda_squared,
            1.0,
            1.0,
            omega,
        );
        let gpu = context
            .solve(
                &labels,
                &boundary,
                &rhs,
                SolveParams {
                    width,
                    height,
                    condition: GpuBoundaryCondition::Neumann,
                    iterations,
                    lambda_squared,
                    weight_x: 1.0,
                    weight_y: 1.0,
                    omega,
                },
            )
            .expect("GPU Poisson solve should succeed");

        for (index, (gpu, cpu)) in gpu.into_iter().zip(cpu).enumerate() {
            assert!(
                (gpu - cpu).abs() <= 5.0e-4,
                "CPU/GPU mismatch at {index}: {gpu} vs {cpu}"
            );
        }
    }

    #[test]
    fn neumann_stencil_has_zero_flux_at_region_boundaries() {
        let labels = [7, 7, BACKGROUND_LABEL];
        let rhs = [0.0; 3];
        let heights = [0.7, 0.7, 0.0];
        let neumann = poisson_candidate(
            0,
            &heights,
            &labels,
            &rhs,
            3,
            1,
            BoundaryCondition::Neumann,
            0.0,
            1.0,
            1.0,
        );
        let dirichlet = poisson_candidate(
            0,
            &heights,
            &labels,
            &rhs,
            3,
            1,
            BoundaryCondition::Dirichlet,
            0.0,
            1.0,
            1.0,
        );
        assert!((neumann - 0.7).abs() < 1.0e-6);
        assert!(dirichlet < neumann);
    }

    #[test]
    fn poisson_invert_is_a_unit_interval_complement() {
        let width = 7;
        let height = 7;
        let labels = vec![0x98_76_54; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let solve = |invert| {
            poisson_region_heights(
                &labels,
                &boundary,
                &distance,
                width,
                height,
                BoundaryCondition::Neumann,
                200,
                1.5,
                0.02,
                2.0,
                invert,
                (1.0, 1.0),
            )
        };
        let raised = solve(false);
        let recessed = solve(true);
        for (a, b) in raised.iter().zip(recessed) {
            assert!((*a + b - 1.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn zero_drive_large_neumann_surface_is_exactly_flat() {
        let width = 129;
        let height = 129;
        let labels = vec![0x24_68_AC; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let solve = |invert| {
            poisson_region_heights(
                &labels,
                &boundary,
                &distance,
                width,
                height,
                BoundaryCondition::Neumann,
                160,
                0.0,
                0.0,
                8.0,
                invert,
                (1.0, 1.0),
            )
        };
        assert!(solve(false).iter().all(|value| *value == 0.0));
        assert!(solve(true).iter().all(|value| *value == 1.0));
    }

    #[test]
    fn curvature_monotonically_controls_poisson_relief() {
        let width = 33;
        let height = 33;
        let labels = vec![0x13_57_9B; width * height];
        let boundary = region_boundary_mask(&labels, width, height);
        let distance = region_boundary_distance(&labels, &boundary, width, height, (1.0, 1.0));
        let solve_max = |curvature| {
            poisson_region_heights(
                &labels,
                &boundary,
                &distance,
                width,
                height,
                BoundaryCondition::Neumann,
                160,
                curvature,
                0.0,
                8.0,
                false,
                (1.0, 1.0),
            )
            .into_iter()
            .fold(f32::NEG_INFINITY, f32::max)
        };
        let low = solve_max(0.25);
        let high = solve_max(2.0);
        assert!(low > 0.0);
        assert!(high > low);
        assert!((low - poisson_relief_scale(0.25, 0.0)).abs() < 1.0e-6);
        assert!((high - poisson_relief_scale(2.0, 0.0)).abs() < 1.0e-6);
    }

    fn test_surface_maps(value: f32) -> SurfaceMaps {
        SurfaceMaps::new(vec![value], vec![[0.0, 0.0, 1.0]])
    }

    #[test]
    fn surface_cache_hits_and_single_entry_invalidates() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let source = vec![PixelF32 {
            alpha: 1.0,
            red: 0.25,
            green: 0.5,
            blue: 0.75,
        }];
        let settings = test_settings();
        let key_a = SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, settings);
        let mut changed_settings = settings;
        changed_settings.normal_strength = 2.0;
        let key_b = SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, changed_settings);
        let cache = SurfaceCache::default();
        let builds = AtomicUsize::new(0);
        let first = cache.get_or_build(key_a.clone(), || {
            builds.fetch_add(1, Ordering::SeqCst);
            test_surface_maps(1.0)
        });
        let hit = cache.get_or_build(key_a.clone(), || {
            builds.fetch_add(1, Ordering::SeqCst);
            test_surface_maps(2.0)
        });
        assert!(Arc::ptr_eq(&first.heights, &hit.heights));
        assert_eq!(builds.load(Ordering::SeqCst), 1);

        let changed = cache.get_or_build(key_b, || {
            builds.fetch_add(1, Ordering::SeqCst);
            test_surface_maps(3.0)
        });
        assert!(!Arc::ptr_eq(&first.heights, &changed.heights));
        let rebuilt = cache.get_or_build(key_a.clone(), || {
            builds.fetch_add(1, Ordering::SeqCst);
            test_surface_maps(4.0)
        });
        assert!(!Arc::ptr_eq(&first.heights, &rebuilt.heights));
        assert_eq!(builds.load(Ordering::SeqCst), 3);

        cache.clear();
        let after_clear = cache.get_or_build(key_a.clone(), || {
            builds.fetch_add(1, Ordering::SeqCst);
            test_surface_maps(5.0)
        });
        assert!(!Arc::ptr_eq(&rebuilt.heights, &after_clear.heights));
        assert_eq!(builds.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn surface_cache_key_tracks_source_origin_settings_and_nan_bits() {
        let source = [PixelF32 {
            alpha: 1.0,
            red: f32::from_bits(0x7FC0_0001),
            green: 0.5,
            blue: 0.75,
        }];
        let settings = test_settings();
        let base = SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, settings);
        let mut changed_source = source;
        changed_source[0].red = f32::from_bits(0x7FC0_0002);
        assert_ne!(
            base,
            SurfaceCacheKey::new(&changed_source, 1, 1, ae::Point { h: 0, v: 0 }, settings,)
        );
        assert_ne!(
            base,
            SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 1, v: 0 }, settings,)
        );
        let mut changed_settings = settings;
        changed_settings.edge_softness = f32::from_bits(0x7FC0_0001);
        let first_nan =
            SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, changed_settings);
        changed_settings.edge_softness = f32::from_bits(0x7FC0_0002);
        assert_ne!(
            first_nan,
            SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, changed_settings,)
        );
    }

    #[test]
    fn competing_tiles_build_cached_surface_once() {
        use std::sync::Barrier;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        let cache = Arc::new(SurfaceCache::default());
        let builds = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));
        let source = [PixelF32 {
            alpha: 1.0,
            red: 0.2,
            green: 0.4,
            blue: 0.6,
        }];
        let key = SurfaceCacheKey::new(&source, 1, 1, ae::Point { h: 0, v: 0 }, test_settings());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let builds = Arc::clone(&builds);
                let barrier = Arc::clone(&barrier);
                let key = key.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    cache.get_or_build(key, || {
                        builds.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(5));
                        test_surface_maps(0.5)
                    })
                })
            })
            .collect();
        let maps: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().expect("cache worker did not panic"))
            .collect();
        assert_eq!(builds.load(Ordering::SeqCst), 1);
        let first = &maps[0].heights;
        assert!(
            maps.iter()
                .skip(1)
                .all(|maps| Arc::ptr_eq(&maps.heights, first))
        );
    }
}
