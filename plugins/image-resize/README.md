# AOD_ImageResize

Crops or extends image edges or a fixed-size canvas without scaling, with boundary sampling and rotation.

画像の上下左右をピクセル数または%で拡張・クリッピングします。画像自体の拡大縮小は行わず、各辺の位置を変更します。

## パラメーター

| 項目 | 初期値 | 動作 |
|---|---:|---|
| Mode | Percent | Pixels／Percent／Output Size。Output Sizeでは幅・高さを指定してクリップ／拡張します。 |
| All % | 100 | 上下左右の調整量に掛ける係数。0で無効、100で指定値どおり、300で3倍。負値は方向を反転。入力範囲は−30000～30000。 |
| Top / Bottom / Left / Right | 0 | 正値で内側へクリップ、負値で外側へ拡張。Modeに応じて末尾が(px)／(%)に切り替わります。小数値も指定可能。 |
| Extend | None | 拡張領域の描画方法。None＝透明、Mirror＝鏡像反復、Tile＝画像全体の繰り返し、Repeat＝最も近い端の画素を引き伸ばす。 |
| Angle | 0° | 調整後の画像全体を元のレイヤー中心の周りに回転。正値は時計回り。All %とは独立。 |
| Interpolation | Bilinear | 回転時の補間。NearestまたはBilinear。Angleが360°の倍数以外のときに表示。 |
| Output Width (px) | 1920 | 指定出力幅。Mode＝Output Sizeのときだけ表示。 |
| Output Height (px) | 1080 | 指定出力高さ。Mode＝Output Sizeのときだけ表示。 |

幅・高さは`Output Size`、上下左右は`Edges`グループにまとめ、両グループとも初期状態で展開します。Output Sizeモードでは`Output Size`を`Edges`と同じ設定領域に表示し、上下左右のグループ名を`Center Adjustment`に切り替えます。このモードでも上下左右は枠の中心調整に使うため表示します。

Interpolationは画像を拡大縮小する設定ではなく、回転後の位置が画素の間に来る場合の補間方法です。Nearestは画素を明確に保ち、Bilinearは階段状の輪郭を滑らかにします。非回転時は結果に影響しないため非表示にし、選択値は保持します。

v0.3では旧`Clamp`を`Repeat`へ改名しました。保存済みの選択と描画結果は維持します。

## %とAll %の計算

各辺の元の位置が0%、反対側の元の辺が100%です。左右は元のレイヤー幅、上下は元のレイヤー高さを基準とします。

- 幅1920pxでLeft＝25%なら、左から480pxをクリップします。
- Left＝−25%なら、左側に480px拡張します。
- 同じ設定でAll %＝50%なら、240pxのクリップ／拡張になります。
- All %＝300%なら指定量の3倍、−100%なら同じ量でクリップ／拡張を反転します。0～100%への制限はありません。
- All %＝0%では4辺とも調整量0になります。Angle＝0°かつMode＝Pixels／Percentなら元画像をそのまま出力します。
- Pixelsでは各辺の指定ピクセル数にAll % / 100を掛けます。
- Mode＝Pixels／Percentで左右または上下のクリップが交差した場合は、全面透明になります。

## 指定ピクセル数でのクリップ・拡張

Mode＝Output Sizeを選ぶと、画像の倍率を維持して、指定した幅・高さの枠を出力します。80×40pxの画像に40×20pxを指定すると中央を切り抜き、120×60pxを指定すると周囲を拡張します。拡張部分はExtendで指定した方式で描画します。

幅・高さは上下左右で求めた寸法より優先されます。このモードでは上下左右を%として扱い、All %と合わせて枠の中心位置を決めます。Angleで回転した後の中心に指定サイズの水平・垂直な枠を置きます。左右対称／上下対称の指定では中心は動きません。例えば80px幅の画像でLeft＝25%、All %＝100なら、中心は右に10px移ります。画素自体は移動・拡大縮小しません（Angleによる回転を除く）。

All %＝0%でも指定サイズとAngleは有効です。幅・高さは1～30000pxの整数として扱い、小数入力やキーフレーム間の値は四捨五入します。奇数サイズなどで枠の端が半端になる場合は、左上を元解像度のピクセル境界へ切り下げます。プレビュー解像度を下げても元解像度の枠位置を維持します。

### v0.1からの切り替え

v0.2ではUnitsをModeへ変更し、Set Output Sizeチェックボックスを廃止しました。保存済みのPixels／Percentの選択と幅・高さは引き継ぎます。旧チェックボックスをOnにしていたエフェクトは、ModeでOutput Sizeを選び直してください。旧Units＝Pixelsで枠の中心をずらしていた場合は、上下左右を元の幅・高さに対する%へ換算してください。

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

`tests/ui-smoke.jsx`は空の検証用プロジェクトにエフェクトを作成し、`target/ae-smoke/image-resize-ui.aep`へ保存します。新しいAEプロセスで実行後、検証用レイヤーを選択して次を確認します。

- Pixels → Percent → Output Size → Pixelsと往復し、(px)／(%)とグループ名が更新されること。
- Output Sizeグループは指定サイズモードだけで表示され、初回表示時に展開されていること。
- Edges／Center Adjustmentグループは初期状態で展開され、Mode変更で数値が失われないこと。
- Angleを0° → 27° → 0°に変更するとInterpolationが表示・非表示になること。
- ExtendにRepeatがあり、Tileと異なって端の画素を延長すること。

## ライセンス

本プラグインの実装と`utils`はMPL-2.0です。生成元の雛形に対する追加許諾は[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)に記載された素材に限られ、本プラグイン固有の実装には適用されません。
配布条件は[ライセンスガイド](../../LICENSING.md)と[投稿条件](../../CONTRIBUTING.md)を参照してください。
