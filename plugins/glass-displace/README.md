# glass-displace ( AOD_GlassDisplace )

Applies map-driven glass refraction with configurable fracture fields, impact cracks, and spectral dispersion.

This is the After Effects plugin **AOD_GlassDisplace**, which provides the **AOD_GlassDisplace.aex** plugin file for Adobe After Effects.

## Controls

- **Height Map**: Generate a Sphere, Rounded Rectangle, Diamond, Ring, full-frame Facet Field, full-frame Fractured Field, or bounded Impact Glass height field; use a custom layer map; or derive height from input luma/alpha. Map channel, levels, and inversion are configurable.
- **Procedural Geometry**: Cell Size controls the actual tessellation scale. Facet Relief controls deterministic per-cell optical depth and Cell Irregularity perturbs the cell sites. Facet and Fractured fields are no longer clipped by the unrelated Size circle; Size is used by bounded shapes and as the Impact Glass diameter.
- **Fracture Detail**: Fractured Field adds adjustable cell-boundary crack width and depth. Impact Glass combines deterministic jagged radial cracks, probabilistic branches, interrupted concentric stress rings, a crushed strike zone, and finer Voronoi seams. Radial count, branching, jitter, ring count/irregularity, edge falloff, and seed are parametric.
- **Refraction**: Central differences convert the height field into a 3D interface normal. Each camera ray is refracted from air into BK7-equivalent optical glass with the vector form of Snell's law, then projected onto the source plane. **Refraction** is calibrated in pixels at a 45-degree surface slope, so stronger slopes can produce substantially larger, nonlinear displacement instead of saturating at the normal's XY component. Nearest and bilinear sampling plus transparent/clamp/tile/mirror edge handling remain available.
- **Spectral Dispersion**: The effect integrates 380–780 nm visible light using wavelength-dependent N-BK7 Sellmeier indices and a CIE 1931 observer response converted to linear sRGB. This produces continuously overlapping spectral fringes instead of three separated channel copies. **Auto Spectral Steps** (enabled by default) selects 6–32 samples from the estimated spectral travel, refraction magnitude, height slope, render resolution, and sampling mode. While it is enabled, **Spectral Steps (Manual)** is visibly disabled; turn Auto off to use a fixed 3–32 samples (default 8).
- **Output**: Mix and alpha controls plus Height, Normal, Displacement, and Facet ID diagnostic views.

Controls that do not apply to the selected height source or shape are hidden dynamically.

## Compatibility

The original parameter order and the first six Shape popup values are retained so existing projects continue to map to the same controls. New controls are appended after the original parameter block; **Auto Spectral Steps** is appended after the existing manual spectral-step control. Existing **Facets** and **Shards** values appear as **Facet Field** and **Fractured Field** and now share the same tessellation controls; Fractured Field adds crack controls. The previous circular envelope in those two modes is intentionally removed.

All image sampling is performed in unpremultiplied RGB and associated again before output. Alpha is spectrally weighted unless **Preserve Input Alpha** is enabled, and the same floating-point render path feeds 8, 16, and 32 bpc output conversion.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
