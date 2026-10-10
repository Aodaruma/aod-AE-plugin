# コーデックの依存関係

`codec.c` はFFmpegの公開C APIを動的に読み込みます。FFmpegの実行ファイルは呼び出しません。プラグイン単体はDLLなしでも読み込めますが、圧縮描画には以下のランタイムが必要です。

## 固定した開発用ランタイム

- 配布元: [GyanD/codexffmpeg 8.1.2](https://github.com/GyanD/codexffmpeg/releases/tag/8.1.2)
- アーカイブ: `ffmpeg-8.1.2-full_build-shared.zip`
- SHA-256: `274923c68904a9b76c73b908f57923dafba81155856cd742138515ded570d066`
- 使用DLL: `avcodec-62.dll`, `avutil-60.dll`, `swresample-6.dll`（avcodecの依存先）
- `include/libavcodec` と `include/libavutil` は上記アーカイブの公開ヘッダーから、使用APIとそのinclude先を変更せず収録しています。各ファイル内の著作権表記を保持し、[LGPL-2.1本文](COPYING.LGPLv2.1)を添付しています。
- FFmpegソース: [FFmpeg](https://git.ffmpeg.org/ffmpeg.git)、ビルド設定・外部依存・ソース取得方法は配布元のREADMEと[ビルド環境](https://github.com/m-ab-s/media-autobuild_suite)を参照してください。
- H.264エンコーダー: [x264](https://www.videolan.org/developers/x264.html)。FFmpegのlibx264ラッパーを使い、デコーダーはFFmpeg内のH.264実装を使用します。

`scripts/prepare-codec-map.ps1` はチェックサムを検証して開発・実機検証用のファイル一式を作ります。ビルド時の暗黙のダウンロードはありません。

## 配置

```text
AOD_CodecMap.aex
CodecMap/
  avcodec-62.dll
  avutil-60.dll
  swresample-6.dll
  FFmpeg-LICENSE.txt
  FFmpeg-README.txt
  SOURCES.md
```

DLLはこのプラグインの場所を基準に探します。開発時は`CODECMAP_CODEC_DIR`に絶対パスを設定できます。システムPATHのFFmpegには依存しません。PowerShellのインストール用スクリプトはWindows x64向けです。

## macOSパッケージ

`scripts/prepare-codec-map-macos.sh`はFFmpegとx264をソースからビルドします。

- FFmpeg 8.1.2: `38b88335f99e76ed89ff3c93f877fdefce736c13`。ソースアーカイブSHA-256: `2ae7e42343cfffb811d15cfe98b6d005f082595fcdf034d30a4ff90cfed9f9c6`。
- x264 stable: `b35605ace3ddf7c1a5d67a2eb553f034aef41d55`。ソースアーカイブSHA-256: `cd71a7515b0e9a012e1ac9b1f8415bebcaf6fc97d4db32286642ac4c0fbe24f9`。
- x264は8-bit 4:2:0の静的ライブラリとしてFFmpegにリンクします。FFmpegはGPL構成、libx264エンコーダー・H.264デコーダー・パーサーのみを有効にし、CLI・ネットワーク・その他の外部ライブラリを無効にします。
- `libavcodec.62.dylib`と`libavutil.60.dylib`はarm64 / x86_64 Universalです。プラグインの`Contents/MacOS/CodecMap/`に同梱し、相互の依存は`@loader_path`へ書き換えます。
- ライブラリとプラグインをアドホック署名し、両CPU・依存先・署名を検証します。AE実機テストやApple公証はCIには含まれません。
- ZIPには使用したFFmpeg・x264のソースアーカイブ、プラグイン・utilsのソース、ビルドスクリプト・設定ログと各ライセンスを添付します。

## 配布境界

このFFmpegビルドはGPL構成でlibx264を含みます。MPL-2.0のプラグインコードのライセンス表記だけで、ランタイムの配布条件を満たしたことにはなりません。DLLを分離することもGPLへの対応を免除するものではありません。

Windowsのスクリプトが作るものはローカル検証用で、対応するソース一式・ビルド情報を添えた正式配布物の生成は対象外です。macOSスクリプトは上記のソース・ビルド情報を添付しますが、いずれも開発検証用パッケージです。第三者に渡す前に、Rust依存も含めた対応ソース・通知と、プラグインと組み合わせた配布条件を整理してください。[FFmpegのライセンス説明](https://ffmpeg.org/legal.html)を参照してください。
