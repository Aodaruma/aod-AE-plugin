# CodecMap 検証記録

2026-10-11、Windows x64、Rust MSVC、FFmpeg 8.1.2 sharedで確認。

## 完了した確認

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

## 未確認・対象外

- AE上の時間リマップ、逆方向の時間伸縮、ROIタイル描画、16 / 32 bpcのHDR入力の数値比較。
- macOS、Premiere、異なるCPU・OS・FFmpegビルド間のビット一致。
- 正式配布用の対応ソース一式と依存ライセンスのパッケージング。
- VP9、双方向GOP、ハードウェアエンコード、CBR/VBR。
