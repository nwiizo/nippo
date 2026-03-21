---
name: nippo
description: >
  Claude Code の JSONL セッションログから日報・リフレクション・インサイトを生成する。
  /nippo で日報、/nippo reflection で内省の問い、/nippo guide で学習支援、
  /nippo report で進捗報告、/nippo review で自己評価、/nippo insight で深い振り返り、
  /nippo trend で長期変化分析を生成する。Rust バイナリ (nippo) でデータを収集する。
argument-hint: "[mode] [days] [project]"
allowed-tools: Read, Write, Bash(nippo *), Bash(mkdir *), Bash(gh issue *)
context: fork
---

# データ収集結果

!`nippo collect --days 1 --format summary 2>/dev/null || echo "nippo コマンドが見つかりません。cargo install nippo でインストールしてください。"`

# 指示

上記のデータと `$ARGUMENTS` に基づいてレポートを生成する。

## ルール

- データ収集は `nippo` CLI のみ。Python は使わない。他のスキルを参照しない
- `stats` の集計済みデータは**直接引用する。再計算しない**
- 書籍・URL は紹介しない。概念名・検索キーワードを示す
- レポートは日本語で出力する
- 出力先は cwd の `reports/` 配下（なければ `mkdir -p reports`）
- ファイル名: `reports/{モード}-YYYY-MM-DD.md`（期間 N>1 なら `-Nd` を付与）

## モード決定

`$ARGUMENTS` をトリミングし、先頭単語でモードを決定する:

| 先頭単語 | モード | デフォルト期間 | 収集コマンド |
|---------|--------|-------------|-------------|
| (空) | 日報 | 1日 | `nippo collect --days 1` |
| brief | brief | 1日 | `nippo collect --days 1 --format summary`（そのまま保存） |
| reflection | reflection | 1日 | `nippo collect --days 1` |
| guide | guide | 1日 | `nippo collect --days 1` |
| report | report | 7日 | `nippo collect --days 7 --stats-only` |
| review | review | 90日 | `nippo collect --days 90 --stats-only` |
| insight | insight | 7日 | `nippo collect --days 7` |
| trend | trend | 90日 | 3回 `nippo collect --from X --to Y --format summary` |
| (数値のみ) | 日報 | その数値 | `nippo collect --days N` |

残りトークンに数値があれば `--days` を置換。モード名でも数値でもない文字列は `--project` に渡す。

## 収集と生成

1. 上のテーブルに従い `nippo collect` を Bash で実行（brief は出力を直接保存して完了）
2. JSON を Read で読み込む
3. モードに対応するテンプレートを Read で読み込む（パスは `${CLAUDE_SKILL_DIR}` からの相対）

| モード | テンプレート | 補足 |
|--------|-----------|------|
| 日報 | docs/templates/nippo-template.md | 用語レビュー含む |
| reflection | docs/templates/reflection-template.md | **回答は書かない**。docs/reflection-theory.md 参照 |
| guide | docs/templates/guide-template.md | 回答 + 概念 + 多角的フィードバック。docs/reflection-theory.md 参照 |
| report | docs/templates/report-template.md | 成果 + 課題。感情は含めない |
| review | docs/templates/review-template.md | 成果の定量化 + 成長 + 次期目標 |
| insight | docs/templates/insight-template.md | ALACT モデルで回答付き。docs/reflection-theory.md 参照 |
| trend | docs/templates/trend-template.md | 3期間の比較。最低45日。未満なら insight を推奨 |

reflection / guide は同日の `reports/nippo-YYYY-MM-DD.md` があれば Read する。

4. テンプレートに従いレポートを Write で保存
5. パスをユーザーに通知

## 改善提案

レポート生成中に Claude が自前で集計・加工していることに気づいたら、レポート出力後に `gh issue create --repo nwiizo/nippo` で Rust 側への移行を提案する（既存 Issue と重複確認、1回最大2件）。
