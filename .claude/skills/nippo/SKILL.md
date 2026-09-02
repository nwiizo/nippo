---
name: nippo
description: >
  ユーザーが日報・振り返り・作業まとめ・週報・自己評価を求めたときに、
  Claude Code / Codex のセッションログから日報・リフレクション・インサイトを生成する。
  /nippo と /nippo daily で日報、/nippo reflection で内省の問い、/nippo guide で学習支援、
  /nippo report で進捗報告、/nippo review で自己評価、/nippo insight で深い振り返り、
  /nippo trend で長期変化分析、/nippo plan で朝の行動実験、
  /nippo ledger で詰まりの累積集計と収束/発散判定を生成する。
  Rust バイナリ (nippo) でデータを収集する。
argument-hint: "[mode] [days|period] [project] [source]"
allowed-tools: Read, Write, Glob, Bash(nippo *), Bash(cargo run -q -p nippo *), Bash(mkdir -p reports), Bash(mkdir -p tmp), Bash(rm -f tmp/nippo-raw.json), Bash(gh issue list *), Bash(gh issue create *)
context: fork
---

# 指示

`$ARGUMENTS` に基づいてモードと収集条件を決め、必要なデータだけを収集してレポートを生成する。

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
- モードと引数を決める前にデータを先読みしない。同じ条件の収集は 1 回だけ実行する
- 収集 JSON の一時ファイル `tmp/nippo-raw.json` はレポート保存後に必ず削除する。収集や生成に失敗して停止する場合も削除する
- コレクターは定型的なプロンプトノイズを既定で除外する。レポート生成では `--include-prompt-noise` を付けない
- 日報モードでは、このターンで取得した JSON を唯一の根拠にする。既存の `reports/nippo-YYYY-MM-DD.md` は読まないし、続きから直さない
- 日報のヘッダと統計は `meta` と `stats` をそのまま使う。`meta.source` `meta.period` `meta.total_sessions` `stats.projects_worked_on` `stats.tool_frequency` を推測で置き換えない
- 対象期間を記載する全モードで `meta.period.from` と `meta.period.to` を使い、実行日から日付を計算し直さない
- 日報本文のプロジェクト節は `stats.projects_worked_on` の順（`message_count` 降順）で選ぶ。上位 3〜5 プロジェクトは必ず個別に触れ、残りだけを `その他` にまとめる
- Codex 由来のレポートは assistant/tool のメトリクスが疎になることがある。数値を捏造せず、疎であることを明示する
- source の解決やデータ欠損について質問されたら `${CLAUDE_SKILL_DIR}/docs/data-sources.md` を Read する
- `decisions` を一部だけ載せる場合は「全N件中M件を記載」と明記する
- 現在の Claude Code / Codex セッションはコレクターが除外するため、`meta` や `stats` から手作業でセッション数を引かない
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
| trend | trend | 90日 | 期間を3等分し、区間ごとに `nippo collect --from X --to Y --format summary` を3回 |
| plan | plan | なし（最新の日報） | 収集なし。`reports/` の既存ファイルを Read |
| ledger | ledger | なし（`reports/nippo-*.md`） | `nippo ledger`（collect は実行しない） |
| (数値のみ) | 日報 | その数値 | `nippo collect --days N` |

`daily` は `(空)` と同じ日報モードのエイリアス。出力ファイル名は `reports/nippo-YYYY-MM-DD.md` を使う。

残りトークンのうち `claude` / `codex` / `opencode` / `all` は `--source` に渡す。`today` / `yesterday` / `this-week` / `last-week` / `week-before-last` / `this-month` / `last-month` / `month-before-last` は `--period` に渡す。この場合、モード既定の `--days` や `--period today` は付けない（明示された `--period` が唯一の期間指定になる）。数値があれば `--days` を置換。それ以外の文字列は `--project` に渡す。先頭単語がモード名・数値・source・period のいずれでもない場合は日報モードとして扱い、その単語を `--project` に渡す。

