# Rustでのカスタムパラメーター：カーブUI

TextureStroke 0.5.0で、`after-effects` crateの任意データ型パラメーターとDrawbotによるカーブUIを実装しました。Windows / After Effects 2025 **25.6.6x4**で表示、追加、ドラッグ、Undo、キーフレーム補間、AEP保存・再読込を確認しています。macOSとPremiereは未検証です。

実装は[曲線とイベント](../plugins/texture-stroke/src/curves.rs)、[パラメーター定義](../plugins/texture-stroke/src/dynamics.rs)、[コマンド振り分け](../plugins/texture-stroke/src/lib.rs)、[PiPL生成](../plugins/texture-stroke/build.rs)を参照してください。使用したbindingsはリポジトリで固定している`after-effects`のリビジョン`0e0b197`です。

## 登録

1. PiPLに`pipl::OutFlags::CustomUI`、`GlobalSetup`に`ae::OutFlags::CustomUi`を設定します。動的表示には両方の`SendUpdateParamsUI`も必要です。
2. `ArbitraryDef::new()`に`set_default(Curve::default())`を設定します。`Parameters::add_customized`で`ParamFlag::SUPERVISE`と`ParamUIFlags::CONTROL`、UIの幅・高さを指定します。TextureStrokeは300×218です。
3. `params_setup`内で`in_data.interact().register_ui(CustomUIInfo::new().events(CustomEventFlags::EFFECT))`を呼びます。
4. `Command::ArbitraryCallback`で全カーブに対して`extra.dispatch::<Curve, Params>(parameter_id)`を呼びます。対象のID判定とnew/dispose/copy/flat-size/flatten/unflatten/compare/interpolate/print/scanはbindingsのdispatcherが扱います。独自に一部のコールバックだけ実装してはいけません。
5. `Command::Event`をカーブ処理へ渡します。ECWの`EffectArea::Control`だけを処理し、`param_index()`で対象のカーブを特定します。

既存パラメーターのenum名を変更しないでください。このbindingsはenum名から永続IDを生成し、任意データのIDも登録時に設定します。カーブの表示順と保存IDは別物です。

## データ・描画・編集

曲線データは`Serialize` / `Deserialize`を実装し、固定長8点の配列、使用点数、形式バージョンを持たせています。ハンドル内に`Vec`やOSポインターを持ち込まず、読み取り時に点数・有限値・Xの順序・端点を検証します。形式変更時にはバージョンと移行方法を検討してください。

`ArbitraryData<Curve>::interpolate`は点数の異なる曲線同士でも動作するよう、8つの共通X座標で評価したYを補間します。描画は単調区間内でオーバーシュートしないcubic Hermite補間です。キー間では点の位置の移動ではなく、曲線の出力を補間します。

Drawbotのdrawing referenceは`EffectCustomUI::drawing_reference(event.context_handle())`から取得します。グラフを`event.current_frame()`の原点に配置し、ウィンドウや画面全体の原点と混同しないでください。supplierから作るpen/path/font等はRAIIで破棄されます。

値の変更はクリック・ドラッグイベントで`params.get_mut(id)?.as_arbitrary_mut()?.set_value(curve)`を使用します。これにより変更フラグも設定されます。続けて`App::invalidate_rect`を呼び、`HANDLED_EVENT | ALWAYS_UPDATE | UPDATE_NOW`を返すと、UIとコンポジションが更新されます。`UpdateParamsUi`では表示・有効状態だけを変更し、曲線の値を変更しません。

ドラッグする点の番号は`continue_refcon[0]`へ入れ、クリックとドラッグで`send_drag`を設定します。点やハンドルへのポインターは保持しません。**AE 2025の実測では、DRAGの`screen_point`がクリック開始位置のままでした。** ドラッグ中は`ae::suites::App::mouse_position()`（SDKの`PF_GetMouse`）から現在のECW内座標を取得すると追従します。Drawイベント内ではマウス取得を呼びません。

関連するAPI仕様はSDKガイドの[イベント](https://ae-plugins.docsforadobe.dev/effect-ui-events/PF_EventUnion/)と[PF_GetMouse・InvalidateRect](https://ae-plugins.docsforadobe.dev/effect-details/useful-utility-functions/)を参照してください。

## 日本語WindowsでのPiPL破損と対処

固定リビジョンの`pipl` crateは、バイナリを`"\x.."`形式のRC文字列として出力します。今回のフラグ組合せでは、期待するOutFlagsのバイト列`46 84 00 06`がWindowsリソースコンパイラーを通ると`46 81 45 00 06`へ変わりました。日本語コードページで`0x84`が2バイトへ変換され、後続のフィールドがずれます。AEはこのプラグインを正常に読み込めませんでした。

TextureStrokeでは次のように、**文字列を通さずバイナリファイルとして埋め込む**ことで解決しています。

```rust
pipl::plugin_build(properties()); // cfg/env生成は通常どおり使用
#[cfg(windows)]
{
    let path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("texture_stroke.pipl");
    std::fs::write(&path, pipl::build_pipl(properties()).unwrap()).unwrap();
    let filename = path.to_str().unwrap().replace('\\', "/");
    let mut resource = winres::WindowsResource::new();
    resource.append_rc_content(&format!("16000 PiPL DISCARDABLE \"{filename}\""));
    resource.compile().unwrap();
}
```

`properties`は同じ定義のVecを返すクロージャーです。Windows限定のbuild dependencyに`winres = "0.1"`を追加し、`plugin_build`が作った同じ`OUT_DIR/resource.lib`を最後に置き換えます。修正後は埋め込みPiPLと元ファイルが**358バイトで完全一致**しました。

```text
python plugins/texture-stroke/tests/verify_pipl.py target/release/texture_stroke.dll <OUT_DIR>/texture_stroke.pipl
```

この対処は当該プラグインに限定しています。将来piplを更新する際は、RC生成実装と実バイナリの一致を確認し、不要になれば取り除いてください。CustomUIフラグだけを外すと、UI登録そのものが成立しません。

## 再検証

- `cargo test -p texture_stroke`：曲線の範囲、評価、マップ座標、周辺パス、時間端点。
- AEで`ae_dynamics.jsx`を実行し、`verify_ae_dynamics.py`で出力を比較します。出力ディレクトリは作業ツリー内を指定します。
- `dynamics.aep`のPath MetricsコンポでSize Responseを開き、点追加・ドラッグ・Flat・Undo、キーフレームを操作します。変更に応じて描画が変わることと、保存・再読込後の出力が一致することを確認します。
- 各パラメーターのInputを切り替え、不要なマップ選択や基準半径の項目が隠れることも確認します。

実機では0秒の線形カーブと1秒のFlatカーブを作り、0.5秒の描画が両端の中間になることを確認しました。同じ0.5秒の画像はAEP保存・再読込前後で全ピクセルが一致しました。

新しいCustom UIを追加するときは、まず最小のArbitraryパラメーター1個で登録・表示・変更・保存を通し、その後にコントロールを増やすと問題を切り分けやすくなります。
