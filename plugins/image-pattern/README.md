# image-pattern (AOD_ImagePattern)

Creates feedback, kaleidoscope, strip repeats, and seamless polygon image patterns with per-copy time offsets.

## Modes

- **Feedback** composites scaled, rotated, offset, and vortex-warped copies. Decay defaults to **1.0**. Opacity controls repeat transparency; Composite Order controls stacking. It is deterministic and does not depend on render order or cached output frames.
- **Polar Kaleidoscope** folds angular sectors, optionally mirrors alternate sectors, repeats radial rings, and adds radial twist.
- **Strip Repeat** crops a band through Center. Angle sets its direction (0 degrees = horizontal), Width its thickness; copies repeat perpendicular to it. Tile repeats the band; Mirror reflects alternate copies. Strip Rotation, Scale X/Y, Skew, and Skew Axis transform the arrangement. Scale X follows the band and Scale Y its normal, before Rotation.
- **Polygon Tile** tessellates triangles, quadrilaterals, or hexagons without geometric gaps. Tile Width/Height set the source cell dimensions; Rotation, Scale X/Y, Skew, and Skew Axis transform the lattice. Repeat selects Tile or Mirror. Quadrilaterals include rectangles and parallelograms; the other shapes can also be stretched and sheared.

**Image Affect (%)** and **Strip Image Affect (%)** default to 100: the image transforms with the mask. At 0 only the mask/arrangement transforms, while the image keeps its size and proportions. Global rotation, scale, and skew are interpolated as parameters; shared-boundary fitting interpolates positive vertical scale and shear. Both remain well defined at intermediate Affect values. Repeat's mirroring remains available at every Affect setting.

## Strip Repeatのクリッピングと重なり

**Clipping** で、各スリットのどちらの境界を残すかを選びます。開いた側には、元のスリットの座標変換を延長して画像の続きを描画します。

| Clipping | 動作 |
|---|---|
| Both Edges（初期値） | 両側をクリップします。従来の描画と同じです。 |
| Negative Edge Only | 法線方向の負側の境界だけを残します。Angle 0°なら上側をクリップし、下側にはみ出します。 |
| Positive Edge Only | 法線方向の正側の境界だけを残します。Angle 0°なら下側をクリップし、上側にはみ出します。 |
| None | 両側のクリッピングを外します。 |

開いた側で重なる画像は、元画像の透明度を使って合成します。**Overlap Order** のOverでは大きい反復番号のスリットが手前、Underでは小さい番号が手前になります。この項目はBoth Edgesでは表示されません。

クリッピングを外す場合は、Angle/WidthのDeltaが0でも **Copies Each Side** を使い、`-Copies〜+Copies` の有限個のスリットを描画します。たとえば1なら中心を含めて3枚です。Copiesを増やすほど描画と時間参照の処理量も増えます。スリット外への描画は出力レイヤーの範囲内で行い、ソース画像の外側はEdge／Edge Directionに従います。

Delta X/Y、Mirror、Image Affect、Time Deltaはクリッピング解除時にも適用されます。Split Overlapsは元の境界配置と消える反復を決めるために引き続き使います。Time Originは、元の両側クリップした領域のうち基準点を含むものを選びます。重なり順を変えても時間差の基準は移動しません。

## Strip deltas

**Delta Angle** changes the angle of shared boundaries between repeats. The two edges of the source band retain Angle; successive boundaries on either side receive the corresponding delta step. **Delta Width (px)** changes the distance between boundaries where they cross the normal through Center. This distance is clamped to at least 1 px before the global strip scale.

**Delta X (px) / Delta Y (px)** は、スリットごとに内部の画像をずらします。初期値は両方0で、境界位置・幅・反復番号は変わりません。ずらし量はソース画像のX/Y座標を基準とし、参照座標から `Delta X/Y × Delta Modeの段数` を引きます。たとえばAngle 0°・Tile・ForwardでDelta Xを10にすると、Centerの帯はそのまま、正側の帯では画像が10px、20px…右へずれ、負側では反対へずれます。回転・変形・Mirrorがある場合は、その画像の配置に沿って見た目の移動方向も変わります。

X/Yにも既存のDelta Mode、Cycle / Range、Random Seedが適用されます。Split Overlapsがオフなら、省かれた帯の番号を含む元の反復番号を使います。画像外はEdgeとEdge Directionに従います。Strip Image Affectが0%でもX/Yのずらしは有効です。基準はCenterの帯で、時間用のTime Originを変更してもX/Yの基準は変わりません。

