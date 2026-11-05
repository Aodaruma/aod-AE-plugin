# AOD_CodecMap 設計

v0.3の実装仕様です。導入・UIの操作は[README](../../plugins/codec-map/README.md)、実施済みの確認は[検証記録](../../plugins/codec-map/tests/VALIDATION.md)を参照してください。

## 目的・構成

量子化・予測圧縮を実際の動画コーデックで適用し、復号画像をAEへ返します。名前はAOD_CodecMap、Match NameはCodecMap、crateはcodec_map。cargo generateのworkspaceテンプレートから生成しました。既存のパラメーターIDを変えず、v0.2の項目は末尾へ追加しています。

FFmpeg 8.1.2の公開C APIを小さなCアダプターから動的に呼び出します。Rustは画像変換・AE連携・再計算計画・キャッシュを担当します。同梱ライブラリはプラグイン基準のパスで読み込み、CLI・一時動画は使いません。AEの任意のスレッドから呼べるよう、要求ごとに独立したencoder / decoderを作ります。

| Codec | 品質 | ROI | Bフレーム |
|---|---|---|---|
| H.264 / libx264 | CRF 1〜51、初期28 | 16×16平均の連続offset ±24。AQ mode 1 | 未来ONで最大2枚。b-adapt=0、b-pyramidなし |
| VP9 / libvpx-vp9 | CQ 0〜63、初期35 | Pフレームのみ。16×16平均を7段階に丸めて8×8へ設定 | なし |
| MPEG-4 Part 2 / mpeg4 | qscale 1〜31、初期8 | エンコーダーの局所QP非対応。マップ合成は対応 | 未来ONで最大2枚 |

VP9はrealtime、cpu-used=5、aq-mode=0、lag=0、auto-alt-ref=0です。ROIの8セグメントには中立値0も含まれるため、offsetを−24 / −16 / −8 / 0 / 8 / 16 / 24へ丸めます。GOP=1・IndependentではROI設定を無効化します。MPEG-4ではROI設定を常に無効化し、対応していない局所QPを適用したとは扱いません。

コーデック・品質・GOP・未来参照設定は固定値、I/B除去チェックボックスとマップ設定はアニメーション可能です。AEではAEGP StreamのHiddenフラグで品質項目を切り替え、ParamUIFlagsのDISABLEDで非対応の項目を無効化します。

## マップと色

マップは入力全体へStretchで合わせ、Luma / R / G / B / Alphaから評価します。RGBを非乗算にしてInvert / Gammaを適用し、alphaを掛けます。完全透明・非有限値は0です。ブロック平均mから BlackOffset + (WhiteOffset - BlackOffset) * m を求めます。これはエンコーダーへの要求値で、絶対QP・局所ビットレートではありません。

入力は8 / 16 / 32 bpcに対応し、圧縮内部では8-bit BT.709 limited-range YUV 4:2:0を使います。奇数サイズは端を複製して補い、復号後に取り除きます。HDRは圧縮経路で0〜1へ制限し、ICC変換は行いません。

Map Isolatedでは元のpremultiplied RGBと復号RGBをマップ×Mixで合成します。重み0では元画素を直接コピーします。Codec ResultではMixだけで合成します。alphaは現在時刻の入力を保持し、合成した画像はコーデックの参照画像へ戻しません。MPEG-4のマップはこの合成にだけ作用します。

## 時間とデータモッシュ

基準時刻は入力ソース時刻0です。整数フレームn = floor(t / dt)、位相phase = t mod dt、GOP先頭s = floor(n / G) * Gとします。サブフレームでも同じ位相のサンプルを使います。

- 通常のPast Onlyはs〜nだけを取得。未来参照は初期OFF。
- Independentは要求フレームだけを新規セッションで圧縮。
- Allow Future Framesが有効なH.264 / MPEG-4では、同じGOP内でn+2まで取得。未来ONでもGOP境界を跨いで先読みしません。
- Drop I-FramesがGOP先頭でONなら、OFFの先頭まで過去へ遡る。ソース0のIは必ず保持。負のソース時刻では現在GOPの先頭を種にし、さらに遡りません。
- 各サンプル時刻のDrop I/B-Framesを読み、圧縮パケットのPTSに対応づけて適用。

入力ではGOP境界にIフレームを要求します。H.264ではIDRを強制し、自動シーンカット・lookahead・MB-tree・open GOPを無効化します。MPEG-4もclosed GOPとし、sc_threshold=1000000000で自動シーンカットを無効化します。符号化後にAV_PKT_DATA_QUALITY_STATSのpicture typeを使ってI/P/Bを判定します。VP9はBを作らず、keyフラグも判定に利用します。指定されたI/Bパケットをデコーダーへ送らず破棄するため、単に入力フレームのエンコードを省く動作とは異なります。

