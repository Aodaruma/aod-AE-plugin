//! Shared AE adapter. SmartFX checks out complete inputs, then crops to the requested output.
use super::{
    Image, Settings,
    field::{self, Segment},
};
use crate::ToPixel;
use ae::Error;
use ae::pf::*;
use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    Mode,
    Length,
    Direction,
    Rotation,
    Center,
    Shutter,
    Bias,
    Curve,
    Path,
    Falloff,
    Map,
    Channel,
    Invert,
    Samples,
    Transfer,
    Boost,
    Threshold,
    Interpolation,
    NoiseScale,
    Evolution,
    Edge,
    Mix,
    Backend,
    ExposureCurve,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Shutter,
    Fake,
}
fn popup(
    p: &mut Parameters<Params>,
    id: Params,
    name: &str,
    options: &[&str],
    default: i32,
) -> Result<(), Error> {
    p.add(
        id,
        name,
        PopupDef::setup(|d| {
            d.set_options(options);
            d.set_default(default);
        }),
    )
}
fn slider(
    p: &mut Parameters<Params>,
    id: Params,
    name: &str,
    min: f32,
    max: f32,
    default: f64,
    range: f32,
) -> Result<(), Error> {
    p.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(min);
            d.set_slider_max(range);
            d.set_default(default);
            d.set_precision(2);
        }),
    )
}
pub fn setup(p: &mut Parameters<Params>, kind: Kind) -> Result<(), Error> {
    let options: &[&str] = if kind == Kind::Shutter {
        &[
            "Directional",
            "Radial Spin",
            "Radial Zoom",
            "Motion (Automatic)",
            "Motion (RG Vectors)",
            "Along Mask Path",
        ]
    } else {
        &["Mipmap", "Displacement", "Downsample / Reconstruct"]
    };
    p.add_with_flags(
        Params::Mode,
        "Blur Mode",
        PopupDef::setup(|d| {
            d.set_options(options);
            d.set_default(1);
        }),
        ae::ParamFlag::SUPERVISE,
        ae::ParamUIFlags::empty(),
    )?;
    slider(
        p,
        Params::Length,
        if kind == Kind::Shutter {
            "Length / Zoom / Max Motion"
        } else {
            "Blur Radius"
        },
        0.,
        4096.,
        32.,
        256.,
    )?;
    if kind == Kind::Shutter {
        slider(
            p,
            Params::Direction,
            "Direction (degrees)",
            -3600.,
            3600.,
            0.,
            360.,
        )?;
        slider(
            p,
            Params::Rotation,
            "Spin Angle (degrees)",
            -360.,
            360.,
            45.,
            180.,
        )?;
        p.add(
            Params::Center,
            "Radial Center",
            PointDef::setup(|d| {
                d.set_default((50., 50.));
            }),
        )?;
        slider(
            p,
            Params::Shutter,
            "Motion Shutter (degrees)",
            0.,
            720.,
            180.,
            360.,
        )?;
        slider(p, Params::Bias, "Shutter Bias", -0.5, 0.5, 0., 0.5)?;
        p.add_with_flags(
            Params::Curve,
            "Shutter Profile",
            PopupDef::setup(|d| {
                d.set_options(&["Uniform", "Triangle", "Cosine", "Custom Curve"]);
                d.set_default(4);
            }),
            ae::ParamFlag::SUPERVISE,
            ae::ParamUIFlags::empty(),
        )?;
        let mut curve = ArbitraryDef::new();
        curve.set_default(super::curve::ShutterCurve::default())?;
        p.add_customized(Params::ExposureCurve, "Shutter Curve", curve, |param| {
            param.set_ui_flags(ae::ParamUIFlags::CONTROL | ae::ParamUIFlags::DO_NOT_ERASE_CONTROL);
            param.set_ui_width(330);
            param.set_ui_height(230);
            -1
        })?;
        p.add(Params::Path, "Mask Path", PathDef::new())?;
        slider(
            p,
            Params::Falloff,
            "Path Influence (0 = Global)",
            0.,
            10000.,
            0.,
            1000.,
        )?;
    }
    p.add(
        Params::Map,
        if kind == Kind::Shutter {
            "Motion Vector Map"
        } else {
            "Blur Map"
        },
        LayerDef::new(),
    )?;
    if kind == Kind::Fake {
        popup(
            p,
            Params::Channel,
            "Map Channel",
            &["Luminance", "Red", "Green", "Blue", "Alpha"],
            1,
        )?;
        p.add(
            Params::Invert,
            "Invert Map",
            CheckBoxDef::setup(|d| {
                d.set_default(false);
            }),
        )?;
    }
    p.add(
        Params::Samples,
        "Samples",
        SliderDef::setup(|d| {
            d.set_valid_min(2);
            d.set_valid_max(512);
            d.set_slider_min(2);
            d.set_slider_max(128);
            d.set_default(64);
        }),
    )?;
    if kind == Kind::Shutter {
        popup(
            p,
            Params::Transfer,
            "Input Transfer",
            &["Linear / HDR", "sRGB", "Gamma 2.2"],
            1,
        )?;
        slider(p, Params::Boost, "Highlight Boost", 0., 32., 0., 8.)?;
        slider(
            p,
            Params::Threshold,
            "Highlight Threshold",
            0.,
            100.,
            1.,
            4.,
        )?;
    } else {
        // Preserve the old parameter's ID, type and position for existing projects.
        // Static INVISIBLE hides the obsolete control in both the ECW and timeline.
        p.add_with_flags(
            Params::NoiseScale,
            "Turbulence Scale",
            FloatSliderDef::setup(|d| {
                d.set_valid_min(1.);
                d.set_valid_max(4096.);
                d.set_slider_min(1.);
                d.set_slider_max(512.);
                d.set_default(64.);
                d.set_precision(2);
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::INVISIBLE,
        )?;
        slider(
            p,
            Params::Evolution,
            "Evolution",
            -100000.,
            100000.,
            0.,
            360.,
        )?;
    }
    popup(
        p,
        Params::Interpolation,
        "Interpolation",
        &["Nearest", "Bilinear", "Cubic B-Spline"],
        2,
    )?;
    popup(p, Params::Edge, "Edges", &["Transparent", "Clamp"], 1)?;
    slider(p, Params::Mix, "Mix (%)", 0., 100., 100., 100.)?;
    popup(
        p,
        Params::Backend,
        "Processing",
        &["Auto", "CPU", "GPU (Required)"],
        1,
    )?;
    Ok(())
}
fn number(p: &Parameters<Params>, id: Params) -> Result<f32, Error> {
    Ok(p.get(id)?.as_float_slider()?.value() as f32)
}
fn choice(p: &Parameters<Params>, id: Params) -> Result<u32, Error> {
    Ok(p.get(id)?.as_popup()?.value().max(1) as u32 - 1)
}
fn mode(p: &Parameters<Params>, kind: Kind) -> Result<u32, Error> {
    let choice = choice(p, Params::Mode)?;
    Ok(if kind == Kind::Fake {
        match choice {
            0 => 18,
            1 => 17,
            _ => 19,
        }
    } else {
        choice
    })
}
fn scale(data: InData) -> [f32; 2] {
    let x = data.downsample_x();
    let y = data.downsample_y();
    [
        x.num as f32 / x.den.max(1) as f32,
        y.num as f32 / y.den.max(1) as f32,
    ]
}
fn settings(p: &Parameters<Params>, data: InData, kind: Kind) -> Result<Settings, Error> {
    let mut s = Settings {
        mode: mode(p, kind)?,
        length: number(p, Params::Length)?,
        scale: scale(data),
        samples: p.get(Params::Samples)?.as_slider()?.value().clamp(2, 512) as u32,
        interpolation: choice(p, Params::Interpolation)?,
        edge: choice(p, Params::Edge)?,
        mix: number(p, Params::Mix)? * 0.01,
        backend: choice(p, Params::Backend)?,
        ..Default::default()
    };
    if kind == Kind::Shutter {
        s.angle = number(
            p,
            if s.mode == 1 {
                Params::Rotation
            } else {
                Params::Direction
            },
        )?
        .to_radians();
        if s.mode == 1 {
            s.length = 1.;
        }
        let point = p.get(Params::Center)?.as_point()?.value();
        s.center = [point.0, point.1];
        s.shutter = number(p, Params::Shutter)? / 360.;
        s.bias = number(p, Params::Bias)?;
        s.curve = choice(p, Params::Curve)?;
        if s.curve == 3 {
            s.curve_table = p
                .get(Params::ExposureCurve)?
                .as_arbitrary()?
                .value::<super::curve::ShutterCurve>()?
                .table();
        }
        s.transfer = choice(p, Params::Transfer)?;
        s.boost = number(p, Params::Boost)?;
        s.threshold = number(p, Params::Threshold)?;
    } else {
        s.evolution = number(p, Params::Evolution)?;
    }
    Ok(s)
}
struct Plan {
    settings: Settings,
    segments: Vec<Segment>,
    falloff: f32,
    map: bool,
    channel: u32,
    invert: bool,
    references: [[usize; 2]; 3],
}
fn plan(p: &Parameters<Params>, data: InData, kind: Kind) -> Result<Plan, Error> {
    let s = settings(p, data, kind)?;
    let mut result = Plan {
        map: s.mode == 4 || kind == Kind::Fake,
        settings: s,
        segments: Vec::new(),
        falloff: 0.,
        channel: 0,
        invert: false,
        references: [[0; 2]; 3],
    };
    if kind == Kind::Fake {
        result.channel = choice(p, Params::Channel)?;
        result.invert = p.get(Params::Invert)?.as_checkbox()?.value();
    }
    if result.settings.mode == 5 {
        result.falloff =
            number(p, Params::Falloff)? * result.settings.scale[0].min(result.settings.scale[1]);
        let id = p.get(Params::Path)?.as_path()?.path_id();
        if id != 0
            && let Some(path) = data.effect().checkout_path(
                id,
                data.current_time(),
                data.time_step(),
                data.time_scale(),
            )?
        {
            let ds = result.settings.scale;
            let origin = data.pre_effect_source_origin();
            for index in 0..path.num_segments()? {
                data.interact().abort()?;
                let mut segment = path.prepare_seg_length(index, 100)?;
                let len = segment.length()?;
                let count = (len * ds[0].max(ds[1]) as f64 / 2.)
                    .ceil()
                    .clamp(1., 16384.) as usize;
                let mut last = segment.eval(0.)?;
                for i in 1..=count {
                    let next = segment.eval(len * i as f64 / count as f64)?;
                    result.segments.push([
                        last.0 as f32 * ds[0] + origin.h as f32,
                        last.1 as f32 * ds[1] + origin.v as f32,
                        next.0 as f32 * ds[0] + origin.h as f32,
                        next.1 as f32 * ds[1] + origin.v as f32,
                    ]);
                    last = next;
                }
            }
        }
    }
    Ok(result)
}
struct Frame {
    image: Image,
    origin: [f32; 2],
    reference: [usize; 2],
}
fn read(layer: &Layer, reference: [usize; 2]) -> Frame {
    let mut image = Image::new(layer.width(), layer.height());
    let origin = layer.origin();
    for y in 0..image.height {
        for x in 0..image.width {
            let p = crate::image::read_pixel(layer, x, y);
            image.pixels[y * image.width + x] =
                [p.red, p.green, p.blue, p.alpha].map(|v| if v.is_finite() { v } else { 0. });
        }
    }
    let reference = if reference[0] == 0 || reference[1] == 0 {
        [image.width, image.height]
    } else {
        reference
    };
    Frame {
        image,
        origin: [origin.h as f32, origin.v as f32],
        reference,
    }
}
fn checkout(
    cb: &SmartRenderCallbacks,
    id: u32,
    reference: [usize; 2],
) -> Result<Option<Frame>, Error> {
    let layer = cb.checkout_layer_pixels(id)?;
    let frame = layer.as_ref().map(|l| read(l, reference));
    drop(layer);
    cb.checkin_layer_pixels(id)?;
    Ok(frame)
}
fn request(
    extra: &PreRenderExtra,
    data: InData,
    index: i32,
    id: i32,
    time: i32,
) -> Result<([usize; 2], ae::Rect), Error> {
    let mut req = extra.output_request();
    req.preserve_rgb_of_zero_alpha = 1;
    req.channel_mask = ae::sys::PF_ChannelMask_ARGB as _;
    req.rect = ae::sys::PF_LRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let cb = extra.callbacks();
    let bounds = cb.checkout_layer(
        index,
        id + 1000,
        &req,
        time,
        data.time_step(),
        data.time_scale(),
    )?;
    req.rect = bounds.max_result_rect;
    cb.checkout_layer(index, id, &req, time, data.time_step(), data.time_scale())?;
    let ds = scale(data);
    Ok((
        [
            (bounds.ref_width.max(0) as f32 * ds[0]).ceil() as usize,
            (bounds.ref_height.max(0) as f32 * ds[1]).ceil() as usize,
        ],
        bounds.max_result_rect.into(),
    ))
}
fn render(
    data: InData,
    plan: &Plan,
    source: &Frame,
    map: Option<&Frame>,
    previous: Option<&Frame>,
    out: &mut Layer,
) -> Result<(), Error> {
    let mut s = plan.settings.clone();
    s.center[0] -= source.origin[0];
    s.center[1] -= source.origin[1];
    let (w, h) = (source.image.width, source.image.height);
    if w == 0 || h == 0 {
        return out.fill(None, None);
    }
    data.interact().abort()?;
    let mut field = Image {
        width: 1,
        height: 1,
        pixels: vec![[0., 0., 1., 0.]],
    };
    if s.mode == 5 {
        let segments = plan
            .segments
            .iter()
            .map(|a| {
                [
                    a[0] - source.origin[0],
                    a[1] - source.origin[1],
                    a[2] - source.origin[0],
                    a[3] - source.origin[1],
                ]
            })
            .collect::<Vec<_>>();
        field = field::path_field(w, h, &segments, plan.falloff);
    } else if s.mode == 3 {
        if let Some(previous) = previous {
            let mut aligned = Image::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    aligned.pixels[y * w + x] = previous.image.sample(
                        x as f32 + source.origin[0] - previous.origin[0],
                        y as f32 + source.origin[1] - previous.origin[1],
                        0,
                        1,
                    );
                }
            }
            field = field::motion_field(
                &source.image,
                &aligned,
                s.length * s.scale[0].max(s.scale[1]),
            );
        }
    } else if plan.map
        && let Some(map) = map
    {
        field = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let mx = (x as f32 + source.origin[0] + 0.5) * map.reference[0] as f32
                    / source.reference[0].max(1) as f32
                    - 0.5
                    - map.origin[0];
                let my = (y as f32 + source.origin[1] + 0.5) * map.reference[1] as f32
                    / source.reference[1].max(1) as f32
                    - 0.5
                    - map.origin[1];
                let mut p = map.image.sample(mx, my, 0, 1);
                let alpha = p[3].clamp(0., 1.);
                if alpha > 1e-8 {
                    for c in &mut p[..3] {
                        *c /= alpha;
                    }
                }
                let v = if s.mode == 4 {
                    if alpha <= 1e-8 {
                        [0., 0., 1., 0.]
                    } else {
                        [
                            (p[0] - 0.5) * 2. * s.length * s.scale[0] * alpha,
                            (p[1] - 0.5) * 2. * s.length * s.scale[1] * alpha,
                            1.,
                            0.,
                        ]
                    }
                } else {
                    let value = match plan.channel {
                        1 => p[0] * alpha,
                        2 => p[1] * alpha,
                        3 => p[2] * alpha,
                        4 => alpha,
                        _ => super::image::luminance(p) * alpha,
                    }
                    .clamp(0., 1.);
                    [0., 0., if plan.invert { 1. - value } else { value }, 0.]
                };
                field.pixels[y * w + x] = v;
            }
        }
    }
    data.interact().abort()?;
    let result = super::render(&source.image, field, &s).map_err(|error| {
        eprintln!("AOD blur rendering failed: {error}");
        Error::InternalStructDamaged
    })?;
    data.interact().abort()?;
    let origin = out.origin();
    out.iterate(0, out.height() as i32, None, |x, y, mut dst| {
        let p = result.sample(
            x as f32 + origin.h as f32 - source.origin[0],
            y as f32 + origin.v as f32 - source.origin[1],
            0,
            0,
        );
        let p = ae::PixelF32 {
            red: p[0],
            green: p[1],
            blue: p[2],
            alpha: p[3],
        };
        match dst {
            GenericPixelMut::Pixel8(_) => dst.set_from_u8(p.to_pixel8()),
            GenericPixelMut::Pixel16(_) => dst.set_from_u16(p.to_pixel16()),
            _ => dst.set_from_f32(p),
        };
        Ok(())
    })
}
pub fn command(
    cmd: ae::Command,
    data: InData,
    mut out: OutData,
    p: &mut Parameters<Params>,
    kind: Kind,
) -> Result<(), Error> {
    match cmd {
        ae::Command::GlobalSetup => {
            out.set_out_flag2(OutFlags2::SupportsSmartRender, true);
            out.set_out_flag(OutFlags::SendUpdateParamsUi, true);
            if kind == Kind::Shutter {
                out.set_out_flag(OutFlags::CustomUi, true);
            }
        }
        ae::Command::ArbitraryCallback { mut extra } if kind == Kind::Shutter => {
            extra.dispatch::<super::curve::ShutterCurve, Params>(Params::ExposureCurve)?;
        }
        ae::Command::Event { mut extra } if kind == Kind::Shutter => {
            super::curve_ui::handle(p, &mut extra, &mut out)?;
        }
        ae::Command::UserChangedParam { param_index } => {
            if matches!(p.type_at(param_index), Params::Mode | Params::Curve) {
                out.set_out_flag(OutFlags::RefreshUi, true);
            }
        }
        ae::Command::UpdateParamsUi => {
            let m = mode(p, kind)?;
            let pairs = if kind == Kind::Shutter {
                vec![
                    (Params::Length, m != 1),
                    (Params::Direction, m == 0),
                    (Params::Rotation, m == 1),
                    (Params::Center, m == 1 || m == 2),
                    (Params::Shutter, m == 3 || m == 4),
                    (Params::Path, m == 5),
                    (Params::Falloff, m == 5),
                    (Params::Map, m == 4),
                ]
            } else {
                vec![(Params::Evolution, m == 17), (Params::Samples, m == 17)]
            };
            for (id, enabled) in pairs {
                let mut v = p.get_mut(id)?;
                let flag = ae::ParamUIFlags::DISABLED;
                if (v.ui_flags().bits() & flag.bits() != 0) == enabled {
                    v.set_ui_flag(flag, !enabled);
                    v.update_param_ui()?;
                }
            }
        }
        ae::Command::SmartPreRender { mut extra } => {
            let mut plan = plan(p, data, kind)?;
            let (reference, bounds) = request(&extra, data, 0, 0, data.current_time())?;
            plan.references[0] = reference;
            if plan.map {
                plan.references[1] = request(
                    &extra,
                    data,
                    p.index(Params::Map).ok_or(Error::InvalidIndex)? as i32,
                    1,
                    data.current_time(),
                )?
                .0;
            }
            if plan.settings.mode == 3 {
                plan.references[2] = request(
                    &extra,
                    data,
                    0,
                    2,
                    data.current_time().saturating_sub(data.time_step()),
                )?
                .0;
            }
            let r = extra.output_request().rect;
            extra.set_max_result_rect(bounds);
            extra.set_result_rect(ae::Rect {
                left: r.left.max(bounds.left).min(bounds.right),
                top: r.top.max(bounds.top).min(bounds.bottom),
                right: r.right.min(bounds.right).max(bounds.left),
                bottom: r.bottom.min(bounds.bottom).max(bounds.top),
            });
            extra.set_pre_render_data(plan);
        }
        ae::Command::SmartRender { extra } => {
            let plan = extra
                .pre_render_data::<Plan>()
                .ok_or(Error::InternalStructDamaged)?;
            let cb = extra.callbacks();
            let src = checkout(&cb, 0, plan.references[0])?;
            let map = if plan.map {
                checkout(&cb, 1, plan.references[1])?
            } else {
                None
            };
            let previous = if plan.settings.mode == 3 {
                checkout(&cb, 2, plan.references[2])?
            } else {
                None
            };
            if let Some(mut output) = cb.checkout_output()? {
                if let Some(src) = src {
                    render(
                        data,
                        plan,
                        &src,
                        map.as_ref(),
                        previous.as_ref(),
                        &mut output,
                    )?;
                } else {
                    output.fill(None, None)?;
                }
            }
        }
        ae::Command::Render {
            in_layer,
            mut out_layer,
        } => {
            let plan = plan(p, data, kind)?;
            let source = read(&in_layer, [0; 2]);
            let fetch = |index, time| -> Result<Option<Frame>, Error> {
                let v = ParamDef::checkout(
                    data,
                    index,
                    time,
                    data.time_step(),
                    data.time_scale(),
                    Some(ParamType::Layer),
                )?;
                Ok(v.as_layer()?.value().as_ref().map(|l| read(l, [0; 2])))
            };
            let map = if plan.map {
                fetch(
                    p.index(Params::Map).ok_or(Error::InvalidIndex)? as i32,
                    data.current_time(),
                )?
            } else {
                None
            };
            let previous = if plan.settings.mode == 3 {
                fetch(0, data.current_time().saturating_sub(data.time_step()))?
            } else {
                None
            };
            render(
                data,
                &plan,
                &source,
                map.as_ref(),
                previous.as_ref(),
                &mut out_layer,
            )?;
        }
        _ => {}
    }
    Ok(())
}
