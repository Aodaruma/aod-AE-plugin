# AOD_CodecMap

Applies map-controlled video codec compression with deterministic temporal replay.

マップで圧縮の強さと適用範囲を指定するAfter Effects用エフェクトです。初版はH.264 / libx264に対応し、実際に圧縮・復号した映像を返します。FFmpegの共有ライブラリを同梱してAEプロセス内で呼び出し、外部プロセスや一時動画は使いません。

## 使い方

1. 入力レイヤーに`Aodaruma > AOD_CodecMap`を適用します。
2. 全体を圧縮する場合は`Map Source = Uniform`を使います。
3. 範囲を指定する場合は`Map Source = Layer`にして`Compression Map`を選びます。初期値では白い場所ほど強く圧縮し、黒い場所は元の画素を保持します。
4. `Base CRF`、`White QP Offset`、`Mix`で質感を調整します。

マップは入力画像全体に伸縮して合わせます。コンポジション上の位置・回転・エフェクトをマップへ反映させたい場合は、マップをプリコンポーズしてください。`Layer`で未選択の場合は黒マップとして扱います。

## パラメーター

| 項目 | 動作・初期値 |
|---|---|
| Base CRF | 28。大きいほど全体の品質が低くなります。1〜51。時間変化不可。 |
| Temporal Mode | `Past Only`が初期値。`Independent`は各フレームを個別に圧縮します。 |
| GOP Length | 12。1〜60。Past Onlyで参照する区間の最大フレーム数。 |
| Map Source / Compression Map | Uniform、または指定レイヤー。Uniformは全白です。 |
| Map Channel | Luma / Red / Green / Blue / Alpha。初期値Luma。 |
| Invert Map / Map Gamma | マップの反転とガンマ。完全透明な画素は反転しても0です。 |
| Black / White QP Offset | 初期値0 / +12。各−24〜+24。正の値で量子化を強めます。 |
| Output | `Map Isolated`はマップに応じて原画と合成。`Codec Result`は復号画像全体を使用。 |
| Mix | 100%。0%では元の画素をそのまま返します。 |
| Preview | Result / Map / Requested QP Offset / Difference。Offset表示は実測QPではなく要求値です。 |
| Reset Cache | このエフェクトのキャッシュ世代を進めます。過去の世代はAEのキャッシュ管理で解放されます。 |

マップの16×16ブロック平均をQP offsetに変換し、CRFとAQに加えます。これはブロックの絶対QPやビットレートの指定ではありません。`Codec Result`では色差サブサンプリング、予測、フィルターの影響が黒領域にも及びます。黒領域の保持には`Map Isolated`を使ってください。

## 時間・キャッシュ

未来参照はOFF固定です。Bフレーム、lookahead、MB-treeを使わず、GOP先頭から要求時刻までを昇順に処理します。例: GOP Length=12でフレーム17を求める場合、12〜17だけを取得します。フレーム0は入力レイヤーのソース時刻0です。サブフレームでは要求時刻の位相を保った間隔でサンプリングします。

各要求は独立したencoder/decoderを作るため、逆再生・ランダムアクセス・並列要求でも過去の描画順序に依存しません。AE Compute Cacheには完成した対象フレームのみを格納し、キーにGOP内の全入力YUVと全マップoffsetを含めます。スイートがないホストではキャッシュを使わず再計算します。過去のマップも各時刻で取得します。

## 色・制限

- AEの8 / 16 / 32 bpc入力に対応します。コーデック内部は8-bit BT.709 limited-range YUV 4:2:0固定で、HDR値は圧縮経路で0〜1へ制限されます。作業色空間のICC変換は行いません。
- 元のalphaを保持し、RGBは一度非乗算にしてから圧縮します。Map Isolatedの重み0の画素は元のRGB/alphaを直接コピーします。
- 奇数サイズは右端・下端を複製して偶数サイズへ補い、復号後に戻します。
- GOP内で入力のサイズや原点が変わる場合はエラーにします。固定サイズのプリコンポーズ、またはIndependentを使ってください。
- 1辺16384画素まで、再計算用YUV合計256 MiBまで。上限を超える場合はGOP Lengthまたはプレビュー解像度を下げてください。
- 初版はCPU版H.264のみです。VP9、未来参照、ハードウェアエンコーダー、CBR/VBR、Premiereは未対応です。
- Windows x64を検証対象とし、macOSのランタイム同梱と実機動作は未検証です。

