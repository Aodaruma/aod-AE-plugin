# Datacode TODO (2026-05-13)

## Decode To Text Layer

- [ ] 生成済みテキストレイヤーの再利用/更新を実装する  
  - 同一ソースレイヤーに紐づく既存 `AOD_DecodeText_*` を優先更新し、新規作成を抑制する。
- [ ] `Generate` 連打時の重複生成を防止する  
  - 既存レイヤーがある場合は更新のみ行うモードをデフォルトにする。
- [ ] テキスト更新の追従性を改善する  
  - ボタン押下時に最新のデコード結果を確実に反映できるよう、取得経路とフォールバック条件を再整理する。
- [ ] スクリプト実行失敗時のログを拡充する  
  - 失敗箇所（デコード失敗 / layerByID 失敗 / script error）を切り分けやすくする。

## 回帰確認タスク

- [ ] AE 2025 で `Encode + Decode` 同時適用の回帰確認を実施する。
- [ ] `QR / rMQR / iQR / DataMatrix / PDF417 / ColorCode / Just Embedding` で Decode To Text Layer の挙動を再確認する。
- [ ] Region マスク・Region only output・Render Decoded Text の組み合わせで表示崩れがないか確認する。
