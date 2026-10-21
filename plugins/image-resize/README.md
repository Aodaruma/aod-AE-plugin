# AOD_ImageResize

Crops or extends image edges or a fixed-size canvas without scaling, with boundary sampling and rotation.

画像の上下左右をピクセル数または%で拡張・クリッピングします。画像自体の拡大縮小は行わず、各辺の位置を変更します。

## パラメーター

| 項目 | 初期値 | 動作 |
|---|---:|---|
| Units | Percent | 上下左右の共通単位。PixelsまたはPercent。 |
| All % | 100 | 上下左右の調整量に掛ける係数。0で無効、50で半分、100で指定値どおり。 |
| Top / Bottom / Left / Right | 0 | 正値で内側へクリップ、負値で外側へ拡張。小数値も指定可能。 |
| Extend | None | 拡張領域の描画方法。None＝透明、Mirror＝鏡像反復、Tile＝繰り返し、Clamp＝端の色を引き伸ばす。 |
| Angle | 0° | 調整後の画像全体を元のレイヤー中心の周りに回転。正値は時計回り。All %とは独立。 |
| Interpolation | Bilinear | 回転時などの補間。NearestまたはBilinear。 |
| Set Output Size | Off | 指定した幅・高さの出力枠でクリップ／拡張します。画像の拡大縮小は行いません。 |
| Output Width (px) | 1920 | 指定出力幅。Set Output SizeがOnのとき有効。 |
| Output Height (px) | 1080 | 指定出力高さ。Set Output SizeがOnのとき有効。 |

## %とAll %の計算

各辺の元の位置が0%、反対側の元の辺が100%です。左右は元のレイヤー幅、上下は元のレイヤー高さを基準とします。

- 幅1920pxでLeft＝25%なら、左から480pxをクリップします。
- Left＝−25%なら、左側に480px拡張します。
- 同じ設定でAll %＝50%なら、240pxのクリップ／拡張になります。
- All %＝0%では4辺とも調整量0になります。Angle＝0°かつSet Output Size＝Offなら元画像をそのまま出力します。
- Pixelsでは各辺の指定ピクセル数にAll % / 100を掛けます。
- Set Output Size＝Offで左右または上下のクリップが交差した場合は、全面透明になります。

## 指定ピクセル数でのクリップ・拡張

Set Output SizeをOnにすると、画像の倍率を維持して、指定した幅・高さの枠を出力します。80×40pxの画像に40×20pxを指定すると中央を切り抜き、120×60pxを指定すると周囲を拡張します。拡張部分はExtendで指定した方式で描画します。

幅・高さは上下左右で求めた寸法より優先されます。上下左右・All %は枠の中心位置を決めるために使い、Angleで回転した後の中心に指定サイズの水平・垂直な枠を置きます。左右対称／上下対称の指定では中心は動きません。例えば80px幅の画像でLeft＝25%、All %＝100なら、中心は右に10px移ります。画素自体は移動・拡大縮小しません（Angleによる回転を除く）。

All %＝0%でも指定サイズとAngleは有効です。幅・高さは1～30000pxの整数として扱い、小数入力やキーフレーム間の値は四捨五入します。奇数サイズなどで枠の端が半端になる場合は、左上を元解像度のピクセル境界へ切り下げます。プレビュー解像度を下げても元解像度の枠位置を維持します。

基準はエフェクト適用前の元レイヤーの寸法です。前段エフェクトやマスクで透明になった余白も寸法に含み、前段エフェクトが元レイヤーの外に生成した画素は参照しません。MirrorとTileの反復周期にも元レイヤーの寸法を使います。

拡張・回転に必要な出力領域をAEへ通知します。クリップしても残った画素のレイヤー座標は維持します。コンポジションのサイズとレイヤーのTransformは変更しません。コンポジション外の結果を見る場合は、十分なサイズのコンポジション内に配置してください。

8／16／32bpc、Smart Render、マルチフレームレンダリングに対応します。回転はピクセル縦横比を考慮し、プレビュー解像度に応じて指定量を換算します。32bpcのRGB値は0～1に制限しません。出力が1辺30000pxを超える設定はエラーになります。

## ビルド・確認

リポジトリ直下で実行します。

```powershell
cargo test -p image_resize
cargo clippy -p image_resize
$env:NO_INSTALL = '1'
just -f plugins/image-resize/Justfile release
```

Windows用ファイルは`target/release/AOD_ImageResize.aex`です。AEのプラグインフォルダーに配置後、AEを再起動すると`Aodaruma > AOD_ImageResize`から選べます。

`tests/smoke.jsx`は、空のプロジェクトで起動した別プロセスのAEで描画テストを実行し、`target/ae-smoke/`にPNG・検証用AEP・ログを保存します。作業中のプロジェクトが開いている場合は実行を拒否します。

```powershell
& 'C:/Program Files/Adobe/Adobe After Effects 2025/Support Files/AfterFX.com' -m -noui -r "$PWD/plugins/image-resize/tests/smoke.jsx"
# PNGの生成完了後、Pillowを導入済みのPythonで検証
python plugins/image-resize/tests/verify_smoke.py
```

## ライセンス

本プラグインの実装と`utils`はMPL-2.0です。生成元の雛形に対する追加許諾は[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)に記載された素材に限られ、本プラグイン固有の実装には適用されません。
配布条件は[ライセンスガイド](../../LICENSING.md)と[投稿条件](../../CONTRIBUTING.md)を参照してください。
