# AOD_CodecMap

Applies map-controlled video codec compression with deterministic temporal replay.

マップで適用範囲を指定できる、実コーデックによる圧縮・データモッシュ用After Effectsエフェクトです。H.264 / VP9 / MPEG-4 Part 2に対応。FFmpeg共有ライブラリを同梱してAEプロセス内で使用し、外部FFmpegのインストールや一時動画は不要です。

## まず使う

1. Aodaruma > AOD_CodecMapを入力レイヤーへ適用します。
2. Styleから質感を選び、Compressionで圧縮を強めます。0は原画をそのまま返します。
3. 部分適用にはArea = Map Layerを選び、Map Layerを指定します。黒が原画、白が効果です。
4. Mixで合成量を調整します。細かい設定はAdvancedを開きます。

| Style | 初期設定 |
|---|---|
| Compression | H.264、Compression=54、過去参照、Refresh Interval=12、Datamosh OFF |
| Blocks | MPEG-4、Compression=80、各フレームを独立して圧縮 |
| Datamosh | MPEG-4、Compression=50、過去参照、Refresh Interval=12、Datamosh ON |
| Custom | 現在設定を保持。詳細なコーデック・時間設定の変更時にも切り替わる |

プリセットは選んだ時点で設定を適用します。Compression、マップ、Mix、Datamoshは後から調整できます。新規適用はCompressionプリセット。未来参照はOFFです。

## 基本操作とAdvanced

基本画面はStyle、Compression、Datamosh、Area、Map Layer、Mixです。Map LayerはAreaで選んだ場合、Datamoshは時間依存する設定で表示します。

Compressionは共通の0〜100。正値をH.264のCRF 1〜51、VP9のCQ 0〜63、MPEG-4のquantizer 1〜31へ変換します。同じ数値で同じ質感になることを保証するものではありません。0またはMix=0では元の画素を保持します。

| Advancedグループ | 項目 |
|---|---|
| Codec | Codec、Quality Control = Compression / Native、Codec Quality。選択した品質だけを表示 |
| Motion | Frame Mode = Past Frames / Independent Frames、Refresh Interval（1〜60フレーム）、Allow Future Frames、Drop B-Frames |
| Map | Channel、Invert、Gamma、Dark Area Bias / Light Area Bias（各±24）、Map Behavior |
| Diagnostics | Display = Result / Map / Compression Bias / Difference、Working Memory (MiB)、Reset Cache |

Native品質はH.264がCRF 1〜51、VP9がCQ 0〜63、MPEG-4がquantizer 1〜31。大きいほど強い圧縮です。Nativeでは基本画面のCompressionが無効になります。コーデック・品質・時間参照・未来参照・Area・メモリは時間変化不可。Datamosh、Drop B-Frames、マップ調整、Map Behavior、Mixはキーフレーム対応です。

マップは入力全体へ伸縮します。位置・回転・エフェクトを含めたい場合はプリコンポーズしてください。マップ未指定なら黒として扱います。完全透明なマップ画素はInvert後も0です。

Dark / Light Area Biasは局所量子化の補正で、正の値で圧縮を強めます。H.264は16×16ブロック平均、VP9は同じ平均を7段階へ丸めたROIを使用します。VP9のROIはPフレームだけに作用し、Interval=1・Independent Framesでは無効です。MPEG-4は局所QP非対応のためBiasは無効で、マップは合成範囲へ使用します。

Map Behavior = Restrict Effectはマップで原画と合成し、重み0では直接コピーします。Full Codec Frameでは復号画像全体を使用し、予測・色差・フィルターの影響が黒領域にも及びます。Compression Bias表示は実測QPではなく要求値で、MPEG-4では中立値です。

## モッシュと時間参照

Datamoshは符号化後のIフレームを捨てます。初期化用の最初のIフレームは保持します。ON/OFFはキーフレーム対応。OFFへ戻した後の次のRefresh Interval境界で参照画像が更新されます。Interval=1では最後に保持した画像を返します。alphaは現在入力を保持します。

通常は未来を参照しません。Allow Future Framesを有効にしたH.264 / MPEG-4、Past Frames、Interval≧3では、同じGOP内の最大2枚先まで取得してBフレームを生成します。Drop B-Framesは符号化後のBを除去します。欠けた表示時刻には最後の復号画像を使い、尺は変えません。VP9はBを生成しません。

基準は入力ソース時刻0です。サブフレームでは位相を維持します。負の時刻では現在GOPの先頭を初期画像にし、それ以前へ履歴を延ばしません。欠落参照の補正・画像保持など、崩れ方はコーデックごとに異なります。

## v0.3の軽量化

- AEの入力状態を使い、画像変換前に完成結果のキャッシュを照合。フレーム単位のYUV・量子化マップも再利用します。
- 未来参照OFFの連続要求では圧縮セッションを継続し、新しいフレームだけを処理。保持は全体で最大2個です。逆順、過去の変更、範囲変更、競合時は再計算します。
- 未来参照ONはキャッシュとストリーミング再計算を使用します。描画順に依存した近似は行いません。
- Interval=1の画像保持は保持画像だけを圧縮。Mix=0、Compression=0、全黒マップの範囲合成では不要な圧縮を省略します。
- 履歴全体の未圧縮画像をまとめて保持せず、必要なフレームを順次変換。白マップ配列・float RGBAコピー・画素の重複読み込みも削減しました。
- FFmpegライブラリを共有して保持し、Windowsでロード・解放を大量に繰り返す際のTLS資源枯渇を防ぎます。

