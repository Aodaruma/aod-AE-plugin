use crate::{curve::Curve, shading, surface, ui};
use after_effects::Error;
use after_effects::pf::*;
use utils::image::finite_or;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
pub enum Params {
    Output,
    AlphaMode,
    ShapeStart,
    Silhouette,
    Threshold,
    SilhouetteBlur,
    Method,
    Direction,
    Width,
    Offset,
    Elevation,
    HeightBlur,
    SmoothRadius,
    Iterations,
    ShapeEnd,
    ProfileStart,
    Preset,
    ApplyPreset,
    Interpolation,
    Profile,
    ProfileEnd,
    MaterialStart,
    BaseSource,
    BaseColor,
    Diffuse,
    Roughness,
    Specular,
    Metallic,
    Ior,
    EmissionColor,
    EmissionStrength,
    MaterialEnd,
    TransmissionStart,
    Transmission,
    Thickness,
    Background,
    TransmissionEnd,
    LightStart,
    LightPosition,
    LightHeight,
    LightColor,
    LightIntensity,
    Ambient,
    LightEnd,
    Exposure,
    Clamp,
}

pub fn setup(params: &mut Parameters<Params>) -> Result<(), Error> {
    popup(
        params,
        Params::Output,
        "Output",
        &[
            "Lit Material",
            "Height (Grayscale)",
            "Normal Map",
            "Diffuse",
            "Specular",
            "Refraction",
        ],
        1,
    )?;
    popup(
        params,
        Params::AlphaMode,
        "Alpha",
        &["Original Alpha", "Curve Zero Cutout", "No Cutout"],
        1,
    )?;
    params.add_group(
        Params::ShapeStart,
        Params::ShapeEnd,
        "Silhouette and Surface",
        false,
        |p| {
            popup(
                p,
                Params::Silhouette,
                "Silhouette",
                &["Alpha", "Luminance x Alpha"],
                1,
            )?;
            slider(p, Params::Threshold, "Threshold", 0.5, 0.0, 1.0, 1.0)?;
            slider(
                p,
                Params::SilhouetteBlur,
                "Silhouette Blur (px)",
                0.0,
                0.0,
                256.0,
                32.0,
            )?;
            popup(
                p,
                Params::Method,
                "Surface Method",
                &[
                    "Signed Distance (SDF)",
                    "Screened Poisson / Neumann",
                    "Screened Poisson / Dirichlet",
                ],
                1,
            )?;
            popup(
                p,
                Params::Direction,
                "Direction",
                &["Inner", "Outer", "Both"],
                1,
            )?;
            slider(
                p,
                Params::Width,
                "Bevel Width (px)",
                32.0,
                0.01,
                4096.0,
                256.0,
            )?;
            slider(
                p,
                Params::Offset,
                "Contour Offset (px)",
                0.0,
                -4096.0,
                4096.0,
                256.0,
            )?;
            slider(
                p,
                Params::Elevation,
                "Height (px)",
                16.0,
                -2048.0,
                2048.0,
                128.0,
            )?;
            slider(
                p,
                Params::HeightBlur,
                "Height Blur (px)",
                0.0,
                0.0,
                256.0,
                32.0,
            )?;
            slider(
                p,
                Params::SmoothRadius,
                "Poisson Smoothing (px)",
                8.0,
                0.0,
                256.0,
                64.0,
            )?;
            p.add(
                Params::Iterations,
                "Poisson Iterations",
                SliderDef::setup(|d| {
                    d.set_valid_min(1);
                    d.set_valid_max(2000);
                    d.set_slider_min(1);
                    d.set_slider_max(500);
                    d.set_default(200);
                }),
            )?;
            Ok(())
        },
    )?;
    params.add_group(
        Params::ProfileStart,
        Params::ProfileEnd,
        "Bevel Profile",
        false,
        |p| {
            popup(
                p,
                Params::Preset,
                "Profile Preset",
                &["Linear", "Round", "Concave", "Ring", "Steps"],
                2,
            )?;
            p.add_with_flags(
                Params::ApplyPreset,
                "Apply Profile Preset",
                ButtonDef::setup(|d| {
                    d.set_label("Apply");
                }),
                ParamFlag::SUPERVISE,
                ParamUIFlags::empty(),
            )?;
            popup(
                p,
                Params::Interpolation,
                "Interpolation",
                &["Smooth", "Linear"],
                1,
            )?;
            let mut arbitrary = ArbitraryDef::new();
            arbitrary.set_default(Curve::default())?;
            p.add_customized(Params::Profile, "Profile Curve", arbitrary, |d| {
                d.set_ui_flags(ParamUIFlags::CONTROL | ParamUIFlags::DO_NOT_ERASE_CONTROL);
                d.set_ui_width(ui::WIDTH);
                d.set_ui_height(ui::HEIGHT);
                -1
            })?;
            Ok(())
        },
    )?;
    params.add_group(
        Params::MaterialStart,
        Params::MaterialEnd,
        "Material",
        true,
        |p| {
            popup(
                p,
                Params::BaseSource,
                "Base Source",
                &["Solid Color", "Source x Base Color"],
                1,
            )?;
            color(p, Params::BaseColor, "Base Color", [166, 184, 209])?;
            slider(p, Params::Diffuse, "Diffuse", 1.0, 0.0, 1.0, 1.0)?;
            slider(p, Params::Roughness, "Roughness", 0.3, 0.0, 1.0, 1.0)?;
            slider(
                p,
                Params::Specular,
                "Specular IOR Level",
                0.5,
                0.0,
                1.0,
                1.0,
            )?;
            slider(p, Params::Metallic, "Metallic", 0.0, 0.0, 1.0, 1.0)?;
            slider(p, Params::Ior, "IOR", 1.5, 1.0, 4.0, 3.0)?;
            color(p, Params::EmissionColor, "Emission Color", [255; 3])?;
            slider(
                p,
                Params::EmissionStrength,
                "Emission Strength",
                0.0,
                0.0,
                100.0,
                5.0,
            )
        },
    )?;
    params.add_group(
        Params::TransmissionStart,
        Params::TransmissionEnd,
        "Refraction",
        true,
        |p| {
            slider(p, Params::Transmission, "Transmission", 0.0, 0.0, 1.0, 1.0)?;
            slider(
                p,
                Params::Thickness,
                "Refraction Depth (px)",
                20.0,
                0.0,
                4096.0,
                256.0,
            )?;
            p.add(Params::Background, "Refraction Layer", LayerDef::new())?;
            Ok(())
        },
    )?;
    params.add_group(
        Params::LightStart,
        Params::LightEnd,
        "Lighting",
        true,
        |p| {
            p.add(
                Params::LightPosition,
                "Light Position",
                PointDef::setup(|d| {
                    d.set_default((25.0, 25.0));
                }),
            )?;
            slider(
                p,
                Params::LightHeight,
                "Light Height (px)",
                300.0,
                0.01,
                10000.0,
                1000.0,
            )?;
            color(p, Params::LightColor, "Light Color", [255; 3])?;
            slider(
                p,
                Params::LightIntensity,
                "Light Intensity",
                3.0,
                0.0,
                100.0,
                10.0,
            )?;
            slider(p, Params::Ambient, "Ambient", 0.15, 0.0, 10.0, 1.0)
        },
    )?;
    slider(
        params,
        Params::Exposure,
        "Exposure (stops)",
        0.0,
        -20.0,
        20.0,
        5.0,
    )?;
    params.add(
        Params::Clamp,
        "Clamp Output",
        CheckBoxDef::setup(|d| {
            d.set_default(false);
        }),
    )?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub surface: surface::Settings,
    pub material: shading::Material,
    pub silhouette: i32,
    pub alpha: i32,
    pub output: i32,
    pub light: [f32; 3],
    pub radiance: [f32; 3],
    pub clamp: bool,
}

