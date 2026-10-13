# image-calculate ( AOD_ImageCalculate )

Applies Blender-style math operations to one or two input layers.

This is the After Effects plugin **AOD_ImageCalculate**, which provides the **ImageCalculate.aex** plugin file for Adobe After Effects.

## Clamp Mode

演算結果をRGBへ戻した後の出力範囲を選択します。既定値は `None` です。

| モード | 処理 | -0.25 | 1.0 | 1.25 | 2.0 |
|---|---|---:|---:|---:|---:|
| None | 値をそのまま出力（32bpc） | -0.25 | 1.0 | 1.25 | 2.0 |
| Clamp | 0～1に制限 | 0.0 | 1.0 | 1.0 | 1.0 |
| Wrap | 周期1で0以上1未満へ繰り返し | 0.75 | 0.0 | 0.25 | 0.0 |
| Pingpong | 周期2で0→1→0と折り返し | 0.25 | 1.0 | 0.75 | 0.0 |

- 8／16bpcでは `None` も最終的に0～1へ制限されます。`Wrap`／`Pingpong` は整数化より前に適用するため、32bpcと同じ繰り返し・折り返し結果になります。
- モードは最終RGB出力全体に適用します。アルファへの `Wrap`／`Pingpong` は、アルファも演算する `Channel = RGBA` または `A` の場合に適用します。`RGBA` と `Wrap` の組み合わせでは、演算後のアルファが1なら0になります。
- `Use Original Alpha` がONなら、従来どおり入力アルファを0～1へ制限して保持し、モード適用後のRGBへ乗算します。
- NaN／Infinityは、どのモードでも0に置換します。
- 旧 `Clamp Result 0..1` とは保存パラメーターを分けています。旧版のプロジェクトでは `Clamp Mode` が `None` で追加されます。旧チェックボックスがONだった場合は `Clamp` を選び直してください。他のパラメーターの保存IDは維持しています。

## Building the Plugin

See the [main README](../../README.md) for instructions on how to build the plugin.
