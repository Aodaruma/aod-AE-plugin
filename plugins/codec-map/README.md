# AOD_CodecMap

Applies map-controlled video codec compression with deterministic temporal replay.

マップで圧縮の強さと適用範囲を指定するAfter Effects用エフェクトです。H.264 / libx264、VP9 / libvpx、MPEG-4 Part 2で実際に圧縮・復号した映像を返します。FFmpeg共有ライブラリを同梱してAEプロセス内で呼び出し、外部プロセスや一時動画は使いません。

## 使い方

1. 入力レイヤーへAodaruma > AOD_CodecMapを適用します。
2. Codecを選び、表示される品質設定で質感を調整します。
3. 全体を圧縮する場合はMap Source = Uniform、範囲を指定する場合はLayerにしてCompression Mapを選びます。
4. Output = Map Isolatedでは黒い場所を元の画素のまま保持し、白い場所へ圧縮結果を適用します。

マップは入力画像全体に伸縮して合わせます。位置・回転・エフェクトを反映させたい場合はマップをプリコンポーズしてください。Layerで未選択の場合は黒マップです。既存プロジェクトのパラメーターIDを保持するため、新しいコーデック設定はコントロールの末尾にあります。

## パラメーター

| 項目 | 動作・初期値 |
|---|---|
| Codec | H.264 / VP9 / MPEG-4 Part 2。初期値H.264。選択した品質設定だけを表示。 |
| Base CRF | H.264の品質。初期値28、1〜51。大きいほど低品質。 |
| VP9 CQ | VP9の品質。初期値35、0〜63。大きいほど低品質。 |
| MPEG-4 Quantizer | MPEG-4の量子化。初期値8、1〜31。大きいほど低品質。 |
| Temporal Mode | Past Onlyが初期値。Independentは各フレームを個別に圧縮。 |
| GOP Length | Iフレームを挿入する間隔。初期値12、1〜60。 |
| Allow Future Frames | 初期値OFF。H.264 / MPEG-4、Past Only、GOP≧3でONにすると最大2枚先を参照してBフレームを生成。 |
| Drop I-Frames | 初期値OFF。符号化後のIフレームを捨てる。キーフレーム可。Independentでは無効。 |
| Drop B-Frames | 初期値OFF。符号化後のBフレームを捨てる。キーフレーム可。Bフレーム生成時だけ有効。VP9では無効。 |
| Map Source / Compression Map | Uniform、または指定レイヤー。Uniformは全白。 |
| Map Channel | Luma / Red / Green / Blue / Alpha。初期値Luma。 |
| Invert Map / Map Gamma | マップの反転とガンマ。完全透明な画素は反転しても0。 |
| Black / White QP Offset | 初期値0 / +12。各−24〜+24。正の値で量子化を強める。 |
| Output | Map Isolatedはマップに応じて原画と合成。Codec Resultは復号画像全体を使用。 |
| Mix | 初期値100%。0%は元の画素をそのまま返す。 |
| Preview | Result / Map / Requested QP Offset / Difference。Offset表示は実測QPではなく要求値。 |
| Reset Cache | このエフェクトのキャッシュ世代を進める。旧世代はAEのキャッシュ管理で解放。 |

Codec、各品質設定、Temporal Mode、GOP Length、Allow Future Frames、Map Sourceは時間変化不可です。Drop I/B-Framesやマップ調整・Mixはキーフレームを打てます。

H.264はマップの16×16ブロック平均をQP offsetに変換し、CRFとAQに加えます。VP9は同じ平均値を−24 / −16 / −8 / 0 / 8 / 16 / 24へ丸め、8×8のセグメントへ渡します。VP9のROIはPフレームのみで、GOP=1・Independentではoffset設定が無効になります。MPEG-4は局所QPに未対応のためoffset設定は無効で、マップはMap Isolatedの合成範囲を指定します。

これらは絶対QPやビットレートの指定ではありません。Codec Resultでは予測・色差サブサンプリング・フィルターの影響が黒領域にも及びます。黒領域の保持にはMap Isolatedを使ってください。Requested QP OffsetはVP9では丸め後、MPEG-4では0を表示します。

## データモッシュ

Temporal Mode = Past OnlyでDrop I-FramesをONにすると、その時刻に符号化されたIフレームをデコーダーへ渡さなくなります。以降のPフレームは古い参照画像を使うため映像が崩れます。H.264 / VP9では欠落参照の補正・直前画像の保持になる場合もあり、典型的なデータモッシュにはMPEG-4を試してください。

チェックボックスはキーフレームを打てます。Iフレームの除去はGOP境界で効き、OFFに戻した後の次のGOP境界で復号の土台が更新されます。初期化用の最初のIフレームは必ず保持します。GOP=1ではすべてIフレームなので、除去中は最後に残した画像を保持します。

Bフレームを除去するにはAllow Future FramesをONにしてDrop B-Framesを使います。除去された表示時刻には、それ以前の最後に復号できた画像を返します。尺は短縮せず、alphaは現在の入力を保持します。Map Isolatedではモッシュ結果もマップに沿って合成します。

