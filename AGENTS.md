# nippo

Claude Code / Codex の作業履歴から日報・リフレクションを生成するリポジトリ。
Rust バイナリがデータ収集を担当し、Claude Code と Codex の skill がレポート生成を担当する。

## Read First

- `README.md`
- `docs/data-sources.md`
- `docs/templates/`

## Development

```bash
cargo fmt --all
cargo clippy -p nippo -- -D warnings
cargo test -p nippo
```

- Rust edition 2024、Rust 1.85+ 前提
- `.unwrap()` は避け、`anyhow::Result` と `?` を使う

## Change Map

- `crates/collector/src/main.rs`: CLI、source 選択、auto 判定
- `crates/collector/src/filter.rs`: 日付・期間フィルタ
- `crates/collector/src/output.rs`: JSON / summary 出力、decision 抽出
- `crates/collector/src/session.rs`: source 共通のセッション表現
- `crates/collector/src/sources/claude_code.rs`: Claude Code パーサ
- `crates/collector/src/sources/codex.rs`: Codex 履歴パーサ
- `.claude/skills/nippo/SKILL.md`: Claude Code 用 skill
- `.agents/skills/nippo/SKILL.md`: Codex 用 skill

## Rules

- データ収集は Rust バイナリのみ。Python スクリプトを追加しない
- `reports/` はコマンドを実行した cwd に出力する
- 新しいモードや source を追加したら `README.md`、`docs/data-sources.md`、関連 skill を更新する
- `Codex` の `logs_2.sqlite` は診断ログ。日報の主データソースにしない
