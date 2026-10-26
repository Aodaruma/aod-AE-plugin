#![allow(clippy::drop_non_drop, clippy::question_mark)]
use ae::pf::*;
use after_effects as ae;
use utils::ToPixel;
use utils::image::{SampleEdge, SampleFilter, TRANSPARENT, read_pixel, sanitize_pixel};

mod depth;
mod geometry;
mod source;
mod ui;
use depth::DepthRange;
use geometry::{Camera, Projection, Quat, V3, add, depth_warp, environment_warp, unit};
use source::{Layout, Panorama};

const PLUGIN_DESCRIPTION: &str = "Reframes 360-degree panoramas with camera projections, quaternion rotation, and After Effects camera synchronization.";

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    // Retired layer selector; retain the discriminant for saved parameter IDs.
    #[allow(dead_code)]
    Source,
    InputProjection,
    CubeLayout,
    FlipHorizontal,
    FlipVertical,
    ViewStart,
    Projection,
    Fov,
    OrthoWidth,
    ViewEnd,
    CameraStart,
    SyncCamera,
    SyncPosition,
    RotationMode,
    RotationX,
    RotationY,
    RotationZ,
    QuaternionW,
    QuaternionX,
    QuaternionY,
    QuaternionZ,
    PositionX,
    PositionY,
    PositionZ,
    Radius,
    // Reserve retired discriminants: the host derives persistent parameter IDs from them.
    #[allow(dead_code)]
    CenterX,
    #[allow(dead_code)]
    CenterY,
    #[allow(dead_code)]
    CenterZ,
    CameraEnd,
    SamplingStart,
    Edge,
    Interpolation,
    MitchellB,
    MitchellC,
    LanczosLobes,
    EwaRadius,
    Samples,
    SamplingEnd,
    ShowControls,
    RigSensitivity,
    #[allow(dead_code)]
    PositionMapping,
    DepthStart,
    DepthLayer,
    DepthNear,
    DepthFar,
    DepthInvert,
    DepthEnd,
    SphereScale,
}
const EULER: [Params; 3] = [Params::RotationX, Params::RotationY, Params::RotationZ];
const QUAT: [Params; 4] = [
    Params::QuaternionW,
    Params::QuaternionX,
    Params::QuaternionY,
    Params::QuaternionZ,
];

#[derive(Default)]
struct Plugin {
    aegp_id: Option<ae::aegp::PluginId>,
}
ae::define_effect!(Plugin, (), Params);

