# texture-stroke ( AOD_TextureStroke )

Generates textured strokes from mask and shape paths.

This is the After Effects plugin **AOD_TextureStroke**, which provides the **AOD_TextureStroke.aex** plugin file for Adobe After Effects.

## Paths

- `Auto (Shape / Mask)` uses Contents paths on shape layers, shapes inside collapsed precomps, and shapes below adjustment layers. Other layers use their own mask paths.
- `All Mask Paths` and `Selected Mask Path` explicitly use masks, including on shape layers. `Shape Paths` explicitly uses Contents.
- With Collapse Transformations enabled, apply the effect to the precomp layer to collect its active 2D shapes. Nested collapsed precomps are traversed, combining group, layer, parent, and precomp transforms. Start time, time stretch, and time remapping are evaluated at each nesting level.
- On an adjustment layer, Auto/Shape Paths collects active 2D shape layers and collapsed precomps below it, from bottom to top. Layers above it and other adjustment layers are excluded. Use Composite (Front) to draw over the lower layers, or Stroke Only to replace their combined input with the generated strokes within the adjustment region.
- Shape support includes open/closed Bezier paths, rectangles (including roundness), and ellipses, with nested group transforms and 2D layer transforms. Disabled paths and groups are skipped. Animated values and expressions are sampled at the render time.
- The effect reads source paths, not the rendered alpha outline. Shape operators such as Trim Paths, Repeater, Merge Paths, and path distortion are not evaluated. Convert polystars to Bezier paths first. Non-collapsed nested precomps and 3D layers (including shapes with 3D parents) are not traversed. Child fill/stroke styles, masks, mattes, blend modes, and opacity are not transferred to the generated brush strokes; use the effect controls for their appearance.
- Shape paths use `Stroke Width`; mask feather width settings apply to masks. Fill and native stroke styles do not define the texture brush.

## Brush and output

- Set `Texture Layer` to use an image or animated layer as the brush. Without it, the built-in circle/square brush is used.
- `Output` defaults to `Composite (Front)`, placing the stroke over the input image. `Composite (Behind)` places it behind the input, visible through transparency. `Stroke Only` accumulates stamps on transparency. Transparent brush pixels preserve earlier stamps.
- `Stamp Order` chooses `Start to End` (default) or `End to Start` within each path. Later stamps cover earlier ones. Reversing the order preserves each stamp's position, random variations, texture frame, and the stacking order between independent paths.
- `Stroke Opacity` controls stamp opacity. `Base Direction Offset` rotates the brush; `Reverse Direction` reverses its orientation. These controls do not change stamp stacking.
- `Texture Time` offers `Current`, `Fixed Frame`, `Along Stroke`, and `Random Still (Layer Range)`. Random Still picks a fixed frame per stamp from the texture layer's in/out range, intersected with its source duration. It respects start time, trimming, and time stretch, and does not drift with the current time. `Time Samples` sets the number of candidate frames (up to 16); `Random Time Seed` changes their assignment. Deliberately transparent source frames remain transparent. For time-remapped textures, the layer's in/out range is used.
- `Time Range (frames)` now applies only to `Along Stroke`, which samples around the current frame. `Current` and `Fixed Frame` retain their existing behavior.
- Paths and brush widths remain in the same layer coordinates when masks crop the input or preview resolution changes. Smart Render includes the generated stroke bounds, even outside the input alpha bounds.
- Empty paths or zero width produce transparency in `Stroke Only`. A zero base width can still use a nonzero mask feather width.

## Path / Map Dynamics

The `Size`, `Density`, `Rotation`, and `Opacity` groups each have an independent `Input` selector and `Response` curve. All default to `None`. Choose one input per channel:

| Input | Meaning |
|---|---|
| Curvature | Turning per path pixel, normalized by `Curvature Radius`. A circle with that radius gives input 0.5; a straight line gives 0. |
| Path Crowding | Nearby **other paths**, weighted by their length and distance within `Crowding Radius`. The same path is excluded; adding vertices alone does not increase crowding. |
| Path Length | Each path's length divided by `Length Reference`, clamped to 0–1. |
| Map Luminance | Premultiplied luminance of the selected map layer at the stamp's absolute composition position. |
| Map Alpha | Alpha of the selected map layer at that same composition position. |

