# AOD_PanoramaReframe

Reframes 360-degree panoramas with camera projections, quaternion rotation, and After Effects camera synchronization.

360度パノラマから通常のカメラ映像を切り出すエフェクトです。正距円筒・Cube Map・Equiangular Cube Map を読み込み、透視投影・等距離魚眼・平行投影・アイソメトリック投影で出力します。8/16/32bpc に対応し、32bpc は HDR の RGB 値を保持します。

## 基本的な使い方

1. パノラマ画像・動画をコンポジションに配置します。
2. 素材レイヤーへ `AOD_PanoramaReframe` を直接適用します。別の平面やカラー入力用レイヤー指定は不要です。
3. `Input Projection` と Cube Map の場合は `Cube Layout` を指定します。
4. `Output Projection`、画角、回転、位置、球体表示サイズを調整します。

同じレイヤーのマスクと、このエフェクトより前のエフェクト結果を標準入力として読み込みます。素材が各面正方形のCube Map、または全周360×180度を表す正距円筒画像になるようにしてください。ステレオの上下・左右パックは片眼ずつプリコンポーズします。

出力ラスタと投影の縦横比は適用レイヤーの寸法・ピクセル縦横比に従います。コンポジション側で切り抜き、レイヤーの位置・スケールで配置できます。正距円筒入力を任意の出力寸法へ合わせたい場合は、全周画像をその寸法のプリコンポジションいっぱいに配置し、そのプリコンポジションへ適用します。Cube Mapは各面が正方形になるアトラス寸法を保ってください。

0.1.4で旧`Panorama Layer`は削除しました。旧版で平面から別レイヤーを指定していた設定は、パノラマ素材側へエフェクトを移して使用してください。深度専用の`Depth Map Layer`は引き続き利用できます。

## パラメーター

| 項目 | 内容 |
|---|---|
| Input Projection | Equirectangular、Cube Map、Equiangular Cube Map（各面の軸が等角度間隔） |
| Cube Layout | 6×1、1×6、3×2、4×3の水平クロス |
| Flip Source Horizontal / Vertical | 入力アトラス全体の反転。各面個別の回転・反転ではありません |
| Output Projection | Perspective、Fisheye (Equidistant)、Orthographic、Isometric |
| Field of View | 透視では水平画角1〜179度（179度超の入力は179度として描画）。魚眼では短辺に内接する円の直径方向の画角1〜360度 |
| Sphere Scale (%) | 魚眼・平行・Isometricの表示サイズ。既定100%、1〜1000%。50%で直径が半分、200%で2倍。画角・回転・視差距離を変えず、出力中心を基準に拡大縮小。Perspectiveでは非表示・無効 |
| Orthographic Width | 平行・アイソメトリック投影の画面幅。球面と同じ座標単位 |
| Rotation Mode | Euler XYZ、Quaternion WXYZ。UIで切り替えると現在の向きを変換 |
| Rotation X / Y / Z | 右手系の回転。X、Y、Zの順に適用。Y=90度は右方向を見る |
| Quaternion W / X / Y / Z | 正規化して使用。全成分0は単位Quaternion扱い |
| Position X / Y / Z | 環境画像の変形による疑似移動。X=右、Y=下、Z=前。AE同期時にもオフセットとして加算 |
| Parallax Distance | 深度未指定時の視差の基準距離（既定10000）。小さいほどPositionによる変形が強くなる。平行投影の基準半径にも使用 |
| Depth Map Layer | 深度用パノラマレイヤー。Noneで無効、指定すると自動的に有効 |
| Depth Near Distance / Far Distance | 深度画像の黒／白に対応する距離（既定1000／10000）。Positionと同じ単位 |
| Invert Depth | 白が近い・黒が遠い深度画像に対応 |
| Sync AE Camera | アクティブAEカメラの向きと透視投影のZoomを同期。親・Orientation・Point of Interestを含むワールド行列を使用 |
| Sync AE Position | AEカメラのワールド位置も同期。Sync AE Cameraがオンの場合のみ有効 |
| Edges | Seamless Panorama、None (Transparent)、Extend、Repeat、Mirror |
| Interpolation | Nearest、Bilinear、Bicubic、Mitchell-Netravali、Lanczos、Cubic B-Spline、EWA Quadratic |
| Mitchell B / C、Lanczos Lobes、EWA Radius | 選択した補間方式の調整。共通 `utils::image` 実装を使用 |
| Antialiasing | 出力1画素あたり1、4、16サンプル。高画角・縮小時のちらつきを軽減 |
| Viewport Controls | コンポジション／レイヤービュー上の操作表示 |
| Rotation Drag Sensitivity | ビュー上の回転操作の感度。既定1、0.1〜4倍 |