各反復は隣接する境界間に配置されます。Image Affect 100%では画像の帯をその領域に合わせて伸縮し、0%では画像を変形せず、途中の値ではその影響を補間します。Mirrorでは交互の帯を反転します。Delta X/Yが0で連続する反復番号の場合、Image Affect 100%で共有境界の参照座標がつながります。

| Delta Mode | Step value for band index i |
|---|---|
| Forward | i: opposite signs on opposite sides of Center |
| Reverse | -i |
| Outward | Absolute value of i: same change toward both sides |
| Ping Pong | 0 to Cycle and back to 0, repeating on both sides |
| Random | Stable random value between -Range and +Range, controlled by Random Seed |

**Delta Cycle / Range** はPing Pongの折り返し位置とRandomの振幅を指定します。Delta AngleまたはDelta Widthが0以外なら、**Copies Each Side** で中心の両側に1〜256領域を設定できます（既定値32）。共有境界の内側に幾何学的な隙間はありません。Split Overlapsがオンなら交差した境界を法線方向に並べ、幅0の領域は交点で消えます。垂直に近い境界は有限の傾きで近似します。Both Edgesでは最も外側の境界を超える部分は透明です。Both EdgesでAngle/WidthのDeltaが両方0なら、Delta X/Yを使っていても反復数は無制限です。

**Split Overlaps** is a checkbox, enabled by default to preserve the previous appearance. When enabled, every crossing boundary subdivides the pattern. When disabled, boundaries are processed from the source band outward in their original order: a boundary folding back into an occupied region is skipped, so it cannot create an extra split there. Collapsed repeats keep their original numbers for Mirror and Time Delta; later repeats are not renumbered. The source band remains intact. With no Delta Angle, the switch does not change the result.

従来の **Overlap Priority** は削除済みです。クリッピング解除時の前後関係は **Overlap Order** で指定します。Delta Mode、Cycle / Range、Random Seed、Copies Each Side、Image Affect、時間差の設定も引き続き利用できます。

## Center and sampling

The animatable **Center** is initialized directly to half the layer width and height (for example, 1920x1080 gives 960,540). Edit the coordinates, point picker, keyframes, or expressions directly. Use Layer Center has been removed. AE's normal parameter reset restores the default point. Existing ImagePattern parameter names retain their persistent IDs.

For triangles, Center is the centroid. Inverted triangle cells use a 180-degree orientation; Mirror adds horizontal reflection on alternating lattice columns/rows. Quadrilateral and hexagon Mirror reflect X/Y respectively.

All modes share Mix, Nearest/Bilinear sampling, and Transparent/Extend/Tile/Mirror source-image edge controls. **Edge defaults to Transparent.** Source transparency is preserved. Polygon tessellation has no geometric holes, but sampling outside a transparent image can still yield transparent pixels.

Strip Repeat's **Edge Direction** defaults to **Along Strip**. Extend continues the boundary pixel along the source band's tangent; Tile and Mirror wrap or reflect along the intersection of that line with the source image. The cross-strip coordinate is preserved instead of clamping X/Y independently. A line that never intersects the image stays transparent. **Image Axes** restores conventional image-edge sampling. Affect and mirroring are accounted for when deriving the sampling direction.

Distances use full-resolution, pixel-aspect-corrected layer coordinates. X/Y downsampling is handled independently, and upstream bounds expansion does not move Center. Triangle Height is altitude; Hexagon Width/Height are its bounding box. For an equilateral triangle use Height = Width * sqrt(3)/2; for a regular point-up hexagon use Height = Width * 2/sqrt(3).

## Time offsets

**Time Delta (frames)** defaults to 0. Each copy samples the input at:

`current time + (i + Time Index Offset) * Time Delta`

Frame duration follows AE's source time step, including reversed playback and fractional frame offsets. Pattern controls are evaluated at the output time; only the input image is sampled at the shifted time. The effect checks out the upstream input at each requested time through SmartFX and releases frame pixels after sampling, without a persistent image cache.

**Time Origin (i = 0)** is a separate point, initialized to the layer center. It selects the pattern cell whose index is zero without moving the spatial pattern. **Time Index Offset** adds an integer offset to every index; use it to select a different zero iteration in Feedback.

| Pattern | Index i |
|---|---|
| Feedback | Copy iteration, starting at 0; Time Origin is hidden |
| Polar | Sector steps around Time Origin's sector, followed by ring steps * Sectors |
| Strip | Signed band number relative to the band containing Time Origin |
| Polygon / From Origin | Lattice distance from the cell containing Time Origin; cells on the same step share a time |
| Polygon / X or Y | Signed column or row difference in the transformed lattice; X counts each triangle separately |

