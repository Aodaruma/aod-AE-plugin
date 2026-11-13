# カタログ用の入力素材

## cat-silhouette.png

`AOD_ImageBevel`のプレビューに使用する、512×512ピクセルの白い猫の透過シルエットです。毛並みの明暗を含めず、アルファから輪郭を読み取ります。

元素材はEmre Gencer氏のUnsplash写真（`emre-153_VPk1NZQ-unsplash.jpg`）を加工した、既存の[Image Relightプレビュー](../previews/image-relight.png)です。After Effects 2025で次の処理を行い、16 bpc・作業用カラースペースなしのコンポジションからPNGを書き出しました。

1. `Color Key`で背景色RGB `(8, 11, 16)`を指定し、`Color Tolerance`を`2`、`Edge Thin`を`0`、`Edge Feather`を`0.5`に設定。
2. `Fill`でRGBを白に統一し、透過アルファを保持。

カタログ再生成時はこのPNGを直接読み込みます。元写真や加工元のプレビューを再度キーイングする必要はありません。
