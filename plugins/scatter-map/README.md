# scatter-map ( AOD_ScatterMap )

Applies map-driven gather and swap scatter to layers.

This is the After Effects plugin **AOD_ScatterMap**, which provides the **ScatterMap.aex** plugin file for Adobe After Effects.

For compatibility with projects created with the comparison plugin, its internal After Effects match name remains `ScatterMapNext`.

## Modes

- **Gather** selects discrete neighboring pixels. One sample copies an existing RGBA pixel without interpolation; multiple samples average the selected pixels.
- **Swap** exchanges disjoint masked grains within the radius. With Fill set to Source Texture, output blending disabled, and original alpha preservation disabled, it preserves the complete RGBA pixel multiset. Average, Median, and Center Pixel Fill intentionally synthesize or duplicate colors after the swap.

Amount is the probability that a destination grain is eligible to participate in each density layer. Radius is the maximum final movement in pixels. Grain Size controls each kernel footprint while Grain Size Map is disabled. When the map is enabled, its Min and Max controls bound the footprint and Max defines the placement grid so pixels share a gather decision or move together as a swap block. Values with Min above Max are clamped to Max. Swap can leave an eligible block unchanged when no compatible partner is available.

Amount Map and Radius Map multiply their corresponding controls. Grain Size Map interpolates from its Min at black to Max at white. Its Min and Max remain visible under Map Grain Size but are disabled until the map is enabled; enabling it disables the standalone Grain Size control without hiding it. A map layer set to None reads the effect input, and each scalar map can read luma or an RGBA channel. Scalar maps are sampled once at each grain center so a Radius Map does not cut a grain into a partial mask.

Direction and Anisotropy turn the circular scatter distribution into a directed ellipse. Anisotropy Map can replace the direction and/or multiply the strength using Hue/Saturation, UV, Normal, Divergence Direction, or Divergence Rotation encoding. The divergence modes derive a gradient from Gray, RGBA, HSV Value, or HSL Lightness; Direction follows the gradient while Rotation follows its perpendicular contour.

The Grain Kernel group supports Square, Circle, and Custom Texture shapes. A custom texture is resampled across each grain and converted to a mask using the selected channel and threshold. Shape Randomness perturbs the kernel boundary or texture threshold per pixel. Grain Size Randomness deterministically varies each grain between 1 px and Grain Size when the map is disabled, or between the map Min and its map-selected size when enabled. At 0% the selected size is fixed; 100% uses the full available range.

Position Randomness moves each grain center by a deterministic, seed-controlled amount. At 0% the first layer retains the original grid placement; increasing it produces irregular, stipple-like plotting. Every active grain receives one seeded random front priority that remains fixed across its whole shape and across all density layers. Where grains overlap, the higher-priority grain is drawn in front instead of splitting both shapes at their nearest-center boundary. Inactive grains and grains disabled by Amount or Radius maps do not occlude active grains; uncovered gaps retain the original image.

Grain Density adds full-size plotting layers without shrinking the Grain Size footprint. 100% is the original single-layer result, 400% plots four layers, and 800% plots eight layers. Fractional hundreds use a proportionally active final layer. Additional layers use evenly distributed sub-grid phases at 0% Position Randomness and blend toward seeded random placement as Position Randomness increases. At very small Grain Sizes the effective layer count saturates once additional sub-pixel placement would be redundant. Higher density adds placement work, but Gather samples only the final visible winner at each pixel once rather than repeatedly rendering pixels that later layers would overwrite.

Fill can preserve Source Texture or flatten each affected grain to its channel-wise Average, Median, or Center Pixel color. For an overlapped grain, the representative is computed from that grain's visible portion; Center Pixel uses the visible pixel nearest its original center. Fill Opacity blends the selected representative color over the grain's scattered texture; 0% preserves the texture and 100% fully flattens it.

Gather applies the Amount Map at the destination grain. Swap requires both grains to be active and uses the smaller mapped radius, so black map regions remain fixed. Swap uses one round of grain pairs per density layer, while the shared front-priority mask keeps visible groups disjoint across layers. A pair is swapped only when both complete visible grain groups fit within Radius, so a partially valid overlap cannot shear a grain; incompatible grains remain unchanged.

The Output group provides deterministic or per-frame seeds, gather boundary handling, blending with the original, alpha preservation, and optional 32bpc clamping.

Smart Render is enabled for 8bpc, 16bpc, and 32bpc rendering. Because grain ownership and Swap pairing are global decisions, Smart Render checks out complete input and enabled map frames instead of recomputing a different grid per ROI tile. Pixel-distance controls follow the After Effects preview downsample factor. Gather and Swap share the same deterministic map, kernel, and seed evaluation. CPU scratch storage uses compact owner and partition buffers, enabled map copies are released before output post-processing, same-size source frames are reused without an extra full-frame clone, and the Source Texture Swap path releases its grain partition before building the output frame.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
