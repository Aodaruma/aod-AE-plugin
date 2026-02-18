#![allow(clippy::drop_non_drop, clippy::question_mark)]

use after_effects as ae;
use color_art::{Color as ArtColor, ColorSpace as ArtColorSpace};
use palette::hues::{OklabHue, RgbHue};
use palette::{FromColor, Hsv, LinSrgb, Oklab, Oklch, Srgb};
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Instant;

#[cfg(feature = "gpu_wgpu")]
use std::sync::Arc;

use ae::pf::*;
use utils::ToPixel;

#[cfg(feature = "gpu_wgpu")]
mod gpu;
#[cfg(feature = "gpu_wgpu")]
use crate::gpu::wgpu::{WgpuContext, WgpuRenderParams};

const MAX_CLUSTERS: usize = 64;
const MAX_SELECTED_COLORS: usize = 8;
const SPLIT_DECISION_MAX_SAMPLES: usize = 4096;
const HAMERLY_EPSILON: f32 = 1.0e-4;
const OKLAB_AB_MAX: f32 = 0.5;
const OKLCH_CHROMA_MAX: f32 = 0.4;
const YIQ_I_MAX: f32 = 0.5957;
const YIQ_Q_MAX: f32 = 0.5226;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ClusterMethod,
    Colors,
    AutoMaxClusters,
    MaxIterations,
    Seed,
    ColorSpace,
    InitMethod,
    AreaSimilarityThreshold,
    SelectedColorCount,
    SelectedColor1,
    SelectedColor2,
    SelectedColor3,
    SelectedColor4,
    SelectedColor5,
    SelectedColor6,
    SelectedColor7,
    SelectedColor8,
    GMeansAlpha,
    RgbOnly,
    UseGpuIfAvailable,
}

