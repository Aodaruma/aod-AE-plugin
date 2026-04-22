# datacode-encode ( AOD_DatacodeEncode )

Encodes 1D and 2D data codes from a Liquid template with dynamic UI parameters.

This is the After Effects plugin **AOD_DatacodeEncode**, which provides the **DatacodeEncode.aex** plugin file for Adobe After Effects.

## Payload Input (Liquid)

1. In Effect Controls, click **Edit...** on **Edit Liquid**.
2. Edit Liquid template text in the ScriptUI editor dialog and press **OK**.
3. Choose payload interpretation in **Payload Mode**:
   - `UTF-8 String`: rendered text is encoded as UTF-8 bytes.
   - `HEX Binary`: rendered text is parsed as hexadecimal bytes.

Template variables:

- Input mode: `input_mode` (`utf8` / `hex`)
- Time: `current_time`, `time_step`, `time_scale`, `time_seconds`, `frame`, `frame_index`
- Transform: `origin_x`, `origin_y`, `origin_direction`, `cell_pixel_size`, `cell_width`, `cell_height`
- Text helpers: `newline`, `tab`
- Paths: `template_path`, `template_dir` (reserved helper values)
- Optional external sources from plugin config directory:
  - `file_utf8` / `source_utf8` from `payload.txt`
  - `file_hex` / `source_hex` from `payload.hex`
  - `file_bin_hex` / `source_bin_hex` from `payload.bin` (auto-converted to hex)

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.

