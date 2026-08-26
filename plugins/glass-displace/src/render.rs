use after_effects as ae;

use ae::Error;
use ae::pf::*;
use utils::ToPixel;
use utils::image::{
    SampleEdge, finite_or, luma, read_layer, sample_bilinear, sample_nearest, sanitize_pixel,
    unpremultiplied_rgb,
};

use crate::glass::{
    DebugView, EdgeMode, HeightSample, HeightSource, MapChannel, PreparedProcedural,
    ProceduralSettings, Sampling, Shape, central_difference, facet_color, map_level,
};
use crate::params::Params;

const SPECTRUM_MIN_NM: f32 = 380.0;
const SPECTRUM_MAX_NM: f32 = 780.0;
// The Fraunhofer d line is the standard reference wavelength for optical
// glass. BK7's refractive index is approximately 1.5168 at this wavelength.
const REFERENCE_WAVELENGTH_NM: f32 = 587.56;
const DISPERSION_REFERENCE_WAVELENGTH_NM: f32 = SPECTRUM_MIN_NM;

#[cfg(test)]
thread_local! {
    static REFRACTED_SCREEN_SLOPE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone)]
pub(crate) struct LayerBuffer {
    pixels: Vec<PixelF32>,
    width: usize,
    height: usize,
    origin: [f32; 2],
}

