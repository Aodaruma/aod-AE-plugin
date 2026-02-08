# aod-AE-plugin

[![CI](https://github.com/Aodaruma/aod-AE-plugin/actions/workflows/ci.yml/badge.svg)](https://github.com/Aodaruma/aod-AE-plugin/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/Aodaruma/aod-AE-plugin)](https://github.com/Aodaruma/aod-AE-plugin/releases/latest)
[![Pre-release](https://img.shields.io/github/v/release/Aodaruma/aod-AE-plugin?include_prereleases&label=pre-release)](https://github.com/Aodaruma/aod-AE-plugin/releases)
[![GitHub Sponsors](https://img.shields.io/badge/Sponsor-GitHub%20Sponsors-ff69b4?logo=githubsponsors)](https://github.com/sponsors/Aodaruma)

[Aodaruma](https://aodaruma.net/)によって開発された、Rust で書かれた Adobe After Effects プラグイン集です。
複数のAEエフェクトプラグインを、テンプレートを用いて構築・量産、自動でMacOS/Windows向けにビルド・リリースします。

A collection of Adobe After Effects plugins written in Rust, developed by [Aodaruma](https://aodaruma.net/).
This repository is a Cargo
workspace that builds multiple AE effect plugins, plus shared utilities and a plugin
template.

## 1. Plugins / プラグイン説明

> [!TIP]
> リリース済みのプラグインは [Releases](https://github.com/Aodaruma/aod-AE-plugin/releases) からダウンロードできます。  
> You can download released plugins from [Releases](https://github.com/Aodaruma/aod-AE-plugin/releases).

- AOD_ColorAjust
  - OKLCH/HSLで色相・彩度・明度を調整します / Adjusts hue, chroma, and lightness in OKLCH or HSL color spaces
- AOD_ColorChange:
  - 指定色を別の色に置換します / Changes a specific color to another color with tolerance
- AOD_ColorConvert
  - RGBと各色空間を相互変換します / Converts between RGB and multiple color spaces
- AOD_ContourGenerate
  - Canny法でレイヤーから輪郭線を抽出します / Extracts contour lines from a layer using the Canny method
- AOD_DifferentialGenerate
  - レイヤーの画像勾配からRGBA微分マップを生成します / Generates RGBA differential maps from image gradients.
- AOD_FFT
  - レイヤーに2D FFTを適用し、実部または虚部を出力します / Performs 2D FFT on RGBA channels and outputs real or imaginary spectra.
- AOD_DistanceGenerate
  - 色領域の輪郭から距離画像を生成します / Generates distance images from the contours of colored regions
- AOD_IFFT
  - 実部・虚部レイヤーから2D IFFTで画像を再構成します / Reconstructs an RGBA image from 2D FFT real and imaginary inputs.
- AOD_ImageCalculate
  - 1つまたは2つのレイヤーにBlender風の数式演算を適用します / Applies Blender-style math operations to one or two input layers.
- AOD_MobiusTransform
  - レイヤーにメビウス変換を適用します / Applies Mobius transformation to layers
- AOD_NormalGenerate
  - 色領域から法線マップを生成します / Generate a normal map from the color region.
- AOD_RegionColorize
  - 不透明または色領域をランダム・位置・インデックスで色分けします / Colors connected regions with random, positional, or index-based schemes.
- AOD_VoronoiGenerate
  - ボロノイテクスチャマップを生成します / Generates Voronoi texture maps.

## 2. Issue / バグ報告

もしバグを見つけた場合は、[Issues](https://github.com/Aodaruma/aod-AE-plugin/issues) ページで報告してください。

If you find a bug, please report it on the [Issues](https://github.com/Aodaruma/aod-AE-plugin/issues).

## 3. Support / 支援

> [!NOTE]
> もしこのプロジェクトが役に立ったら、GitHub Sponsors での支援をご検討ください。  
> If this project helps you, please consider supporting it via GitHub Sponsors.

https://github.com/sponsors/Aodaruma

## 4. License

ライセンスはMPL-2.0です。`LICENSE` ファイルを参照してください。

Licensed under the MPL-2.0. See `LICENSE`.

---

## 5. For Developers / 開発者向け情報

> [!NOTE]
> 以下は開発者向け情報です。利用のみの場合は上部のReleasesを参照してください（英語のみ）。
> 
> The following is for developers. If you only want to use the plugins, see the Releases section above.

### Build and install

Prerequisites:

- Rust toolchain and cargo
- cargo-generate
- just (recommended)

Build all plugins:

```sh
# for debug versions:
just build

# you can also build release versions:
just release
```

> [!WARNING]
> `just build` installs to the Adobe Common Plug-ins folder by default.  
> Skip installation with `NO_INSTALL=1 just build`.

By default the build installs to the Adobe Common Plug-ins folder. To skip installation:

```sh
NO_INSTALL=1 just build
```

Outputs:

- Windows: `target/debug/*.aex` or `target/release/*.aex`
- macOS: `target/debug/*.plugin` or `target/release/*.plugin`

You can also build a single plugin:

```sh
just -f plugins/color-ajust/Justfile build
```

### Create a new plugin

The repo includes a `cargo-generate` template:

```sh
cargo new-plugin

# or manually:
cargo generate --path templates/plugin --destination plugins
```

### Repository layout

- `plugins/`: each plugin crate
- `crates/utils/`: shared pixel conversion helpers
- `templates/plugin/`: plugin template for `cargo-generate`
- `tester/`: sample After Effects project for manual testing

### Contribution

Issues and pull requests are welcome. Please keep `cargo fmt` and `cargo clippy` clean when possible.
