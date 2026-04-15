---
name: nippo
description: >
  Claude Code / Codex のセッションログから日報・リフレクション・インサイトを生成する。
  /nippo と /nippo daily で日報、/nippo reflection で内省の問い、/nippo guide で学習支援、
  /nippo report で進捗報告、/nippo review で自己評価、/nippo insight で深い振り返り、
  /nippo trend で長期変化分析を生成する。Rust バイナリ (nippo) でデータを収集する。
argument-hint: "[mode] [days] [project] [source]"
allowed-tools: Read, Write, Bash(nippo *), Bash(cargo run *), Bash(mkdir *), Bash(gh issue *)
context: fork
---

# データ収集結果

!`if [ -f Cargo.toml ] && grep -q 'members = \["crates/collector"\]' Cargo.toml 2>/dev/null; then cargo run -q -p nippo -- collect --period today --format summary 2>/dev/null; else nippo collect --period today --format summary 2>/dev/null; fi || echo "nippo コマンドが見つかりません。cargo install nippo でインストールしてください。詳細: https://github.com/nwiizo/nippo"`

# 指示

上記のデータと `$ARGUMENTS` に基づいてレポートを生成する。

## ルール

- データ収集は `nippo` CLI のみ。**Python は絶対に使わない**（python, python3, python -c すべて禁止）
- `stats` の集計済みデータは**直接引用する。再計算しない**
- 書籍・URL は紹介しない。概念名・検索キーワードを示す
- レポートは日本語で出力する
- 出力先は cwd の `reports/` 配下（なければ `mkdir -p reports`）
- ファイル名: `reports/{モード}-YYYY-MM-DD.md`（期間 N>1 なら `-Nd` を付与）
- 日付境界は実行環境のローカルタイムゾーン基準。`--days 1` と `daily` は「今日のローカル日付」を意味する
- デフォルト source は `auto`。Codex では `history.jsonl` と `state_5.sqlite`、および `rollout_path` が指す rollout データを使う。`logs_2.sqlite` は診断用で、日報の主データソースにはしない
- このリポジトリ内で実行している場合は、グローバル `nippo` より `cargo run -q -p nippo -- collect ...` を優先する（ローカル実装が新しい可能性があるため）
- 日報モードでは、このターンで取得した JSON を唯一の根拠にする。既存の `reports/nippo-YYYY-MM-DD.md` は読まないし、続きから直さない
- 日報のヘッダと統計は `meta` と `stats` をそのまま使う。`meta.source` `meta.total_sessions` `stats.projects_worked_on` `stats.tool_frequency` を推測で置き換えない
- 日報本文のプロジェクト節は `stats.projects_worked_on` の順で選ぶ。上位 3〜5 プロジェクトは必ず個別に触れ、残りだけを `その他` にまとめる
- `decisions` を一部だけ載せる場合は「全N件中M件を記載」と明記する
- このレポート生成・修正そのものが小さな `nippo` プロジェクトとして混ざることがある。その場合でもヘッダの統計は維持しつつ、本文では「日報生成・修正」と明示して軽く扱う
- 参考リンクの URL はそのまま貼らず、末尾の日本語や句読点を落として正しい URL だけを残す

## モード決定

`$ARGUMENTS` をトリミングし、先頭単語でモードを決定する:

| 先頭単語 | モード | デフォルト期間 | 収集コマンド |
|---------|--------|-------------|-------------|
| (空) | 日報 | 1日 | `nippo collect --period today` |
| daily | 日報 | 1日 | `nippo collect --period today` |
| brief | brief | 1日 | `nippo collect --period today --format summary`（そのまま保存） |
| reflection | reflection | 1日 | `nippo collect --period today` |
| guide | guide | 1日 | `nippo collect --period today` |
| report | report | 7日 | `nippo collect --days 7 --stats-only` |
| review | review | 90日 | `nippo collect --days 90 --stats-only` |
| insight | insight | 7日 | `nippo collect --days 7` |
| trend | trend | 90日 | 3回 `nippo collect --from X --to Y --format summary` |
| (数値のみ) | 日報 | その数値 | `nippo collect --days N` |

`daily` は `(空)` と同じ日報モードのエイリアス。出力ファイル名は `reports/nippo-YYYY-MM-DD.md` を使う。

残りトークンのうち `claude` / `codex` / `all` は `--source` に渡す。数値があれば `--days` を置換。それ以外の文字列は `--project` に渡す。

## 収集と生成

1. このリポジトリ内なら `cargo run -q -p nippo -- collect ...`、それ以外は `nippo collect ...` を Bash で実行（brief は出力を直接保存して完了）
2. JSON を Read で読み込む
3. モードに対応するテンプレートを Read で読み込む

テンプレートは `${CLAUDE_SKILL_DIR}/docs/templates/` にある:

| モード | テンプレートファイル | 補足 |
|--------|-------------------|------|
| 日報 | `${CLAUDE_SKILL_DIR}/docs/templates/nippo-template.md` | 用語レビュー含む |
| reflection | `${CLAUDE_SKILL_DIR}/docs/templates/reflection-template.md` | **回答は書かない** |
| guide | `${CLAUDE_SKILL_DIR}/docs/templates/guide-template.md` | 回答 + 概念 + 多角的フィードバック |
| report | `${CLAUDE_SKILL_DIR}/docs/templates/report-template.md` | 成果 + 課題。感情は含めない |
| review | `${CLAUDE_SKILL_DIR}/docs/templates/review-template.md` | 成果の定量化 + 成長 + 次期目標 |
| insight | `${CLAUDE_SKILL_DIR}/docs/templates/insight-template.md` | ALACT モデルで回答付き |
| trend | `${CLAUDE_SKILL_DIR}/docs/templates/trend-template.md` | 3期間の比較。最低45日 |

reflection / guide / insight は `${CLAUDE_SKILL_DIR}/docs/reflection-theory.md` も Read する。
reflection / guide は同日の `reports/nippo-YYYY-MM-DD.md` があれば Read する。

4. テンプレートに従いレポートを Write で保存（日報モードは既存ファイルがあっても上書き）
5. パスをユーザーに通知

## 改善提案

レポート生成中に Claude が自前で集計・加工していることに気づいたら、レポート出力後に `gh issue create --repo nwiizo/nippo` で Rust 側への移行を提案する（既存 Issue と重複確認、1回最大2件）。