With Split Overlaps on, Strip numbers the regions in boundary order. With it off, Strip keeps the original repeat numbers, including skipped repeats. Time Origin selects the region containing that point. The polygon origin can be placed anywhere inside the desired tile. **Tile Time Direction** is shown only for Polygon Tile. **Time Edge** defaults to Extend (hold the first/last frame); Transparent delegates out-of-range times to the host, Loop wraps the layer duration, and Ping Pong reflects at the endpoints. Time offsets increase rendering work according to the number of reference frames.

## Time Deltaの処理速度（v0.4.5）

Strip Repeatのクリッピング解除時は、境界の交差位置から並び順を事前計算し、各画素・各コピーでの並び替えを省きます。浮動小数点の丸めが影響する交点付近では従来の計算を使い、描画結果を維持します。両側クリップ時も、反復番号とTime Originを求める重複計算を減らしました。

Strip RepeatとFeedbackでは、直近に参照した別時刻の入力画像を同じ描画内で再利用します。Time EdgeのExtendなどで同じフレームが続く場合に、AEへの要求と画像変換を省きます。保持する参照画像は1枚に制限し、反復数に比例して画像メモリを増やしません。描画をまたぐ永続キャッシュは使いません。

AE 2025、640×360、8bit、両側32コピー、Angle 8°、Delta Angle 4°で、キャッシュを消去して3回測定した中央値です。PNGの保存時間を含みます。

| Strip Repeatの設定 | v0.4.4 | v0.4.5 | 速度比 |
|---|---:|---:|---:|
| 両側クリップ | 472 ms | 420 ms | 1.12倍 |
| 負側のみクリップ、Split Overlapsオン | 11,178 ms | 1,395 ms | 8.01倍 |
| クリップなし、Time Delta 120・Time Edge Extend | 10,784 ms | 981 ms | 10.99倍 |
| 負側のみクリップ、Split Overlapsオフ | 1,980 ms | 1,189 ms | 1.67倍 |

設定、解像度、入力素材や前段エフェクトによって改善幅は変わります。異なる時刻の入力を生成する費用と、各コピーの画像合成は引き続き必要です。既存の8／16／32bit出力385枚と、速度測定用12枚は変更前と一致しました。

再測定には[速度測定スクリプト](tests/ae_temporal_performance.jsx)を、使い捨てのAEプロジェクトで実行します。各測定前にAEのキャッシュを消去し、`target/ae-image-pattern-performance/`へ測定時間とPNGを保存します。バージョン比較では、更新前の結果を別名で保存してから更新版で再実行してください。

## Rename

This replaces experimental PatternTransform with display name **AOD_ImagePattern**, Match Name **ImagePattern**, and binary **AOD_ImagePattern.aex**. Wave Lattice and Complex Map have been removed. Existing PatternTransform instances are not automatically converted; add ImagePattern and transfer the remaining settings.

## Build and verification

See the [development guide](../../docs/development.md).

```text
cargo fmt --all -- --check
cargo clippy -p image_pattern
cargo test -p image_pattern
cargo build -p image_pattern --release
```

ImagePatternをインストールしたAEで、[基本テスト](tests/ae_smoke_test.jsx)、[拡張テスト](tests/ae_extended_test.jsx)、[クリッピングテスト](tests/ae_clipping_test.jsx)を実行します。結果はそれぞれ`target/ae-image-pattern-smoke/`、`target/ae-image-pattern-extended/`、`target/ae-image-pattern-clipping/`へ保存します。拡張・クリッピングテストは検証用AEPも保存するため、使い捨てのテストプロジェクトで実行してください。

[tests/verify_ae_outputs.py](tests/verify_ae_outputs.py) は、AEで描画したPNGをソース画像の参照フレームと照合します（Pillowが必要です）。Delta X/Yは405点、時間差は115点を比較します。[tests/verify_clipping_outputs.py](tests/verify_clipping_outputs.py) は、4種類のクリッピング、Over／Under、半透明合成、Mirror、境界交差、Time Origin、負の時間差、同じ時刻を参照するExtend／Loopを照合します。Rustの46テストでは、変形、境界交差、折り返した境界の省略、開いた側への座標の延長、画像位置ずれと反復番号の独立性、アルファ合成、タイルの被覆、ダウンサンプルとピクセル縦横比、境界計算の再利用と参照画像の保持などを確認しています。