#[derive(Clone, Copy)]
struct Settings {
    camera: Camera,
    layout: Layout,
    equiangular: bool,
    edge: Option<SampleEdge>,
    filter: SampleFilter,
    samples: usize,
    flip: [bool; 2],
    size: [f64; 2],
    source_size: [usize; 2],
    depth_size: [usize; 2],
    depth_range: DepthRange,
}
fn popup(p: &Parameters<Params>, id: Params) -> Result<i32, Error> {
    Ok(p.get(id)?.as_popup()?.value())
}
fn value(p: &Parameters<Params>, id: Params) -> Result<f64, Error> {
    Ok(p.get(id)?.as_float_slider()?.value())
}
fn checked(p: &Parameters<Params>, id: Params) -> Result<bool, Error> {
    Ok(p.get(id)?.as_checkbox()?.value())
}
fn add_popup(
    p: &mut Parameters<Params>,
    id: Params,
    name: &str,
    options: &[&str],
    default: i32,
) -> Result<(), Error> {
    p.add_with_flags(
        id,
        name,
        PopupDef::setup(|d| {
            d.set_options(options);
            d.set_default(default);
        }),
        ParamFlag::SUPERVISE,
        ParamUIFlags::empty(),
    )
}
fn slider(
    p: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: f64,
    max: f64,
    default: f64,
) -> Result<(), Error> {
    p.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min as f32);
            d.set_valid_max(max as f32);
            d.set_slider_min(min.max(-10000.0) as f32);
            d.set_slider_max(max.min(20000.0) as f32);
            d.set_default(default);
            d.set_precision(3);
        }),
    )
}
fn checkbox(
    p: &mut Parameters<Params>,
    id: Params,
    name: &str,
    default: bool,
) -> Result<(), Error> {
    p.add_with_flags(
        id,
        name,
        CheckBoxDef::setup(|d| {
            d.set_default(default);
        }),
        ParamFlag::SUPERVISE,
        ParamUIFlags::empty(),
    )
}
fn rotation(p: &Parameters<Params>) -> Result<Quat, Error> {
    if popup(p, Params::RotationMode)? == 2 {
        Ok(Quat([
            value(p, QUAT[0])?,
            value(p, QUAT[1])?,
            value(p, QUAT[2])?,
            value(p, QUAT[3])?,
        ])
        .normalized())
    } else {
        let mut a = [0.0; 3];
        for i in 0..3 {
            a[i] = (p.get(EULER[i])?.as_angle()?.value() as f64).to_radians();
        }
        Ok(Quat::from_euler(a))
    }
}
fn write_rotation(p: &mut Parameters<Params>, q: Quat) -> Result<(), Error> {
    if popup(p, Params::RotationMode)? == 2 {
        for (id, v) in QUAT.into_iter().zip(q.normalized().0) {
            set_float(p, id, v)?;
        }
    } else {
        for (id, v) in EULER.into_iter().zip(q.euler()) {
            let mut param = p.get_mut(id)?;
            param.as_angle_mut()?.set_value(v.to_degrees() as f32);
            param.set_value_changed();
        }
    }
    Ok(())
}
fn add_rotation(p: &mut Parameters<Params>, delta: V3) -> Result<(), Error> {
    if popup(p, Params::RotationMode)? == 2 {
        write_rotation(p, Quat::from_rotation_vector(delta).mul(rotation(p)?))
    } else {
        // Keep Euler revolution counts instead of canonicalizing through a quaternion.
        for (id, angle) in EULER.into_iter().zip(delta) {
            if angle != 0.0 {
                let mut param = p.get_mut(id)?;
                let old = param.as_angle()?.value();
                param
                    .as_angle_mut()?
                    .set_value(old + angle.to_degrees() as f32);
                param.set_value_changed();
            }
        }
        Ok(())
    }
}
fn set_float(p: &mut Parameters<Params>, id: Params, v: f64) -> Result<(), Error> {
    let mut d = p.get_mut(id)?;
    d.as_float_slider_mut()?.set_value(v);
    d.set_value_changed();
    Ok(())
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        p: &mut Parameters<Params>,
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        add_popup(
            p,
            Params::InputProjection,
            "Input Projection",
            &[
                "Equirectangular (360 x 180)",
                "Cube Map",
                "Equiangular Cube Map",
            ],
            1,
        )?;
        add_popup(
            p,
            Params::CubeLayout,
            "Cube Layout",
            &[
                "Horizontal Strip (6 x 1)",
                "Vertical Strip (1 x 6)",
                "Grid (3 x 2)",
                "Horizontal Cross (4 x 3)",
            ],
            1,
        )?;
        checkbox(p, Params::FlipHorizontal, "Flip Source Horizontal", false)?;
        checkbox(p, Params::FlipVertical, "Flip Source Vertical", false)?;
        p.add_group(
            Params::ViewStart,
            Params::ViewEnd,
            "Projection",
            false,
            |p| {
                add_popup(
                    p,
                    Params::Projection,
                    "Output Projection",
                    &[
                        "Perspective",
                        "Fisheye (Equidistant)",
                        "Orthographic",
                        "Isometric",
                    ],
                    1,
                )?;
                slider(p, Params::Fov, "Field of View", 1.0, 360.0, 90.0)?;
                slider(
                    p,
                    Params::SphereScale,
                    "Sphere Scale (%)",
                    1.0,
                    1000.0,
                    100.0,
                )?;
                slider(
                    p,
                    Params::OrthoWidth,
                    "Orthographic Width",
                    1.0,
                    1000000.0,
                    10000.0,
                )
            },
        )?;
        p.add_group(
            Params::CameraStart,
            Params::CameraEnd,
            "Camera",
            false,
            |p| {
                checkbox(p, Params::SyncCamera, "Sync AE Camera", false)?;
                checkbox(p, Params::SyncPosition, "Sync AE Position", false)?;
                add_popup(
                    p,
                    Params::RotationMode,
                    "Rotation Mode",
                    &["Euler XYZ", "Quaternion WXYZ"],
                    1,
                )?;
                for (id, name) in EULER
                    .into_iter()
                    .zip(["Rotation X", "Rotation Y", "Rotation Z"])
                {
                    p.add(
                        id,
                        name,
                        AngleDef::setup(|d| {
                            d.set_default(0.0);
                        }),
                    )?;
                }
                for (i, id) in QUAT.into_iter().enumerate() {
                    slider(
                        p,
                        id,
                        [
                            "Quaternion W",
                            "Quaternion X",
                            "Quaternion Y",
                            "Quaternion Z",
                        ][i],
                        -1.0,
                        1.0,
                        if i == 0 { 1.0 } else { 0.0 },
                    )?;
                }
                for (id, name) in [
                    (Params::PositionX, "Position X"),
                    (Params::PositionY, "Position Y"),
                    (Params::PositionZ, "Position Z"),
                ] {
                    slider(p, id, name, -1000000.0, 1000000.0, 0.0)?;
                }
                slider(
                    p,
                    Params::Radius,
                    "Parallax Distance",
                    1.0,
                    1000000.0,
                    10000.0,
                )?;
                Ok(())
            },
        )?;
        p.add_group(
            Params::DepthStart,
            Params::DepthEnd,
            "Depth Map",
            false,
            |p| {
                p.add(Params::DepthLayer, "Depth Map Layer", LayerDef::new())?;
                slider(
                    p,
                    Params::DepthNear,
                    "Depth Near Distance",
                    1.0,
                    1000000.0,
                    1000.0,
                )?;
                slider(
                    p,
                    Params::DepthFar,
                    "Depth Far Distance",
                    1.0,
                    1000000.0,
                    10000.0,
                )?;
                checkbox(p, Params::DepthInvert, "Invert Depth", false)
            },
        )?;
        p.add_group(
            Params::SamplingStart,
            Params::SamplingEnd,
            "Sampling",
            true,
            |p| {
                add_popup(
                    p,
                    Params::Edge,
                    "Edges",
                    &[
                        "Seamless Panorama",
                        "None (Transparent)",
                        "Extend",
                        "Repeat",
                        "Mirror",
                    ],
                    1,
                )?;
                add_popup(
                    p,
                    Params::Interpolation,
                    "Interpolation",
                    &[
                        "Nearest",
                        "Bilinear",
                        "Bicubic",
                        "Mitchell-Netravali",
                        "Lanczos",
                        "Cubic B-Spline",
                        "EWA Quadratic",
                    ],
                    2,
                )?;
                slider(p, Params::MitchellB, "Mitchell B", 0.0, 1.0, 1.0 / 3.0)?;
                slider(p, Params::MitchellC, "Mitchell C", 0.0, 1.0, 1.0 / 3.0)?;
                slider(p, Params::LanczosLobes, "Lanczos Lobes", 1.0, 8.0, 3.0)?;
                slider(p, Params::EwaRadius, "EWA Radius", 0.5, 8.0, 1.5)?;
                add_popup(
                    p,
                    Params::Samples,
                    "Antialiasing",
                    &["1 sample", "4 samples", "16 samples"],
                    1,
                )
            },
        )?;
        checkbox(p, Params::ShowControls, "Viewport Controls", true)?;
        slider(
            p,
            Params::RigSensitivity,
            "Rotation Drag Sensitivity",
            0.1,
            4.0,
            1.0,
        )?;
        if !in_data.is_premiere() {
            in_data.interact().register_ui(
                CustomUIInfo::new()
                    .events(ae::CustomEventFlags::LAYER | ae::CustomEventFlags::COMP),
            )?;
        }
        Ok(())
    }
    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out: OutData,
        p: &mut Parameters<Params>,
    ) -> Result<(), Error> {
        match cmd {
            ae::Command::About => out.set_return_msg(&format!(
                "AOD_PanoramaReframe {}\r\r{}",
                env!("CARGO_PKG_VERSION"),
                PLUGIN_DESCRIPTION
            )),
            ae::Command::GlobalSetup => {
                out.set_out_flag(OutFlags::SendUpdateParamsUi, true);
                out.set_out_flag(OutFlags::CustomUi, true);
                out.set_out_flag2(OutFlags2::SupportsSmartRender, true);
                out.set_out_flag2(OutFlags2::IUse3DCamera, true);
                out.set_out_flag2(OutFlags2::RevealsZeroAlpha, true);
                out.set_out_flag2(OutFlags2::ParamGroupStartCollapsedFlag, true);
                if let Ok(s) = ae::aegp::suites::Utility::new() {
                    self.aegp_id = s.register_with_aegp("AOD_PanoramaReframe").ok();
                }
            }
            ae::Command::SmartPreRender { mut extra } => {
                let mut s = read_settings(p, in_data)?;
                let mut req = extra.output_request();
                req.preserve_rgb_of_zero_alpha = 1;
                let cb = extra.callbacks();
                // Parameter zero is the effect input, including preceding effects.
                let depth_index = p.index(Params::DepthLayer).ok_or(Error::InvalidIndex)? as i32;
                for (index, id, size) in [
                    (0, 0, &mut s.source_size),
                    (depth_index, 1, &mut s.depth_size),
                ] {
                    let query = cb.checkout_layer(
                        index,
                        1000 + id,
                        &req,
                        in_data.current_time(),
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                    *size = [
                        (query.ref_width as f64 * f64::from(f32::from(in_data.downsample_x())))
                            .ceil()
                            .max(0.0) as usize,
                        (query.ref_height as f64 * f64::from(f32::from(in_data.downsample_y())))
                            .ceil()
                            .max(0.0) as usize,
                    ];
                    let mut full_request = req;
                    full_request.rect = ae::sys::PF_LRect {
                        left: 0,
                        top: 0,
                        right: size[0] as i32,
                        bottom: size[1] as i32,
                    };
                    cb.checkout_layer(
                        index,
                        id,
                        &full_request,
                        in_data.current_time(),
                        in_data.time_step(),
                        in_data.time_scale(),
                    )?;
                }
                let bounds = ae::Rect {
                    left: 0,
                    top: 0,
                    right: s.size[0].ceil() as i32,
                    bottom: s.size[1].ceil() as i32,
                };
                extra.set_result_rect(bounds);
                extra.set_max_result_rect(bounds);
                // The full layer can extend beyond the host's cropped output request.
                extra.set_returns_extra_pixels(true);
                extra.set_pre_render_data(s);
            }
            ae::Command::SmartRender { extra } => {
                let s = *extra
                    .pre_render_data::<Settings>()
                    .ok_or(Error::InternalStructDamaged)?;
                let cb = extra.callbacks();
                let source = cb.checkout_layer_pixels(0)?;
                let result = (|| {
                    let depth = cb.checkout_layer_pixels(1)?;
                    let result = (|| {
                        if let Some(output) = cb.checkout_output()? {
                            render(in_data, source.as_ref(), depth.as_ref(), output, s, true)?;
                        }
                        Ok::<(), Error>(())
                    })();
                    let checkin = cb.checkin_layer_pixels(1);
                    result.and(checkin)
                })();
                let checkin = cb.checkin_layer_pixels(0);
                result?;
                checkin?;
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                let mut s = read_settings(p, in_data)?;
                s.source_size = [in_layer.width(), in_layer.height()];
                let depth_checkout = p.checkout_at(Params::DepthLayer, None, None, None)?;
                let depth = depth_checkout.as_layer()?.value();
                if let Some(ref layer) = depth {
                    s.depth_size = [layer.width(), layer.height()];
                }
                render(
                    in_data,
                    Some(&in_layer),
                    depth.as_ref(),
                    out_layer,
                    s,
                    false,
                )?;
            }
            ae::Command::UpdateParamsUi => {
                self.update_ui(in_data, &mut p.cloned())?;
            }
            ae::Command::UserChangedParam { param_index } => {
                if p.type_at(param_index) == Params::RotationMode {
                    // Preserve the current orientation when changing representations.
                    let q = if popup(p, Params::RotationMode)? == 2 {
                        let mut a = [0.0; 3];
                        for i in 0..3 {
                            a[i] = (p.get(EULER[i])?.as_angle()?.value() as f64).to_radians();
                        }
                        Quat::from_euler(a)
                    } else {
                        Quat([
                            value(p, QUAT[0])?,
                            value(p, QUAT[1])?,
                            value(p, QUAT[2])?,
                            value(p, QUAT[3])?,
                        ])
                        .normalized()
                    };
                    write_rotation(p, q)?;
                    // Refresh the composition overlay as well as the ECW on a mode switch.
                    out.set_out_flag(OutFlags::ForceRerender, true);
                }
                out.set_out_flag(OutFlags::RefreshUi, true);
            }
            ae::Command::Event { mut extra } => ui::event(in_data, p, &mut extra)?,
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn visible(
        &self,
        in_data: InData,
        p: &mut Parameters<Params>,
        id: Params,
        visible: bool,
    ) -> Result<(), Error> {
        if !in_data.is_premiere()
            && let Some(plugin_id) = self.aegp_id
        {
            let effect = in_data.effect().aegp_effect(plugin_id)?;
            let stream = effect
                .new_stream_by_index(plugin_id, p.index(id).ok_or(Error::InvalidIndex)? as i32)?;
            stream.set_dynamic_stream_flag(
                ae::aegp::DynamicStreamFlags::Hidden,
                false,
                !visible,
            )?;
        } else {
            set_ui_flag(p, id, ParamUIFlags::INVISIBLE, !visible)?;
        }
        Ok(())
    }
    fn update_ui(&self, in_data: InData, p: &mut Parameters<Params>) -> Result<(), Error> {
        let quaternion = popup(p, Params::RotationMode)? == 2;
        for id in EULER {
            self.visible(in_data, p, id, !quaternion)?;
        }
        for id in QUAT {
            self.visible(in_data, p, id, quaternion)?;
        }
        self.visible(
            in_data,
            p,
            Params::CubeLayout,
            popup(p, Params::InputProjection)? != 1,
        )?;
        let proj = popup(p, Params::Projection)?;
        self.visible(in_data, p, Params::OrthoWidth, proj >= 3)?;
        self.visible(in_data, p, Params::Fov, proj < 3)?;
        self.visible(in_data, p, Params::SphereScale, proj != 1)?;
        let sync = checked(p, Params::SyncCamera)? && !in_data.is_premiere();
        set_ui_flag(
            p,
            Params::SyncCamera,
            ParamUIFlags::DISABLED,
            in_data.is_premiere(),
        )?;
        set_ui_flag(p, Params::SyncPosition, ParamUIFlags::DISABLED, !sync)?;
        set_ui_flag(p, Params::Fov, ParamUIFlags::DISABLED, sync && proj == 1)?;
        let filter = popup(p, Params::Interpolation)?;
        for (id, show) in [
            (Params::MitchellB, filter == 4),
            (Params::MitchellC, filter == 4),
            (Params::LanczosLobes, filter == 5),
            (Params::EwaRadius, filter == 7),
        ] {
            self.visible(in_data, p, id, show)?;
        }
        Ok(())
    }
}
fn set_ui_flag(
    p: &mut Parameters<Params>,
    id: Params,
    flag: ParamUIFlags,
    on: bool,
) -> Result<(), Error> {
    if ((p.get(id)?.ui_flags().bits() & flag.bits()) != 0) != on {
        let mut d = p.get_mut(id)?;
        d.set_ui_flag(flag, on);
        d.update_param_ui()?;
    }
    Ok(())
}

fn read_settings(p: &Parameters<Params>, in_data: InData) -> Result<Settings, Error> {
    let input = popup(p, Params::InputProjection)?;
    let layout = if input == 1 {
        Layout::LatLong
    } else {
        match popup(p, Params::CubeLayout)? {
            2 => Layout::StripVertical,
            3 => Layout::Grid,
            4 => Layout::Cross,
            _ => Layout::StripHorizontal,
        }
    };
    let ds = [
        f64::from(f32::from(in_data.downsample_x())),
        f64::from(f32::from(in_data.downsample_y())),
    ];
    let size = [
        in_data.width() as f64 * ds[0],
        in_data.height() as f64 * ds[1],
    ];
    let aspect = in_data.width() as f64 * f64::from(f32::from(in_data.pixel_aspect_ratio()))
        / in_data.height().max(1) as f64;
    let mut camera = Camera {
        projection: match popup(p, Params::Projection)? {
            2 => Projection::Fisheye,
            3 => Projection::Orthographic,
            4 => Projection::Isometric,
            _ => Projection::Perspective,
        },
        fov: value(p, Params::Fov)?.to_radians(),
        ortho_width: value(p, Params::OrthoWidth)?,
        sphere_scale: value(p, Params::SphereScale)? / 100.0,
        radius: value(p, Params::Radius)?,
        rotation: rotation(p)?,
        position: [
            value(p, Params::PositionX)?,
            value(p, Params::PositionY)?,
            value(p, Params::PositionZ)?,
        ],
        aspect,
        ..Camera::default()
    };
    if checked(p, Params::SyncCamera)? && !in_data.is_premiere() {
        let effect = in_data.effect();
        let time = effect.comp_time(in_data.current_time(), in_data.time_scale())?;
        if let Some(layer) = effect.camera(time)? {
            let matrix = layer.to_world_xform(time)?;
            let m = matrix.as_slice();
            camera.basis = [
                unit([m[0], m[1], m[2]]),
                unit([m[4], m[5], m[6]]),
                unit([m[8], m[9], m[10]]),
            ];
            let comp = effect.layer()?.parent_comp()?;
            let (w, h) = comp.item()?.dimensions()?;
            if camera.projection == Projection::Perspective {
                let zoom: f64 = layer
                    .layer_stream_value(
                        ae::aegp::LayerStream::Zoom,
                        ae::aegp::TimeMode::CompTime,
                        time,
                        false,
                    )?
                    .try_into()
                    .map_err(|_| Error::BadCallbackParameter)?;
                // AE zoom is in composition pixels; vertical FOV is invariant under PAR.
                camera.fov = 2.0 * ((h as f64 * 0.5 / zoom.max(0.01)) * aspect).atan();
            }
            if checked(p, Params::SyncPosition)? {
                camera.position = add(
                    camera.position,
                    [m[12] - w as f64 * 0.5, m[13] - h as f64 * 0.5, m[14]],
                );
            }
        }
    }
    Ok(Settings {
        camera,
        layout,
        equiangular: input == 3,
        edge: match popup(p, Params::Edge)? {
            2 => Some(SampleEdge::Transparent),
            3 => Some(SampleEdge::Clamp),
            4 => Some(SampleEdge::Tile),
            5 => Some(SampleEdge::Mirror),
            _ => None,
        },
        filter: match popup(p, Params::Interpolation)? {
            1 => SampleFilter::Nearest,
            3 => SampleFilter::Bicubic,
            4 => SampleFilter::Mitchell {
                b: value(p, Params::MitchellB)? as f32,
                c: value(p, Params::MitchellC)? as f32,
            },
            5 => SampleFilter::Lanczos {
                lobes: value(p, Params::LanczosLobes)? as f32,
            },
            6 => SampleFilter::CubicBSpline,
            7 => SampleFilter::EwaQuadratic {
                radius: value(p, Params::EwaRadius)? as f32,
            },
            _ => SampleFilter::Bilinear,
        },
        samples: match popup(p, Params::Samples)? {
            2 => 2,
            3 => 4,
            _ => 1,
        },
        flip: [
            checked(p, Params::FlipHorizontal)?,
            checked(p, Params::FlipVertical)?,
        ],
        size,
        source_size: [0, 0],
        depth_size: [0, 0],
        depth_range: DepthRange {
            near: value(p, Params::DepthNear)?,
            far: value(p, Params::DepthFar)?,
            invert: checked(p, Params::DepthInvert)?,
        },
    })
}

fn load_panorama(
    in_data: InData,
    source: Option<&Layer>,
    size: [usize; 2],
    filter: SampleFilter,
    s: Settings,
    smart: bool,
) -> Result<Option<Panorama>, Error> {
    let panorama = if let Some(src) = source {
        let [w, h] = size;
        if w == 0 || h == 0 {
            None
        } else {
            let mut pixels = Vec::new();
            pixels
                .try_reserve_exact(w.checked_mul(h).ok_or(Error::OutOfMemory)?)
                .map_err(|_| Error::OutOfMemory)?;
            pixels.resize(w * h, TRANSPARENT);
            let origin = if smart {
                src.origin()
            } else {
                ae::Point { h: 0, v: 0 }
            };
            for y in 0..src.height() {
                if y % 64 == 0 {
                    in_data.interact().abort()?;
                }
                for x in 0..src.width() {
                    let tx = x as i32 + origin.h;
                    let ty = y as i32 + origin.v;
                    if tx >= 0 && ty >= 0 && tx < w as i32 && ty < h as i32 {
                        let dx = if s.flip[0] {
                            w - 1 - tx as usize
                        } else {
                            tx as usize
                        };
                        let dy = if s.flip[1] {
                            h - 1 - ty as usize
                        } else {
                            ty as usize
                        };
                        pixels[dy * w + dx] = read_pixel(src, x, y);
                    }
                }
            }
            Some(Panorama::new(
                &pixels,
                w,
                h,
                s.layout,
                s.equiangular,
                s.edge,
                filter,
            )?)
        }
    } else {
        None
    };
    Ok(panorama)
}

fn render(
    in_data: InData,
    source: Option<&Layer>,
    depth: Option<&Layer>,
    mut output: Layer,
    s: Settings,
    smart: bool,
) -> Result<(), Error> {
    let panorama = load_panorama(in_data, source, s.source_size, s.filter, s, smart)?;
    let depth = load_panorama(
        in_data,
        depth,
        s.depth_size,
        SampleFilter::Bilinear,
        s,
        smart,
    )?;
    let origin = if smart {
        output.origin()
    } else {
        let o = in_data.output_origin();
        ae::Point { h: -o.h, v: -o.v }
    };
    let world = output.world_type();
    let h = output.height();
    output.iterate(0, h as i32, None, |x, y, mut dst| {
        let mut pixel = TRANSPARENT;
        if let Some(ref pano) = panorama {
            let inv = 1.0 / (s.samples * s.samples) as f32;
            for sy in 0..s.samples {
                for sx in 0..s.samples {
                    let px = x as f64 + origin.h as f64 + (sx as f64 + 0.5) / s.samples as f64;
                    let py = y as f64 + origin.v as f64 + (sy as f64 + 0.5) / s.samples as f64;
                    if let Some(d) = s.camera.direction(
                        2.0 * px / s.size[0] - 1.0,
                        (2.0 * py / s.size[1] - 1.0) / s.camera.aspect,
                    ) {
                        let d = if let Some(ref map) = depth {
                            depth_warp(d, s.camera.position, |direction| {
                                s.depth_range
                                    .distance(map.sample(direction), s.camera.radius)
                            })
                        } else {
                            environment_warp(d, s.camera.position, s.camera.radius)
                        };
                        let p = pano.sample(d);
                        pixel.alpha += p.alpha * inv;
                        pixel.red += p.red * inv;
                        pixel.green += p.green * inv;
                        pixel.blue += p.blue * inv;
                    }
                }
            }
        }
        let p = sanitize_pixel(pixel, false);
        match world {
            ae::aegp::WorldType::U8 => dst.set_from_u8(p.to_pixel8()),
            ae::aegp::WorldType::U15 => dst.set_from_u16(p.to_pixel16()),
            _ => dst.set_from_f32(p),
        }
        Ok(())
    })
}
