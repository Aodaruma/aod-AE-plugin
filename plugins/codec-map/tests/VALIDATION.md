# CodecMap 検証記録

## 0.3.0：軽量化・プリセットUI（2026-11-06）

Windows x64、AE 2025 25.6.6x4、固定FFmpeg 8.1.2で確認。fmt、clippy（all-targets、warningsをエラー扱い）、機能試験17件が成功。性能・メモリ計測用の2エントリーは別途実行した。

- 3コーデックの90フレームで、継続出力が各時刻の従来flush再計算と全画素一致。投入は各90回。逆順・8並列・過去変更・キャンセルも確認した。
- 既存114画像・61項目を最終ビルドで再実行し成功。入力fixture2枚を含む116 PNGすべてが0.2.1時とSHA-256一致。
- 旧AEPはCustom＋Native品質で復元され、保存値・キーフレームによる出力も旧PNGと一致。
- v03.jsxで初期値・3プリセット・時間変化フラグを確認。16画像・8項目の画素比較に成功。Compression=0は8/16/32 bpc・4Kで原画と一致し、Working Memory=64 MiBでも4Kバイパスが動作。
- 現在時刻を変えずseedの入力だけを編集すると結果が変わり、編集後のwarmとpurge後coldが一致。
- AE内の連続36フレームをCODECMAP_PROFILEで計測。通常GOP=12は投入36回、GOP=1のI除去は投入1回で72 PNGを生成。集計はtarget/codec-map-v03/ae-profile.json。
- 短期セッションの大量試験でWindowsのTLS index枯渇を再現し、共有runtime保持へ修正。修正後は全9条件×3回の性能計測を884.64秒実行して完了した。
- パラメーター名の最終整理後もコーデック40画像・21比較とv03の16画像・8比較を再実行し成功。旧AEPのCustom / Native復元、旧PNGとのSHA-256一致も再確認した。
- MPEG-4の同じフレームでGamma・Mixだけを変更し、3画像で合成結果が変わる一方、CODECMAP_PROFILEの符号化投入が1回だけであることを確認した。
- カタログAEPを新UIで保存し、個別プレビュー・Photo一覧・全体一覧をAEで再生成。画素は従来と一致し、個別プレビューも目視確認した。

### 圧縮経路の時間

合成YUVを使い、同じruntime条件で従来の毎回再計算と継続方式を比較。各条件3回の中央値、単位ms、全フレーム順次要求の合計。1080p・90フレームはI除去を継続する条件で、他は通常GOP=12。AEの入力レンダー・色変換・PNG保存時間は含めず、AE全体の速度倍率とは異なる。

| Codec | 解像度・枚数 | 再計算 | 継続 | 比率 |
|---|---|---:|---:|---:|
| H264 | 1920×1080・36 | 2042 | 293 | 6.97倍 |
| Vp9 | 1920×1080・36 | 10740 | 1653 | 6.50倍 |
| Mpeg4 | 1920×1080・36 | 2653 | 403 | 6.58倍 |
| H264 | 1920×1080・90 | 62112 | 1407 | 44.14倍 |
| Vp9 | 1920×1080・90 | 162616 | 3681 | 44.18倍 |
| Mpeg4 | 1920×1080・90 | 42154 | 980 | 43.01倍 |
| H264 | 3840×2160・12 | 3712 | 730 | 5.08倍 |
| Vp9 | 3840×2160・12 | 14201 | 2166 | 6.56倍 |
| Mpeg4 | 3840×2160・12 | 3484 | 528 | 6.60倍 |

CODECMAP_CODEC_DIRを同梱DLLへ、CODECMAP_BENCH=1を設定し、cargo test -p codec_map benchmark_streaming -- --ignored --nocapture --test-threads=1を実行。結果はtarget/codec-map-v03/native-benchmark-pinned.txt、native-summary.json。

### メモリ

