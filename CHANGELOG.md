# Changelog

## 0.1.8 — 2026-10-30

- Claude Code の自動挿入メッセージを本人の判断から除外し、診断用出力では `is_meta: true` として保持。
- Claude Code の発言の由来と Codex の対応する provenance を読み取り、本人の発言と区別。
- `CLAUDE_CONFIG_DIR` / `CODEX_HOME` に対応。明示した CLI の保存先が優先。
- Codex の `token_usage_record` に対応し、旧形式との重複通知や期間外の利用量の混入を抑止。
- source 間でプロンプトの省略処理を共通化し、日本語も500文字を超えた場合だけ省略。
- Claude Code / Codex の Skill の共通手順を同梱。用途の判定、根拠の扱い、一時ファイルの分離、報告テンプレートを改善。
- 保存したレポートを入力と照合する手順を同梱し、guide / insight で判断理由・理解度・感情を補う指示を修正。

通常出力は本人の発言が残らないセッションを含まない。Codex の `sqlite_home` 設定の自動解決と、
成果の完了状態の網羅的な収集は未対応。詳細は `docs/data-sources.md` と `docs/product-jobs.md` を参照。
