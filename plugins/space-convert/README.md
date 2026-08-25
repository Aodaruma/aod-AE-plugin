# space-convert ( AOD_SpaceConvert )

Converts images between Cartesian, curvilinear coordinate, Radon, and line Hough representations.

## Modes

- **Polar / Log-Polar**: angle is horizontal and radius is vertical.
- **Spiral Polar**: polar coordinates with a parametric radial twist. `Spiral Turns` may be
  positive or negative and remains analytically invertible.
- **Square-Disc**: concentric square/disc mapping with an analytic coordinate map. The default
  `Auto (Full Coverage)` radius uses the canvas-inscribed disc so the mapped square is not clipped.
- **Elliptic Coordinates**: confocal ellipse/hyperbola coordinates. `Focus / Radius` controls the
  focal distance.
- **Parabolic Coordinates**: an analytic complex-square coordinate map using signed parabolic axes.
- **Bipolar Coordinates**: two-focus coordinates with adjustable focus distance and finite
  coordinate extent.
- **Radon**: angle is horizontal and signed detector distance is vertical. Forward projections are
  path-length normalized; inverse applies the corresponding finite Ram-Lak/FBP normalization.
- **Line Hough**: line-evidence accumulator in the same layout. `Luminance` preserves the original
  behavior; `RGBA` transforms all four channels independently; `Red`, `Green`, `Blue`, and `Alpha`
  provide single-channel grayscale analysis and reconstruction views.

Spatial geometry is evaluated in full-resolution square-pixel coordinates. Pixel aspect ratio and
independent horizontal/vertical downsampling are accounted for, so custom and log radii, circular
maps, Radon lines, and Hough edge thresholds remain stable across render resolutions. Custom Center
is localized against the checked-out world's origin when preceding effects expand the bounds.

Coordinate modes use inverse-mapped sampling, so repeated conversion loses information through
resampling. `Radon` inverse uses finite Ram-Lak filtered backprojection and `Line Hough` inverse uses
normalized backprojection. Both are labelled **Inverse (Approximate)** in the UI and are not exact
reconstruction methods.

`Angle Samples`, `Detector Samples`, and `Samples per Ray` explicitly trade analysis quality for CPU
cost. At extreme combinations the effective ray count, filter radius, and inverse working resolution
are reduced to a fixed safe work budget. The analysis buffer is resampled to the layer dimensions for
display.

This is the After Effects plugin **AOD_SpaceConvert**, which provides the **AOD_SpaceConvert.aex** plugin file for Adobe After Effects.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
