# image-transform ( AOD_ImageTransform )

Affine image transform for After Effects with independent anchor, position, scale, rotation, skew,
skew axis, and opacity controls.

## Sampling

- Nearest, Bilinear, Bicubic (Catmull-Rom), Mitchell-Netravali, Lanczos, Cubic B-spline, and
  EWA Quadratic reconstruction filters.
- Mitchell B/C, Lanczos lobe count, and EWA radius appear only for the corresponding filter.
- `Sample Outside Image` enables Clamp, Tile, or Mirror sampling. When disabled, samples beyond the
  source image are transparent.
- Geometry accounts for pixel aspect ratio and render downsampling.

This plugin provides **AOD_ImageTransform.aex**.

## Building the Plugin

See the [main README](../../README.md) for build instructions.
