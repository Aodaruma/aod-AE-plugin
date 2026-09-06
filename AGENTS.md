# aod-AE-plugin 開発ガイド（AGENTS）

Rust 製の Adobe After Effects エフェクトプラグイン集（Cargo workspace）です。
`plugins/` は各プラグイン、`crates/utils/` は共通処理、`templates/plugin/` は生成用テンプレートです。

## 実装と互換性

- リポジトリに追加するプラグインは `cargo new-plugin` で生成する。生成・ビルド手順は [開発手順](docs/development.md) を参照。
- リポジトリ内の命名は、ディレクトリが kebab-case、crate が snake_case、AE 表示名が `AOD_` + PascalCase、Match Name が PascalCase、Category が `Aodaruma`。この命名規則は外部向け生成物には強制しない。
- 既存の `AE_Effect_Match_Name` は AE プロジェクトの互換性に関わるため、原則変更しない。
- Cargo の `description`、`PLUGIN_DESCRIPTION`、プラグイン README の概要には、同じ短い英語の1文を使い、末尾をピリオドにする。カタログには同じ意味の日本語説明と主要パラメーターを記載する。
- 共有処理は `crates/utils/` を優先利用する。
- 動的 UI を変更する場合は、[動的 UI の実装ルール](docs/development.md#動的-ui) を参照する。

## ライセンス

- 境界と投稿条件は [LICENSING.md](LICENSING.md) / [CONTRIBUTING.md](CONTRIBUTING.md) に従う。
- 既存プラグインと utils は MPL-2.0。生成用の追加許諾は列挙された素材だけに適用され、コードの移動だけで範囲は広がらない。MPL のみの実装をテンプレートへ移す場合は、追加許諾できる権利を確認する。

## 検証と完了条件

変更の影響に応じて次を実行する（複数該当時は組み合わせる）。必要な検証が通れば、新たな変更・失敗・未解決の懸念がない限り繰り返さない。

| 変更対象 | 必要な確認 |
|---|---|
| 文書のみ | 記述の整合性、変更したリンク、`git diff --check` |
| 単一プラグイン内の Rust・依存関係 | `cargo fmt --all -- --check`、`cargo clippy -p <crate_name>`、`cargo test -p <crate_name>` |
| utils・workspace 共通の Rust・依存関係 | `cargo fmt --all -- --check`、`cargo clippy --workspace`、`cargo test` |
| テンプレート・ライセンス同梱処理 | `python -m unittest discover -s tests -p "test_licensing.py"`（生成4構成のビルドを含む） |
| ビルド・CI 設定 | 変更したコマンド・対象環境で影響する経路を確認 |
| 描画・UI・ホスト連携 | 上記の該当検証に加え、`tester/` の .aep などで After Effects 上の挙動を確認。Premiere 固有の変更は同ホストでも確認 |

- `just build` / `just release` は既定で Adobe 共通フォルダーへインストールする。ビルド検証のみの場合は [NO_INSTALL の指定方法](docs/development.md#ビルドとインストール) を使う。
- プラグイン追加・描画結果の変更時は、[カタログ更新手順](docs/catalog/README.md#カタログ更新の対象) に従う。実際の AE 出力に合わせて説明・個別プレビュー・分類別一覧・全体一覧を同じ変更内で更新する。
- 実行できない検証は、その理由と未確認の範囲を完了報告に記載する。

## ブランチと PR

- `main` / `dev` への直接コミットは禁止し、PR 経由で統合する。
- 通常開発（機能追加・通常の不具合修正・文書・保守）は、fetch 済みの最新 `origin/dev` からトピックブランチを作成し、`dev` へ PR を作成する。
- 原則1トピック1ブランチ。同じ依頼の続きは既存の作業ブランチで進め、無関係な変更を混在させない。調査・レビューだけならブランチや PR は不要。
- 他の作業の未コミット変更を保護し、必要なら worktree を作成する。分岐のために別のチェックアウトを `dev` へ切り替える必要はない。
- コミット・PR 作成をユーザーが依頼または承認済みなら、再確認せず実装・検証後に進める。未承認なら、差分と検証結果を用意してから確認する。PR 作成の依頼だけでマージまで承認されたとは扱わない。
- コミットは `TAG: Summary`（英語1行）。TAG は `ADD` / `REFACTOR` / `CHORE` / `FIX` / `DOCS` を使う。