## ビルドとローカル検証用パッケージ

workspaceルートから実行します。MSVCのCコンパイラーとRustが必要です。

```powershell
cargo build -p codec_map --release
./scripts/prepare-codec-map.ps1 -SkipBuild
```

2つ目のコマンドは固定したFFmpegアーカイブを`target/codec-map-setup`へ取得し、SHA-256を検証します。出力は`target/release/AOD_CodecMap-package/`です。`AOD_CodecMap.aex`と`CodecMap`フォルダーを一緒に設置してください。通常の`cargo build` / 共通`just release`はプラグイン本体だけを作成します。

```powershell
./scripts/prepare-codec-map.ps1 -SkipBuild -InstallDirectory 'C:/Program Files/Adobe/Common/Plug-ins/7.0/MediaCore/AOD_CodecMap'
```

インストール先への書き込み権限が必要です。AE起動中は更新せず、更新後に再起動してください。別の場所から読み込む開発用途では、AEを起動するプロセスの`CODECMAP_CODEC_DIR`にDLLのあるディレクトリの絶対パスを指定できます。DLLがない場合は明示的な描画エラーとなり、代替の疑似圧縮は行いません。

### macOS

macOS、Xcode Command Line Tools、Rust、Python 3.11以降、NASM、pkg-configを用意し、workspaceルートから実行します。

```sh
bash scripts/prepare-codec-map-macos.sh
```

Apple Silicon・Intel両対応のプラグインとFFmpeg共有ライブラリを作成します。FFmpeg 8.1.2と固定したx264ソースのSHA-256を確認し、H.264に必要な構成だけをビルドします。生成先は`target/codec-map-macos/AOD_CodecMap-0.1.0-macos-universal.zip`です。共有ライブラリは`.plugin/Contents/MacOS/CodecMap/`に収録し、外部のFFmpegインストールを必要としません。

ZIP内の`AOD_CodecMap.plugin`を`/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/`へコピーします。アドホック署名を施した開発検証用パッケージで、Developer ID署名・Apple公証は行いません。ビルドスクリプトは両CPUの存在・依存先・署名を検証し、ネイティブCPUで実コーデックのテストを実行します。macOS版AEの動作確認は別途必要です。

専用CIは[CodecMap macOS Preview](../../.github/workflows/codec_map_macos_preview.yml)です。`feat/codec-map`への対象ファイルのpushで実行し、ZIPをActionsの成果物として保存します。GitHub Releaseは作成しません。

## 検証

```powershell
cargo fmt --all -- --check
cargo clippy -p codec_map --all-targets
cargo test -p codec_map
$env:CODECMAP_CODEC_DIR = (Resolve-Path target/release/AOD_CodecMap-package/CodecMap).Path
cargo test -p codec_map -- --include-ignored
```

実コーデックのテストにはROIの反転、I/Pフレーム、過去の入力・マップ変更、要求順序、並列セッション、キャンセルを含みます。AE実機用は[smoke.jsx](tests/smoke.jsx)です。結果と未確認事項は[検証記録](tests/VALIDATION.md)を参照してください。

## ライセンスと設計

プラグイン実装はMPL-2.0です。テンプレート由来の素材には[TEMPLATE-LICENSE.txt](TEMPLATE-LICENSE.txt)の追加許諾が適用されますが、実装コード・utils・RD由来のFFIコードには通常のMPLが適用されます。

同梱候補のFFmpeg/libx264はGPL構成です。ローカル検証用パッケージをそのまま正式配布物とは扱わず、[依存関係と配布境界](native/README.md)を確認してください。[設計](../../docs/design/codec-map.md)、[リポジトリのライセンスガイド](../../LICENSING.md)も参照してください。
