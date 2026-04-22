# Data Code Plugins Plan

## Scope

- Add two plugins from template:
  - `plugins/datacode-encode` (`AOD_DatacodeEncode` / `DatacodeEncode`)
  - `plugins/datacode-decode` (`AOD_DatacodeDecode` / `DatacodeDecode`)
- Implement dynamic UI (`UpdateParamsUi` + `UserChangedParam` + `SUPERVISE`) and host-aware visibility handling (AE dynamic stream / Premiere `INVISIBLE`).

## Encoder

### Required controls

- Common: `cell pixel size`, `cell width`, `cell height`, `origin pos`, payload mode (UTF-8 / HEX), payload arbitrary parameter.
- Type-dependent: show/hide controls per `datacode type`.

### Implemented code types

- 1D:
  - JAN/EAN (EAN-13 path)
  - CODE39
  - CODE128
  - Custom 1D (plugin-local encoder)
- 2D:
  - QR Code
  - Data Matrix
  - PDF417
  - iQR (fallback strategy to QR/rMQR path)
  - rMQR
  - Color code (JAB-like plugin-local embedding)
  - Just embedding (plugin-local embedding, monochrome/color, optional recovery)

### Library policy

- Prefer existing libraries:
  - `rxing` for encode/decode of common 1D/2D formats.
  - `qrqrpar` for rMQR.
- If no practical library path, implement plugin-local module logic:
  - Custom 1D, JAB-like color embedding, Just embedding, iQR fallback policy.

### Payload parameter

- Use arbitrary parameter (`ArbitraryDef`) as custom payload store (UTF-8 + HEX fields).
- Keep payload as parameter data (not popup dialog input).

### Error rendering

- No modal dialog on validation/size error.
- Draw in-frame error panel:
  - white/black double outline rectangle in target region
  - red monospaced bitmap text rendered from built-in glyph table (font file not required)

## Decoder

### Required controls

- Detection mode:
  - Auto detect
  - Region-based detect
- Optional render of decoded text/binary on frame.
- Decode dump parameter:
  - Arbitrary parameter used as non-editable dump target (best effort; UI disabled)

### Behavior

- Decode from current frame luminance buffer via `rxing`.
- In auto mode, detect on full frame.
- In region mode, crop and detect inside user region.
- Store decode summary (status/format/text/hex) into dump parameter when available.
- Optional on-frame render of decoded contents with built-in bitmap glyph renderer.

## Notes

- iQR is implemented as fallback behavior because robust Rust-native iQR encoder support is limited.
- Color code entry uses a JAB-like plugin-local strategy to keep build portability and avoid external runtime font requirements.