別プロセスでFull HD・90フレームのI除去を実行し、WindowsのPeakWorkingSet64を記録。従来一括保持は338.35 MiB、ストリーミングは68.37 MiB（約80%減）。共通runtime・テストプロセスを含み、AE全体のRAM使用量とは異なる。CODECMAP_MEMORY_CASE=cold / streamでbenchmark_memoryを個別実行し、memory-summary.jsonへ保存した。

### 配布用の検証ZIP

両OSの対象ソースは32e9c8b389c70fd113cece5170d59eb8727a340d。Windows本体のSHA-256はc065930a9c7e5542a576b91e9c90ebd8197fe5cf3d70f15d40e88b55f315911eで、AEに設置した本体と一致する。PE x64、ZIP CRC、各同梱ファイルのハッシュ、ライセンス・ソースの収録を検証した。

macOSは[CI run 42091876592](https://github.com/Aodaruma/aod-AE-plugin/actions/runs/42091876592)で成功。macOS 15.8.2 / arm64、Rust 1.99.1、Apple clang 17.0.0、deployment target 11.0。実ランタイムによる機能試験17件が成功し、性能計測用2エントリーは環境変数未指定のため計測せず終了した。プラグインと2共有ライブラリのarm64 / x86_64、依存先、アドホック署名を検証。取得後も両CPUのMach-Oと署名データ、依存先、ソース・ライセンス、ZIP CRCとCIのSHA-256を照合した。

Downloadsへ配置した0.3.0 ZIPとSHA-256:

- AOD_CodecMap-0.3.0-windows-x64.zip（37,044,094 bytes）: e1dd41546ceab11431bec3eace7372bfdfb745159e84877d67cad44f02c584d2
- AOD_CodecMap-0.3.0-macos-universal.zip（29,551,272 bytes）: 6327d72684fe115dd5e12c9a0484ca8f80ad38fae4378a85c777e6ba2de4cd05

macOS版AEとIntel Mac実機は未確認。作業用のビルド成果物はcheckout内のtargetへ集約し、外部ドライブの一時フォルダーは使用していない。

## 0.2.1：履歴メモリ・可変入力範囲（2026-10-12）

- Windows x64でfmt、clippy（all-targets、warningsをエラー扱い）、実コーデックを含む13テスト、releaseビルドに成功。
- AE 2025（25.6.6x4）で`replay-bounds.jsx`と`shadow-bounds.jsx`から55枚を生成し、Pillowによる29項目の画素検証に成功。
- Replay Memoryの既定1024 MiB、追加ID=22、時間変化不可をAE上で確認。既存のDrop I/BなどのIDは変更なし。
- Full HDの90フレーム（YUV＋ROI約270 MiB）、GOP=1、I除去ONで旧256 MiB上限を超えるレンダーに成功。seedとの一致、キャッシュpurge後の完全一致を確認。
- Grow Boundsの拡大・縮小、および黒いDrop Shadowの方向・距離の変化を3コーデックで確認。I除去後もレイヤー座標が揃い、現在alphaを保持した通常GOP=1描画と一致。逆順取得・purge後も一致。
- 64 MiB設定かつ6500フレーム目でも、Mix=0は元画像と完全一致、Mapは全白、MPEG-4のRequested QP Offsetは中立値の表示に成功。圧縮不要の表示で履歴上限を評価しないことを確認。
- 単体試験は負原点、異なるサイズ、領域外、空入力、原点の整数境界、旧256 MiB／1024 MiB／2048 MiBの容量判定、サイズ計算overflowを確認。
- 既存の`codecs.jsx`を再実行し40枚・21項目が成功。40枚すべてが0.2.0時のPNGとSHA-256一致。旧AEPのReplay Memory既定値1024、Drop IのID=20を確認し、同じ時刻のPNGも旧出力と一致。
- `smoke.jsx`を再実行し19枚・11項目が成功。8 / 16 / 32 bpc、Mix=0、alpha、マップ、過去変更を確認。新規試験と合わせて114枚・61項目。
- カタログの個別プレビュー、P一覧、全体一覧をAEで再生成・目視確認し、3枚とも従来と同じ内容。AEPは新パラメーターを含む状態で保存。

テスト出力は`target/codec-map-replay-bounds/`のAEP、PNG、host-report、shadow-report、pixel-report。Replay MemoryはAE全体の使用量を制限するものではなく、履歴バッファだけの上限。

0.2.1パッケージも両OS分を作成。Windows本体のSHA-256は`c80d8c3ee39b824dce1357e7b765636aa4732bef114956e4c6c7fb4da2815cfd`で、AEに設置・検証した本体と一致。Windowsのソーススナップショットは98e11c5、実装は905b03e。macOSは[CI run 38243129653](https://github.com/Aodaruma/aod-AE-plugin/actions/runs/38243129653)で905b03eをビルドし、Apple Silicon上で13テスト成功。両CPUのMach-O、依存先、アドホック署名、ソース同梱、ZIP CRC・SHA-256を確認した。macOS版AE実機とIntelでの実行は未確認。

Downloadsへ配置したZIP（SHA-256）:

- `AOD_CodecMap-0.2.1-windows-x64.zip`: `bf01479a70625abaf193d57f051b9d5266332bca8e6da85898f954d07094af58`
- `AOD_CodecMap-0.2.1-macos-universal.zip`: `2815591c94d5d5cb9431bc047aeb043598f77df6fb14a5f84ae1feb1ae63a3b3`

2026-10-11、Windows x64、Rust MSVC、FFmpeg 8.1.2 sharedで確認。

## 初版（0.1.0）で完了した確認

- `cargo generate --path templates/plugin --destination plugins --name codec-map --values-file target/codec-map-setup/values.toml --silent`で生成。`repository_plugin=true`とSmart Render / deep color / threaded renderingを指定。
- `cargo fmt --all -- --check`: 成功。
- `cargo clippy -p codec_map --all-targets -- -D warnings`: 成功。固定しているafter-effectsのentrypointマクロに由来する2つのlintはソース先頭で理由を記して除外。
- `cargo test -p codec_map -- --include-ignored`: 5テスト成功。`CODECMAP_CODEC_DIR`で固定したランタイムを指定。
- `cargo build -p codec_map --release`: 成功。
- `scripts/prepare-codec-map.ps1 -SkipBuild`: SHA-256照合、プラグインと依存DLLのパッケージ生成に成功。
- AE共通プラグインフォルダーの`AOD_CodecMap`へローカル検証用一式を設置。

実コーデックの試験では、IフレームとPフレームでマップ反転によって左右の量子化誤差の大小も反転すること、同じ要求の反復・逆順・8並列セッションの結果一致、過去のマップまたは画像だけを変更した場合の結果変化、キャンセルを確認。単体試験では奇数サイズ、部分ブロック、負時刻・サブフレーム、未来を含まない範囲、alpha保持、重み0でのHDR画素保持も確認。

## AE実機テスト

専用の空プロジェクトで、以下を実行する。

```powershell
./plugins/codec-map/tests/smoke.ps1
```

AEで`smoke.jsx`を実行し、完了後に次を実行する。

```powershell
./plugins/codec-map/tests/smoke.ps1 -ValidateOnly
```

出力先は`target/codec-map-smoke/`。入力fixture、各条件のPNG、専用AEP、`host-report.txt`、`pixel-report.txt`を保存する。既存プロジェクトに項目がある場合はテストを開始しない。

確認項目: 8 / 16 / 32 bpc、奇数サイズ、半透明alpha、マップの黒領域保持、Mix=0、マップ未選択、ランダムな時刻要求、全キャッシュpurge後の再現性、過去のマップ編集、独立モード、プレビュー解像度。

AE 2025（25.6.6x4）で19条件のPNG生成と11項目の画素比較に成功。8 / 16 / 32 bpcで黒マップ領域、alpha、Mix=0の一致を確認した。8 bpcではマップ未選択の完全一致、キャッシュpurge後の一致、Independentとの差、過去のQP offsetだけを変更した際の結果変化も確認した。

実機でSmartFXの出力を入力より先にcheckoutしていたエラー（25::244）を発見し、入力→出力の順に修正した。描画失敗はホストのエラーフラグ付きで報告する。PNG保存APIが失敗を例外にせず、書き込みがスクリプト終了後になる場合もあるため、ファイル検証はPowerShell側で行う。

`SupportsGetFlattenedSequenceData`を追加したPiPLではAE 2025の起動が止まったため、既存テンプレートと同じくこのフラグを省いている。シーケンスデータは使用しない。ビルド時に古いAEとの互換性に関するPiPL警告が出るため、検証済みホストは上記のAE 2025に限定する。

写真を使う512×512の実描画から、個別プレビュー、Photo一覧（18種）、全体一覧（40種）とカタログAEPを更新した。

最終ビルドでエフェクトコントロールを目視確認。IndependentでGOP Length、Uniformでマップ選択・チャンネル・反転・ガンマが無効になり、Past Only／Layerへ戻すと有効になる。内部のCache GenerationはAEGPのHiddenフラグで隠し、Reset Cacheのクリックで世代が0→1へ変わることを確認した。

MFR無効の`aerender -mfr OFF 50`と、AE上で`app.setMultiFrameRenderingConfig(true, 50)`を設定してキャッシュpurge後にレンダーキューを実行した場合を比較した。フレーム0〜11の12枚すべてがPNGファイルのSHA-256で一致し、レンダーキューはDONEで完了した。出力と記録は`target/codec-map-smoke/{serial,mfr}/`、`serial.log`、`mfr-report.txt`。この環境では2回目のaerender起動がAEを起動せず待機したため、MFR有効側はAE本体のスクリプトAPIで実施した。同時実行数の計測・負荷試験までは行っていない。

## 複数コーデック・データモッシュ（0.2.0）

Windows x64・AE 2025（25.6.6x4）で確認。実FFmpegを使う11単体試験が成功。H.264・VP9・MPEG-4それぞれの品質差、VP9のPフレームROI、符号化パケットのI/P/B数と除去数、初期I保持、GOP=1の画像保持、B除去時の表示時刻選択、I除去後のPフレーム、キャンセルとキャッシュキーを検証した。H.264 / MPEG-4のB生成時は、GOP端を含む12時刻・I除去ON/OFFで必要なprefixだけを処理した結果と全12フレーム処理の結果が一致した。

tests/codecs.jsxは動く素材を固定サイズのプリコンポーズへ入れ、各コーデック・キーフレームでI/B除去を切り替える40PNGを生成。tests/codecs.ps1の21比較が成功し、除去による画像変化、purge後の再現性、OFF後の次のGOP境界での復帰、GOP=1の保持、Map Isolatedの黒領域と現在alphaの保持を確認した。出力はtarget/codec-map-codecsに保存。

Computer UseでAEのCodecドロップダウンを操作し、H.264のBase CRF、VP9 CQ、MPEG-4 Quantizerの表示切り替えを確認。VP9で未来参照・Drop Bが無効、MPEG-4でQP offsetが無効、未来参照ONでDrop Bが有効になることを確認。Drop I/BはcanVaryOverTimeがtrueで、キーフレーム設定と描画結果を確認した。

MPEG-4 Quantizer=20のAE実描画から個別プレビュー・Photo一覧・全体一覧・カタログAEPを更新し、画像を目視確認した。

最終Windowsビルドで40PNG・21比較を再実行して成功。0.1.xで保存したGOP=1のAEPを読み込み、Codec=H.264・Future=OFF・Drop I/B=OFFで復元されることを確認。フレーム5のPNGは以前の出力とSHA-256が一致した（AAAB8BC3B008448BDD09C48C946A5C8008B86A744F4150B2E9FF28B5CAF83650）。

## 描画キャンセルの修正（0.1.1）

ホストの`PF_Interrupt_CANCEL`を文字列へ変換し、処理終了時にabortを再確認していたため、一度だけのキャンセル通知が通常エラーとして表示される経路があった。PreRenderのレイヤーcheckoutで返るキャンセルにもエラーメッセージフラグを付けていた。ホストのエラー型をCompute Cacheのコールバックを含めて維持し、再問い合わせやダイアログ表示をせずそのまま返すよう修正した。

7単体試験で、単発のキャンセルがキャッシュ値を公開せず、エラーメッセージフラグも立てずに戻ることを確認。AE 2025の`tests/gop-one.jsx`では、Uniform / Layer、GOP=1 / Independent、キャッシュpurgeの前後、順序を入れ替えた5時刻の40PNGを生成し、同条件の結果がSHA-256で一致した。報告元のOS・AEバージョンは不明で、報告環境での直接再現は未確認。

## macOS Universalパッケージ（0.1.0）

2026-10-11、[CodecMap macOS PreviewのCI](https://github.com/Aodaruma/aod-AE-plugin/actions/runs/38079738940)で成功。対象ソースは`5b4186295645f0895694d565f504feb46f7c4280`。macOS 15.7.9 / arm64、Rust 1.99.0、Apple clang 17.0.0でビルドした。

- プラグイン、FFmpeg 8.1.2、x264のarm64 / x86_64ビルドとUniversal化に成功。
- FFmpegの依存先を同梱ライブラリへ固定し、外部ビルドディレクトリへの依存がないことを確認。
- 同梱ライブラリとプラグインをアドホック署名し、`codesign --verify --deep --strict`に成功。
- ZIPに収録するランタイムを使い、arm64上で実H.264処理を含む5テストに成功。
- ZIP取得後、プラグインと2つの共有ライブラリすべてにarm64 / x86_64と署名データがあること、依存先、ライセンス・ソースの収録、CI成果物とのSHA-256一致を確認。
- ZIP: `AOD_CodecMap-0.1.0-macos-universal.zip`、21,345,440 bytes、SHA-256 `2290bf118f3c69590be437f27e69503c755e04a7a99cfa81a011cdede16a88f9`。

macOS版AEの読み込み・描画、Intel実機上の実行、Developer ID署名・Apple公証は実施していない。

## macOS Universalパッケージ（0.2.0）

2026-10-11、[CI 38140889685](https://github.com/Aodaruma/aod-AE-plugin/actions/runs/38140889685)で成功。ソースは59f5cb3a91ae95685da9dfbe6ec118148f76075f、macOS 15.7.9 / arm64、Rust 1.99.0、Apple clang 17.0.0。

- x264、libvpx 1.15.2、FFmpeg 8.1.2とプラグインをarm64 / x86_64へビルドしてUniversal化。
- ZIPに同梱するランタイムで11試験すべて成功。VP9・MPEG-4・I/B除去・GOP境界も含む。
- codesign --verify --deep --strict、Info.plist検査、両CPU・依存先の検査が成功。
- 取得したZIPのCRCとSHA-256を検証。3バイナリそれぞれの両CPU・署名データ・依存先と、libvpxのLICENSE / PATENTS、3ライブラリのソース、プラグインソースの収録を確認。
- ZIPは29,483,702 bytes。SHA-256: fa19f04cc003831d1fa0b24026164c04d8bf2e78d328f01be88e074e53f58aa5。
- DownloadsへAOD_CodecMap-0.2.0-macos-universal.zipとチェックサムを配置。

macOS版AE、Intel Mac実機での実行、Developer ID署名・Apple公証は未確認・未実施です。

## 未確認・対象外

- AE上の時間リマップ、逆方向の時間伸縮、ROIタイル描画、16 / 32 bpcのHDR入力の数値比較。
- macOS版AE、Intel Mac上の実行、Premiere、異なるCPU・OS・FFmpegビルド間のビット一致。
- 正式配布用の対応ソース一式と依存ライセンスのパッケージング。
- ハードウェアエンコード、H.265、AV1、CBR/VBR。
