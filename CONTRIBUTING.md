# Contributing

Thank you for contributing. Please follow [AGENTS.md](AGENTS.md) for development
and validation, and [LICENSING.md](LICENSING.md) for the licensing boundaries.

By intentionally submitting a contribution for inclusion in this repository,
you agree to license the rights you control in that contribution under MPL-2.0.
For contributions to the eight scaffold files listed in
[TEMPLATE-LICENSE.txt](templates/plugin/TEMPLATE-LICENSE.txt), you also expressly
grant the Template Output Permission in that file for those contributions.
This lets downstream developers continue using the scaffold for proprietary
plugins while distributions of the template itself remain subject to the MPL.

Only contribute material you have sufficient rights to license on those terms.
Identify third-party material and its original source and notices in the pull
request; do not assume that moving existing MPL-only code into a template grants
the additional permission. Maintainers must verify that imported material can
be released on the destination file's terms before accepting it.

For new effects contributed to `plugins/`, use `cargo new-plugin`. Their own
effect implementations are MPL-2.0; the template permission does not extend to
those implementations merely because they started from the scaffold. Existing
contributions are not retroactively relicensed by this document.

## 日本語

本リポジトリへ取り込む目的で変更を投稿する場合、投稿者が許諾できる権利について MPL-2.0 で提供することに同意してください。追加許諾で列挙した8つの生成用ファイルへの寄与には、Template Output Permission も明示的に付与するものとします。

第三者のコードは元の出典と通知を明示してください。既存の MPL 限定コードをテンプレートへ移すだけでは追加許諾を付けられません。取り込み先の条件で許諾できる権利があることを確認します。

`plugins/` に追加する独自のエフェクト実装は MPL-2.0 です。新規作成には `cargo new-plugin` を使用してください。この投稿条件によって過去の寄与が自動的に再許諾されることはありません。
