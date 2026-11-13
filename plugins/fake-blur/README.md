# AOD_FakeBlur

Creates map-controlled mipmap, displacement, and interpolated blur effects.

ガウシアン以外の方式で、粗さ、歪み、縮小補間の質感を作るブラーです。既定はMipmap。8／16／32bpc、SmartFX、Multi-Frame Renderingに対応します。

| Blur Mode | 挙動 |
|---|---|
| Mipmap（既定） | 面積平均の縮小ピラミッドから半径に対応するLODを選び、隣接LODを混合します。 |
| Displacement | コピーごとにランダムな方向を1つ決め、その方向へ変位させた画像を等しい比率で重ねます。各コピー内では全画素が同じ方向を使い、変位量だけがBlur Mapに従います。 |
| Downsample / Reconstruct | LODを整数に丸め、縮小画像を指定の補間で拡大。Nearestではブロック状の表現になります。 |

Blur Radiusはレイヤーピクセル単位。Displacementの変位量はRadius × MAP値、Mip系はlog2(1 + Radius × MAP値)をLODに使います。同じ値でもモード間のぼけ方は異なります。InterpolationはNearest／Bilinear／Cubic B-Splineです。

DisplacementのSamplesは重ねる枚数（2〜512）です。Evolutionは度数指定でランダムな方向の組み合わせ全体を回転させ、360で一周します。固定値では毎フレーム同じ方向を使い、キーフレームで連続的に回転できます。白いMAPに小さな光点を入力すると、指定半径の円周上に光点のコピーが並びます。MAPに濃淡がある場合も、各コピー内の方向は変えずに移動距離だけを変えます。

0.3.0でTurbulent StackをDisplacementに置き換えました。0.2.0のMipmap／Downsampleの選択は維持され、旧Turbulent StackはDisplacementとして描画されます。旧Turbulence Scaleは保存データとの互換性のため内部に残していますが、UIには表示せず描画にも使用しません。0.2.0ではBoxを削除しています。0.1.0で保存したプロジェクトはBlur Modeを再選択してください。

## Blur Map

Blur Mapにレイヤーを選ぶと、黒は半径0、白は指定半径、中間調は比例した半径になります。未選択では全画面に指定半径を適用します。Luminance／R／G／B／AlphaとInvert Mapを選べます。RGBはアンプリマルチプライ後の値にマップのアルファを掛けます。透明部分は黒扱いです。反転後はその部分も白になります。

マップは入力レイヤー全体の正規化座標へ伸縮します。コンポジション内のマップレイヤーのPosition／Scaleは直接参照しません。必要ならマップをプリコンポーズしてください。SmartFXではマップ依存を事前申告し、ROIの外側もチェックアウトします。

EdgesはTransparent／Clamp、Mixは元画像との混合。出力領域は拡張せず、アルファもぼかします。32bpcのHDR値を保持し、入力の色空間上で平均します。

## 高速化

Displacementはwgpuの並列サンプリングを利用します。各コピーの方向は描画前に一度だけ計算し、CPU／GPUで同じ値を使います。Mip系の縮小画像は一度生成して再利用します。AutoではMip系にCPUを使用し、十分な処理量のDisplacementにGPUを使用します。GPUを明示指定するとMip系もGPUで補間します。

GPU初期化・パイプライン・入出力バッファを再利用します。AutoのGPUエラー時はCPUへフォールバックし、GPU (Required)は利用できない場合にエラーを返します。Nearestの境界判定はCPU／GPUの丸め差を抑えるため1/4096px単位に揃えます。

## ビルド・検証

```powershell
cargo build --release -p fake_blur
cargo test -p utils --features blur_wgpu blur::
```

Windowsではtarget/release/fake_blur.dllをAOD_FakeBlur.aexの名前でAEのプラグインフォルダーへコピーします。GPUなしは--no-default-features。[0.3.0の検証記録](../../docs/fake-blur-displacement-validation.md)を参照してください。

## ライセンス

実装はMPL-2.0。テンプレート由来の素材には[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)の追加許諾が適用されますが、独自実装とutilsには拡張されません。[ライセンスガイド](../../LICENSING.md)に従ってください。