pub(crate) struct OwnedMaps {
    pub custom: Option<LayerBuffer>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct RenderLayout {
    pub input_rect: Option<ae::Rect>,
    pub output_rect: Option<ae::Rect>,
}

#[derive(Clone, Copy)]
pub(crate) struct Settings {
    pub height_source: HeightSource,
    map_channel: MapChannel,
    invert_map: bool,
    map_black: f32,
    map_white: f32,
    procedural: ProceduralSettings,
    height_strength: f32,
    normal_radius: f32,
    refraction: f32,
    dispersion: f32,
    dispersion_steps: usize,
    auto_spectral_steps: bool,
    sampling: Sampling,
    edge: EdgeMode,
    mix: f32,
    preserve_alpha: bool,
    debug: DebugView,
    clamp_32: bool,
    downsample: [f32; 2],
}

#[derive(Clone, Copy, Debug)]
struct OpticalCalibration {
    reference_index: f32,
    projection_scale: f32,
    dispersion_scale: f32,
}

impl OpticalCalibration {
    fn bk7() -> Self {
        let reference_index = bk7_refractive_index(REFERENCE_WAVELENGTH_NM);
        // Refraction is calibrated so its UI value is the displacement in
        // pixels for a 45-degree height-field slope. The render still uses the
        // full nonlinear Snell solution at every actual surface orientation.
        let calibration_normal = surface_normal([1.0, 0.0]);
        let reference_ray = refracted_screen_slope(calibration_normal, reference_index);
        let blue_ray = refracted_screen_slope(
            calibration_normal,
            bk7_refractive_index(DISPERSION_REFERENCE_WAVELENGTH_NM),
        );
        Self {
            reference_index,
            projection_scale: 1.0 / length2(reference_ray).max(1.0e-6),
            // Chromatic Dispersion retains pixel units: at the calibration
            // slope, its value is the d-line-to-380 nm separation.
            dispersion_scale: 1.0 / length2(sub2(blue_ray, reference_ray)).max(1.0e-6),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SpectralBand {
    refractive_index: f32,
    rgb_response: [f32; 3],
    luminance_response: f32,
}

impl LayerBuffer {
    fn sample(&self, x: f32, y: f32, sampling: Sampling, edge: EdgeMode) -> PixelF32 {
        let edge = sample_edge(edge);
        let x = x - self.origin[0];
        let y = y - self.origin[1];
        match sampling {
            Sampling::Nearest => sample_nearest(&self.pixels, self.width, self.height, x, y, edge),
            Sampling::Bilinear => {
                sample_bilinear(&self.pixels, self.width, self.height, x, y, edge)
            }
        }
    }
}

pub(crate) fn read_layer_buffer(layer: &Layer) -> LayerBuffer {
    read_layer_buffer_with_rect(layer, None)
}

pub(crate) fn read_layer_buffer_with_rect(
    layer: &Layer,
    requested_rect: Option<ae::Rect>,
) -> LayerBuffer {
    let origin = layer_buffer_origin(layer, requested_rect);
    LayerBuffer {
        pixels: read_layer(layer),
        width: layer.width(),
        height: layer.height(),
        origin,
    }
}

pub(crate) fn read_settings(
    params: &Parameters<Params>,
    in_data: InData,
) -> Result<Settings, Error> {
    let downsample = [
        f32::from(in_data.downsample_x()).abs().max(1.0e-6),
        f32::from(in_data.downsample_y()).abs().max(1.0e-6),
    ];
    let center = point_to_full(point(params, Params::Center)?, downsample);
    Ok(Settings {
        height_source: height_source(params.get(Params::HeightSource)?.as_popup()?.value()),
        map_channel: map_channel(params.get(Params::MapChannel)?.as_popup()?.value()),
        invert_map: params.get(Params::InvertMap)?.as_checkbox()?.value(),
        map_black: slider(params, Params::MapBlack, 0.0),
        map_white: slider(params, Params::MapWhite, 1.0),
        procedural: ProceduralSettings {
            shape: shape(params.get(Params::Shape)?.as_popup()?.value()),
            center,
            size: slider(params, Params::Size, 500.0).max(0.01),
            aspect: slider(params, Params::Aspect, 1.0).max(0.01),
            rotation: finite_or(
                params.get(Params::Rotation)?.as_angle()?.float_value()? as f32,
                0.0,
            )
            .to_radians(),
            roundness: slider(params, Params::Roundness, 50.0).clamp(0.0, 100.0) * 0.01,
            ring_width: slider(params, Params::RingWidth, 25.0).clamp(1.0, 100.0) * 0.01,
            facet_size: slider(params, Params::FacetSize, 48.0).max(1.0),
            facet_amount: slider(params, Params::FacetAmount, 100.0).clamp(0.0, 100.0) * 0.01,
            facet_jitter: slider(params, Params::FacetJitter, 75.0).clamp(0.0, 100.0) * 0.01,
            crack_width: slider(params, Params::CrackWidth, 2.0).max(0.0),
            crack_depth: slider(params, Params::CrackDepth, 90.0).clamp(0.0, 100.0) * 0.01,
            radial_cracks: integer_slider(params, Params::RadialCracks, 18).clamp(3, 96) as u32,
            crack_branching: slider(params, Params::CrackBranching, 55.0).clamp(0.0, 100.0) * 0.01,
            crack_jitter: slider(params, Params::CrackJitter, 45.0).clamp(0.0, 100.0) * 0.01,
            stress_rings: integer_slider(params, Params::StressRings, 6).clamp(0, 32) as u32,
            ring_jitter: slider(params, Params::RingJitter, 35.0).clamp(0.0, 100.0) * 0.01,
            impact_falloff: slider(params, Params::ImpactFalloff, 20.0).clamp(0.0, 100.0) * 0.01,
            seed: slider(params, Params::Seed, 1.0).round().max(0.0) as u32,
        },
        height_strength: slider(params, Params::HeightStrength, 100.0),
        normal_radius: slider(params, Params::NormalRadius, 1.0).max(0.25),
        refraction: slider(params, Params::Refraction, 40.0),
        dispersion: slider(params, Params::Dispersion, 2.0),
        dispersion_steps: integer_slider(params, Params::DispersionSteps, 8).clamp(3, 32) as usize,
        auto_spectral_steps: params
            .get(Params::AutoSpectralSteps)?
            .as_checkbox()?
            .value(),
        sampling: sampling(params.get(Params::Sampling)?.as_popup()?.value()),
        edge: edge(params.get(Params::Edge)?.as_popup()?.value()),
        mix: slider(params, Params::Mix, 100.0).clamp(0.0, 100.0) * 0.01,
        preserve_alpha: params.get(Params::PreserveAlpha)?.as_checkbox()?.value(),
        debug: debug_view(params.get(Params::Debug)?.as_popup()?.value()),
        clamp_32: params.get(Params::Clamp32)?.as_checkbox()?.value(),
        downsample,
    })
}

pub(crate) fn render(
    _in_data: InData,
    in_layer: Layer,
    mut out_layer: Layer,
    settings: Settings,
    maps: OwnedMaps,
    layout: RenderLayout,
) -> Result<(), Error> {
    let source = read_layer_buffer_with_rect(&in_layer, layout.input_rect);
    let width = out_layer.width();
    let height = out_layer.height();
    if width == 0 || height == 0 || source.width == 0 || source.height == 0 {
        return Ok(());
    }
    // Point controls already include the pre-effect origin. Use the actual
    // PF_EffectWorld origin (with the pre-render request as a fallback) so
    // buffer-local pixels map back into the same layer coordinate system.
    let output_origin = layer_buffer_origin(&out_layer, layout.output_rect);
    let origin_x = output_origin[0];
    let origin_y = output_origin[1];
    let out_world_type = out_layer.world_type();
    let custom_map = maps.custom.as_ref();
    let prepared_procedural = (settings.height_source == HeightSource::Procedural)
        .then(|| PreparedProcedural::new(settings.procedural));
    let context = HeightContext {
        source: &source,
        custom_map,
        settings,
        prepared_procedural,
    };
    let (optics, spectral_bands) = prepare_render_optics(settings);

    out_layer.iterate(0, height as i32, None, |x, y, mut destination| {
        let global_x = (x as f32 + origin_x) / settings.downsample[0];
        let global_y = (y as f32 + origin_y) / settings.downsample[1];
        let output = match settings.debug {
            DebugView::Final => {
                let normal = context.normal(global_x, global_y);
                let base = context.source_sample(global_x, global_y);
                refracted_pixel(
                    [global_x, global_y],
                    normal,
                    base,
                    &source,
                    settings,
                    optics.expect("final view prepares optics"),
                    &spectral_bands,
                )
            }
            DebugView::Height => opaque_gray(context.height_sample(global_x, global_y).value),
            DebugView::Normal => {
                let normal = context.normal(global_x, global_y);
                PixelF32 {
                    alpha: 1.0,
                    red: normal[0] * 0.5 + 0.5,
                    green: normal[1] * 0.5 + 0.5,
                    blue: normal[2],
                }
            }
            DebugView::Displacement => {
                let normal = context.normal(global_x, global_y);
                let offset = reference_refraction_offset(
                    normal,
                    settings.refraction,
                    optics.expect("displacement view prepares optics"),
                );
                let magnitude = length2(offset);
                opaque_gray((magnitude / 128.0).clamp(0.0, 1.0))
            }
            DebugView::FacetId => {
                let color = facet_color(context.facet_id(global_x, global_y));
                PixelF32 {
                    alpha: 1.0,
                    red: color[0],
                    green: color[1],
                    blue: color[2],
                }
            }
        };
        let output = sanitize_associated(output, settings.clamp_32 || !is_f32(out_world_type));
        match out_world_type {
            ae::aegp::WorldType::U8 => destination.set_from_u8(output.to_pixel8()),
            ae::aegp::WorldType::U15 => destination.set_from_u16(output.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => {
                destination.set_from_f32(output)
            }
        }
        Ok(())
    })?;
    Ok(())
}

struct HeightContext<'a> {
    source: &'a LayerBuffer,
    custom_map: Option<&'a LayerBuffer>,
    settings: Settings,
    prepared_procedural: Option<PreparedProcedural>,
}

impl HeightContext<'_> {
    fn source_sample(&self, x: f32, y: f32) -> PixelF32 {
        self.source.sample(
            x * self.settings.downsample[0],
            y * self.settings.downsample[1],
            self.settings.sampling,
            self.settings.edge,
        )
    }

    fn height_sample(&self, x: f32, y: f32) -> HeightSample {
        let raw = match self.settings.height_source {
            HeightSource::Procedural => {
                return self
                    .prepared_procedural
                    .as_ref()
                    .expect("procedural height source prepares its constants")
                    .height(x, y);
            }
            HeightSource::CustomMap => self
                .custom_map
                .map(|map| {
                    let source_x = x * self.settings.downsample[0];
                    let source_y = y * self.settings.downsample[1];
                    let map_x = scaled_coordinate(
                        source_x,
                        self.source.origin[0],
                        self.source.width,
                        map.origin[0],
                        map.width,
                    );
                    let map_y = scaled_coordinate(
                        source_y,
                        self.source.origin[1],
                        self.source.height,
                        map.origin[1],
                        map.height,
                    );
                    channel_value(
                        map.sample(map_x, map_y, self.settings.sampling, self.settings.edge),
                        self.settings.map_channel,
                    )
                })
                .unwrap_or_else(|| luma(self.source_sample(x, y))),
            HeightSource::InputLuma => luma(self.source_sample(x, y)),
            HeightSource::InputAlpha => self.source_sample(x, y).alpha,
        };
        HeightSample {
            value: map_level(
                finite_or(raw, 0.0),
                self.settings.map_black,
                self.settings.map_white,
                self.settings.invert_map,
            ),
            facet_id: 0,
        }
    }

    fn normal(&self, x: f32, y: f32) -> [f32; 3] {
        let slope = central_difference(
            x,
            y,
            self.settings.normal_radius,
            self.settings.height_strength,
            |sample_x, sample_y| self.height_sample(sample_x, sample_y).value,
        );
        surface_normal(slope)
    }

    fn facet_id(&self, x: f32, y: f32) -> u32 {
        if self.settings.height_source != HeightSource::Procedural
            || !matches!(
                self.settings.procedural.shape,
                Shape::Facets | Shape::Shards | Shape::ImpactGlass
            )
        {
            return 0;
        }
        self.prepared_procedural
            .as_ref()
            .expect("procedural height source prepares its constants")
            .height(x, y)
            .facet_id
    }
}

fn prepare_render_optics(settings: Settings) -> (Option<OpticalCalibration>, Vec<SpectralBand>) {
    let needs_optics = matches!(settings.debug, DebugView::Final | DebugView::Displacement);
    let optics = needs_optics.then(OpticalCalibration::bk7);
    let spectral_bands = if settings.debug == DebugView::Final {
        build_spectral_bands(settings, optics.expect("final view prepares optics"))
    } else {
        Vec::new()
    };
    (optics, spectral_bands)
}

fn refracted_pixel(
    position: [f32; 2],
    normal: [f32; 3],
    base: PixelF32,
    source: &LayerBuffer,
    settings: Settings,
    optics: OpticalCalibration,
    spectral_bands: &[SpectralBand],
) -> PixelF32 {
    let (refracted_rgb, refracted_alpha) = if spectral_bands.is_empty() {
        let offset = reference_refraction_offset(normal, settings.refraction, optics);
        let sample = displaced_sample(source, position[0], position[1], offset, settings);
        (unpremultiplied_rgb(sample), sample.alpha.clamp(0.0, 1.0))
    } else {
        spectral_refraction(
            position[0],
            position[1],
            normal,
            source,
            settings,
            optics,
            spectral_bands,
        )
    };
    let refracted = PixelF32 {
        alpha: refracted_alpha,
        red: refracted_rgb[0] * refracted_alpha,
        green: refracted_rgb[1] * refracted_alpha,
        blue: refracted_rgb[2] * refracted_alpha,
    };
    composite_refracted(base, refracted, settings.mix, settings.preserve_alpha)
}

/// Integrates the CIE 1931 observer response across the visible range. Every
/// wavelength follows its own BK7 Sellmeier index through the same Snell ray
/// calculation, rather than moving three independent RGB copies.
fn spectral_refraction(
    x: f32,
    y: f32,
    normal: [f32; 3],
    source: &LayerBuffer,
    settings: Settings,
    optics: OpticalCalibration,
    spectral_bands: &[SpectralBand],
) -> ([f32; 3], f32) {
    let mut rgb_sum = [0.0_f32; 3];
    let mut rgb_weight = [0.0_f32; 3];
    let mut alpha_sum = 0.0_f32;
    let mut alpha_weight = 0.0_f32;
    let reference_ray = refracted_screen_slope(normal, optics.reference_index);
    let reference_offset = mul2(reference_ray, settings.refraction * optics.projection_scale);

    for band in spectral_bands {
        let offset = wavelength_refraction_offset_from_reference(
            normal,
            settings.dispersion,
            band.refractive_index,
            optics,
            reference_ray,
            reference_offset,
        );
        let sample = displaced_sample(source, x, y, offset, settings);
        let rgb = unpremultiplied_rgb(sample);
        for channel in 0..3 {
            rgb_sum[channel] += rgb[channel] * band.rgb_response[channel];
            rgb_weight[channel] += band.rgb_response[channel];
        }
        alpha_sum += sample.alpha.clamp(0.0, 1.0) * band.luminance_response;
        alpha_weight += band.luminance_response;
    }

    let rgb = [
        rgb_sum[0] / rgb_weight[0].max(1.0e-8),
        rgb_sum[1] / rgb_weight[1].max(1.0e-8),
        rgb_sum[2] / rgb_weight[2].max(1.0e-8),
    ];
    (rgb, alpha_sum / alpha_weight.max(1.0e-8))
}

fn build_spectral_bands(settings: Settings, optics: OpticalCalibration) -> Vec<SpectralBand> {
    if settings.dispersion.abs() <= 1.0e-6 {
        return Vec::new();
    }
    let steps = effective_spectral_steps(settings, optics);
    (0..steps)
        .map(|step| {
            // Stratified midpoint quadrature avoids endpoint bias, especially
            // for the lowest allowed manual sample count.
            let wavelength_nm = SPECTRUM_MIN_NM
                + (SPECTRUM_MAX_NM - SPECTRUM_MIN_NM) * (step as f32 + 0.5) / steps as f32;
            SpectralBand {
                refractive_index: bk7_refractive_index(wavelength_nm),
                rgb_response: spectral_response(wavelength_nm),
                luminance_response: cie_xyz_1931(wavelength_nm)[1].max(0.0),
            }
        })
        .collect()
}

fn effective_spectral_steps(settings: Settings, optics: OpticalCalibration) -> usize {
    if !settings.auto_spectral_steps {
        return settings.dispersion_steps.clamp(3, 32);
    }
    automatic_spectral_steps(
        settings.refraction,
        settings.dispersion,
        settings.height_strength,
        settings.normal_radius,
        settings.downsample,
        settings.sampling,
        optics,
    )
}

#[allow(clippy::too_many_arguments)]
fn automatic_spectral_steps(
    refraction: f32,
    dispersion: f32,
    height_strength: f32,
    normal_radius: f32,
    downsample: [f32; 2],
    sampling: Sampling,
    optics: OpticalCalibration,
) -> usize {
    // A normalized height map can change by at most one across a central
    // difference pair. Cap the estimate short of a grazing interface: beyond
    // this point the Snell projection is already close to its asymptote.
    let estimated_slope = (height_strength.abs() / (2.0 * normal_radius.max(0.25))).clamp(0.0, 8.0);
    let normal = surface_normal([estimated_slope, 0.0]);
    let blue = wavelength_refraction_offset(
        normal,
        refraction,
        dispersion,
        bk7_refractive_index(SPECTRUM_MIN_NM),
        optics,
    );
    let red = wavelength_refraction_offset(
        normal,
        refraction,
        dispersion,
        bk7_refractive_index(SPECTRUM_MAX_NM),
        optics,
    );
    let reference = reference_refraction_offset(normal, refraction, optics);
    let render_scale = downsample[0].abs().max(downsample[1].abs()).max(1.0e-3);
    let spectral_span = length2(sub2(blue, red)) * render_scale;
    let displacement = length2(reference) * render_scale;
    // Eight samples cover the broad CIE response at modest settings. Add
    // samples for sub-pixel spectral travel, strong ray projection, steep
    // interfaces, and nearest-neighbor quantization. This is intentionally a
    // smooth estimate to avoid large performance jumps between adjacent UI
    // values.
    let sampling_cost = if sampling == Sampling::Nearest {
        2.0
    } else {
        0.0
    };
    let estimate = 8.0
        + spectral_span * 0.75
        + displacement.sqrt() * 0.30
        + estimated_slope.ln_1p() * 1.5
        + sampling_cost;
    (estimate.ceil() as usize).clamp(6, 32)
}

fn spectral_response(wavelength_nm: f32) -> [f32; 3] {
    // Convert the analytic CIE 1931 2-degree observer fit to linear sRGB.
    // Negative matrix lobes are out-of-gamut monochromatic colors, so clamp
    // them before using the values as positive sensor-integration weights.
    let xyz = cie_xyz_1931(wavelength_nm);
    [
        (3.240_6 * xyz[0] - 1.537_2 * xyz[1] - 0.498_6 * xyz[2]).max(0.0),
        (-0.968_9 * xyz[0] + 1.875_8 * xyz[1] + 0.041_5 * xyz[2]).max(0.0),
        (0.055_7 * xyz[0] - 0.204_0 * xyz[1] + 1.057_0 * xyz[2]).max(0.0),
    ]
}

fn cie_xyz_1931(wavelength_nm: f32) -> [f32; 3] {
    // Smooth analytic fit by Wyman, Sloan, and Shirley. Its asymmetric lobes
    // reproduce the CIE color-matching curves far more closely than three
    // hand-tuned RGB Gaussians while remaining practical in a pixel effect.
    let x = 0.362 * cie_lobe(wavelength_nm, 442.0, 0.062_4, 0.037_4)
        + 1.056 * cie_lobe(wavelength_nm, 599.8, 0.026_4, 0.032_3)
        - 0.065 * cie_lobe(wavelength_nm, 501.1, 0.049_0, 0.038_2);
    let y = 0.821 * cie_lobe(wavelength_nm, 568.8, 0.021_3, 0.024_7)
        + 0.286 * cie_lobe(wavelength_nm, 530.9, 0.061_3, 0.032_2);
    let z = 1.217 * cie_lobe(wavelength_nm, 437.0, 0.084_5, 0.027_8)
        + 0.681 * cie_lobe(wavelength_nm, 459.0, 0.038_5, 0.072_5);
    [x.max(0.0), y.max(0.0), z.max(0.0)]
}

fn cie_lobe(value: f32, center: f32, left_scale: f32, right_scale: f32) -> f32 {
    let scale = if value < center {
        left_scale
    } else {
        right_scale
    };
    let t = (value - center) * scale;
    (-0.5 * t * t).exp()
}

fn bk7_refractive_index(wavelength_nm: f32) -> f32 {
    // Schott N-BK7 Sellmeier coefficients; wavelength is in micrometers.
    let wavelength_um = (wavelength_nm * 0.001).clamp(0.36, 2.4);
    let lambda_squared = wavelength_um * wavelength_um;
    let n_squared = 1.0
        + 1.039_612 * lambda_squared / (lambda_squared - 0.006_000_699)
        + 0.231_792_35 * lambda_squared / (lambda_squared - 0.020_017_914)
        + 1.010_469_4 * lambda_squared / (lambda_squared - 103.560_65);
    n_squared.max(1.0).sqrt()
}

fn surface_normal(slope: [f32; 2]) -> [f32; 3] {
    let inverse_length = (slope[0].mul_add(slope[0], slope[1] * slope[1]) + 1.0)
        .sqrt()
        .recip();
    [
        finite_or(slope[0] * inverse_length, 0.0),
        finite_or(slope[1] * inverse_length, 0.0),
        finite_or(inverse_length, 1.0),
    ]
}

fn refracted_ray(surface_normal: [f32; 3], refractive_index: f32) -> [f32; 3] {
    // Incident camera ray (0, 0, -1), refracted from air into glass. This is
    // the vector form of Snell's law with eta = n_air / n_glass.
    let eta = 1.0 / refractive_index.max(1.000_001);
    let cos_incident = surface_normal[2].clamp(0.0, 1.0);
    let transmitted_cosine = (1.0 - eta * eta * (1.0 - cos_incident * cos_incident))
        .max(0.0)
        .sqrt();
    let normal_scale = eta * cos_incident - transmitted_cosine;
    [
        normal_scale * surface_normal[0],
        normal_scale * surface_normal[1],
        -eta + normal_scale * surface_normal[2],
    ]
}

fn refracted_screen_slope(surface_normal: [f32; 3], refractive_index: f32) -> [f32; 2] {
    #[cfg(test)]
    REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| calls.set(calls.get() + 1));
    let ray = refracted_ray(surface_normal, refractive_index);
    let depth = (-ray[2]).max(1.0e-6);
    [
        finite_or(ray[0] / depth, 0.0),
        finite_or(ray[1] / depth, 0.0),
    ]
}

fn reference_refraction_offset(
    surface_normal: [f32; 3],
    refraction: f32,
    optics: OpticalCalibration,
) -> [f32; 2] {
    mul2(
        refracted_screen_slope(surface_normal, optics.reference_index),
        refraction * optics.projection_scale,
    )
}

fn wavelength_refraction_offset(
    surface_normal: [f32; 3],
    refraction: f32,
    dispersion: f32,
    refractive_index: f32,
    optics: OpticalCalibration,
) -> [f32; 2] {
    let reference_ray = refracted_screen_slope(surface_normal, optics.reference_index);
    let reference_offset = mul2(reference_ray, refraction * optics.projection_scale);
    wavelength_refraction_offset_from_reference(
        surface_normal,
        dispersion,
        refractive_index,
        optics,
        reference_ray,
        reference_offset,
    )
}

fn wavelength_refraction_offset_from_reference(
    surface_normal: [f32; 3],
    dispersion: f32,
    refractive_index: f32,
    optics: OpticalCalibration,
    reference_ray: [f32; 2],
    reference_offset: [f32; 2],
) -> [f32; 2] {
    let wavelength_ray = refracted_screen_slope(surface_normal, refractive_index);
    add2(
        reference_offset,
        mul2(
            sub2(wavelength_ray, reference_ray),
            dispersion * optics.dispersion_scale,
        ),
    )
}

fn composite_refracted(
    base: PixelF32,
    refracted: PixelF32,
    mix: f32,
    preserve_alpha: bool,
) -> PixelF32 {
    let mix = finite_or(mix, 1.0).clamp(0.0, 1.0);
    let base_alpha = finite_or(base.alpha, 0.0).clamp(0.0, 1.0);
    let refracted_alpha = finite_or(refracted.alpha, 0.0).clamp(0.0, 1.0);
    if preserve_alpha {
        let base_rgb = unpremultiplied_rgb(base);
        let refracted_rgb = unpremultiplied_rgb(refracted);
        let rgb = [
            lerp(base_rgb[0], refracted_rgb[0], mix),
            lerp(base_rgb[1], refracted_rgb[1], mix),
            lerp(base_rgb[2], refracted_rgb[2], mix),
        ];
        PixelF32 {
            alpha: base_alpha,
            red: rgb[0] * base_alpha,
            green: rgb[1] * base_alpha,
            blue: rgb[2] * base_alpha,
        }
    } else {
        PixelF32 {
            alpha: lerp(base_alpha, refracted_alpha, mix),
            red: lerp(finite_or(base.red, 0.0), finite_or(refracted.red, 0.0), mix),
            green: lerp(
                finite_or(base.green, 0.0),
                finite_or(refracted.green, 0.0),
                mix,
            ),
            blue: lerp(
                finite_or(base.blue, 0.0),
                finite_or(refracted.blue, 0.0),
                mix,
            ),
        }
    }
}

fn displaced_sample(
    source: &LayerBuffer,
    x: f32,
    y: f32,
    offset: [f32; 2],
    settings: Settings,
) -> PixelF32 {
    source.sample(
        (x + offset[0]) * settings.downsample[0],
        (y + offset[1]) * settings.downsample[1],
        settings.sampling,
        settings.edge,
    )
}

fn channel_value(pixel: PixelF32, channel: MapChannel) -> f32 {
    let rgb = unpremultiplied_rgb(pixel);
    match channel {
        MapChannel::Luma => 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2],
        MapChannel::Alpha => pixel.alpha,
        MapChannel::Red => rgb[0],
        MapChannel::Green => rgb[1],
        MapChannel::Blue => rgb[2],
    }
}

fn opaque_gray(value: f32) -> PixelF32 {
    let value = finite_or(value, 0.0).clamp(0.0, 1.0);
    PixelF32 {
        alpha: 1.0,
        red: value,
        green: value,
        blue: value,
    }
}

fn sanitize_associated(pixel: PixelF32, clamp_rgb: bool) -> PixelF32 {
    let mut pixel = sanitize_pixel(pixel, false);
    if clamp_rgb {
        pixel.red = pixel.red.clamp(0.0, pixel.alpha);
        pixel.green = pixel.green.clamp(0.0, pixel.alpha);
        pixel.blue = pixel.blue.clamp(0.0, pixel.alpha);
    }
    pixel
}

fn scaled_coordinate(
    value: f32,
    source_origin: f32,
    source_length: usize,
    target_origin: f32,
    target_length: usize,
) -> f32 {
    if source_length <= 1 || target_length <= 1 {
        target_origin
    } else {
        target_origin
            + (value - source_origin) * (target_length - 1) as f32 / (source_length - 1) as f32
    }
}

fn layer_buffer_origin(layer: &Layer, requested_rect: Option<ae::Rect>) -> [f32; 2] {
    let native = layer.origin();
    if native.h != 0 || native.v != 0 {
        return [native.h as f32, native.v as f32];
    }
    if let Some(rect) = requested_rect
        && rect.width() == layer.width() as i32
        && rect.height() == layer.height() as i32
    {
        return [rect.left as f32, rect.top as f32];
    }
    [native.h as f32, native.v as f32]
}

fn point(params: &Parameters<Params>, id: Params) -> Result<[f32; 2], Error> {
    let param = params.get(id)?;
    let point = param.as_point()?;
    Ok(match point.float_value() {
        Ok(value) => [value.x as f32, value.y as f32],
        Err(_) => {
            let (x, y) = point.value();
            [x, y]
        }
    })
}

fn point_to_full(point: [f32; 2], downsample: [f32; 2]) -> [f32; 2] {
    [point[0] / downsample[0], point[1] / downsample[1]]
}

fn slider(params: &Parameters<Params>, id: Params, fallback: f32) -> f32 {
    let Ok(param) = params.get(id) else {
        return fallback;
    };
    let Ok(slider) = param.as_float_slider() else {
        return fallback;
    };
    finite_or(slider.value() as f32, fallback)
}

fn integer_slider(params: &Parameters<Params>, id: Params, fallback: i32) -> i32 {
    let Ok(param) = params.get(id) else {
        return fallback;
    };
    let Ok(slider) = param.as_slider() else {
        return fallback;
    };
    slider.value()
}

fn height_source(value: i32) -> HeightSource {
    match value {
        2 => HeightSource::CustomMap,
        3 => HeightSource::InputLuma,
        4 => HeightSource::InputAlpha,
        _ => HeightSource::Procedural,
    }
}

pub(crate) fn shape(value: i32) -> Shape {
    match value {
        2 => Shape::RoundedRectangle,
        3 => Shape::Diamond,
        4 => Shape::Ring,
        5 => Shape::Facets,
        6 => Shape::Shards,
        7 => Shape::ImpactGlass,
        _ => Shape::Sphere,
    }
}

fn map_channel(value: i32) -> MapChannel {
    match value {
        2 => MapChannel::Alpha,
        3 => MapChannel::Red,
        4 => MapChannel::Green,
        5 => MapChannel::Blue,
        _ => MapChannel::Luma,
    }
}

fn sampling(value: i32) -> Sampling {
    if value == 1 {
        Sampling::Nearest
    } else {
        Sampling::Bilinear
    }
}

fn edge(value: i32) -> EdgeMode {
    match value {
        1 => EdgeMode::Transparent,
        3 => EdgeMode::Tile,
        4 => EdgeMode::Mirror,
        _ => EdgeMode::Clamp,
    }
}

fn sample_edge(edge: EdgeMode) -> SampleEdge {
    match edge {
        EdgeMode::Transparent => SampleEdge::Transparent,
        EdgeMode::Clamp => SampleEdge::Clamp,
        EdgeMode::Tile => SampleEdge::Tile,
        EdgeMode::Mirror => SampleEdge::Mirror,
    }
}

fn debug_view(value: i32) -> DebugView {
    match value {
        2 => DebugView::Height,
        3 => DebugView::Normal,
        4 => DebugView::Displacement,
        5 => DebugView::FacetId,
        _ => DebugView::Final,
    }
}

fn is_f32(world: ae::aegp::WorldType) -> bool {
    matches!(world, ae::aegp::WorldType::F32 | ae::aegp::WorldType::None)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn add2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul2(value: [f32; 2], scale: f32) -> [f32; 2] {
    [value[0] * scale, value[1] * scale]
}

fn length2(value: [f32; 2]) -> f32 {
    value[0].hypot(value[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings(dispersion: f32) -> Settings {
        Settings {
            height_source: HeightSource::Procedural,
            map_channel: MapChannel::Luma,
            invert_map: false,
            map_black: 0.0,
            map_white: 1.0,
            procedural: ProceduralSettings {
                shape: Shape::Sphere,
                center: [2.0, 2.0],
                size: 8.0,
                aspect: 1.0,
                rotation: 0.0,
                roundness: 0.5,
                ring_width: 0.25,
                facet_size: 4.0,
                facet_amount: 1.0,
                facet_jitter: 0.75,
                crack_width: 1.0,
                crack_depth: 0.9,
                radial_cracks: 18,
                crack_branching: 0.55,
                crack_jitter: 0.45,
                stress_rings: 6,
                ring_jitter: 0.35,
                impact_falloff: 0.2,
                seed: 1,
            },
            height_strength: 100.0,
            normal_radius: 1.0,
            refraction: 7.25,
            dispersion,
            dispersion_steps: 8,
            auto_spectral_steps: false,
            sampling: Sampling::Bilinear,
            edge: EdgeMode::Clamp,
            mix: 1.0,
            preserve_alpha: false,
            debug: DebugView::Final,
            clamp_32: false,
            downsample: [1.0, 1.0],
        }
    }

    fn test_source() -> LayerBuffer {
        let mut pixels = Vec::new();
        for y in 0..6 {
            for x in 0..7 {
                let alpha = 0.35 + (x + y) as f32 * 0.04;
                pixels.push(PixelF32 {
                    alpha,
                    red: alpha * (x as f32 / 6.0),
                    green: alpha * (y as f32 / 5.0),
                    blue: alpha * ((x + y) as f32 / 11.0),
                });
            }
        }
        LayerBuffer {
            pixels,
            width: 7,
            height: 6,
            origin: [0.0, 0.0],
        }
    }

    fn legacy_wavelength_refraction_offset(
        surface_normal: [f32; 3],
        refraction: f32,
        dispersion: f32,
        refractive_index: f32,
        optics: OpticalCalibration,
    ) -> [f32; 2] {
        let reference_ray = refracted_screen_slope(surface_normal, optics.reference_index);
        let wavelength_ray = refracted_screen_slope(surface_normal, refractive_index);
        add2(
            mul2(reference_ray, refraction * optics.projection_scale),
            mul2(
                sub2(wavelength_ray, reference_ray),
                dispersion * optics.dispersion_scale,
            ),
        )
    }

    fn legacy_spectral_refraction(
        x: f32,
        y: f32,
        normal: [f32; 3],
        source: &LayerBuffer,
        settings: Settings,
        optics: OpticalCalibration,
        spectral_bands: &[SpectralBand],
    ) -> ([f32; 3], f32) {
        let mut rgb_sum = [0.0_f32; 3];
        let mut rgb_weight = [0.0_f32; 3];
        let mut alpha_sum = 0.0_f32;
        let mut alpha_weight = 0.0_f32;
        for band in spectral_bands {
            let offset = legacy_wavelength_refraction_offset(
                normal,
                settings.refraction,
                settings.dispersion,
                band.refractive_index,
                optics,
            );
            let sample = displaced_sample(source, x, y, offset, settings);
            let rgb = unpremultiplied_rgb(sample);
            for channel in 0..3 {
                rgb_sum[channel] += rgb[channel] * band.rgb_response[channel];
                rgb_weight[channel] += band.rgb_response[channel];
            }
            alpha_sum += sample.alpha.clamp(0.0, 1.0) * band.luminance_response;
            alpha_weight += band.luminance_response;
        }
        (
            [
                rgb_sum[0] / rgb_weight[0].max(1.0e-8),
                rgb_sum[1] / rgb_weight[1].max(1.0e-8),
                rgb_sum[2] / rgb_weight[2].max(1.0e-8),
            ],
            alpha_sum / alpha_weight.max(1.0e-8),
        )
    }

    #[test]
    fn shared_reference_ray_preserves_legacy_spectral_result_bitwise() {
        let settings = test_settings(4.75);
        let optics = OpticalCalibration::bk7();
        let bands = build_spectral_bands(settings, optics);
        let source = test_source();
        for slope in [[-1.25, 0.4], [0.0, 0.0], [0.35, -2.0]] {
            let normal = surface_normal(slope);
            let expected =
                legacy_spectral_refraction(3.125, 2.75, normal, &source, settings, optics, &bands);
            let actual =
                spectral_refraction(3.125, 2.75, normal, &source, settings, optics, &bands);
            for channel in 0..3 {
                assert_eq!(actual.0[channel].to_bits(), expected.0[channel].to_bits());
            }
            assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        }
    }

    #[test]
    fn spectral_refraction_computes_reference_ray_once_per_pixel() {
        let settings = test_settings(4.75);
        let optics = OpticalCalibration::bk7();
        let bands = build_spectral_bands(settings, optics);
        let source = test_source();
        let normal = surface_normal([-0.75, 0.25]);

        REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| calls.set(0));
        let _ = spectral_refraction(3.0, 2.0, normal, &source, settings, optics, &bands);
        REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| assert_eq!(calls.get(), bands.len() + 1));

        REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| calls.set(0));
        let _ = legacy_spectral_refraction(3.0, 2.0, normal, &source, settings, optics, &bands);
        REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| assert_eq!(calls.get(), bands.len() * 2));
    }

