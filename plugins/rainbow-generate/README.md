# rainbow-generate ( AOD_RainbowGenerate )

Generates parametric gradients in perceptual and cylindrical color spaces across multiple geometric shapes.

This is the After Effects plugin **AOD_RainbowGenerate**, which provides the **AOD_RainbowGenerate.aex** plugin file for Adobe After Effects.

## Controls

- **Geometry**: Linear, Reflected Linear, Radial (L2), Diamond (L1), Box (L-infinity), generalized Minkowski (Lp), Conic, Spiral, and Starburst gradients. Use two on-canvas points or a parametric center, angle, and length/radius.
- **Shape Parameters**: `Minkowski Exponent` continuously moves between L1 diamond, L2 radial, and a box-like high-p norm. `Spiral Turns` and `Ray Count` parameterize the procedural shapes.
- **Aspect**: A symmetric `-1..1` control. Negative values stretch vertically and positive values stretch horizontally; the internal reciprocal logarithmic mapping runs from `0.1x` through `1x` to `10x`.
- **Skew**: Applies a horizontal inverse-coordinate shear to every shape. `0%` leaves the existing geometry unchanged and `100%` corresponds to a shear factor of `1`.
- **Generation Mode**: `Parametric` remains the default and generates the rainbow directly from cylindrical components. `Two Color` exposes start/end color pickers and an independent interpolation-space selector.
- **Color Model**: In Parametric mode, generate directly in OKLCH, HSV, HSL, CIELCh(ab), CIELCh(uv), JzCzHz, IPT ICh, OkHSL, OkHSV, or CAM16-UCS J'M'h'. The original seven popup entries retain their indices and behavior.
- **Two Color Space**: Interpolate the selected colors in OKLab, OKLCH, CAM16-UCS J'a'b', or CAM16-UCS J'M'h'. Both polar spaces follow the shortest hue path and hold the chromatic endpoint's hue when the other endpoint is neutral. Endpoint conversion is prepared once per render rather than once per pixel.
- **Perceptual Component Mapping**: `100%` chroma maps to `0.2` in OKLCH, `C*=80` in CIELCh(ab), `C*=100` in CIELCh(uv), `Cz=0.08` in JzCzHz, `C=0.30` in IPT ICh, and `M'=50` in CAM16-UCS. For JzCzHz, `100%` lightness is the Jz value of a 100-nit D65 white (`Jz=0.167174`); IPT exposes its intensity axis as `Intensity`; CAM16-UCS maps `100%` lightness to `J'=100`.
- **Scale + Offset**: With `Split Range Start / End` off, each cylindrical component is an affine function of the eased geometry coordinate `t`: `component(t) = offset + scale * t`. `Hue Scale = 100%` is one full 360-degree cycle. Saturation/chroma and brightness/lightness scales are the percentage-point change across the range.
- **Explicit Range Ends**: `Split Range Start / End` replaces the affine controls with explicit Hue, Saturation/Chroma, and Brightness/Lightness values for each end. The values interpolate continuously and unwrapped hue values allow reverse or multiple rainbow cycles.
- **Easing**: Common easing presets plus a four-value cubic Bezier editor (`x1`, `y1`, `x2`, `y2`).
- **Output**: Blend with the source and optionally preserve its alpha.

Controls that do not apply to the selected mode are hidden dynamically. The standard cubic controls are intentionally host-native so animation, project serialization, and Premiere Pro fallback remain reliable.

## Generation Modes

Direct parametric rainbow generation is the default. The optional Two Color mode restores endpoint color selection without changing the parameter indices or behavior of the existing parametric controls.

## Version 0.3 Color Models

Version 0.3 appends four distinct cylindrical models without changing the existing OKLCH, HSV, and HSL popup indices. CIELCh(ab) uses D50 CIELAB with D65 chromatic adaptation for display output; CIELCh(uv) uses D65 CIELUV; JzCzHz provides an HDR-oriented lightness/opponent model normalized to an SDR 100-nit white; and IPT ICh provides an intensity/opponent model with a hue-uniformity-oriented nonlinearity. Component names update dynamically for Chroma, Lightness, Jz, and Intensity.

## Version 0.4 Perceptual Models

OkHSL and OkHSV are later sRGB-gamut color-picking spaces derived from OKLab. CAM16-UCS is a uniform color space derived from the more comprehensive CAM16 color appearance model and is available in polar J'M'h' form for parametric generation, plus Cartesian J'a'b' and polar J'M'h' forms for two-color interpolation.

CAM16-UCS uses fixed display-referred viewing conditions so animated frames remain deterministic: D65 white, `40 cd/m²` adapting luminance, a `20%` background luminance factor, average surround, and automatic illuminant discounting. These parameters are baked once per render. The implementation uses `palette 0.7.7`; see [Third-Party Notices](THIRD_PARTY_NOTICES.md).

Definitions: [OkHSL and OkHSV](https://bottosson.github.io/posts/colorpicker/), [CAM16 and CAM16-UCS](https://doi.org/10.1002/col.22131), and the [palette CAM16 API](https://docs.rs/palette/0.7.7/palette/cam16/).

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