encoderとdecoderの遅延出力をflushしてから、PTS≦要求時刻の最後に復号できた画像を選びます。未来参照画像を要求時刻の出力へ代用しません。欠落した表示時刻では最後の画像を保持し、尺を変えません。I除去後の不正な参照によるAVERROR_INVALIDDATAだけを許容し、それ以外の実行エラーは報告します。欠落参照の補正方法により、H.264 / VP9とMPEG-4では崩れ方が異なります。

I除去OFFへ戻した後の次のGOP境界から新規セッションへ戻ります。GOP=1ではI除去が画像保持になり、Independentでは初期Iを捨てられないためチェックボックスを無効化します。未来参照とDrop BはGOP≧3で使用できます。

## UIと互換性

基本画面はStyle、Compression、Datamosh、Area、Map Layer、Mix。Advancedは初期状態で閉じ、Codec Settings / Motion / Map / Diagnosticsを置きます。プリセットの適用と値変更はUserChangedParamだけで行います。

Compressionは0でバイパス、正値s/100をH.264の1+50s、VP9の63s、MPEG-4の1+30sへ変換します。Native指定では既存の品質値を使用。USE_VALUE_FOR_OLD_PROJECTSで旧AEPにStyle=Custom、Quality Control=Nativeを補います。保存IDを作るenum variant名は維持し、表示名・配列順を変更しました。旧名称・番号による式は互換性の対象外です。

## SmartFX・キャッシュ

SmartPreRenderで依存時刻を宣言し、PF_GetCurrentStateによる入力0・必要なマップの状態キーを取得します。画像を読む前にAE Compute Cache（values.v4）を照合。キーはcodec・品質・時間・B設定・除去設定・マップ設定・入力状態・現在キャンバス・bit depth・世代を含み、復号後のMixや表示方法は含めません。

完成YUVとフレーム単位のYUV＋ROIを別ラベルでキャッシュします。miss時だけ画像を取得し、順次変換して圧縮へ渡します。キャッシュ値は不変のVec<u8>で、receiptの借用ポインターを保持しません。変換・最終結果の公開前後に状態を再確認し、途中失敗・キャンセルでは完成値を公開しません。PF_Stateの内容を解釈せず、SDKの128-bit状態値をキーへ混ぜます。

未来OFFでは全入力の累積キーが一致するセッションを、プロセス内で最大2個のpoolから取り出します。単一所有で追加フレームをpushし、flushなしでsnapshotを取得します。C側のcapacityは6003。逆順・不一致・競合ではcold再計算します。未来ONではencoder / decoderをflushして表示対象を選択します。セッションにAEバッファやコールバックを保持しません。

GOP=1のI除去は保持seedだけの計画へ縮約し、現在のalphaとマップは別に取得。負の時刻・サブフレームの規則、入力→出力checkout順を維持します。入力範囲の変化は現在キャンバスへ位置合わせし、伸縮しません。

Working Memoryは一時バッファの保守的な見積もりへ適用し、全履歴の合計を要求しません。AEの入力・キャッシュ、FFmpeg内部メモリは別途必要です。各codecは1スレッドとし、AEの並列レンダーに任せます。

loader参照をArcで共有し、各セッションにも所有権を持たせます。Windowsで大量のDLLロード・解放によるTLS index枯渇を防ぐため、共有runtimeはGlobalSetdownまで保持。setdownではpoolを破棄した後にruntimeを解放します。

ホストエラーはRenderErrorで分離し、PF_Interrupt_CANCELなどを再問い合わせや独自ダイアログへ変換しません。開発用CODECMAP_PROFILE環境変数を設定したホストでは、stderrへ数値だけの処理回数・時間を出力します。通常UIに実装用の診断項目は追加しません。

## 参照・依存

キャッシュのFFI・receipt管理・世代キーはreaction-diffusion-rsの8b107de652598a8f22003d1e4da74f753eadfa7dを参考にしています。画像だけではencoderの予測状態・レート制御を復元できないため、途中状態のcheckpointではなく種からの再計算を採用しました。after-effectsは0e0b197850f28c1bb24e8c82fce2e3d4c120d1e9へ固定しています。

[元のROI調査](https://chatgpt.com/c/6aca603f-c598-83ec-87f4-029f566a1ffe)は参考資料として扱い、採用条件は固定FFmpeg / x264 / libvpxソースと実行試験で確認しています。VP9のIフレームROIは有効とは扱いません。

macOSはFFmpeg・x264・libvpxをソースからUniversalビルドし、署名・依存先・両CPUを検査します。Windowsは固定した共有ビルドを同梱します。ソースの固定値・ライセンス・配布範囲は[native/README](../../plugins/codec-map/native/README.md)を参照してください。H.265、AV1、ハードウェアエンコード、CBR/VBR、Premiereは今回の範囲外です。