    #[test]
    fn debug_views_only_prepare_optical_work_they_use() {
        for (view, expects_optics, expected_bands, expected_ray_calls) in [
            (DebugView::Final, true, 8, 2),
            (DebugView::Height, false, 0, 0),
            (DebugView::Normal, false, 0, 0),
            (DebugView::Displacement, true, 0, 2),
            (DebugView::FacetId, false, 0, 0),
        ] {
            let mut settings = test_settings(4.75);
            settings.debug = view;
            REFRACTED_SCREEN_SLOPE_CALLS.with(|calls| calls.set(0));
            let (optics, bands) = prepare_render_optics(settings);
            assert_eq!(optics.is_some(), expects_optics, "{view:?}");
            assert_eq!(bands.len(), expected_bands, "{view:?}");
            REFRACTED_SCREEN_SLOPE_CALLS
                .with(|calls| assert_eq!(calls.get(), expected_ray_calls, "{view:?}"));
        }
    }

    #[test]
    fn scaled_coordinates_preserve_endpoints() {
        assert_eq!(scaled_coordinate(-100.0, -100.0, 1920, 25.0, 960), 25.0);
        assert!((scaled_coordinate(1819.0, -100.0, 1920, 25.0, 960) - 984.0).abs() < 1.0e-5);
    }

