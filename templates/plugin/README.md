# {{AE_PLUGIN_NAME}}

## 対応

- After Effects: {{AE_VERSION}} 系
- ライセンス: MPL-2.0

## ビルド例

- `cargo build -p {{CRATE_NAME}}`
- OpenCV等を有効にする場合:
  - `cargo build -p {{CRATE_NAME}} --features opencv,fft`

## メモ

- AE SDK の導入方法や、成果物（.aex/.dll）の出力場所はリポジトリ方針に合わせて調整してください。
