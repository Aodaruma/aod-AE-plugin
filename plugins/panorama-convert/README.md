# AOD_PanoramaConvert

Converts panoramic projections while resizing only the effect output bounds.

正距円筒図法・キューブマップ・180°魚眼の投影形式を相互変換します。**コンポジションサイズやレイヤーのTransformを変更せず、エフェクトの出力範囲だけを変更**します。

## 使い方

1. パノラマ素材のレイヤーへ `Aodaruma > AOD_PanoramaConvert` を適用します。
2. `Input` で素材の投影形式、`Output` で変換先を指定します。
3. `Output Width (0 = Input)` で出力幅を指定します。0は元レイヤー幅を使います。
4. 必要に応じて `Pan / Tilt / Roll` で向きを調整します。

高さは出力形式から自動計算します。たとえば800×400の正距円筒画像を幅800のCross 4x3へ変換すると、描画範囲は800×600になり、元の中心から上下に100pxずつ広がります。親コンポのサイズは変わりません。コンポ外の画素は通常どおり最終出力で切れますので、表示したい範囲に応じてレイヤーのスケールを調整してください。

変更されるのはエフェクトがAEに返す描画範囲です。素材そのものの寸法、式の `thisLayer.width/height`、アンカーポイント、位置、スケールは書き換えません。元の中心を基準に配置し、奇数サイズで半端になる場合は左上のピクセル境界に切り下げます。

## 対応形式

| 形式 | 出力比率 | 内容 |
|---|---|---|
| Equirectangular | 2:1 | 横360°・縦180°の正距円筒図法。入力は2:1以外の比率でも画像全域を360×180°として解釈します。 |
| Cubemap 3x2 | 3:2 | 6面を3列×2行に配置します。 |
| Cubemap 6x1 | 6:1 | 6面を横一列に配置します。 |
| Cubemap Cross 4x3 | 4:3 | 水平十字配置。未使用部分は透明です。 |
| Fisheye 180 | 1:1 | 前方180°の等距離魚眼。円外は透明です。 |

幅は各面が整数サイズになるよう、Equirectangularは2、3x2は3、6x1は6、Crossは4の倍数へ最も近い値に丸めます。幅800の3x2は801×534になります。各辺の上限は30000pxです。正方形ピクセル専用です。

魚眼入力から全周形式に変換した場合、撮影されていない後ろ半球は透明になります。通常の平面映像から見えていない景色を生成する機能ではありません。

### キューブマップの面順と向き

座標は右が+X、上が+Y、正面が+Zです。Equirectangularの画像中央が+Z、右から1/4の位置が+X、上下端が±Yです。

- 6x1：`+X, -X, +Y, -Y, +Z, -Z`
- 3x2：上段 `+X, -X, +Y`、下段 `-Y, +Z, -Z`
- Cross：

```text
      +Y
 -X   +Z   +X   -Z
      -Y
```

面内座標を左から右の `a=-1..1`、上から下の `b=-1..1` とした場合、各面の方向は次のとおりです（正規化前）。

| 面 | 方向 |
|---|---|
| +X | (1, -b, -a) |
| -X | (-1, -b, a) |
| +Y | (a, 1, b) |
| -Y | (a, -1, -b) |
| +Z | (a, -b, 1) |
| -Z | (-a, -b, -1) |

配置名が同じでも他ソフトの面順・回転規約は異なる場合があります。AdobeのFacebook／Pano2VR／GearVR専用プリセットと同一の互換性は保証しません。

## パラメーター

| 項目 | 動作 |
|---|---|
| Input / Output | 入出力の投影形式。初期値は両方ともEquirectangularです。 |
| Output Width (0 = Input) | 元解像度での出力幅。高さは形式から算出します。 |
| Pan (Y) / Tilt (X) / Roll (Z) | 出力の各視線を入力空間へ回す角度。X→Y→Zの順に適用します。Pan=90°で正面に入力の+X方向が来ます。 |
| Interpolation | Nearest／Bilinear。Bilinearは正距円筒の左右端を接続し、キューブ面の端では隣接する球面方向を参照します。 |
| Antialiasing | 1／4／16サンプル。縮小や魚眼の輪郭に有効ですが、サンプル数に応じて処理が増えます。 |

8／16／32bpc、Smart Render、マルチフレームレンダリングに対応します。32bpcのRGBは0～1に制限しません。CPU処理で、GPU専用処理はありません。

入力キャンバスの基準は元レイヤーの寸法です。マスクや前段エフェクトによる透明部分は位置を維持しますが、元の範囲外に拡張された画素は入力に含めません。投影形式を変えた本エフェクトを複数積み、前段の新しい寸法を自動認識する用途には対応しません。ステレオの左右／上下パック、Equiangular Cube Map、各面別ファイルの直接入力には未対応です。

## ビルドと確認

リポジトリ直下で実行します。

```powershell
cargo test -p panorama_convert
cargo clippy -p panorama_convert
$env:NO_INSTALL = '1'
just -f plugins/panorama-convert/Justfile release
```

Windows用成果物は `target/release/AOD_PanoramaConvert.aex` です。AEのプラグインフォルダーに配置して再起動すると使用できます。

`tests/make_fixtures.py` で方向を色にした入力画像を作成し、`tests/smoke.jsx` を**空のプロジェクトで起動した別プロセスのAE**で実行します。Pillowを導入済みのPythonで `tests/verify_smoke.py` を実行すると、全25通りの投影変換、縮小、回転、低解像度プレビュー、8／16／32bpcと寸法維持を検証できます。出力先は `target/panorama-smoke/` です。

```powershell
python plugins/panorama-convert/tests/make_fixtures.py
& 'C:/Program Files/Adobe/Adobe After Effects 2025/Support Files/AfterFX.com' -m -noui -r "$PWD/plugins/panorama-convert/tests/smoke.jsx"
# report.txt の完了とPNGの生成を確認してから実行
python plugins/panorama-convert/tests/verify_smoke.py
```

0.3では投影変換専用に整理し、`Operation` と展開・復元用の設定を削除しました。0.1／0.2の投影変換パラメーターの識別子は維持しています。旧版で展開・復元を設定したエフェクトも、更新後は投影変換として動作します。

旧版の保存設定は `tests/compat-smoke.jsx` で確認します。空の別プロセスで実行し、`target/panorama-smoke/compat-report.txt` の結果を確認してください。`compat-0.2.aep` は旧版で保存した投影変換の検証プロジェクトを使用します。

AE実機確認はWindows版25.6.6です。macOS用バイナリと他のAEバージョンは未検証です。

## 標準VRコンバーターとの違い

標準エフェクトでは出力幅がコンポジションサイズと連動します（[Adobeの解説](https://www.adobe.com/learn/after-effects/web/vr-converter-effect)）。変換専用プリコンポーズを使えば親コンポへの影響を避けられますが、本プラグインはプリコンポーズを追加せず同一コンポ内で使うためのものです。

## ライセンス

実装と共通 `utils` はMPL-2.0です。生成元の雛形に対する追加許諾は [TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt) に列挙された素材に限られます。[ライセンスガイド](../../LICENSING.md)と[投稿条件](../../CONTRIBUTING.md)を参照してください。