Maps use the selected layer's current frame and account for 2D position, anchor, scale, rotation, and parenting. They are not stretched to the stroke's bounds. Each channel can select a different layer. Pixels outside the map are zero; selecting no map leaves that channel unchanged. AE's layer selector controls whether source, masks, or effects are included. Map sampling supports 2D layers; camera projection is not supported.

Click the graph to add a point (up to eight), drag points to edit, or Alt-click an interior point to remove it. `Reset` restores the linear response, `Flat` gives neutral output, and `Invert` flips the curve vertically. The curve supports Undo, project save/reload, and keyframes. X is normalized input 0–1. Y is 0–200% for size/density/opacity, or −180° to +180° added rotation. These values modify the existing brush settings; opacity is clamped at 100%. Density changes placement frequency; size also affects the existing size-relative spacing.

See the [Rust custom parameter implementation notes](../../docs/custom-parameter-curves.md) for the UI, serialization, and Windows PiPL setup.

## UI and compatibility

`Texture Time` stays visible when `Texture Layer` is `None`; its controls are disabled until a texture is selected. Mode-specific timing fields appear when applicable. `Brush Stamp` contains `Stamp Order`, followed by `Size`, `Spacing`, `Rotation`, `Opacity`, and `Fallback Brush`. Fallback controls appear when no texture is selected. Mask selection and feather scaling appear when applicable; feather debug logging is hidden from the normal UI.

Version 0.3.0 removes `Side Color`, `Stroke Side`, and the associated half-stroke edge softness. These retired settings no longer affect rendering. Retained parameter IDs, the effect Match Name, and the first two Path Source/Output choices are preserved so their saved values and animation survive the UI reordering. New instances default to automatic path selection; existing projects retain their saved path selection. Corrected overlap and direction-offset behavior may change previously affected renders.

Version 0.4.0 extends Auto/Shape Paths to collapsed precomps and adjustment layers without adding or renumbering parameters. Geometry read from other layers is included in AE's render-cache identity, so editing those paths updates the output without manually purging the cache.

Version 0.5.0 adds optional dynamics while retaining existing parameter IDs. Existing effects have dynamics disabled. The saved Random mode is intentionally updated to the fixed layer-range behavior; its previous relative time range no longer applies.

## Verification

`cargo test -p texture_stroke` covers overlapping/transparent stamps, both stamp orders, front/behind compositing, alpha blending, cropped buffers, reduced resolution, independent paths, and width/rotation edge cases. Run `tests/ae_smoke_test.jsx` in After Effects with the built plugin installed to render host fixtures under the temporary directory's `texture-stroke-smoke` folder. Set `$.global.TEXTURE_STROKE_TEST_OUTPUT` before running to choose an output directory. The script restores the project bit depth and removes only its temporary fixture folder.

Run `python tests/verify_ae_smoke.py <fixture-output-directory>` (requires Pillow) to compare the full rendered contours of a polygon and asymmetric/translated Bezier curves against AE's native strokes. This detects incorrect relative-handle conversion even when the output bounding boxes match. Version 0.2.1 corrects this conversion for Bezier shape paths.

Set `$.global.TEXTURE_STROKE_TEST_OUTPUT` to a directory in your worktree and run `tests/ae_collapsed_shapes.jsx`, then `python tests/verify_ae_collapsed.py <fixture-output-directory>`. These fixtures compare transformed/nested precomps, time offsets/stretch/remapping, visibility/solo, adjustment layers, and edits to otherwise transparent source paths against native AE strokes.

Run `tests/ae_dynamics.jsx` with the same output variable, followed by `python tests/verify_ae_dynamics.py <fixture-output-directory>`, for random timing, map coordinates, cropped masks, reduced resolution, and path metrics. This script retains its fixture folder and saves `dynamics.aep` for manual curve editing and Undo/save tests. On Windows, `python tests/verify_pipl.py <aex-or-dll> <OUT_DIR/texture_stroke.pipl>` verifies the embedded binary resource.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
