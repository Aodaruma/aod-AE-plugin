# texture-stroke ( AOD_TextureStroke )

Generates textured strokes from mask and shape paths.

This is the After Effects plugin **AOD_TextureStroke**, which provides the **AOD_TextureStroke.aex** plugin file for Adobe After Effects.

## Paths

- `Auto (Shape / Mask)` uses Contents paths when the effect is applied to a shape layer, and mask paths on other layers.
- `All Mask Paths` and `Selected Mask Path` explicitly use masks, including on shape layers. `Shape Paths` explicitly uses Contents.
- Shape support includes open/closed Bezier paths, rectangles (including roundness), and ellipses, with nested group transforms and 2D layer transforms. Disabled paths and groups are skipped. Animated values and expressions are sampled at the render time.
- The effect reads source paths. Shape operators such as Trim Paths, Repeater, Merge Paths, and path distortion are not evaluated. Convert polystars to Bezier paths first. Camera-projected 3D shape layers are not supported; precompose them or use a 2D path layer.
- Shape paths use `Stroke Width`; mask feather width settings apply to masks. Fill and native stroke styles do not define the texture brush.

## Brush and output

- Set `Texture Layer` to use an image or animated layer as the brush. Without it, the built-in circle/square brush is used.
- `Output` defaults to `Composite (Front)`, placing the stroke over the input image. `Composite (Behind)` places it behind the input, visible through transparency. `Stroke Only` accumulates stamps on transparency. Transparent brush pixels preserve earlier stamps.
- `Stamp Order` chooses `Start to End` (default) or `End to Start` within each path. Later stamps cover earlier ones. Reversing the order preserves each stamp's position, random variations, texture frame, and the stacking order between independent paths.
- `Stroke Opacity` controls stamp opacity. `Base Direction Offset` rotates the brush; `Reverse Direction` reverses its orientation. These controls do not change stamp stacking.
- `Texture Time` offers `Current`, `Fixed Frame`, `Along Stroke`, and `Random Per Stamp`. Along/random sampling uses the time range around the current frame; `Random Time Seed` makes the random selection repeatable.
- Paths and brush widths remain in the same layer coordinates when masks crop the input or preview resolution changes. Smart Render includes the generated stroke bounds, even outside the input alpha bounds.
- Empty paths or zero width produce transparency in `Stroke Only`. A zero base width can still use a nonzero mask feather width.

## UI and compatibility

`Texture Time` stays visible when `Texture Layer` is `None`; its controls are disabled until a texture is selected. Mode-specific timing fields appear when applicable. `Brush Stamp` contains `Stamp Order`, followed by `Size`, `Spacing`, `Rotation`, `Opacity`, and `Fallback Brush`. Fallback controls appear when no texture is selected. Mask selection and feather scaling appear when applicable; feather debug logging is hidden from the normal UI.

Version 0.3.0 removes `Side Color`, `Stroke Side`, and the associated half-stroke edge softness. These retired settings no longer affect rendering. Retained parameter IDs, the effect Match Name, and the first two Path Source/Output choices are preserved so their saved values and animation survive the UI reordering. New instances default to automatic path selection; existing projects retain their saved path selection. Corrected overlap and direction-offset behavior may change previously affected renders.

## Verification

`cargo test -p texture_stroke` covers overlapping/transparent stamps, both stamp orders, front/behind compositing, alpha blending, cropped buffers, reduced resolution, independent paths, and width/rotation edge cases. Run `tests/ae_smoke_test.jsx` in After Effects with the built plugin installed to render host fixtures under the temporary directory's `texture-stroke-smoke` folder. Set `$.global.TEXTURE_STROKE_TEST_OUTPUT` before running to choose an output directory. The script restores the project bit depth and removes only its temporary fixture folder.

Run `python tests/verify_ae_smoke.py <fixture-output-directory>` (requires Pillow) to compare the full rendered contours of a polygon and asymmetric/translated Bezier curves against AE's native strokes. This detects incorrect relative-handle conversion even when the output bounding boxes match. Version 0.2.1 corrects this conversion for Bezier shape paths.

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
