# scatter-map ( AOD_ScatterMap )

Applies map-driven stochastic scatter to layers.

This is the After Effects plugin **AOD_ScatterMap**, which provides the **ScatterMap.aex** plugin file for Adobe After Effects.

Scatter Algorithm defaults to fBM Vector displacement and also provides Cell Block, Cell Smooth, Noise Vector, Domain Warp fBM, Curl fBM, and legacy multi-sample modes.

Sampling Distribution controls the radial displacement curve with Uniform, Gaussian, and Exponential modes.

Noise Offset (XY) and Noise W (Z) translate the procedural noise field without moving the sampled layer.

Maps can read either a selected layer or the input layer, and selected layer controls fall back to the input layer when set to None.

Output can optionally blend the scatter result back with the original layer using selectable blend modes and opacity.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
