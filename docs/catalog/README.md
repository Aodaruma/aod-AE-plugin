# AOD After Effects Plugin Catalog

用途に合う入力素材からエフェクトを探せるよう、現在のリポジトリに含まれる38プラグインを4分類に整理しています。

| 分類 | 適した素材 | 収録数 | 詳細 |
|---|---|---:|---|
| P | Photo：一般写真・連続階調画像 | 17 | [Photo / 一般画像](photo.md) |
| A | Anime：アニメ素材（セル塗り・線画・透過キャラクター） | 13 | [Anime / アニメ素材](anime.md) |
| D | Data & Map：データ・マップ画像 | 4 | [Data & Map / データ・マップ](map-data.md) |
| G | Generator：生成用キャンバス | 4 | [Generator / 生成系](generator.md) |

分類は「最も分かりやすく効果を確認できる主入力」を示します。たとえば `AOD_ScatterMap` はP分類ですが、Amount Mapなどの補助入力としてD分類の画像を併用できます。

プレビューは効果の違いを見分けやすくするための強調設定で、デフォルト設定とは限りません。写真素材にはEmre Gencer氏のUnsplash写真を加工して使用しています。

## Overview

### P：一般写真・連続階調画像

[![P：一般写真・連続階調画像](assets/overview/photo.png)](photo.md)

### A：アニメ素材

[![A：アニメ素材](assets/overview/anime.png)](anime.md)

### D：データ・マップ画像

[![D：データ・マップ画像](assets/overview/map-data.png)](map-data.md)

### G：生成用キャンバス

[![G：生成用キャンバス](assets/overview/generator.png)](generator.md)

## カタログ更新の対象

プラグインを追加した場合、または描画結果・見た目に影響する変更を行った場合は、実際の After Effects 上の出力に合わせて次を更新します。

- 対応する `docs/catalog/*.md` の日本語説明、主要パラメーター、個別プレビュー
- `docs/catalog/assets/previews/<plugin>.png`
- 対応分類の一覧画像 `docs/catalog/assets/overview/<category>.png`
- 全エフェクト一覧画像 `docs/catalog/assets/overview/all-effects.png`
- 必要に応じて `docs/catalog/build-catalog.jsx` の分類、エフェクト一覧、プレビュー設定

個別プレビューを変更したら分類別・全体一覧も同じ変更内で再生成し、ルート README と詳細カタログに古い描画結果を残さないでください。

## カタログ画像の再生成

After Effects 2025で[`catalog/ae2025-plugin-catalog/aod-plugin-catalog-ae2025.aep`](../../catalog/ae2025-plugin-catalog/aod-plugin-catalog-ae2025.aep)を開き、`ファイル > スクリプト > スクリプトファイルを実行`から次の順に実行します。プラグイン追加時は、このAEPにプレビュー用コンポジションを追加してから再生成してください。

1. `run-build.jsx`: プレビューコンポジションと一覧グリッドを構築します。
2. `run-previews.jsx`: 更新対象エフェクトの個別PNGを保存します。
3. `run-overviews.jsx`: P／A／D／G分類と全エフェクト一覧のPNGを保存します。

生成先は`docs/catalog/assets/`です。素材プロジェクトに`Cat Crop 512x512`がない場合は、比較用の手続きテスト素材を自動生成します。
