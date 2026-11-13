# 検証記録（2026-10-20）

## 完了した確認

- `cargo new-plugin`（cargo-generate）で独立プラグインを生成しました。既存の `feat/seam-scale` worktreeを継続利用しています。
- `cargo fmt --all -- --check`: 成功。
- `cargo clippy --workspace --all-targets`: 成功。既存の他プラグインには警告があります。
- 変更した `utils`、`seam_map`、`seam_scale` は `cargo clippy ... --all-targets -- -D warnings` で警告なし。
- `cargo test --workspace`: 271成功、0失敗、4件除外（計測用など）。
- `cargo build --release -p seam_map -p seam_scale`: 成功。
- PiPLバイナリーの埋め込み一致を自動テストで確認しました。
- 複数倍率を共有計算した画素と、倍率ごとに独立してシーム計算した画素が一致します。全7チャンネル、縮小、4倍までの多段拡大、反転、中央配置、元サイズへの復元を含みます。
- 追跡した元画像の座標も、独立した計算と一致します。
- 黒〜白の倍率補間は直線のままです。アンカー0/30/50/100%、縮小・拡大・反転の混在、両軸、復元ON/OFFで固定画素の値を検証しました。
- 1点アンカーの平行移動、複数アンカー、画素間での指定輝度の交差、アンカーがない行を確認しました。

## 試作版のCLI速度

Releaseビルド、格子状の手続き画像、横方向のグレースケールマップ、Xが黒50%〜白150%、Yが100%、Restore OFF、RGBAです。各アンカー設定を1回ずつ計測しています。入力作成・ファイル保存は計測外です。動画・AEの実測ではありません。

正確な数値はworktree内の以下のログにあります。

- `target/seam-map-development/benchmark-1080.txt`
- `target/seam-map-development/benchmark-4k.txt`

1920×1080ではアンカー30%で1.444秒、70%で1.485秒でした。4Kではアンカー30%で16.456秒、70%で16.415秒です。試作段階では通常のSeamScaleより重く、4Kの実用速度には改善の余地があります。

## 成果物とホスト検証

- `target/release/AOD_SeamMap.aex`: 300,544 bytes。
- SHA-256: `71AF0B683643B052EC2A887C67D0EF3D640197605C71F500F3C589424DF66127`。
- `target/seam-map-development/preview/comparison.png`: 計算部分で作成した描画例。AE出力ではありません。
- `tests/ae_input_map.jsx`: AE 2025で8/16/32bpcの33画像を生成した検証スクリプト。
- `tests/verify_input_map.py`: 27設定のマップ参照・アンカー画素・無指定時の原画保持・X/Y個別設定を16bit PNGの値で確認した検証スクリプト。

ユーザーの許可とAE終了を確認したうえで、Adobe共通プラグインフォルダーへ配置しました。AE 2025（25.6.6x4）でロード・マップ参照・27設定の描画に成功しました。固定領域は各設定2,048画素すべてで元画像と一致し、固定領域外に変形があることも確認しました。マップ未指定と100%設定は全6,144画素が一致しました。

Computer Useでパラメーター表示とSeparate X/Y Scaleの切り替えを確認しました。AE実出力で個別プレビュー、Photo一覧、全体一覧を更新しています。検証プロジェクト・出力・照合JSONは `target/seam-map-development/ae2025/` にあります。既存のインストール済みAOD_SeamScaleは変更していません。

CLIの速度は単一フレームの計算時間です。AEでのHD/4K動画の処理時間、長尺の時間方向の安定性、macOSは今回未確認です。
