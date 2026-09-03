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

現在のカタログには、写真・アニメ素材・データ画像・生成用途を横断する38種類のエフェクトを収録しています。
The current catalog contains 38 effects for photo, anime, data/map, and generator workflows.

## Effect List / エフェクト一覧

[![All Effects / 全エフェクト一覧](docs/catalog/assets/overview/all-effects.png)](docs/catalog/README.md)

- [Catalog Index / カタログ索引](docs/catalog/README.md)
- [P: Photo / 一般写真・連続階調画像](docs/catalog/photo.md)
- [A: Anime / アニメ素材](docs/catalog/anime.md)
- [D: Data & Map / データ・マップ画像](docs/catalog/map-data.md)
- [G: Generator / 生成用キャンバス](docs/catalog/generator.md)
- [Releases / ダウンロード](https://github.com/Aodaruma/aod-AE-plugin/releases)

## 1. Issue / バグ報告

もしバグを見つけた場合は、[Issues](https://github.com/Aodaruma/aod-AE-plugin/issues) ページで報告してください。

If you find a bug, please report it on the [Issues](https://github.com/Aodaruma/aod-AE-plugin/issues).

## 2. Support / 支援

> [!NOTE]
> もしこのプロジェクトが役に立ったら、GitHub Sponsors での支援をご検討ください。  
> If this project helps you, please consider supporting it via GitHub Sponsors.

https://github.com/sponsors/Aodaruma

## 3. License

ライセンスはMPL-2.0です。`LICENSE` ファイルを参照してください。

Licensed under the MPL-2.0. See `LICENSE`.

---

## 4. For Developers / 開発者向け情報

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
