# image-relight ( AOD_ImageRelight )

Relights images using color-region, channel-generated, or supplied normal maps with configurable
material and light controls.

## Normal sources and outputs

- **Generated / Color Regions** is the default. It separates a cel-painted image by its straight
  (unpremultiplied) RGB colors, then builds an independent height surface inside each connected
  color region. `Color Tolerance` controls RGB label quantization, `Alpha Threshold` excludes
  transparent pixels, and `Edge Softness` feathers normals near region boundaries.
  - **Distance Field (SDF)** preserves the original fast surface based on
    an unsigned interior chamfer distance field (rather than a two-sided exact signed distance).
    `Region Radius` sets the physical bevel/dome reach and `Height Shape` controls its falloff.
  - **Poisson / Neumann (Smooth)** is the default solver. It solves a screened Poisson system
    independently for every four-connected color region. `Boundary Condition` selects fixed-zero
    height (Dirichlet) or zero-flux normal continuity (Neumann, the default for this solver).
    `Iterations`, `Divergence / Curvature`, `Screened Damping`, and `Edge Feather` control
    convergence and the smooth surface profile derived from the same region-boundary distance. The
    shape is normalized per connected region, then its 0..1 relief is scaled by a bounded response
    to Curvature and Damping. Consequently Curvature changes the actual relief magnitude, while
    zero Curvature plus zero Damping produces a strictly flat surface.
- **Generated / Height Channel** retains the original luminance, alpha, red, green, or blue height
  source. It can blur the height before converting its gradient to a tangent-space normal.
- Generated methods share `Normal Strength` and `Invert Height`. In Color Regions mode, inversion
  turns raised region interiors into recessed surfaces while keeping every color boundary isolated.
- **Normal Layer** decodes an independently selected RGB normal layer. Checked-out layer origins are
  used to align it with the source in layer coordinates; areas outside the supplied map use a flat
  normal. OpenGL (+Y) and DirectX (-Y) conventions are supported.
- Output views include the final relight, normal map, height, diffuse, specular, and combined
  lighting. Height output is available for generated normals; supplied normal maps have no unique
  recoverable height, so that view uses neutral gray.

The default **Principled (GGX)** surface uses the source image as its base color. `Base Tint`
multiplies that color, while `Metallic`, `Roughness`, `IOR`, and `Specular IOR Level` follow the
familiar Principled-material convention. The direct-light BRDF combines a GGX normal distribution,
Smith masking-shadowing, Schlick Fresnel, and an energy-conserving Lambert diffuse lobe. IOR 1.5 and
Specular IOR Level 0.5 produce the conventional 4% dielectric normal-incidence reflectance.
`Environment Strength` supplies a simple base-color environment term because this effect has no
environment-map input. **Legacy Blinn-Phong** remains selectable and retains the original
Ambient/Diffuse/Specular/Shininess controls. Controls that do not apply to the selected source,
surface, light, or output view are hidden dynamically.
Generated-normal gradients, region distances, Poisson Laplacian weights, edge softness, height blur,
and point-light distance/falloff are evaluated in full-resolution square-pixel coordinates,
accounting for pixel aspect ratio and independent horizontal/vertical downsampling. For unscreened
Neumann solves, the right-hand side is made mean-free and the otherwise-undetermined component mean
is fixed during iteration, preventing drift while satisfying the Neumann compatibility condition.
Color Region generation requests the full Smart Render source so region surfaces remain stable
across tiled output requests. A thread-safe, single-entry transient cache reuses the generated
height and normal maps between tiles with identical full-source content, bounds, and surface
settings. The entry is replaced on any key change, so memory does not grow across frames or
parameter edits. Point Position is localized against the checked-out source origin when preceding
effects expand the bounds. Input and output support 8/16/32 bpc; color labels are derived after
unpremultiplication and all rendered views retain the source alpha in premultiplied form.

Large screened Poisson solves use a cached WGPU compute pipeline automatically. Small surfaces,
pure unscreened Neumann systems, unavailable adapters, unsupported limits, and GPU execution errors
fall back to the equivalent CPU solver. Set `AOD_IMAGE_RELIGHT_GPU=cpu` or `gpu` before launching
After Effects to force either backend while testing.

The v0.3 solver controls are appended after the v0.2 parameter sequence, and the v0.5 Principled
material controls are appended after the complete v0.4 sequence, so all earlier parameter indices
remain unchanged. Existing values for the original material controls are retained and can be used
by selecting Legacy Blinn-Phong. The GPU backend and material update change no PiPL or sequence
flags.

This is the After Effects plugin **AOD_ImageRelight**, which provides the **AOD_ImageRelight.aex** plugin file for Adobe After Effects.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