プレビューはAEの1/2・1/4解像度を使用し、本番はFullで出力してください。画素数が減るとブロック感やモッシュの見え方も変わります。初回の長い履歴やランダムアクセスには再計算が必要です。

Working Memoryは一時バッファの保守的な見積もり上限で、既定1024 MiB、64〜8192 MiB。全履歴の未圧縮サイズでは判定しません。AEの入力・キャッシュ、FFmpeg内部メモリは別途必要です。メモリ上限を増やすだけで高速化する設定ではありません。

## 互換性・制限

- 旧AEPはCustom＋Native品質で読み込み、保存値・キーフレームから描画します。プリセットは自動適用しません。旧名称・番号による式やスクリプトは修正が必要です。
- 8 / 16 / 32 bpcに対応。内部は8-bit BT.709 limited-range YUV 4:2:0。HDRは圧縮経路で0〜1へ制限し、ICC変換は行いません。
- 元のalphaを保持。奇数サイズは端を複製して偶数へ補います。サイズ・原点の変化は現在範囲へ位置合わせし、はみ出しを切り取り、不足は黒で補います。固定キャンバスにはプリコンポーズを使用してください。
- 1辺16384画素まで、モッシュ履歴は6000フレーム未満。長い区間では近い更新境界でDatamoshをOFFにして参照画像を残してください。
- CPU処理です。H.265、AV1、CBR/VBR、Premiereは未対応。OS・CPU・FFmpegビルド間のビット一致は保証しません。
- 実施環境・未確認範囲と性能計測は[検証記録](tests/VALIDATION.md)を参照してください。

## ビルド・検証用パッケージ

WindowsではMSVCとRustを用意し、workspaceルートで cargo build -p codec_map --release の後、./scripts/prepare-codec-map.ps1 -SkipBuild を実行します。固定FFmpegアーカイブをtarget/codec-map-setupへ取得し、SHA-256を検証します。生成先はtarget/release/AOD_CodecMap-packageです。

AOD_CodecMap.aexとCodecMapフォルダーを一緒に設置します。スクリプトの -InstallDirectory にAdobe MediaCore内の設置先を指定するとコピーできます。AEを終了してから更新し、再起動してください。通常のcargo build / 共通just releaseは本体だけを作成します。

開発用にCODECMAP_CODEC_DIRへランタイムの絶対パスを指定できます。ランタイムがない場合は明示的な描画エラーとし、疑似圧縮へ切り替えません。

### macOS

macOS、Xcode Command Line Tools、Rust、Python 3.11以降、NASM、pkg-configを用意し、bash scripts/prepare-codec-map-macos.sh を実行します。FFmpeg 8.1.2、固定x264、libvpx 1.15.2のSHA-256を確認し、3コーデックに必要な構成をビルドします。

生成先はtarget/codec-map-macos/AOD_CodecMap-0.3.0-macos-universal.zip。Apple Silicon・Intel両対応のAOD_CodecMap.pluginを /Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/ へコピーします。共有ライブラリは.plugin内に同梱し、外部FFmpegは不要です。

開発検証用のアドホック署名で、Developer ID署名・Apple公証はありません。両CPU・依存先・署名を検証し、ネイティブCPUで実コーデック試験を行います。専用CIは[CodecMap macOS Preview](../../.github/workflows/codec_map_macos_preview.yml)です。GitHub Releaseは作成しません。

## 検証

cargo fmt --all -- --check、cargo clippy -p codec_map --all-targets、cargo test -p codec_map を実行します。CODECMAP_CODEC_DIRへ同梱ランタイムの場所を設定し、cargo test -p codec_map -- --include-ignored で実コーデック試験も実行します。

ROI、各コーデックの品質、符号化後のI/B除去、表示時刻の保持、過去の入力・マップ変更、並列セッション、キャンセルを検証します。AE実機用は[smoke.jsx](tests/smoke.jsx)、[GOP=1](tests/gop-one.jsx)、[コーデック・データモッシュ](tests/codecs.jsx)です。

メモリと可変入力範囲の回帰試験は、空プロジェクトで[replay-bounds.jsx](tests/replay-bounds.jsx)、続けて[shadow-bounds.jsx](tests/shadow-bounds.jsx)を実行します。PNG保存完了後に、PillowのあるPythonで[replay-bounds.py](tests/replay-bounds.py)を実行すると55枚・29項目を検証します。

## ライセンスと設計

プラグイン実装はMPL-2.0です。テンプレート由来の素材には[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)の追加許諾が適用されますが、実装コード・utils・RD由来のFFIには通常のMPLが適用されます。

同梱FFmpeg/libx264はGPL構成です。検証用パッケージをそのまま正式配布物とは扱わず、[依存関係と配布境界](native/README.md)を確認してください。[設計](../../docs/design/codec-map.md)、[ライセンスガイド](../../LICENSING.md)も参照してください。
