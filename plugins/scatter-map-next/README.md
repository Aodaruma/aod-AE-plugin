# scatter-map-next ( AOD_ScatterMapNext )

Applies map-driven gather and swap scatter to layers.

This is the After Effects plugin **AOD_ScatterMapNext**, which provides the **ScatterMapNext.aex** plugin file for Adobe After Effects.

## Modes

- **Gather** selects discrete neighboring pixels. One sample copies an existing RGBA pixel without interpolation; multiple samples average the selected pixels.
- **Swap** exchanges disjoint square grains within the radius. With output blending disabled and original alpha preservation disabled, it preserves the complete RGBA pixel multiset.

Amount is the probability that a destination grain is eligible to participate. Radius is the maximum movement in pixels. Grain Size makes pixels share a gather decision or move together as a swap block. Swap can leave an eligible block unchanged when no compatible partner is available.

Amount Map and Radius Map multiply their corresponding controls. A map layer set to None reads the effect input, and each map can read luma or an RGBA channel.

Gather applies the Amount Map at the destination. Swap requires both blocks to be active and uses the smaller mapped radius, so black map regions remain fixed. Swap uses a single round of disjoint block pairs; edge blocks only pair with blocks of the same dimensions, and a radius smaller than Grain Size can leave blocks unchanged.

The Output group provides deterministic or per-frame seeds, gather boundary handling, blending with the original, alpha preservation, and optional 32bpc clamping.

Smart Render is intentionally disabled in this comparison plugin so Gather and especially global Swap always operate on the complete frame instead of independently processed render tiles.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
