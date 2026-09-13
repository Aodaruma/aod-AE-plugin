# checker-generate ( AOD_CheckerGenerate )

Generates customizable two-color checkerboard patterns with adjustable geometry, edge treatment, and compositing.

This is the After Effects plugin **AOD_CheckerGenerate**, which provides the **CheckerGenerate.aex** plugin file for Adobe After Effects.

## Controls

- **Separate Width / Height**: Uses one Cell Size control (8 px by default) when disabled, or independent Cell Width and Cell Height controls when enabled.
- **Center**: Places the center of a Color A cell.
- **Color A / Color B**: Sets the two checker colors. Color A defaults to `#CCCCCC`.
- **Color A Opacity / Color B Opacity**: Sets each checker color's transparency independently from 0–100%.
- **Edge Interpolation**: Applies subpixel edge smoothing.
- **Apply Feather / Feather Width**: Enables a wider soft transition between cells.
- **Blend Mode / Blend Opacity**: Composites the generated pattern over the input layer.
- **Preserve Original Alpha**: Keeps the source alpha while applying the pattern.
- **Clamp (32bpc)**: Restricts HDR output channels to the 0–1 range.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