`--period` を指定して `daily` を実行する場合、出力ファイル名の日付は collector が返す `meta.period.to` を使う。実行日から計算し直さない。

## 収集と生成

`plan` モードは収集を行わない。Glob で cwd の `reports/nippo-*.md` を探し、
ファイル名の日付が最新のものを Read する。`reports/ledger.yaml` が存在すればそれも
Read し、最近も再出現している未収束の General Fix Rule を候補材料にする。
`${CLAUDE_SKILL_DIR}/docs/templates/plan-template.md` と
`${CLAUDE_SKILL_DIR}/docs/reflection-theory.md` を Read し、候補を 1〜3 個だけ提示する。
選ぶ実験・理由・最初の一手の記入欄は空白のまま
`reports/plan-YYYY-MM-DD.md` に保存し、パスを通知して完了。

`ledger` モードは収集・テンプレートを使わない。このリポジトリ内なら `cargo run -q -p nippo -- ledger`、それ以外は `nippo ledger` を実行する。cwd の `reports/nippo-*.md` から `## Unclear points` セクションを横断パースして `reports/ledger.yaml` に累積し、CONVERGED / DIVERGENCE-SIGNAL / CONTINUE の判定を出力する。この判定はそのままユーザーに伝え、過剰に解釈しない。利用者が agent 設定向けの候補を求めた場合は `ledger --export` を実行する。複数回出現した General Fix Rule が `reports/ledger-export.md` に出力されるが、候補にすぎないため、CLAUDE.md / AGENTS.md へコピーする前に人間が取捨選択すると伝える。以上で完了。

その他のモード:

1. `mkdir -p tmp` を実行する。このリポジトリ内なら `cargo run -q -p nippo -- collect ...`、それ以外は `nippo collect ...` を Bash で 1 回実行し、JSON を `tmp/nippo-raw.json` にリダイレクトする（brief はリダイレクトせず出力を直接保存して完了）
2. `tmp/nippo-raw.json` を Read で読み込む。大きい場合は分割して読み、途中で切れた Bash 出力から JSON を推測しない
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
| plan | `${CLAUDE_SKILL_DIR}/docs/templates/plan-template.md` | 候補のみ提示。選択と記入は利用者 |

reflection / guide / insight / plan は `${CLAUDE_SKILL_DIR}/docs/reflection-theory.md` も Read する。
reflection / guide は同日の `reports/nippo-YYYY-MM-DD.md` があれば Read する。

4. テンプレートに従いレポートを Write で保存（日報モードは既存ファイルがあっても上書き）
5. `rm -f tmp/nippo-raw.json` で収集 JSON を削除する
6. パスとレポートの要点を簡潔に通知する。改善提案は後述の条件を満たす場合だけ続ける
7. 日報（`Unclear points` セクションを含む）を生成した後は、`/nippo ledger` で詰まりを累積集計できることを一言案内する（自動では実行しない）

## 改善提案と Issue 作成

レポート生成中に、同じ決定的な集計・整形をモデルが繰り返し担う不足や、複数モードへ影響する構造的な問題を見つけた場合は Issue を提案できる。個別の不足をばらばらに起票せず、同じ原因から生じる症状は一つの課題としてまとめる。1 回のレポート生成で提案するのは最大 2 件とする。

次は既存データから直接わかるため、改善候補にしない:

- プロジェクト総数: `stats.projects_worked_on` の要素数
- 対象期間: `meta.period`
- 上位ツールと主要時間帯: `render_helpers`
- テンプレート内だけで完結する文章表現や要約

提案前に `gh issue list --repo nwiizo/nippo --state all` で重複を確認する。既存 Issue で扱われている場合や、既存フィールドの読み方を直せば済む場合は提案しない。新しい Issue が必要な場合は、確認できた事実、影響するモード、共通の原因、解決後に省ける処理をまとめたタイトルと本文案をユーザーに示し、明示的な了承を得てから `gh issue create --repo nwiizo/nippo` を実行する。