const SELECTED_COLOR_PARAMS: [Params; MAX_SELECTED_COLORS] = [
    Params::SelectedColor1,
    Params::SelectedColor2,
    Params::SelectedColor3,
    Params::SelectedColor4,
    Params::SelectedColor5,
    Params::SelectedColor6,
    Params::SelectedColor7,
    Params::SelectedColor8,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClusterMethod {
    KMeans = 0,
    XMeans = 1,
    GMeans = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitMethod {
    Random = 0,
    Area = 1,
    SelectedColors = 2,
    KMeansParallel = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ColorSpace {
    LinearRgb = 0,
    Oklab = 1,
    Oklch = 2,
    Hsv = 3,
    Yiq = 4,
    AlphaOnly = 5,
}

impl ColorSpace {
    fn has_circular_hue(self) -> bool {
        matches!(self, Self::Oklch | Self::Hsv)
    }
}

#[derive(Clone)]
struct RenderSettings {
    cluster_method: ClusterMethod,
    cluster_count: usize,
    auto_max_clusters: usize,
    max_iterations: usize,
    seed: u32,
    color_space: ColorSpace,
    init_method: InitMethod,
    area_similarity_threshold: f32,
    selected_colors: Vec<[f32; 3]>,
    gmeans_alpha: f32,
    rgb_only: bool,
    use_gpu_if_available: bool,
}

#[derive(Clone)]
struct ClusterResult {
    centroids: Vec<[f32; 4]>,
    labels: Vec<usize>,
    counts: Vec<usize>,
    sse_per_cluster: Vec<f64>,
}

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}

ae::define_effect!(Plugin, (), Params);

const PLUGIN_DESCRIPTION: &str = "Reduces image colors with k-means clustering.";
#[cfg(feature = "gpu_wgpu")]
static WGPU_CONTEXT: OnceLock<Result<Arc<WgpuContext>, String>> = OnceLock::new();
static PERF_LOG_ENABLED: OnceLock<bool> = OnceLock::new();

#[cfg(feature = "gpu_wgpu")]
fn wgpu_context() -> Result<Arc<WgpuContext>, String> {
    match WGPU_CONTEXT.get_or_init(|| WgpuContext::new().map(Arc::new)) {
        Ok(ctx) => Ok(ctx.clone()),
        Err(reason) => Err(reason.clone()),
    }
}

#[derive(Default)]
struct PerfFrame {
    enabled: bool,
    start: Option<Instant>,
    last: Option<Instant>,
    stages: Vec<(&'static str, f64)>,
}

impl PerfFrame {
    fn new() -> Self {
        let enabled = perf_log_enabled();
        let now = if enabled { Some(Instant::now()) } else { None };
        Self {
            enabled,
            start: now,
            last: now,
            stages: Vec::new(),
        }
    }

    fn mark(&mut self, stage: &'static str) {
        if !self.enabled {
            return;
        }
        if let Some(last) = self.last {
            let now = Instant::now();
            self.stages
                .push((stage, now.duration_since(last).as_secs_f64() * 1000.0));
            self.last = Some(now);
        }
    }

    fn flush(
        &self,
        width: usize,
        height: usize,
        method: ClusterMethod,
        backend: &str,
        extra: &str,
    ) {
        if !self.enabled {
            return;
        }
        let total_ms = self
            .start
            .map(|s| s.elapsed().as_secs_f64() * 1000.0)
            .unwrap_or(0.0);

        let mut summary = format!(
            "[AOD_ColorQuantize][perf] {}x{} method={:?} backend={} total={:.3}ms",
            width, height, method, backend, total_ms
        );
        if !extra.is_empty() {
            summary.push(' ');
            summary.push_str(extra);
        }

        for (stage, ms) in &self.stages {
            summary.push_str(&format!(" | {}={:.3}ms", stage, ms));
        }

        perf_log(summary.as_str());
    }
}

fn perf_log_enabled() -> bool {
    *PERF_LOG_ENABLED.get_or_init(|| cfg!(debug_assertions))
}

fn perf_log(message: &str) {
    #[cfg(target_os = "windows")]
    {
        let mut wide = message.encode_utf16().collect::<Vec<u16>>();
        wide.push(b'\n' as u16);
        wide.push(0);
        unsafe {
            OutputDebugStringW(wide.as_ptr());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("{}", message);
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OutputDebugStringW(lp_output_string: *const u16);
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        let supervise_flags = || {
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP
        };

        params.add_with_flags(
            Params::ClusterMethod,
            "Cluster Method",
            PopupDef::setup(|d| {
                d.set_options(&["k-means", "x-means", "g-means"]);
                d.set_default(1);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::Colors,
            "Colors (K)",
            SliderDef::setup(|d| {
                d.set_valid_min(2);
                d.set_valid_max(MAX_CLUSTERS as i32);
                d.set_slider_min(2);
                d.set_slider_max(32);
                d.set_default(8);
            }),
        )?;

        params.add(
            Params::AutoMaxClusters,
            "Auto Max Clusters",
            SliderDef::setup(|d| {
                d.set_valid_min(2);
                d.set_valid_max(MAX_CLUSTERS as i32);
                d.set_slider_min(2);
                d.set_slider_max(32);
                d.set_default(16);
            }),
        )?;

        params.add(
            Params::MaxIterations,
            "Max Iterations",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(128);
                d.set_slider_min(1);
                d.set_slider_max(32);
                d.set_default(16);
            }),
        )?;

        params.add(
            Params::Seed,
            "Seed",
            SliderDef::setup(|d| {
                d.set_valid_min(0);
                d.set_valid_max(1_000_000);
                d.set_slider_min(0);
                d.set_slider_max(10_000);
                d.set_default(0);
            }),
        )?;

        params.add_with_flags(
            Params::ColorSpace,
            "Color Space",
            PopupDef::setup(|d| {
                d.set_options(&["Linear RGB", "OKLab", "OKLCH", "HSV", "YIQ", "Alpha Only"]);
                d.set_default(2);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::InitMethod,
            "Init Method",
            PopupDef::setup(|d| {
                d.set_options(&["Random", "Area Similarity", "Selected Colors", "k-means||"]);
                d.set_default(1);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::AreaSimilarityThreshold,
            "Area Similarity Threshold",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.001);
                d.set_valid_max(1.0);
                d.set_slider_min(0.005);
                d.set_slider_max(0.2);
                d.set_default(0.04);
                d.set_precision(4);
            }),
        )?;

        params.add_with_flags(
            Params::SelectedColorCount,
            "Selected Colors Count",
            SliderDef::setup(|d| {
                d.set_valid_min(1);
                d.set_valid_max(MAX_SELECTED_COLORS as i32);
                d.set_slider_min(1);
                d.set_slider_max(MAX_SELECTED_COLORS as i32);
                d.set_default(4);
            }),
            supervise_flags(),
            ae::ParamUIFlags::empty(),
        )?;

        let defaults = [
            Pixel8 {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Pixel8 {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255,
            },
            Pixel8 {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
            Pixel8 {
                red: 255,
                green: 255,
                blue: 0,
                alpha: 255,
            },
            Pixel8 {
                red: 255,
                green: 0,
                blue: 255,
                alpha: 255,
            },
            Pixel8 {
                red: 0,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            Pixel8 {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            Pixel8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
        ];

        for (idx, param_id) in SELECTED_COLOR_PARAMS.iter().enumerate() {
            params.add(
                *param_id,
                &format!("Selected Color {}", idx + 1),
                ColorDef::setup(|d| {
                    d.set_default(defaults[idx]);
                }),
            )?;
        }

        params.add(
            Params::GMeansAlpha,
            "g-means Alpha",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(0.0001);
                d.set_valid_max(0.5);
                d.set_slider_min(0.001);
                d.set_slider_max(0.2);
                d.set_default(0.05);
                d.set_precision(4);
            }),
        )?;

        params.add(
            Params::RgbOnly,
            "RGB Only",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
            }),
        )?;

        params.add(
            Params::UseGpuIfAvailable,
            "Use GPU (if available)",
            CheckBoxDef::setup(|d| {
                d.set_default(true);
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
                        "AOD_ColorQuantize - {version}\r\r{PLUGIN_DESCRIPTION}\rCopyright (c) 2026-{build_year} Aodaruma",
                        version = env!("CARGO_PKG_VERSION"),
                        build_year = env!("BUILD_YEAR")
                    )
                    .as_str(),
                );
            }
            ae::Command::GlobalSetup => {
                out_data.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out_data.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                if let Ok(suite) = ae::aegp::suites::Utility::new()
                    && let Ok(plugin_id) = suite.register_with_aegp("AOD_ColorQuantize")
                {
                    self.aegp_id = Some(plugin_id);
                }
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let mut out_layer = out_layer;
                self.render_auto(&in_layer, &mut out_layer, params)?;
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

                if let (Some(in_layer), Some(mut out_layer)) = (in_layer_opt, out_layer_opt) {
                    self.render_auto(&in_layer, &mut out_layer, params)?;
                }
                cb.checkin_layer_pixels(0)?;
            }
            ae::Command::UserChangedParam { param_index } => {
                let t = params.type_at(param_index);
                if t == Params::ClusterMethod
                    || t == Params::InitMethod
                    || t == Params::SelectedColorCount
                    || t == Params::ColorSpace
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
        let method =
            cluster_method_from_popup(params.get(Params::ClusterMethod)?.as_popup()?.value());
        let init_method =
            init_method_from_popup(params.get(Params::InitMethod)?.as_popup()?.value());
        let selected_count = params
            .get(Params::SelectedColorCount)?
            .as_slider()?
            .value()
            .clamp(1, MAX_SELECTED_COLORS as i32) as usize;
        let color_space =
            color_space_from_popup(params.get(Params::ColorSpace)?.as_popup()?.value());

        let is_kmeans = matches!(method, ClusterMethod::KMeans);
        let is_auto = !is_kmeans;

        Self::set_param_enabled(params, Params::Colors, is_kmeans)?;
        self.set_param_visible(in_data, params, Params::AutoMaxClusters, is_auto)?;
        Self::set_param_enabled(params, Params::AutoMaxClusters, is_auto)?;

        let show_area = matches!(init_method, InitMethod::Area);
        self.set_param_visible(in_data, params, Params::AreaSimilarityThreshold, show_area)?;
        Self::set_param_enabled(params, Params::AreaSimilarityThreshold, show_area)?;

        let show_selected = matches!(init_method, InitMethod::SelectedColors);
        self.set_param_visible(in_data, params, Params::SelectedColorCount, show_selected)?;
        Self::set_param_enabled(params, Params::SelectedColorCount, show_selected)?;

        for (idx, id) in SELECTED_COLOR_PARAMS.iter().enumerate() {
            let visible = show_selected && idx < selected_count;
            self.set_param_visible(in_data, params, *id, visible)?;
            Self::set_param_enabled(params, *id, visible)?;
        }

        let show_gmeans_alpha = matches!(method, ClusterMethod::GMeans);
        self.set_param_visible(in_data, params, Params::GMeansAlpha, show_gmeans_alpha)?;
        Self::set_param_enabled(params, Params::GMeansAlpha, show_gmeans_alpha)?;
        let allow_rgb_only = !matches!(color_space, ColorSpace::AlphaOnly);
        Self::set_param_enabled(params, Params::RgbOnly, allow_rgb_only)?;

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

    fn set_param_enabled(
        params: &mut ae::Parameters<Params>,
        id: Params,
        enabled: bool,
    ) -> Result<(), Error> {
        Self::set_param_ui_flag(params, id, ae::pf::ParamUIFlags::DISABLED, !enabled)
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

    fn render_auto(
        &self,
        in_layer: &Layer,
        out_layer: &mut Layer,
        params: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        let width = out_layer.width();
        let height = out_layer.height();
        if width == 0 || height == 0 {
            return Ok(());
        }
        let mut perf = PerfFrame::new();

        let pixel_count = width
            .checked_mul(height)
            .ok_or(Error::BadCallbackParameter)?;
        let settings = read_settings(params, pixel_count)?;
        perf.mark("read_settings");
        let encoded_pixels = encode_input_layer(in_layer, width, height, settings.color_space);
        perf.mark("encode_input");
        let _allow_gpu = settings.use_gpu_if_available;

        #[cfg(feature = "gpu_wgpu")]
        {
            if _allow_gpu && matches!(settings.cluster_method, ClusterMethod::KMeans) {
                let target_k = target_k_for_kmeans(&settings, pixel_count);
                let initial = build_initial_centroids(&encoded_pixels, &settings, target_k);
                perf.mark("init_centroids");
                if initial.is_empty() {
                    if perf_log_enabled() {
                        perf_log("[AOD_ColorQuantize][gpu] fallback: initial centroids are empty");
                    }
                } else {
                    match wgpu_context() {
                        Ok(ctx) => match self.do_render_wgpu(
                            width,
                            height,
                            &encoded_pixels,
                            &initial,
                            &settings,
                            &ctx,
                        ) {
                            Ok(output) => {
                                perf.mark("gpu_cluster");
                                write_output_layer(out_layer, &output.data)?;
                                perf.mark("write_output");
                                let extra = format!(
                                    "iters={} converged={}",
                                    output.stats.iterations_executed, output.stats.converged
                                );
                                perf.flush(
                                    width,
                                    height,
                                    settings.cluster_method,
                                    "gpu",
                                    extra.as_str(),
                                );
                                return Ok(());
                            }
                            Err(err) => {
                                if perf_log_enabled() {
                                    let msg = format!(
                                        "[AOD_ColorQuantize][gpu] fallback: render failed: {:?}",
                                        err
                                    );
                                    perf_log(&msg);
                                }
                            }
                        },
                        Err(reason) => {
                            if perf_log_enabled() {
                                let msg = format!(
                                    "[AOD_ColorQuantize][gpu] fallback: context init failed: {}",
                                    reason
                                );
                                perf_log(&msg);
                            }
                        }
                    }
                }
            }
        }

        let result = run_cluster_method(&encoded_pixels, &settings);
        perf.mark("cpu_cluster");

        #[cfg(feature = "gpu_wgpu")]
        {
            if _allow_gpu
                && !matches!(settings.cluster_method, ClusterMethod::KMeans)
                && !result.centroids.is_empty()
            {
                match wgpu_context() {
                    Ok(ctx) => match self.do_render_wgpu(
                        width,
                        height,
                        &encoded_pixels,
                        result.centroids.as_slice(),
                        &settings,
                        &ctx,
                    ) {
                        Ok(output) => {
                            perf.mark("gpu_refine");
                            write_output_layer(out_layer, &output.data)?;
                            perf.mark("write_output");
                            let extra = format!(
                                "iters={} converged={} seed_clusters={}",
                                output.stats.iterations_executed,
                                output.stats.converged,
                                result.centroids.len()
                            );
                            perf.flush(
                                width,
                                height,
                                settings.cluster_method,
                                "gpu_refine",
                                extra.as_str(),
                            );
                            return Ok(());
                        }
                        Err(err) => {
                            if perf_log_enabled() {
                                let msg = format!(
                                    "[AOD_ColorQuantize][gpu] fallback: refine failed: {:?}",
                                    err
                                );
                                perf_log(&msg);
                            }
                        }
                    },
                    Err(reason) => {
                        if perf_log_enabled() {
                            let msg = format!(
                                "[AOD_ColorQuantize][gpu] fallback: refine context init failed: {}",
                                reason
                            );
                            perf_log(&msg);
                        }
                    }
                }
            }
        }

        let output = compose_output_from_clusters(&encoded_pixels, &result, &settings);
        perf.mark("compose_output");
        write_output_layer(out_layer, &output)?;
        perf.mark("write_output");
        perf.flush(width, height, settings.cluster_method, "cpu", "");
        Ok(())
    }

    #[cfg(feature = "gpu_wgpu")]
    fn do_render_wgpu(
        &self,
        width: usize,
        height: usize,
        encoded_pixels: &[f32],
        initial_centroids: &[[f32; 4]],
        settings: &RenderSettings,
        ctx: &WgpuContext,
    ) -> Result<crate::gpu::wgpu::WgpuOutput, Error> {
        if initial_centroids.is_empty() {
            return Err(Error::BadCallbackParameter);
        }

        let render_params = WgpuRenderParams {
            out_w: width as u32,
            out_h: height as u32,
            cluster_count: initial_centroids.len() as u32,
            max_iterations: settings.max_iterations as u32,
            color_space: settings.color_space as u32,
            rgb_only: settings.rgb_only,
            sum_scale: compute_sum_scale((width as u32).saturating_mul(height as u32)),
        };

        let output = ctx.render(&render_params, encoded_pixels, initial_centroids)?;
        Ok(output)
    }
}

fn read_settings(
    params: &mut Parameters<Params>,
    pixel_count: usize,
) -> Result<RenderSettings, Error> {
    let cluster_method =
        cluster_method_from_popup(params.get(Params::ClusterMethod)?.as_popup()?.value());
    let cluster_count = params.get(Params::Colors)?.as_slider()?.value().max(2) as usize;
    let auto_max_clusters = params
        .get(Params::AutoMaxClusters)?
        .as_slider()?
        .value()
        .max(2) as usize;
    let max_iterations = params
        .get(Params::MaxIterations)?
        .as_slider()?
        .value()
        .max(1) as usize;
    let seed = params.get(Params::Seed)?.as_slider()?.value().max(0) as u32;
    let color_space = color_space_from_popup(params.get(Params::ColorSpace)?.as_popup()?.value());
    let init_method = init_method_from_popup(params.get(Params::InitMethod)?.as_popup()?.value());
    let area_similarity_threshold = params
        .get(Params::AreaSimilarityThreshold)?
        .as_float_slider()?
        .value() as f32;
    let selected_count = params
        .get(Params::SelectedColorCount)?
        .as_slider()?
        .value()
        .clamp(1, MAX_SELECTED_COLORS as i32) as usize;
    let gmeans_alpha = params.get(Params::GMeansAlpha)?.as_float_slider()?.value() as f32;
    let rgb_only = params.get(Params::RgbOnly)?.as_checkbox()?.value()
        && !matches!(color_space, ColorSpace::AlphaOnly);
    let use_gpu_if_available = params
        .get(Params::UseGpuIfAvailable)?
        .as_checkbox()?
        .value();

    let mut selected_colors = Vec::with_capacity(selected_count);
    for param_id in SELECTED_COLOR_PARAMS.iter().take(selected_count) {
        let c = params.get(*param_id)?.as_color()?.value().to_pixel32();
        selected_colors.push([sanitize01(c.red), sanitize01(c.green), sanitize01(c.blue)]);
    }

    Ok(RenderSettings {
        cluster_method,
        cluster_count: cluster_count.min(MAX_CLUSTERS).min(pixel_count.max(1)),
        auto_max_clusters: auto_max_clusters.min(MAX_CLUSTERS).min(pixel_count.max(1)),
        max_iterations,
        seed,
        color_space,
        init_method,
        area_similarity_threshold: area_similarity_threshold.clamp(0.0001, 1.0),
        selected_colors,
        gmeans_alpha: gmeans_alpha.clamp(0.0001, 0.5),
        rgb_only,
        use_gpu_if_available,
    })
}

fn target_k_for_kmeans(settings: &RenderSettings, pixel_count: usize) -> usize {
    settings
        .cluster_count
        .clamp(1, MAX_CLUSTERS)
        .min(pixel_count.max(1))
}

fn run_cluster_method(samples: &[f32], settings: &RenderSettings) -> ClusterResult {
    match settings.cluster_method {
        ClusterMethod::KMeans => {
            let target_k = target_k_for_kmeans(settings, samples.len() / 4);
            let initial = build_initial_centroids(samples, settings, target_k);
            run_kmeans(
                samples,
                initial,
                settings.max_iterations,
                settings.color_space,
                settings.seed,
            )
        }
        ClusterMethod::XMeans => run_xmeans(samples, settings),
        ClusterMethod::GMeans => run_gmeans(samples, settings),
    }
}

fn run_xmeans(samples: &[f32], settings: &RenderSettings) -> ClusterResult {
    let sample_count = samples.len() / 4;
    if sample_count == 0 {
        return ClusterResult {
            centroids: vec![],
            labels: vec![],
            counts: vec![],
            sse_per_cluster: vec![],
        };
    }

    let start_k = match settings.init_method {
        InitMethod::SelectedColors => settings.selected_colors.len().max(1),
        _ => 2,
    }
    .min(settings.auto_max_clusters)
    .min(sample_count);

    let mut centroids = build_initial_centroids(samples, settings, start_k.max(1));
    if centroids.is_empty() {
        centroids.push(sample_at(samples, 0));
    }

    let mut round = 0u32;
    loop {
        let result = run_kmeans(
            samples,
            centroids.clone(),
            settings.max_iterations,
            settings.color_space,
            settings.seed.wrapping_add(round),
        );
        round = round.wrapping_add(17);
        if result.centroids.is_empty() {
            return result;
        }

        let clusters = build_cluster_indices(&result.labels, result.centroids.len());
        let mut next_centroids = Vec::with_capacity(settings.auto_max_clusters);
        let mut split_count = 0usize;

        for (cluster_idx, indices) in clusters.iter().enumerate() {
            let parent = result.centroids[cluster_idx];
            if indices.len() < 8 || next_centroids.len() + 1 > settings.auto_max_clusters {
                next_centroids.push(parent);
                continue;
            }

            if let Some((c1, c2)) = split_cluster_xmeans(
                samples,
                indices,
                parent,
                result.sse_per_cluster[cluster_idx],
                settings,
                settings.seed.wrapping_add(cluster_idx as u32 * 97 + round),
            ) && next_centroids.len() + 2 <= settings.auto_max_clusters
            {
                next_centroids.push(c1);
                next_centroids.push(c2);
                split_count += 1;
                continue;
            }

            next_centroids.push(parent);
        }

        if split_count == 0 || next_centroids.len() == centroids.len() {
            return run_kmeans(
                samples,
                result.centroids,
                settings.max_iterations,
                settings.color_space,
                settings.seed.wrapping_add(0xA55A_5AA5),
            );
        }

        centroids = dedup_centroids(next_centroids, settings.color_space);
        if centroids.len() >= settings.auto_max_clusters {
            return run_kmeans(
                samples,
                centroids,
                settings.max_iterations,
                settings.color_space,
                settings.seed.wrapping_add(0x11CC_22DD),
            );
        }
    }
}

fn run_gmeans(samples: &[f32], settings: &RenderSettings) -> ClusterResult {
    let sample_count = samples.len() / 4;
    if sample_count == 0 {
        return ClusterResult {
            centroids: vec![],
            labels: vec![],
            counts: vec![],
            sse_per_cluster: vec![],
        };
    }

    let start_k = match settings.init_method {
        InitMethod::SelectedColors => settings.selected_colors.len().max(1),
        _ => 1,
    }
    .min(settings.auto_max_clusters)
    .min(sample_count);

    let mut centroids = build_initial_centroids(samples, settings, start_k.max(1));
    if centroids.is_empty() {
        centroids.push(sample_at(samples, 0));
    }

    let mut round = 0u32;
    loop {
        let result = run_kmeans(
            samples,
            centroids.clone(),
            settings.max_iterations,
            settings.color_space,
            settings.seed.wrapping_add(round),
        );
        round = round.wrapping_add(31);
        if result.centroids.is_empty() {
            return result;
        }

        let clusters = build_cluster_indices(&result.labels, result.centroids.len());
        let mut next_centroids = Vec::with_capacity(settings.auto_max_clusters);
        let mut split_count = 0usize;

        for (cluster_idx, indices) in clusters.iter().enumerate() {
            let parent = result.centroids[cluster_idx];
            if indices.len() < 10 || next_centroids.len() + 1 > settings.auto_max_clusters {
                next_centroids.push(parent);
                continue;
            }

            if let Some((c1, c2)) = split_cluster_gmeans(
                samples,
                indices,
                parent,
                settings,
                settings.seed.wrapping_add(cluster_idx as u32 * 131 + round),
            ) && next_centroids.len() + 2 <= settings.auto_max_clusters
            {
                next_centroids.push(c1);
                next_centroids.push(c2);
                split_count += 1;
                continue;
            }

            next_centroids.push(parent);
        }

        if split_count == 0 || next_centroids.len() == centroids.len() {
            return run_kmeans(
                samples,
                result.centroids,
                settings.max_iterations,
                settings.color_space,
                settings.seed.wrapping_add(0x77DD_33BB),
            );
        }

        centroids = dedup_centroids(next_centroids, settings.color_space);
        if centroids.len() >= settings.auto_max_clusters {
            return run_kmeans(
                samples,
                centroids,
                settings.max_iterations,
                settings.color_space,
                settings.seed.wrapping_add(0x5A5A_22EE),
            );
        }
    }
}

fn split_cluster_xmeans(
    samples: &[f32],
    indices: &[usize],
    parent_centroid: [f32; 4],
    parent_sse: f64,
    settings: &RenderSettings,
    seed: u32,
) -> Option<([f32; 4], [f32; 4])> {
    let sampled_indices = sample_indices_for_split(indices, SPLIT_DECISION_MAX_SAMPLES, seed);
    let (seed_a, seed_b) = make_split_seeds(
        samples,
        sampled_indices.as_slice(),
        parent_centroid,
        settings.color_space,
    );
    let subset = collect_subset_samples(samples, sampled_indices.as_slice());
    let local = run_kmeans(
        &subset,
        vec![seed_a, seed_b],
        settings.max_iterations.min(20),
        settings.color_space,
        seed,
    );
    if local.centroids.len() != 2 || local.counts.contains(&0) {
        return None;
    }

    let child_sse = estimate_two_centroid_sse(
        samples,
        indices,
        local.centroids[0],
        local.centroids[1],
        settings.color_space,
    );
    let n = indices.len();
    if n < 3 {
        return None;
    }

    let bic_parent = bic_score(parent_sse, n, 1, 3);
    let bic_children = bic_score(child_sse, n, 2, 3);

    if bic_children > bic_parent + 0.5 {
        Some((local.centroids[0], local.centroids[1]))
    } else {
        None
    }
}

fn split_cluster_gmeans(
    samples: &[f32],
    indices: &[usize],
    parent_centroid: [f32; 4],
    settings: &RenderSettings,
    seed: u32,
) -> Option<([f32; 4], [f32; 4])> {
    let sampled_indices = sample_indices_for_split(indices, SPLIT_DECISION_MAX_SAMPLES, seed);
    let (seed_a, seed_b) = make_split_seeds(
        samples,
        sampled_indices.as_slice(),
        parent_centroid,
        settings.color_space,
    );
    let subset = collect_subset_samples(samples, sampled_indices.as_slice());
    let local = run_kmeans(
        &subset,
        vec![seed_a, seed_b],
        settings.max_iterations.min(20),
        settings.color_space,
        seed,
    );
    if local.centroids.len() != 2 || local.counts.iter().any(|&c| c < 2) {
        return None;
    }

    let projections = build_split_projections(
        &subset,
        parent_centroid,
        local.centroids[0],
        local.centroids[1],
        settings.color_space,
    );
    if projections.len() < 8 {
        return None;
    }

    let p_value = jarque_bera_p_value(&projections);
    if p_value < settings.gmeans_alpha as f64 {
        Some((local.centroids[0], local.centroids[1]))
    } else {
        None
    }
}
fn run_kmeans(
    samples: &[f32],
    initial_centroids: Vec<[f32; 4]>,
    max_iterations: usize,
    color_space: ColorSpace,
    seed: u32,
) -> ClusterResult {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || initial_centroids.is_empty() {
        return ClusterResult {
            centroids: vec![],
            labels: vec![],
            counts: vec![],
            sse_per_cluster: vec![],
        };
    }

    let mut centroids = dedup_centroids(initial_centroids, color_space);
    if centroids.is_empty() {
        centroids.push(sample_at(samples, 0));
    }
    let k = centroids.len();
    let mut labels = vec![0usize; sample_count];
    let mut upper = vec![f32::INFINITY; sample_count];
    let mut lower = vec![0.0f32; sample_count];
    let mut rng = XorShift64::new(seed as u64 ^ 0xA1D2_C3F4_55AA_1100);

    for idx in 0..sample_count {
        let sample = sample_at(samples, idx);
        let (best_idx, best_dist_sq, second_dist_sq) =
            nearest_two_centroids(sample, &centroids, color_space);
        labels[idx] = best_idx;
        upper[idx] = best_dist_sq.sqrt();
        lower[idx] = second_dist_sq.sqrt();
    }

    for _ in 0..max_iterations.max(1) {
        let separation = half_min_center_distances(&centroids, color_space);
        let mut accums = vec![ClusterAccum::default(); k];
        let mut changed = false;

        for idx in 0..sample_count {
            let sample = sample_at(samples, idx);
            let current = labels[idx];

            if upper[idx] <= separation[current].max(lower[idx]) {
                accums[current].accumulate(sample, color_space);
                continue;
            }

            let current_dist = feature_distance_sq(sample, centroids[current], color_space).sqrt();
            upper[idx] = current_dist;
            if upper[idx] <= separation[current].max(lower[idx]) {
                accums[current].accumulate(sample, color_space);
                continue;
            }

            let (best_idx, best_dist_sq, second_dist_sq) =
                nearest_two_centroids(sample, &centroids, color_space);
            if best_idx != current {
                changed = true;
                labels[idx] = best_idx;
            }
            upper[idx] = best_dist_sq.sqrt();
            lower[idx] = second_dist_sq.sqrt();
            accums[labels[idx]].accumulate(sample, color_space);
        }

        let old_centroids = centroids.clone();
        let mut movements = vec![0.0f32; k];
        let mut max_move = 0.0f32;
        for cluster_idx in 0..k {
            if accums[cluster_idx].count == 0 {
                centroids[cluster_idx] = sample_at(samples, rng.gen_index(sample_count));
            } else {
                centroids[cluster_idx] = accums[cluster_idx].mean(color_space);
            }
            let shift = feature_distance_sq(
                old_centroids[cluster_idx],
                centroids[cluster_idx],
                color_space,
            )
            .sqrt();
            movements[cluster_idx] = shift;
            if shift > max_move {
                max_move = shift;
            }
        }

        for idx in 0..sample_count {
            let label = labels[idx];
            upper[idx] += movements[label];
            lower[idx] = (lower[idx] - max_move).max(0.0);
        }

        if !changed && max_move <= HAMERLY_EPSILON {
            break;
        }
    }

    for idx in 0..sample_count {
        let sample = sample_at(samples, idx);
        labels[idx] = nearest_centroid_idx(sample, &centroids, color_space).0;
    }

    let mut counts = vec![0usize; k];
    let mut sse_per_cluster = vec![0.0f64; k];

    for (idx, label) in labels.iter().copied().enumerate().take(sample_count) {
        let sample = sample_at(samples, idx);
        let dist = feature_distance_sq(sample, centroids[label], color_space) as f64;
        counts[label] += 1;
        sse_per_cluster[label] += dist;
    }

    ClusterResult {
        centroids,
        labels,
        counts,
        sse_per_cluster,
    }
}

fn build_initial_centroids(
    samples: &[f32],
    settings: &RenderSettings,
    target_k: usize,
) -> Vec<[f32; 4]> {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || target_k == 0 {
        return vec![];
    }
    let target_k = target_k.min(sample_count).min(MAX_CLUSTERS);

    let mut centroids = match settings.init_method {
        InitMethod::Random => init_centroids_random(samples, target_k, settings.seed),
        InitMethod::Area => init_centroids_area_based(
            samples,
            target_k,
            settings.area_similarity_threshold,
            settings.color_space,
            settings.seed,
        ),
        InitMethod::SelectedColors => {
            init_centroids_selected(samples, settings, target_k, settings.seed)
        }
        InitMethod::KMeansParallel => {
            init_centroids_kmeans_parallel(samples, target_k, settings.color_space, settings.seed)
        }
    };

    if centroids.len() < target_k {
        let mut rng = XorShift64::new(settings.seed as u64 ^ 0x5511_CC88_2299_AA44);
        while centroids.len() < target_k {
            let idx = rng.gen_index(sample_count);
            centroids.push(sample_at(samples, idx));
        }
    }

    centroids.truncate(target_k);
    dedup_centroids(centroids, settings.color_space)
}

fn init_centroids_random(samples: &[f32], target_k: usize, seed: u32) -> Vec<[f32; 4]> {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || target_k == 0 {
        return vec![];
    }
    let mut rng = XorShift64::new(seed as u64 ^ 0x1234_ABCD_7890_EF01);
    let mut centroids = Vec::with_capacity(target_k);
    let mut attempts = 0usize;

    while centroids.len() < target_k && attempts < target_k * 16 {
        let idx = rng.gen_index(sample_count);
        let sample = sample_at(samples, idx);
        if !centroids.iter().any(|c| nearly_same_feature(*c, sample)) {
            centroids.push(sample);
        }
        attempts += 1;
    }

    if centroids.is_empty() {
        centroids.push(sample_at(samples, 0));
    }
    centroids
}

fn init_centroids_kmeans_parallel(
    samples: &[f32],
    target_k: usize,
    color_space: ColorSpace,
    seed: u32,
) -> Vec<[f32; 4]> {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || target_k == 0 {
        return vec![];
    }
    if target_k == 1 {
        return vec![sample_at(samples, seed as usize % sample_count)];
    }

    let mut rng = XorShift64::new(seed as u64 ^ 0xB7E1_5163_9A4F_2D11);
    let mut candidates = Vec::with_capacity(target_k * 4);
    candidates.push(sample_at(samples, rng.gen_index(sample_count)));

    let oversampling = (target_k * 2).clamp(2, 64);
    let rounds = 5usize;
    let mut nearest_d2 = vec![f32::INFINITY; sample_count];

    for idx in 0..sample_count {
        let sample = sample_at(samples, idx);
        nearest_d2[idx] = feature_distance_sq(sample, candidates[0], color_space);
    }

    for _ in 0..rounds {
        let phi = nearest_d2
            .iter()
            .map(|&d| d as f64)
            .sum::<f64>()
            .max(1.0e-12);
        for idx in 0..sample_count {
            let p = ((oversampling as f64) * (nearest_d2[idx] as f64) / phi).min(1.0);
            if rng.next_f64() < p {
                candidates.push(sample_at(samples, idx));
            }
        }
        candidates = dedup_centroids(candidates, color_space);
        if candidates.len() > MAX_CLUSTERS * 8 {
            break;
        }

        for idx in 0..sample_count {
            let sample = sample_at(samples, idx);
            let mut best = nearest_d2[idx];
            for &c in &candidates {
                let d = feature_distance_sq(sample, c, color_space);
                if d < best {
                    best = d;
                }
            }
            nearest_d2[idx] = best;
        }
    }

    candidates = dedup_centroids(candidates, color_space);
    if candidates.is_empty() {
        return init_centroids_random(samples, target_k, seed);
    }
    if candidates.len() <= target_k {
        let mut out = candidates;
        if out.len() < target_k {
            let mut fallback =
                init_centroids_random(samples, target_k.saturating_sub(out.len()), seed);
            out.append(&mut fallback);
        }
        out.truncate(target_k);
        return dedup_centroids(out, color_space);
    }

    let mut weights = vec![0usize; candidates.len()];
    for idx in 0..sample_count {
        let sample = sample_at(samples, idx);
        let (nearest, _) = nearest_centroid_idx(sample, &candidates, color_space);
        weights[nearest] += 1;
    }

    let mut chosen = Vec::with_capacity(target_k);
    let mut chosen_mask = vec![false; candidates.len()];
    let first_idx = weighted_pick_usize(&weights, &mut rng).unwrap_or(0);
    chosen.push(candidates[first_idx]);
    chosen_mask[first_idx] = true;

    let mut cand_to_chosen_d2 = vec![f32::INFINITY; candidates.len()];
    for c_idx in 0..candidates.len() {
        cand_to_chosen_d2[c_idx] = feature_distance_sq(candidates[c_idx], chosen[0], color_space);
    }

    while chosen.len() < target_k {
        let mut score_sum = 0.0f64;
        for idx in 0..candidates.len() {
            if chosen_mask[idx] || weights[idx] == 0 {
                continue;
            }
            score_sum += (weights[idx] as f64) * (cand_to_chosen_d2[idx] as f64);
        }

        let next_idx = if score_sum <= 1.0e-20 {
            (0..candidates.len())
                .filter(|&i| !chosen_mask[i])
                .max_by_key(|&i| weights[i])
                .unwrap_or(first_idx)
        } else {
            let mut threshold = rng.next_f64() * score_sum;
            let mut picked = first_idx;
            for idx in 0..candidates.len() {
                if chosen_mask[idx] || weights[idx] == 0 {
                    continue;
                }
                threshold -= (weights[idx] as f64) * (cand_to_chosen_d2[idx] as f64);
                if threshold <= 0.0 {
                    picked = idx;
                    break;
                }
            }
            picked
        };

        if chosen_mask[next_idx] {
            break;
        }
        chosen_mask[next_idx] = true;
        chosen.push(candidates[next_idx]);

        for c_idx in 0..candidates.len() {
            let d = feature_distance_sq(candidates[c_idx], candidates[next_idx], color_space);
            if d < cand_to_chosen_d2[c_idx] {
                cand_to_chosen_d2[c_idx] = d;
            }
        }
    }

    if chosen.len() < target_k {
        let mut fallback =
            init_centroids_random(samples, target_k.saturating_sub(chosen.len()), seed);
        chosen.append(&mut fallback);
    }
    chosen.truncate(target_k);
    dedup_centroids(chosen, color_space)
}

fn init_centroids_area_based(
    samples: &[f32],
    target_k: usize,
    threshold: f32,
    color_space: ColorSpace,
    seed: u32,
) -> Vec<[f32; 4]> {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || target_k == 0 {
        return vec![];
    }

    let mut groups: Vec<AreaGroup> = Vec::new();
    let max_samples = 50_000usize;
    let step = (sample_count / max_samples.max(1)).max(1);
    let threshold_sq = threshold.max(0.0001).powi(2);

    let mut sampled = 0usize;
    let mut idx = 0usize;
    while idx < sample_count && sampled < max_samples {
        let sample = sample_at(samples, idx);
        let mut best_group = None;
        let mut best_dist = f32::INFINITY;

        for (group_idx, group) in groups.iter().enumerate() {
            let dist = feature_distance_sq(sample, group.center(color_space), color_space);
            if dist < best_dist {
                best_dist = dist;
                best_group = Some(group_idx);
            }
        }

        if let Some(group_idx) = best_group {
            if best_dist <= threshold_sq {
                groups[group_idx].add(sample, color_space);
            } else {
                groups.push(AreaGroup::new(sample, color_space));
            }
        } else {
            groups.push(AreaGroup::new(sample, color_space));
        }

        sampled += 1;
        idx += step;
    }

    groups.sort_by(|a, b| b.count.cmp(&a.count));
    let mut centroids: Vec<[f32; 4]> = groups
        .iter()
        .take(target_k)
        .map(|group| group.center(color_space))
        .collect();

    if centroids.len() < target_k {
        let mut fallback = init_centroids_random(samples, target_k - centroids.len(), seed);
        centroids.append(&mut fallback);
    }

    centroids
}

fn init_centroids_selected(
    samples: &[f32],
    settings: &RenderSettings,
    target_k: usize,
    seed: u32,
) -> Vec<[f32; 4]> {
    let mut centroids = Vec::with_capacity(target_k);
    for rgb in &settings.selected_colors {
        let feature = encode_feature(*rgb, settings.color_space);
        centroids.push([feature[0], feature[1], feature[2], 1.0]);
        if centroids.len() >= target_k {
            break;
        }
    }

    if centroids.len() < target_k {
        let mut fallback = init_centroids_random(samples, target_k - centroids.len(), seed);
        centroids.append(&mut fallback);
    }

    centroids
}

fn compose_output_from_clusters(
    samples: &[f32],
    result: &ClusterResult,
    settings: &RenderSettings,
) -> Vec<f32> {
    let sample_count = samples.len() / 4;
    if sample_count == 0 || result.centroids.is_empty() || result.labels.is_empty() {
        return vec![0.0; sample_count * 4];
    }

    let palette_rgb: Vec<[f32; 3]> = result
        .centroids
        .iter()
        .map(|c| decode_feature([c[0], c[1], c[2]], settings.color_space))
        .collect();

    let mut output = vec![0.0f32; sample_count * 4];
    for idx in 0..sample_count {
        let label = result.labels[idx].min(result.centroids.len() - 1);
        let rgb = palette_rgb[label];
        let alpha = if settings.rgb_only {
            sample_at(samples, idx)[3]
        } else {
            result.centroids[label][3]
        };

        let base = idx * 4;
        output[base] = sanitize01(rgb[0]);
        output[base + 1] = sanitize01(rgb[1]);
        output[base + 2] = sanitize01(rgb[2]);
        output[base + 3] = sanitize01(alpha);
    }

    output
}

fn write_output_layer(out_layer: &mut Layer, data: &[f32]) -> Result<(), Error> {
    let width = out_layer.width();
    let height = out_layer.height();
    if data.len() < width.saturating_mul(height).saturating_mul(4) {
        return Err(Error::BadCallbackParameter);
    }

    let world_type = out_layer.world_type();
    let progress_final = height as i32;
    out_layer.iterate(0, progress_final, None, |x, y, mut dst| {
        let idx = ((y as usize) * width + x as usize) * 4;
        let out_px = PixelF32 {
            red: sanitize01(data[idx]),
            green: sanitize01(data[idx + 1]),
            blue: sanitize01(data[idx + 2]),
            alpha: sanitize01(data[idx + 3]),
        };

        match world_type {
            ae::aegp::WorldType::U8 => dst.set_from_u8(out_px.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(out_px.to_pixel16()),
            ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => dst.set_from_f32(out_px),
        }

        Ok(())
    })?;

    Ok(())
}

fn encode_input_layer(
    in_layer: &Layer,
    width: usize,
    height: usize,
    color_space: ColorSpace,
) -> Vec<f32> {
    let world_type = in_layer.world_type();
    let mut encoded = vec![0.0f32; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let px = read_pixel_f32(in_layer, world_type, x, y);
            let rgb = [
                sanitize01(px.red),
                sanitize01(px.green),
                sanitize01(px.blue),
            ];
            let alpha = sanitize01(px.alpha);
            let feature = if matches!(color_space, ColorSpace::AlphaOnly) {
                [alpha, 0.0, 0.0]
            } else {
                encode_feature(rgb, color_space)
            };
            let idx = (y * width + x) * 4;
            encoded[idx] = feature[0];
            encoded[idx + 1] = feature[1];
            encoded[idx + 2] = feature[2];
            encoded[idx + 3] = alpha;
        }
    }

    encoded
}
fn encode_feature(rgb: [f32; 3], color_space: ColorSpace) -> [f32; 3] {
    let srgb = Srgb::new(clamp01(rgb[0]), clamp01(rgb[1]), clamp01(rgb[2]));
    let lin: LinSrgb<f32> = srgb.into_linear();

    match color_space {
        ColorSpace::LinearRgb => [clamp01(lin.red), clamp01(lin.green), clamp01(lin.blue)],
        ColorSpace::Oklab => {
            let c: Oklab<f32> = Oklab::from_color(lin);
            [
                encode_signed(c.a, OKLAB_AB_MAX),
                encode_signed(c.b, OKLAB_AB_MAX),
                clamp01(c.l),
            ]
        }
        ColorSpace::Oklch => {
            let c: Oklch<f32> = Oklch::from_color(lin);
            [
                wrap01(c.hue.into_degrees() / 360.0),
                encode_pos(c.chroma, OKLCH_CHROMA_MAX),
                clamp01(c.l),
            ]
        }
        ColorSpace::Hsv => {
            let hsv: Hsv = Hsv::from_color(srgb);
            [
                wrap01(hsv.hue.into_degrees() / 360.0),
                clamp01(hsv.saturation),
                clamp01(hsv.value),
            ]
        }
        ColorSpace::Yiq => {
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            let art = ArtColor::new(
                (srgb.red as f64) * 255.0,
                (srgb.green as f64) * 255.0,
                (srgb.blue as f64) * 255.0,
                1.0,
            );
            let yiq = art.vec_of(ArtColorSpace::YIQ);
            [
                encode_signed(yiq[1] as f32, YIQ_I_MAX),
                encode_signed(yiq[2] as f32, YIQ_Q_MAX),
                clamp01(yiq[0] as f32),
            ]
        }
        ColorSpace::AlphaOnly => {
            let y = clamp01(0.2126 * lin.red + 0.7152 * lin.green + 0.0722 * lin.blue);
            [y, 0.0, 0.0]
        }
    }
}

fn decode_feature(feature: [f32; 3], color_space: ColorSpace) -> [f32; 3] {
    match color_space {
        ColorSpace::LinearRgb => {
            let lin = LinSrgb::new(
                clamp01(feature[0]),
                clamp01(feature[1]),
                clamp01(feature[2]),
            );
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            [
                sanitize01(srgb.red),
                sanitize01(srgb.green),
                sanitize01(srgb.blue),
            ]
        }
        ColorSpace::Oklab => {
            let l = clamp01(feature[2]);
            let a = decode_signed(feature[0], OKLAB_AB_MAX);
            let b = decode_signed(feature[1], OKLAB_AB_MAX);
            let lin: LinSrgb<f32> = LinSrgb::from_color(Oklab::new(l, a, b));
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            [
                sanitize01(srgb.red),
                sanitize01(srgb.green),
                sanitize01(srgb.blue),
            ]
        }
        ColorSpace::Oklch => {
            let l = clamp01(feature[2]);
            let chroma = decode_pos(feature[1], OKLCH_CHROMA_MAX);
            let hue = wrap01(feature[0]) * 360.0;
            let lin: LinSrgb<f32> =
                LinSrgb::from_color(Oklch::new(l, chroma, OklabHue::from_degrees(hue)));
            let srgb: Srgb<f32> = Srgb::from_linear(lin);
            [
                sanitize01(srgb.red),
                sanitize01(srgb.green),
                sanitize01(srgb.blue),
            ]
        }
        ColorSpace::Hsv => {
            let hsv = Hsv::new(
                RgbHue::from_degrees(wrap01(feature[0]) * 360.0),
                clamp01(feature[1]),
                clamp01(feature[2]),
            );
            let srgb: Srgb<f32> = Srgb::from_color(hsv);
            [
                sanitize01(srgb.red),
                sanitize01(srgb.green),
                sanitize01(srgb.blue),
            ]
        }
        ColorSpace::Yiq => {
            let y = clamp01(feature[2]);
            let i = decode_signed(feature[0], YIQ_I_MAX);
            let q = decode_signed(feature[1], YIQ_Q_MAX);
            let spec = format!("yiq({:.6},{:.6},{:.6})", y, i, q);
            if let Ok(color) = ArtColor::from_str(&spec) {
                let rgb = color.vec_of(ArtColorSpace::RGB);
                [
                    sanitize01((rgb[0] / 255.0) as f32),
                    sanitize01((rgb[1] / 255.0) as f32),
                    sanitize01((rgb[2] / 255.0) as f32),
                ]
            } else {
                let r = y + 0.956 * i + 0.619 * q;
                let g = y - 0.272 * i - 0.647 * q;
                let b = y - 1.106 * i + 1.703 * q;
                [sanitize01(r), sanitize01(g), sanitize01(b)]
            }
        }
        ColorSpace::AlphaOnly => {
            let v = clamp01(feature[0]);
            [v, v, v]
        }
    }
}

fn cluster_method_from_popup(value: i32) -> ClusterMethod {
    match value {
        2 => ClusterMethod::XMeans,
        3 => ClusterMethod::GMeans,
        _ => ClusterMethod::KMeans,
    }
}

fn init_method_from_popup(value: i32) -> InitMethod {
    match value {
        2 => InitMethod::Area,
        3 => InitMethod::SelectedColors,
        4 => InitMethod::KMeansParallel,
        _ => InitMethod::Random,
    }
}

fn color_space_from_popup(value: i32) -> ColorSpace {
    match value {
        2 => ColorSpace::Oklab,
        3 => ColorSpace::Oklch,
        4 => ColorSpace::Hsv,
        5 => ColorSpace::Yiq,
        6 => ColorSpace::AlphaOnly,
        _ => ColorSpace::LinearRgb,
    }
}

fn feature_distance_sq(a: [f32; 4], b: [f32; 4], color_space: ColorSpace) -> f32 {
    let dx = feature_delta(a[0], b[0], color_space);
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn feature_delta(a: f32, b: f32, color_space: ColorSpace) -> f32 {
    if color_space.has_circular_hue() {
        circular_delta(a, b)
    } else {
        a - b
    }
}

fn circular_delta(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    if d > 0.5 {
        d -= 1.0;
    } else if d < -0.5 {
        d += 1.0;
    }
    d
}

fn nearest_centroid_idx(
    sample: [f32; 4],
    centroids: &[[f32; 4]],
    color_space: ColorSpace,
) -> (usize, f32) {
    let mut best_idx = 0usize;
    let mut best_dist = f32::INFINITY;
    for (idx, centroid) in centroids.iter().enumerate() {
        let dist = feature_distance_sq(sample, *centroid, color_space);
        if dist < best_dist {
            best_dist = dist;
            best_idx = idx;
        }
    }
    (best_idx, best_dist)
}

fn nearest_two_centroids(
    sample: [f32; 4],
    centroids: &[[f32; 4]],
    color_space: ColorSpace,
) -> (usize, f32, f32) {
    let mut best_idx = 0usize;
    let mut best_dist = f32::INFINITY;
    let mut second_dist = f32::INFINITY;

    for (idx, centroid) in centroids.iter().enumerate() {
        let dist = feature_distance_sq(sample, *centroid, color_space);
        if dist < best_dist {
            second_dist = best_dist;
            best_dist = dist;
            best_idx = idx;
        } else if dist < second_dist {
            second_dist = dist;
        }
    }

    (best_idx, best_dist, second_dist)
}

fn half_min_center_distances(centroids: &[[f32; 4]], color_space: ColorSpace) -> Vec<f32> {
    let k = centroids.len();
    if k == 0 {
        return vec![];
    }
    if k == 1 {
        return vec![f32::INFINITY];
    }

    let mut mins = vec![f32::INFINITY; k];
    for i in 0..k {
        for j in (i + 1)..k {
            let dist = feature_distance_sq(centroids[i], centroids[j], color_space).sqrt() * 0.5;
            if dist < mins[i] {
                mins[i] = dist;
            }
            if dist < mins[j] {
                mins[j] = dist;
            }
        }
    }

    mins
}

fn dedup_centroids(mut centroids: Vec<[f32; 4]>, color_space: ColorSpace) -> Vec<[f32; 4]> {
    let mut unique = Vec::with_capacity(centroids.len());
    for centroid in centroids.drain(..) {
        if !unique
            .iter()
            .any(|c| feature_distance_sq(*c, centroid, color_space) < 1.0e-8)
        {
            unique.push(centroid);
        }
    }
    unique
}

fn nearly_same_feature(a: [f32; 4], b: [f32; 4]) -> bool {
    (a[0] - b[0]).abs() < 1.0e-6
        && (a[1] - b[1]).abs() < 1.0e-6
        && (a[2] - b[2]).abs() < 1.0e-6
        && (a[3] - b[3]).abs() < 1.0e-6
}

fn make_split_seeds(
    samples: &[f32],
    indices: &[usize],
    parent_centroid: [f32; 4],
    color_space: ColorSpace,
) -> ([f32; 4], [f32; 4]) {
    let n = indices.len().max(1) as f32;
    let mut var0 = 0.0f32;
    let mut var1 = 0.0f32;
    let mut var2 = 0.0f32;

    for &idx in indices {
        let sample = sample_at(samples, idx);
        let d0 = feature_delta(sample[0], parent_centroid[0], color_space);
        let d1 = sample[1] - parent_centroid[1];
        let d2 = sample[2] - parent_centroid[2];
        var0 += d0 * d0;
        var1 += d1 * d1;
        var2 += d2 * d2;
    }

    var0 /= n;
    var1 /= n;
    var2 /= n;

    let (axis, sigma) = if var0 >= var1 && var0 >= var2 {
        (0usize, var0.sqrt())
    } else if var1 >= var2 {
        (1usize, var1.sqrt())
    } else {
        (2usize, var2.sqrt())
    };

    let delta = (sigma * 0.5).clamp(0.01, 0.25);
    let mut c1 = parent_centroid;
    let mut c2 = parent_centroid;

    match axis {
        0 if color_space.has_circular_hue() => {
            c1[0] = wrap01(parent_centroid[0] - delta);
            c2[0] = wrap01(parent_centroid[0] + delta);
        }
        0 => {
            c1[0] = clamp01(parent_centroid[0] - delta);
            c2[0] = clamp01(parent_centroid[0] + delta);
        }
        1 => {
            c1[1] = clamp01(parent_centroid[1] - delta);
            c2[1] = clamp01(parent_centroid[1] + delta);
        }
        _ => {
            c1[2] = clamp01(parent_centroid[2] - delta);
            c2[2] = clamp01(parent_centroid[2] + delta);
        }
    }

    (c1, c2)
}

fn collect_subset_samples(samples: &[f32], indices: &[usize]) -> Vec<f32> {
    let mut subset = Vec::with_capacity(indices.len() * 4);
    for &idx in indices {
        let sample = sample_at(samples, idx);
        subset.extend_from_slice(&sample);
    }
    subset
}

fn sample_indices_for_split(indices: &[usize], limit: usize, seed: u32) -> Vec<usize> {
    if indices.len() <= limit {
        return indices.to_vec();
    }

    let mut rng = XorShift64::new(seed as u64 ^ 0xDD22_55AA_7711_CC33);
    let mut out = Vec::with_capacity(limit);
    let mut remaining = indices.len();
    let mut need = limit;

    for &idx in indices {
        if need == 0 {
            break;
        }
        if rng.gen_index(remaining) < need {
            out.push(idx);
            need -= 1;
        }
        remaining -= 1;
    }

    out
}

fn estimate_two_centroid_sse(
    samples: &[f32],
    indices: &[usize],
    c1: [f32; 4],
    c2: [f32; 4],
    color_space: ColorSpace,
) -> f64 {
    let mut sse = 0.0f64;
    for &idx in indices {
        let sample = sample_at(samples, idx);
        let d1 = feature_distance_sq(sample, c1, color_space);
        let d2 = feature_distance_sq(sample, c2, color_space);
        sse += d1.min(d2) as f64;
    }
    sse
}

fn build_cluster_indices(labels: &[usize], cluster_count: usize) -> Vec<Vec<usize>> {
    if cluster_count == 0 {
        return vec![];
    }

    let mut clusters = vec![Vec::new(); cluster_count];
    for (idx, label) in labels.iter().copied().enumerate() {
        if label < cluster_count {
            clusters[label].push(idx);
        }
    }
    clusters
}

fn bic_score(sse: f64, sample_count: usize, cluster_count: usize, dims: usize) -> f64 {
    if sample_count <= cluster_count || cluster_count == 0 || dims == 0 {
        return f64::NEG_INFINITY;
    }

    let n = sample_count as f64;
    let k = cluster_count as f64;
    let d = dims as f64;
    let variance = (sse / ((sample_count - cluster_count) as f64)).max(1.0e-12);
    let params = k * (d + 1.0);
    let log_likelihood = -0.5 * n * d * ((2.0 * std::f64::consts::PI * variance).ln() + 1.0);

    log_likelihood - 0.5 * params * n.ln()
}

fn build_split_projections(
    subset_samples: &[f32],
    parent_centroid: [f32; 4],
    child_a: [f32; 4],
    child_b: [f32; 4],
    color_space: ColorSpace,
) -> Vec<f64> {
    let axis0 = feature_delta(child_b[0], child_a[0], color_space);
    let axis1 = child_b[1] - child_a[1];
    let axis2 = child_b[2] - child_a[2];

    let axis_norm = (axis0 * axis0 + axis1 * axis1 + axis2 * axis2).sqrt();
    if axis_norm <= 1.0e-8 {
        return vec![];
    }

    let inv_norm = 1.0 / axis_norm;
    let ux = axis0 * inv_norm;
    let uy = axis1 * inv_norm;
    let uz = axis2 * inv_norm;

    let sample_count = subset_samples.len() / 4;
    let mut projections = Vec::with_capacity(sample_count);
    for idx in 0..sample_count {
        let sample = sample_at(subset_samples, idx);
        let d0 = feature_delta(sample[0], parent_centroid[0], color_space);
        let d1 = sample[1] - parent_centroid[1];
        let d2 = sample[2] - parent_centroid[2];
        projections.push((d0 * ux + d1 * uy + d2 * uz) as f64);
    }

    projections
}

fn jarque_bera_p_value(samples: &[f64]) -> f64 {
    let n = samples.len();
    if n < 8 {
        return 1.0;
    }

    let n_f = n as f64;
    let mean = samples.iter().sum::<f64>() / n_f;
    let mut m2 = 0.0f64;
    let mut m3 = 0.0f64;
    let mut m4 = 0.0f64;

    for &v in samples {
        let d = v - mean;
        let d2 = d * d;
        m2 += d2;
        m3 += d2 * d;
        m4 += d2 * d2;
    }

    m2 /= n_f;
    m3 /= n_f;
    m4 /= n_f;
    if m2 <= 1.0e-18 {
        return 1.0;
    }

    let skew = m3 / m2.powf(1.5);
    let kurtosis = m4 / (m2 * m2);
    let jb = (n_f / 6.0) * (skew * skew + 0.25 * (kurtosis - 3.0).powi(2));

    (-0.5 * jb.max(0.0)).exp().clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, Default)]
struct ClusterAccum {
    sum0: f32,
    sum1: f32,
    sum2: f32,
    sum3: f32,
    hue_cos: f32,
    hue_sin: f32,
    count: usize,
}

impl ClusterAccum {
    fn accumulate(&mut self, sample: [f32; 4], color_space: ColorSpace) {
        self.sum0 += sample[0];
        self.sum1 += sample[1];
        self.sum2 += sample[2];
        self.sum3 += sample[3];

        if color_space.has_circular_hue() {
            let theta = wrap01(sample[0]) * std::f32::consts::TAU;
            self.hue_cos += theta.cos();
            self.hue_sin += theta.sin();
        }

        self.count += 1;
    }

    fn mean(&self, color_space: ColorSpace) -> [f32; 4] {
        if self.count == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }

        let inv = 1.0 / self.count as f32;
        let c0 = if color_space.has_circular_hue() {
            if self.hue_cos.abs() < 1.0e-8 && self.hue_sin.abs() < 1.0e-8 {
                wrap01(self.sum0 * inv)
            } else {
                wrap01(self.hue_sin.atan2(self.hue_cos) / std::f32::consts::TAU)
            }
        } else {
            clamp01(self.sum0 * inv)
        };

        [
            c0,
            clamp01(self.sum1 * inv),
            clamp01(self.sum2 * inv),
            clamp01(self.sum3 * inv),
        ]
    }
}

#[derive(Clone, Debug)]
struct AreaGroup {
    accum: ClusterAccum,
    count: usize,
}

impl AreaGroup {
    fn new(sample: [f32; 4], color_space: ColorSpace) -> Self {
        let mut group = Self {
            accum: ClusterAccum::default(),
            count: 0,
        };
        group.add(sample, color_space);
        group
    }

    fn add(&mut self, sample: [f32; 4], color_space: ColorSpace) {
        self.accum.accumulate(sample, color_space);
        self.count += 1;
    }

    fn center(&self, color_space: ColorSpace) -> [f32; 4] {
        self.accum.mean(color_space)
    }
}

#[inline]
fn sample_at(samples: &[f32], idx: usize) -> [f32; 4] {
    let base = idx * 4;
    [
        samples[base],
        samples[base + 1],
        samples[base + 2],
        samples[base + 3],
    ]
}

fn weighted_pick_usize(weights: &[usize], rng: &mut XorShift64) -> Option<usize> {
    let total = weights.iter().copied().map(|w| w as u128).sum::<u128>();
    if total == 0 {
        return None;
    }

    let mut r = (rng.next_u64() as u128) % total;
    for (idx, &w) in weights.iter().enumerate() {
        let w = w as u128;
        if r < w {
            return Some(idx);
        }
        r -= w;
    }
    Some(weights.len().saturating_sub(1))
}

#[inline]
fn encode_signed(value: f32, max_abs: f32) -> f32 {
    if max_abs <= 0.0 {
        return 0.5;
    }
    clamp01((value / max_abs + 1.0) * 0.5)
}

#[inline]
fn decode_signed(channel: f32, max_abs: f32) -> f32 {
    if max_abs <= 0.0 {
        return 0.0;
    }
    (clamp01(channel) * 2.0 - 1.0) * max_abs
}

#[inline]
fn encode_pos(value: f32, max: f32) -> f32 {
    if max <= 0.0 {
        return 0.0;
    }
    clamp01(value / max)
}

#[inline]
fn decode_pos(channel: f32, max: f32) -> f32 {
    if max <= 0.0 {
        return 0.0;
    }
    clamp01(channel) * max
}

#[inline]
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[inline]
fn sanitize01(value: f32) -> f32 {
    if value.is_finite() {
        clamp01(value)
    } else {
        0.0
    }
}

#[inline]
fn wrap01(value: f32) -> f32 {
    let mut wrapped = value % 1.0;
    if wrapped < 0.0 {
        wrapped += 1.0;
    }
    wrapped
}

#[cfg(feature = "gpu_wgpu")]
fn compute_sum_scale(pixel_count: u32) -> f32 {
    let n = pixel_count.max(1) as f64;
    let headroom = (u32::MAX as f64) * 0.5;
    (headroom / n).floor().clamp(1.0, 65535.0) as f32
}

fn read_pixel_f32(layer: &Layer, world_type: ae::aegp::WorldType, x: usize, y: usize) -> PixelF32 {
    match world_type {
        ae::aegp::WorldType::U8 => layer.as_pixel8(x, y).to_pixel32(),
        ae::aegp::WorldType::U15 => layer.as_pixel16(x, y).to_pixel32(),
        ae::aegp::WorldType::F32 | ae::aegp::WorldType::None => *layer.as_pixel32(x, y),
    }
}

#[derive(Clone, Copy, Debug)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = (1u64 << 53) as f64;
        ((self.next_u64() >> 11) as f64) / SCALE
    }

    fn gen_index(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % upper
    }
}