    #[test]
    fn channel_value_unpremultiplies_rgb() {
        let pixel = PixelF32 {
            alpha: 0.5,
            red: 0.4,
            green: 0.2,
            blue: 0.1,
        };
        assert!((channel_value(pixel, MapChannel::Red) - 0.8).abs() < 1.0e-6);
        assert!((channel_value(pixel, MapChannel::Alpha) - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn refraction_mix_interpolates_associated_color() {
        let transparent = PixelF32 {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
        let opaque_red = PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        };
        let output = composite_refracted(transparent, opaque_red, 0.5, false);
        assert!((output.alpha - 0.5).abs() < 1.0e-6);
        assert!((output.red - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn layer_buffer_samples_in_layer_coordinates() {
        let buffer = LayerBuffer {
            pixels: vec![PixelF32 {
                alpha: 1.0,
                red: 0.75,
                green: 0.0,
                blue: 0.0,
            }],
            width: 1,
            height: 1,
            origin: [-12.0, 7.0],
        };
        let pixel = buffer.sample(-12.0, 7.0, Sampling::Nearest, EdgeMode::Transparent);
        assert_eq!(pixel.red, 0.75);
    }

    #[test]
    fn sellmeier_matches_bk7_reference_indices() {
        // Published Schott values at the F, d, and C Fraunhofer lines.
        for (wavelength, expected) in [(486.13, 1.522_38), (587.56, 1.516_80), (656.27, 1.514_32)] {
            assert!((bk7_refractive_index(wavelength) - expected).abs() < 5.0e-5);
        }
        assert!(bk7_refractive_index(380.0) > bk7_refractive_index(780.0));
    }

    #[test]
    fn snell_ray_is_normalized_and_obeys_the_sine_law() {
        let normal = surface_normal([1.0, 0.0]);
        let index = bk7_refractive_index(REFERENCE_WAVELENGTH_NM);
        let ray = refracted_ray(normal, index);
        let ray_length = (ray[0].mul_add(ray[0], ray[1] * ray[1]) + ray[2] * ray[2]).sqrt();
        assert!((ray_length - 1.0).abs() < 1.0e-5);

        let sin_incident = (1.0 - normal[2] * normal[2]).sqrt();
        let cos_transmitted = -(ray[0] * normal[0] + ray[1] * normal[1] + ray[2] * normal[2]);
        let sin_transmitted = (1.0 - cos_transmitted * cos_transmitted).max(0.0).sqrt();
        assert!((sin_transmitted - sin_incident / index).abs() < 1.0e-5);
    }

    #[test]
    fn flat_interface_has_no_screen_displacement() {
        let optics = OpticalCalibration::bk7();
        let normal = surface_normal([0.0, 0.0]);
        assert_eq!(
            reference_refraction_offset(normal, 100.0, optics),
            [0.0, 0.0]
        );
        assert_eq!(
            wavelength_refraction_offset(normal, 100.0, 20.0, bk7_refractive_index(380.0), optics,),
            [0.0, 0.0]
        );
    }

    #[test]
    fn refraction_control_is_calibrated_at_a_45_degree_slope() {
        let optics = OpticalCalibration::bk7();
        let offset = reference_refraction_offset(surface_normal([1.0, 0.0]), 40.0, optics);
        assert!((length2(offset) - 40.0).abs() < 1.0e-4);
    }

    #[test]
    fn snell_projection_tracks_height_gradient_direction() {
        // central_difference returns the outward normal's XY terms. A height
        // ramp rising toward +X therefore sends the camera ray toward +X
        // inside the glass backing plane.
        let optics = OpticalCalibration::bk7();
        let offset = reference_refraction_offset(surface_normal([-1.0, 0.0]), 40.0, optics);
        assert!(offset[0] > 0.0);
        assert!(offset[1].abs() < 1.0e-6);
    }

    #[test]
    fn sellmeier_dispersion_orders_blue_reference_and_red_rays() {
        let optics = OpticalCalibration::bk7();
        let normal = surface_normal([-1.0, 0.0]);
        let blue =
            wavelength_refraction_offset(normal, 40.0, 5.0, bk7_refractive_index(380.0), optics);
        let reference = reference_refraction_offset(normal, 40.0, optics);
        let red =
            wavelength_refraction_offset(normal, 40.0, 5.0, bk7_refractive_index(780.0), optics);
        assert!((blue[0] - reference[0] - 5.0).abs() < 1.0e-4);
        assert!(blue[0] > reference[0] && reference[0] > red[0]);
    }

    #[test]
    fn legacy_shape_popup_indices_remain_stable() {
        assert_eq!(shape(5), Shape::Facets);
        assert_eq!(shape(6), Shape::Shards);
        assert_eq!(shape(7), Shape::ImpactGlass);
    }

    #[test]
    fn spectral_responses_are_smooth_and_cover_every_channel() {
        let violet = spectral_response(420.0);
        let green = spectral_response(545.0);
        let red = spectral_response(650.0);
        assert!(violet[2] > violet[1] && violet[2] > violet[0]);
        assert!(green[1] > green[0] && green[1] > green[2]);
        assert!(red[0] > red[1] && red[0] > red[2]);
        let mut integrated = [0.0_f32; 3];
        for wavelength in (380..=780).step_by(5) {
            let response = spectral_response(wavelength as f32);
            assert!(
                response
                    .iter()
                    .all(|weight| weight.is_finite() && *weight >= 0.0)
            );
            for channel in 0..3 {
                integrated[channel] += response[channel];
            }
        }
        assert!(integrated.iter().all(|weight| *weight > 1.0));
        assert!(cie_xyz_1931(555.0)[1] > cie_xyz_1931(450.0)[1]);
        assert!(cie_xyz_1931(555.0)[1] > cie_xyz_1931(650.0)[1]);
    }

    #[test]
    fn automatic_steps_scale_with_optical_difficulty_and_stay_bounded() {
        let optics = OpticalCalibration::bk7();
        let easy =
            automatic_spectral_steps(20.0, 1.0, 20.0, 4.0, [0.5, 0.5], Sampling::Bilinear, optics);
        let hard = automatic_spectral_steps(
            256.0,
            64.0,
            200.0,
            0.25,
            [1.0, 1.0],
            Sampling::Nearest,
            optics,
        );
        assert!((6..=32).contains(&easy));
        assert!((6..=32).contains(&hard));
        assert!(hard > easy);
    }

    #[test]
    fn automatic_steps_respect_render_resolution() {
        let optics = OpticalCalibration::bk7();
        let full = automatic_spectral_steps(
            100.0,
            12.0,
            80.0,
            1.0,
            [1.0, 1.0],
            Sampling::Bilinear,
            optics,
        );
        let quarter = automatic_spectral_steps(
            100.0,
            12.0,
            80.0,
            1.0,
            [0.25, 0.25],
            Sampling::Bilinear,
            optics,
        );
        assert!(full >= quarter);
    }
}