`Seamless Panorama` は正距円筒の左右を連続させ、上下端は極の色を延長します。Cube Map では隣接面へ投影して補間領域を補います。他の端処理は各面の内部に対して行い、アトラス上で隣に置かれただけの面を混ぜません。魚眼円の外側、平行投影の投影範囲外は透明です。

## Positionによる環境画像の変形

Positionはパノラマを方向ごとの環境画像として扱い、Position方向の景色を広げ、反対側を圧縮する連続した角度変換で移動感を作ります。小さな移動では一定距離の環境に対する視差を近似し、大きな移動でも球体の外へ出て画像が欠けることはありません。Positionを0にすると元の投影に戻ります。

深度未指定時は`Parallax Distance`に対するPositionの比率で変形量が決まります。まず距離10000に対してPositionを1000〜5000程度動かして調整してください。旧Finite Sphere方式とSphere Centerは削除しました。旧プロジェクトも現在の環境変形を使用します。

### 深度マップを使う

1. カラー素材と同じ方向・投影形式・Cube Layoutのグレースケール深度画像をコンポジションへ追加し、表示をオフにします。解像度はカラー素材と異なっていても構いません。
2. `Depth Map`グループの`Depth Map Layer`に指定します。指定だけで有効になり、`None`へ戻すと通常の環境変形に戻ります。
3. 黒の距離を`Depth Near Distance`、白の距離を`Depth Far Distance`で設定します。白が近い素材では`Invert Depth`をオンにします。中間値は距離を線形補間します。

近い領域ほど大きく、遠い領域ほど小さく変形します。カラーと同じ方向座標で深度を読み、移動経路に沿って深度を参照する角度変形です。深度が一定の場合は、その距離を`Parallax Distance`に設定した場合と同じ結果になります。深度はBilinearで補間し、入力反転・端処理はカラーと共有します。深度画像のRGB輝度を0〜1に制限して使い、透明部分・非数値は`Parallax Distance`へフォールバックします。

深度を色管理で変換すると距離も変わるため、数値素材として色を保持する設定を推奨します。入力のエフェクトやマスクを含める場合はプリコンポーズしてください。プリコンポーズした深度のアニメーションにも対応します。

## Cube Map の面配置

座標は +X=右、+Y=下、+Z=前です。各面の上方向を含む定義は `src/geometry.rs` の `cube_direction` を参照してください。OpenGL系の+Y=上という名称とは上下の符号が逆です。

6×1 と 1×6 は `+X, -X, +Y, -Y, +Z, -Z` の順です。3×2 は行順に次の配置です。

```text
+X  -X  +Y
-Y  +Z  -Z
```

4×3 の水平クロスは以下です。空欄の色は無視します。

```text
     -Y
-X   +Z   +X   -Z
     +Y
```

Equiangular Cube Map は同じ配置で各面の座標を等角度に変換します。特定サービス固有の面順、面回転、パディングには自動対応しません。

## ビュー上での操作

エフェクト名を選択してレイヤーコントロールを表示します。Euler／Quaternionのどちらでも同じ円・軸・枠を表示し、現在のモード名を円の上に表示します。中央のAE標準アンカーポイントと重なる場所は避けてドラッグしてください。

- 中央の円の内側からドラッグ：横移動でY方向、縦移動でX方向へ回転。円の外に出てもドラッグ距離に比例して回転し続けます。既定の感度では円の直径分で180度です。
- 円周をドラッグ：画面に垂直な軸を中心に回転します。
- 水色の枠の右側ハンドル：画角、または平行投影の幅を調整します。
- 黄色の下側ハンドル：Parallax Distanceを調整します。
- 紫色の輪郭と右上の`Size`ハンドル：球体の表示サイズを調整します。中心から離すと拡大、近づけると縮小します。魚眼・平行・Isometricで表示します。

紫色の輪郭は実際の投影範囲を示し、ピクセル縦横比・レイヤー変形・ビュー倍率に追従します。球体が極端に大きい／小さい場合はハンドルをキャンバス内の操作しやすい位置へ寄せます。100%は従来の表示サイズなので、平行・Isometricでは`Orthographic Width`と`Parallax Distance`によって球体が画面外に広がる場合があります。拡大時に出力レイヤーの範囲を超えた部分は切り取られます。

円・枠は操作用の模式図です。AEカメラ同期時の回転操作はカメラに追加する回転オフセットを変更します。同期中の透視画角はAEカメラのZoomで調整します。数値パラメーターへの変更なのでキーフレームとUndoを使用できます。

Eulerでは角度に直接加算し、360度を超える回転数を保持します。Quaternionでは回転増分を現在の姿勢に合成します。半球への投影や円の端での停止はありません。モード切り替えは向きを保ちますが、Quaternion自体はEulerの回転回数を保存しません。