## 時間・キャッシュ

未来参照は初期値OFFです。OFFではBフレーム、lookahead、MB-treeを使わず、GOP先頭から要求時刻までを昇順に処理します。GOP=12でフレーム17を求める場合、12〜17だけを取得します。Iフレームを除去する場合は最後に保持するGOP先頭まで遡ります。未来ONでは同じGOP内の最大2フレーム先まで取得します。

フレーム0は入力レイヤーのソース時刻0です。サブフレームでは要求時刻の位相を保ってサンプリングします。時刻0より前では現在のGOP先頭を保持し、それ以前へモッシュの履歴を延ばしません。

各要求は独立したencoder/decoderを作り、逆再生・ランダムアクセス・並列要求でも過去の描画順序に依存しません。Compute Cacheには対象フレームの完成結果のみを保存し、キーに全入力YUV・全offset・コーデック・品質・I/B除去設定・対象時刻を含めます。過去・未来のマップや除去設定も各時刻で取得します。スイートがないホストでは再計算します。

## 色・制限

- AEの8 / 16 / 32 bpc入力に対応。コーデック内部は8-bit BT.709 limited-range YUV 4:2:0固定で、HDR値は圧縮経路で0〜1へ制限。作業色空間のICC変換は行いません。
- 元のalphaを保持し、RGBを非乗算にしてから圧縮。Map Isolatedの重み0では元のRGB/alphaを直接コピー。
- 奇数サイズは右端・下端を複製して偶数へ補い、復号後に戻します。
- 履歴内で入力のサイズや原点が変わる場合はエラー。固定サイズのプリコンポーズまたはIndependentを使用してください。
- 1辺16384画素、再計算用YUV合計256 MiBまで、モッシュの履歴は6000フレーム未満。上限を超える場合はプレビュー解像度を下げるか、直近のGOP境界でDrop I-FramesをOFFにしてIフレームを残してください。
- CPU処理です。ハードウェアエンコーダー、H.265、AV1、CBR/VBR、Premiereは未対応。
- macOS版AE、Intel Mac実機、異なるOS・CPU・FFmpegビルド間のビット一致は未確認。[検証記録](tests/VALIDATION.md)を参照。

## ビルド・検証用パッケージ

WindowsではMSVCとRustを用意し、workspaceルートで cargo build -p codec_map --release の後、./scripts/prepare-codec-map.ps1 -SkipBuild を実行します。固定FFmpegアーカイブをtarget/codec-map-setupへ取得し、SHA-256を検証します。生成先はtarget/release/AOD_CodecMap-packageです。

AOD_CodecMap.aexとCodecMapフォルダーを一緒に設置します。スクリプトの -InstallDirectory にAdobe MediaCore内の設置先を指定するとコピーできます。AEを終了してから更新し、再起動してください。通常のcargo build / 共通just releaseは本体だけを作成します。

開発用にCODECMAP_CODEC_DIRへランタイムの絶対パスを指定できます。ランタイムがない場合は明示的な描画エラーとし、疑似圧縮へ切り替えません。

### macOS

macOS、Xcode Command Line Tools、Rust、Python 3.11以降、NASM、pkg-configを用意し、bash scripts/prepare-codec-map-macos.sh を実行します。FFmpeg 8.1.2、固定x264、libvpx 1.15.2のSHA-256を確認し、3コーデックに必要な構成をビルドします。

生成先はtarget/codec-map-macos/AOD_CodecMap-0.2.0-macos-universal.zip。Apple Silicon・Intel両対応のAOD_CodecMap.pluginを /Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/ へコピーします。共有ライブラリは.plugin内に同梱し、外部FFmpegは不要です。

開発検証用のアドホック署名で、Developer ID署名・Apple公証はありません。両CPU・依存先・署名を検証し、ネイティブCPUで実コーデック試験を行います。専用CIは[CodecMap macOS Preview](../../.github/workflows/codec_map_macos_preview.yml)です。GitHub Releaseは作成しません。

## 検証

cargo fmt --all -- --check、cargo clippy -p codec_map --all-targets、cargo test -p codec_map を実行します。CODECMAP_CODEC_DIRへ同梱ランタイムの場所を設定し、cargo test -p codec_map -- --include-ignored で実コーデック試験も実行します。

ROI、各コーデックの品質、符号化後のI/B除去、表示時刻の保持、過去の入力・マップ変更、並列セッション、キャンセルを検証します。AE実機用は[smoke.jsx](tests/smoke.jsx)、[GOP=1](tests/gop-one.jsx)、[コーデック・データモッシュ](tests/codecs.jsx)です。

## ライセンスと設計

プラグイン実装はMPL-2.0です。テンプレート由来の素材には[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)の追加許諾が適用されますが、実装コード・utils・RD由来のFFIには通常のMPLが適用されます。

同梱FFmpeg/libx264はGPL構成です。検証用パッケージをそのまま正式配布物とは扱わず、[依存関係と配布境界](native/README.md)を確認してください。[設計](../../docs/design/codec-map.md)、[ライセンスガイド](../../LICENSING.md)も参照してください。
