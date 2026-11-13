# AOD_SeamScale v0.1.3 検証記録

実施日: 2026-10-14（日本時間）。ブランチ: `feat/seam-scale`。

この記録はv0.1.3の実機検証です。後続v0.1.4の比較は[性能検証記録](PERFORMANCE.md)、最新v0.1.5の配置状況と4K比較は[4K性能検証記録](PERFORMANCE_4K.md)を参照してください。

## 起動エラーの修正

AEの「effect AOD_SeamScale is missing a required PiPL property」（25::31）は、PiPLをWindowsリソースの文字列に変換する際のバイト破損が原因でした。生成元には必須項目がありましたが、埋め込み後は途中のバージョン値と後続のレコード境界が変わっていました。

本プラグインのWindowsビルドでPiPLをバイナリファイルとして直接埋め込むよう変更しました。埋め込み後の全346バイト・13項目を確認し、AE 2025の正常起動とエフェクト適用を確認しています。PiPLとGlobalSetupのフラグを揃え、SupportsGetFlattenedSequenceDataも宣言しています。

## CLI検証

- `cargo fmt --all -- --check`: 成功。
- `cargo clippy -p seam_scale --all-targets -- -D warnings`: 成功。
- `cargo test -p seam_scale`: 21件成功。計測専用の2件は通常実行から除外。
- `cargo build -p seam_scale --release`: 成功。Windows x86-64向け。
- `benchmark_video_1080`: v0.1.2でrelease実行済み。全体DPと出力画素が完全一致。v0.1.3の変更はPiPLの埋め込み・メタデータで、シーム計算は同じです。

AVX2と通常DP、各シーム除去後のリンク構造版と全体DP、密な更新時の切替、全経路探索との一致を確認しています。保持するDP結果は同一フレーム内だけです。

全7チャンネルの特徴量を独立した計算式と照合しました。400%までの段階的な拡大、X/Y拡縮混在、単一画素幅、両軸の反転、復元ON/OFF、負の倍率・0%・非有限値も確認しました。既定RGBAは従来のLightness＋Alphaと同じ計算です。Windowsの新しい回帰テストは、実行ファイルに格納されたPiPLと生成バイト列の完全一致を検証します。

## AE 2025での検証

After Effects 25.6.6x4。ユーザーの明示的な許可に基づき、Computer Useで検証用AEを操作しました。AE MCPも接続・実行を試みましたが、既存ブリッジのスケジューラー停止とスクリプトエラーにより結果が返らず、テストJSXをAEの「スクリプトファイルを実行」から実行しました。MCP経由の成功とは扱っていません。ブリッジのインストール済みファイルやセキュリティ設定は変更していません。

`tests/ae_smoke_test.jsx`の156枚のPNGを`tests/verify_ae_smoke.py`で比較し、8/16/32bpcすべて成功しました。

- 100%の画素一致、共通倍率と同値のX/Y個別倍率の一致。
- Xのみの縮小、元サイズ復元、拡大・縮小の混在、単色画像の400%拡大。
- 全7チャンネルの縮小・拡大。異なるチャンネルで結果が変わること。
- 負のX、Y、両軸が正の倍率の出力の反転と完全一致すること。
- RGBAとLightnessが完全不透明な入力では一致すること。
- 0%が最小1pxになり、縮小時の透明余白が正しいこと。
- ネストした切り抜き、マスクの直接適用とプリコンポーズ、半解像度。

Computer Useでは、Separate X/Y ScaleのON/OFFで使用する倍率欄だけが有効になること、7種類のSeam Channel、負の倍率の入力表示と描画を確認しました。

写真を入力にした個別プレビュー、Photo一覧、全体一覧もAEで出力しました。カタログ用AEPにSeam Scaleコンポジションを追加し、既存39エフェクトのPNGを利用して一覧を更新しています。

## 1920×1080・共通倍率50%の性能

Intel Core i9-14900KF、Windows、AVX2、既定RGBA（Lightness＋Alpha）。入力素材により時間が変わります。

### AE内の合成動画

8bpc、フル解像度、Restore Original Size=OFF。グラデーション背景と動く12本の図形を、異なる時刻の3フレームとしてレンダーキューから出力しました。PNGは1920×1080で、画像の領域が中央の960×540であること、3フレームの画素が異なることを確認しました。

- 3フレーム合計: 3.640秒。
- 平均: 約1.213秒/フレーム（素材の描画・PNG書き出し・キューの準備処理を含む）。
- AEのレイヤーRender Time表示: 約1.05秒（表示した1フレーム）。

出力テンプレートはこのPCの`PNG seq (untone-map)`（RGB＋Alpha、16bit PNG、リサイズなし）です。処理プロジェクトは8bpcです。`saveFrameToPng`の呼び出し時間は実際の描画完了時間にならなかったため採用せず、レンダーキュー完了までの時間を計測しています。

ユーザーの8.56秒というスクリーンショットとは素材・条件が異なるため、高速化倍率は算出できません。動画のリアルタイム処理には達していません。

### CLIの参考値

Rust 1.97.1。1920×1080の合成画像を960×540へ縮小するシーム処理単体で、AEの入出力・動画デコードは含みません。各版は別時点での単回計測です。

| 入力 | v0.1.1 | v0.1.2（v0.1.3と同じ計算） |
| --- | ---: | ---: |
| 単色 | 0.179秒 | 0.167秒 |
| グラデーション | 2.326秒 | 1.950秒 |
| 図形＋グラデーション | 1.853秒 | 1.566秒 |
| ノイズ | 1.914秒 | 1.805秒 |

再現: `cargo test -p seam_scale --release benchmark_video_1080 -- --ignored --nocapture`

## 未確認の範囲

- 修正版のAE 2026、Mac向けビルド・実機動作。
- ユーザーの既存プロジェクト・実動画での速度と互換性、長尺・大量並列レンダー。
- 32bpcのAE出力をHDRファイルの数値として比較する検証。今回はPNGの比較であり、HDR値の保持はRust側で検証しています。
- AE MCP経由のテスト実行完了。

## 成果物と配置

検証時の`target/release/AOD_SeamScale.aex`は263168 bytesでした。AEが終了していることを確認し、既存ファイルをworktree内にバックアップしてから配置しました。配置後のSHA-256は当時のビルド成果物と一致しています。現在のreleaseフォルダーは後続版のビルドに使っています。v0.1.3のバイナリは`target/seam-scale-perf-0.1.4/AOD_SeamScale-0.1.3.aex`にも保存しています。

配置先: `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\AOD_SeamScale.aex`

v0.1.3のSHA-256:

```text
546F0FA9387F05465AE0327873A81DA1AC576AB902FFF91E62F13C7F557E38A9
```

バックアップは`target/seam-deployment-backups/`です。元のv0.1.0は`AOD_SeamScale-before-0.1.2-20261014-044701.aex`、置換直前の試作版は`AOD_SeamScale-before-pipl-binary-fix-20261014-053634.aex`にあります。

描画テストのPNG・ログ・AEP、計測結果は`target/seam-scale-ae-0.1.3/`に保存しました。初回の計測試行には失敗記録があり、採用した完了結果は`benchmark-render.txt`です。すべてこのworktree内です。この検証の終了時点では、アプリは保存済みの検証用`seam-scale-benchmark.aep`を開いた状態でした。