## 注意点

- 深度指定時も画像変形による疑似視差です。深度の境界や大きな移動では伸びが生じます。実形状の再構築・遮蔽判定・隠れた領域の復元は行いません。
- 平行投影・Isometricは球面上の方向分布を基準に投影し、その後に環境変形を加えます。画像から建物などの3D形状を推定する機能ではありません。Isometricは固定の等角視点に手動回転を追加します。
- AEカメラが存在しない場合は手動の向き・画角・位置を使います。AEの既定カメラやカスタムビューは同期対象外です。
- 素材は2Dレイヤーでの使用を推奨します。3Dレイヤーに適用するとAE側でも投影されます。
- Quaternionの4成分はAEの通常のキーフレーム補間です。SLERPは行いません。隣接キーでは同じ半球の符号を使い、qと−qを直接補間しないでください。
- GPUレンダリング、AEの被写界深度・モーションブラーの複製、マルチフレームレンダリングの宣言は未対応です。画素処理はCPUで行います。
- PremiereではAEカメラ同期とカスタムビュー操作は無効です。Premiereでの実機検証は行っていません。

## ビルドと検証

```powershell
cargo test -p panorama_reframe
cargo clippy -p panorama_reframe
cargo fmt --all -- --check
$env:NO_INSTALL = '1'
just -f plugins/panorama-reframe/Justfile release
```

ホスト検証は `tests/make_fixtures.py .task/ae` で素材を生成してから、専用の空のAEプロセスで `tests/ae_smoke_test.jsx` を実行します。Pillowが必要です。既存プロジェクトに混入しないよう、空でないプロジェクトでは停止します。出力と検証用AEPは作業ツリー内 `.task/ae/` に保存します。

```powershell
python plugins/panorama-reframe/tests/make_fixtures.py .task/ae
# 空の検証専用AEで plugins/panorama-reframe/tests/ae_smoke_test.jsx を実行
python plugins/panorama-reframe/tests/verify_ae_smoke.py .task/ae
```

Windows / After Effects 25.6.6で、8/16/32bpc描画、4種類のCube Map配置とEAC、AEカメラの親アニメーション・位置同期、半解像度、全補間方式を確認しました。ビュー操作はEuler／Quaternionの回転、姿勢を維持するモード切り替え、画角・半径のドラッグ、Undoを確認しています。macOSは未検証です。

0.1.1の修正はAfter Effects **2025（25.6.6x4）**で検証しました。同じ長さのドラッグ2回でEulerのYが0度→372.5度→745度と増え、Undoで372.5度へ戻ること、Quaternionでの一周超の回転、Eulerへ戻した後のリグ表示を確認しています。描画テストではXYZ各軸±30000（基準距離10000）で環境変形の不透明領域が維持されることを確認しています。

この検証環境では `saveFrameToPng` が32bpcのRGB値を縮小して保存するため、32bpcの確認には `sampleImage` の実値と、エフェクト未適用素材のPNGとの比較を併用しています。プラグイン側ではこの書き出し挙動を補正しません。

0.1.2もAfter Effects **2025（25.6.6x4）**で検証しました。8/16/32bpcで一定深度と通常変形の一致、上下で深度が異なる画像、透明深度のフォールバック、Noneへの解除、反転、深度プリコンポーズのアニメーション、異なる入力解像度、半解像度、全Cube Map／EAC配置を確認しています。実画面で深度の指定解除・反転・Undoを確認し、0.1.1のAEPから回転・Positionを保持して読み込めることも確認しました。Rustの16件のテスト、clippy、fmtも通過しています。

0.1.3の表示サイズもAfter Effects **2025（25.6.6x4）**で検証しました。8/16/32bpcで魚眼・平行・Isometricの50／100／150%描画、画角を維持したサンプリング、キーフレーム、半解像度、非正方形ピクセルを確認しています。実画面では魚眼の100%→49.492%→191.544%のドラッグとUndo、Isometricの49.492%→149.878%のドラッグ、拡大時のハンドル位置を確認しました。Rustの18件のテスト、clippy、fmtも通過しています。

0.1.4の標準入力への変更もAfter Effects **2025（25.6.6x4）**で検証しました。素材への直接適用、前段の色反転、マスクで切り抜いた入力、素材より小さいコンポジションでの描画を確認しています。8/16/32bpcの描画、全Cube Map／EAC配置、深度マップ、球体表示サイズ、AEカメラ同期の画像比較と、Rustの18件のテスト、clippy、fmtも通過しています。実画面では旧レイヤー選択の削除、Eulerリグの表示、球体サイズ85%→58.303%のドラッグとUndoを確認しました。
