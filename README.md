# aod-AE-plugin

[![CI](https://github.com/Aodaruma/aod-AE-plugin/actions/workflows/ci.yml/badge.svg)](https://github.com/Aodaruma/aod-AE-plugin/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/Aodaruma/aod-AE-plugin)](https://github.com/Aodaruma/aod-AE-plugin/releases/latest)
[![Pre-release](https://img.shields.io/github/v/release/Aodaruma/aod-AE-plugin?include_prereleases&label=pre-release)](https://github.com/Aodaruma/aod-AE-plugin/releases)

Aodaruma 開発による、Rust で書かれた Adobe After Effects プラグイン集です。
複数のAEエフェクトプラグインを、テンプレートを用いて構築・量産、自動でMacOS/Windows向けにビルド・リリースします。

A collection of Adobe After Effects plugins written in Rust, developed by Aodaruma.
This repository is a Cargo
workspace that builds multiple AE effect plugins, plus shared utilities and a plugin
template.

## 1. Plugins / プラグイン説明

リリース済みのプラグインは、[Releases](https://github.com/Aodaruma/aod-AE-plugin/releases) ページからダウンロードできます。

You can download released plugins from the [Releases](https://github.com/Aodaruma/aod-AE-plugin/releases) page.

- AOD_ColorAjust
  - OKLCH/HSLで色相・彩度・明度を調整します / Adjusts hue, chroma, and lightness in OKLCH or HSL color spaces
- AOD_ColorChange:
  - 指定色を別の色に置換します / Changes a specific color to another color with tolerance
    
## 2. Issue / バグ報告

もしバグを見つけた場合は、[Issues](https://github.com/Aodaruma/aod-AE-plugin/issues) ページで報告してください。

If you find a bug, please report it on the [Issues](https://github.com/Aodaruma/aod-AE-plugin/issues).

## 3. License

ライセンスはMPL-2.0です。`LICENSE` ファイルを参照してください。

Licensed under the MPL-2.0. See `LICENSE`.

---

## 4. For Developers / 開発者向け情報

以下は開発者向け情報です。プラグインのビルド方法や新規プラグインの作成方法を説明します（英語のみ）

### Build and install

Prerequisites:

- Rust (stable)
- just (recommended)

Build all plugins:

```sh
just build
just release
```

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
