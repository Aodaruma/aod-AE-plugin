# 開発手順

## プラグインの生成

リポジトリへ追加するプラグインは、ルートで `cargo new-plugin` を実行します。
[Cargo エイリアス](../.cargo/config.toml) が `repository_plugin=true` を指定し、`plugins/` 内に生成します。
外部向けの生成方法と配布条件は [README](../README.md#create-a-new-plugin) と [ライセンスガイド](../LICENSING.md) を参照してください。

生成後は `Cargo.toml` の description、`build.rs` の Name / Match Name、`src/lib.rs` の `PLUGIN_DESCRIPTION` を確認し、`cargo check -p <crate_name>` で置換漏れ・構文崩れを確認します。
リポジトリに追加する場合は [カタログ更新手順](catalog/README.md#カタログ更新の対象) に従って説明と画像も追加します。

非対話で生成する場合、`--values-file` に渡す TOML には `[values]` セクションが必要です。

```toml
[values]
description = "Short description."
features = []
with_deepcolor = true
with_thrededrender = true
with_smartrender = true
```

```sh
cargo new-plugin --name example-effect --values-file values.toml --silent
```

`[values]` がないと `missing field 'values'` で失敗します。
`with_thrededrender` は既存テンプレートのキー名なので、綴りを変更せず指定してください。
選択した機能に応じて必要な追加項目は [cargo-generate.toml](../templates/plugin/cargo-generate.toml) に定義されています。

## ビルドとインストール

Rust / cargo、cargo-generate、just を使用します。全体は `just build` / `just release`、単体は `just -f plugins/<name>/Justfile build` です。
これらは既定で Adobe Common Plug-ins フォルダーへインストールします。ビルド検証だけの場合は次のようにインストールを無効にします。

macOS / bash:

```sh
NO_INSTALL=1 just build
```

Windows / PowerShell:

```powershell
$env:NO_INSTALL = "1"
just build
```

PowerShell の指定は現在のセッションで有効です。通常のインストールへ戻す場合は `Remove-Item Env:NO_INSTALL` で解除します。
出力先は [README の Outputs](../README.md#build-and-install) を参照してください。

テンプレート・ライセンス同梱処理の検証には Python 3.12 と cargo-generate（CI は 0.23.7）も使用します。
[AGENTS.md の検証表](../AGENTS.md#検証と完了条件) に従い、変更内容に必要なチェックを実行してください。

## 動的 UI

AE / Premiere の動的 UI を変更するときは、次のホスト固有の制約を守ってください。

- `build.rs` の PiPL と `GlobalSetup` の両方で `OutFlags::SendUpdateParamsUI` を有効化する。
- UI 更新は `Command::UpdateParamsUi`、値変更は `Command::UserChangedParam` で行う。`UpdateParamsUi` 内では値を変更しない。
- `UserChangedParam` を受けるパラメータには `ParamFlag::SUPERVISE` を付ける。
- `set_ui_flag(...); update_param_ui();` は必要な場合だけ呼び、同値を再設定しない。
- 表示・非表示は、After Effects では AEGP Stream の `DynamicStreamFlags::Hidden`、Premiere では `ParamUIFlags::INVISIBLE` を使う。
- `PF_Param_NO_DATA`（`NullDef`）を未処理のまま ECW に出すと「Unsupported Effects Control」になりうるため、説明表示用途では原則使わない。既存パラメータ名の動的変更（`set_name`）を優先する。
