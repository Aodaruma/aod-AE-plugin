# rainbow-generate ( AOD_RainbowGenerate )

Generates parametric gradients in perceptual and cylindrical color spaces across multiple geometric shapes.

This is the After Effects plugin **AOD_RainbowGenerate**, which provides the **AOD_RainbowGenerate.aex** plugin file for Adobe After Effects.

## Controls

- **Geometry**: Linear, Reflected Linear, Radial (L2), Diamond (L1), Box (L-infinity), generalized Minkowski (Lp), Conic, Spiral, and Starburst gradients. Use two on-canvas points or a parametric center, angle, and length/radius.
- **Shape Parameters**: `Minkowski Exponent` continuously moves between L1 diamond, L2 radial, and a box-like high-p norm. `Spiral Turns` and `Ray Count` parameterize the procedural shapes.
- **Aspect**: A symmetric `-1..1` control. Negative values stretch vertically and positive values stretch horizontally; the internal reciprocal logarithmic mapping runs from `0.1x` through `1x` to `10x`.
- **Color Model**: Generate the rainbow directly in OKLCH, HSV, HSL, CIELCh(ab), CIELCh(uv), JzCzHz, or IPT ICh. There are no endpoint color pickers. The first three popup entries retain their original indices and behavior.
- **Perceptual Component Mapping**: `100%` chroma maps to `0.2` in OKLCH, `C*=80` in CIELCh(ab), `C*=100` in CIELCh(uv), `Cz=0.08` in JzCzHz, and `C=0.30` in IPT ICh. For JzCzHz, `100%` lightness is the Jz value of a 100-nit D65 white (`Jz=0.167174`); IPT exposes its intensity axis as `Intensity`.
- **Scale + Offset**: With `Split Range Start / End` off, each cylindrical component is an affine function of the eased geometry coordinate `t`: `component(t) = offset + scale * t`. `Hue Scale = 100%` is one full 360-degree cycle. Saturation/chroma and brightness/lightness scales are the percentage-point change across the range.
- **Explicit Range Ends**: `Split Range Start / End` replaces the affine controls with explicit Hue, Saturation/Chroma, and Brightness/Lightness values for each end. The values interpolate continuously and unwrapped hue values allow reverse or multiple rainbow cycles.
- **Easing**: Common easing presets plus a four-value cubic Bezier editor (`x1`, `y1`, `x2`, `y2`).
- **Output**: Blend with the source and optionally preserve its alpha.

Controls that do not apply to the selected mode are hidden dynamically. The standard cubic controls are intentionally host-native so animation, project serialization, and Premiere Pro fallback remain reliable.

## Version 0.2 Design Change

Version 0.2 intentionally replaces the original two-color interpolation UI and render model with direct parametric rainbow generation. Projects created with the earlier development build should recreate their color settings with the new component controls.

## Version 0.3 Color Models

Version 0.3 appends four distinct cylindrical models without changing the existing OKLCH, HSV, and HSL popup indices. CIELCh(ab) uses D50 CIELAB with D65 chromatic adaptation for display output; CIELCh(uv) uses D65 CIELUV; JzCzHz provides an HDR-oriented lightness/opponent model normalized to an SDR 100-nit white; and IPT ICh provides an intensity/opponent model with a hue-uniformity-oriented nonlinearity. Component names update dynamically for Chroma, Lightness, Jz, and Intensity.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