impl Settings {
    pub fn read(p: &Parameters<Params>, data: InData) -> Result<Self, Error> {
        let pop = |id| p.get(id)?.as_popup().map(|d| d.value());
        let number = |id| {
            p.get(id)?
                .as_float_slider()
                .map(|d| finite_or(d.value() as f32, 0.0))
        };
        let rgb = |id| -> Result<[f32; 3], Error> {
            let c = p.get(id)?.as_color()?.float_value()?;
            Ok([c.red, c.green, c.blue].map(|v| finite_or(v, 0.0).max(0.0)))
        };
        let ratio = |r: RationalScale| {
            if r.num > 0 && r.den > 0 {
                r.num as f32 / r.den as f32
            } else {
                1.0
            }
        };
        let scale = [
            ratio(data.pixel_aspect_ratio()) / ratio(data.downsample_x()),
            1.0 / ratio(data.downsample_y()),
        ];
        let point = p.get(Params::LightPosition)?.as_point()?.value();
        let intensity = number(Params::LightIntensity)?.clamp(0.0, 100.0);
        Ok(Self {
            surface: surface::Settings {
                width: number(Params::Width)?.clamp(0.01, 4096.0),
                offset: number(Params::Offset)?.clamp(-4096.0, 4096.0),
                direction: pop(Params::Direction)?,
                threshold: number(Params::Threshold)?.clamp(0.0, 1.0),
                silhouette_blur: number(Params::SilhouetteBlur)?.clamp(0.0, 256.0),
                height_blur: number(Params::HeightBlur)?.clamp(0.0, 256.0),
                method: pop(Params::Method)?,
                smooth_radius: number(Params::SmoothRadius)?.clamp(0.0, 256.0),
                iterations: p
                    .get(Params::Iterations)?
                    .as_slider()?
                    .value()
                    .clamp(1, 2000) as usize,
                elevation: number(Params::Elevation)?.clamp(-2048.0, 2048.0),
                smooth_curve: pop(Params::Interpolation)? == 1,
                scale,
                curve: (*p.get(Params::Profile)?.as_arbitrary()?.value::<Curve>()?).sanitized(),
            },
            material: shading::Material {
                base: rgb(Params::BaseColor)?,
                source_color: pop(Params::BaseSource)? == 2,
                diffuse: number(Params::Diffuse)?.clamp(0.0, 1.0),
                roughness: number(Params::Roughness)?.clamp(0.0, 1.0),
                specular: number(Params::Specular)?.clamp(0.0, 1.0),
                metallic: number(Params::Metallic)?.clamp(0.0, 1.0),
                ior: number(Params::Ior)?.clamp(1.0, 4.0),
                transmission: number(Params::Transmission)?.clamp(0.0, 1.0),
                thickness: number(Params::Thickness)?.clamp(0.0, 4096.0),
                emission: rgb(Params::EmissionColor)?,
                emission_strength: number(Params::EmissionStrength)?.clamp(0.0, 100.0),
                ambient: number(Params::Ambient)?.clamp(0.0, 10.0),
                exposure: number(Params::Exposure)?.clamp(-20.0, 20.0),
            },
            silhouette: pop(Params::Silhouette)?,
            alpha: pop(Params::AlphaMode)?,
            output: pop(Params::Output)?,
            light: [
                finite_or(point.0, 0.0),
                finite_or(point.1, 0.0),
                number(Params::LightHeight)?.clamp(0.01, 10000.0),
            ],
            radiance: rgb(Params::LightColor)?.map(|v| v * intensity),
            clamp: p.get(Params::Clamp)?.as_checkbox()?.value(),
        })
    }
}

fn popup(
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
    default: f64,
    min: f32,
    max: f32,
    slider_max: f32,
) -> Result<(), Error> {
    p.add(
        id,
        name,
        FloatSliderDef::setup(|d| {
            d.set_valid_min(min);
            d.set_valid_max(max);
            d.set_slider_min(if min < 0.0 { -slider_max } else { min });
            d.set_slider_max(slider_max);
            d.set_default(default);
            d.set_precision(3);
        }),
    )
}

fn color(p: &mut Parameters<Params>, id: Params, name: &str, rgb: [u8; 3]) -> Result<(), Error> {
    p.add(
        id,
        name,
        ColorDef::setup(|d| {
            d.set_default(Pixel8 {
                alpha: 255,
                red: rgb[0],
                green: rgb[1],
                blue: rgb[2],
            });
        }),
    )
}
