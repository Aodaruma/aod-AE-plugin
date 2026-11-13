# FakeBlur Displacement 検証記録

検証日: 2026-11-13。AOD_FakeBlur 0.3.0、Windows。以下は元の開発チェックアウトで実行した検証の記録です。

## 変更内容

- Turbulent StackをDisplacementに置き換え。コピーごとにランダムな単位方向ベクトルを生成し、そのコピー内の全画素で共有します。画素位置によって方向を変えるノイズ処理は削除しました。
- 変位量はBlur Radius × Blur Map値。コピーを等しい比率で平均し、Samplesで枚数を指定します。Evolutionで方向の組み合わせ全体を度数指定で回転させます。既存の式からの参照を保つため、Evolutionの項目名も維持します。
- 方向は描画前に一度だけ計算し、CPUとGPUで共有します。GPUには2方向を1つのvec4に詰めて渡し、512方向で4 KiBです。
- Mipmapを既定のまま維持。メニューの2番目をDisplacementに置き換えたため、0.2.0で保存したMipmap／Downsampleの選択値は変わりません。旧Turbulence ScaleのパラメータID・型・位置は保持し、静的INVISIBLEで非表示にしています。値は描画に使用しません。
- ShutterBlurのシャッターカーブ、露光計算、パス処理は変更していません。

## 自動検証

以下が通過しました。ログは作業checkout内の`target/fake-displacement-20261113/`に保存しています。

| コマンド | 結果 |
|---|---|
| `cargo fmt --all -- --check` | 通過 |
| `cargo clippy --workspace` | 通過。既存の他プラグインの警告とPiPL生成器の既存警告あり |
| `cargo test` | workspace全体の250件が通過。明示実行用5件はスキップ |
| `cargo test -p utils --features blur_wgpu blur::` | 通常テスト15件が通過。GPU用2件と速度測定1件は明示実行用 |
| `cargo test -p utils --features blur_wgpu blur::tests::gpu_ -- --ignored --nocapture` | 実GPUを使用した2件、計78条件が通過 |
| `cargo check -p fake_blur -p shutter_blur --no-default-features` | 元の開発チェックアウトでCPUのみの構成が通過 |
| `cargo build --release -p fake_blur` | 通過。既存のPiPL警告、リンカーの出力メッセージあり |
| `git diff --check` | 通過 |

追加した通常テストでは、異なる場所の同じ光点が同じ形に広がること、HDR・アルファの積算量を保持すること、各画素のMAP値に比例して変位量が変わることを確認しました。方向の再現性、Evolutionの連続性・360度周期、サンプル数の上下限も確認しています。

GPU比較は、全9モード × 3補間 × 2境界の54条件に加え、Displacementの2／3／511／512サンプル × 3補間 × 2境界の24条件です。後者では0／0.25／0.5／1のMAP、異方的なプレビュー縮小率、Mix、Evolutionの大きな正負値を含めています。CPUとの最大許容差は0.0004。黒MAP画素は元画像と完全一致しています。

## 成果物とAE確認の範囲

新版は`target/release/AOD_FakeBlur.aex`です。元のDLLは`target/release/fake_blur.dll`です。

2026-11-13 20:01（日本時間）、ユーザーによるAE終了と他担当のAE利用終了が共有された後、`C:/Program Files/Adobe/Common/Plug-ins/7.0/MediaCore/AOD_FakeBlur.aex`を0.3.0へ差し替えました。差し替え時にAfterFX／aerenderプロセスがないことを確認し、インストール済みファイルと検証済みビルドのSHA-256一致を確認しました。

SHA-256: `480C922E256576D127A031750CDDAA611B231C7E308943F51431C0683469DF71`（7,674,880 bytes）。インストール記録は`target/fake-displacement-20261113/install-result.json`、旧版バックアップは`target/fake-displacement-20261113/AOD_FakeBlur-0.2.0.aex.bak`です。

今回のUI・見た目のAE確認はユーザーに依頼し、承認を得ています。既存プロジェクトの再保存はしていません。作業前に記録したAEP／AEPX・カタログ画像の計50ファイルについて、作業後のSHA-256一致を確認しました。

macOS、他社GPU、Premiereでの確認と長時間のMFR負荷試験は未実施です。今回の速度測定は行っておらず、以前のTurbulent Stackの測定値はDisplacementの速度を示すものではありません。

## ユーザー側でのUI・見た目の確認手順

1. AEを起動し、確認用コンポジションのレイヤーへAOD_FakeBlurを新規適用します。Blur Modeの初期値がMipmapで、選択肢がMipmap／Displacement／Downsample / Reconstructになっていることを確認します。
2. 離れた位置に光点や文字を配置し、Displacement、Blur Radius=32、Samples=4、Blur Mapなしにします。各コピーが同じ方向へ移動し、離れた場所でも同じ重なり方になることを確認します。Evolutionで方向群が回転し、Samplesを64などへ増やすとコピーが増えます。Turbulence Scaleは非表示です。
3. 黒・50%灰・白を含む不透明なマップレイヤーをBlur Mapへ指定します。Map Channel=Luminance、Invert Mapオフで、黒部分は元画像、灰部分は半分、白部分は指定半径の変位になることを確認します。必要に応じてマップをプリコンポーズし、入力レイヤーと同じ寸法にします。
4. ProcessingをCPU／GPU (Required)へ切り替えて見た目を比較します。既存の0.2.0プロジェクトでは、旧Turbulent Stackの選択はDisplacementへ置き換わり、Mipmap／Downsampleの選択は維持されます。
