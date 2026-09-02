# nippo

Claude Code / Codex の作業履歴から、日報・振り返り・進捗報告を生成するツールです。

Rust 製の collector がローカルのセッション履歴を収集し、Claude Code / Codex の
`nippo` skill が用途に合ったレポートへまとめます。日々の作業を別の場所へ
書き写す必要はありません。

## クイックスタート

### 必要なもの

- Rust 1.85 以上
- Claude Code または Codex

### インストール

```bash
cargo install nippo
nippo skill install
```

`nippo skill install` は、Claude Code 用の `~/.claude/skills/nippo` と
Codex 用の `~/.agents/skills/nippo` をセットアップします。片方だけ使う場合は
`--target claude` または `--target codex` を指定してください。

既存のファイルや想定外のシンボリックリンクを置き換える場合は `--force` が必要です。

```bash
nippo skill install --target codex --force
```

### 日報を作る

レポートを保存したいプロジェクトで、次のコマンドを実行します。

```text
# Claude Code
/nippo

# Codex
$nippo
```

結果は、実行したディレクトリの `reports/nippo-YYYY-MM-DD.md` に保存されます。

アップグレード時は、バイナリとインストール済み skill を揃えてください。

```bash
cargo install nippo
nippo skill install --force
```

nippo のリポジトリ内から `skill install` を実行した場合は、checkout 内の skill へ
シンボリックリンクを作ります。この方式では checkout の変更がそのまま反映されるため、
更新のたびに再インストールする必要はありません。

## 生成される日報

日報には、対象期間、使用した source、主な作業、判断の記録、用語レビュー、統計が入ります。

```markdown
# 日報 2026年08月31日（月）

## 今日の作業

- 作業時間帯: 09:14 〜 18:32
- ソース: requested all | resolved claude, codex
- プロジェクト: nippo
- セッション数: 12

**やったこと**

- collector の source 選択処理を整理
- README の利用手順を更新

## 判断の記録

| 場面 | 選んだこと | 理由 |
| --- | --- | --- |
| ドキュメント構成 | 利用手順を先に置く | 初回利用に必要な情報を探しやすくする |

## 統計

- メッセージ: user 32 / assistant 184
- ツール使用: Read(48), Edit(12), Bash(10)
```

数値は collector の集計結果を使い、作業内容や判断は skill が履歴から整理します。

## コマンド

以下は Claude Code の表記です。Codex では先頭の `/nippo` を `$nippo` に
置き換えてください。

| コマンド | 用途 | 既定の入力 |
| --- | --- | --- |
| `/nippo` | 日報を作る | 今日 |
| `/nippo daily` | `/nippo` の明示的な別名 | 今日 |
| `/nippo brief` | 統計と要点だけを出す | 今日 |
| `/nippo reflection` | 自分で振り返るための問いを出す | 今日 |
| `/nippo plan` | 今日試すことの候補を1〜3件出す | 最新の日報、任意の `ledger.yaml` |
| `/nippo guide` | 作業内容に沿った学習ガイドを作る | 今日 |
| `/nippo report` | 上司・メンター向けの進捗報告を作る | 過去7日 |
| `/nippo review` | 評価面談向けの自己評価を作る | 過去90日 |
| `/nippo insight` | 仕事の進め方や判断の傾向を振り返る | 過去7日 |
| `/nippo trend` | 期間を3分割して変化を見る | 過去90日、最低45日 |
| `/nippo ledger` | 日報の詰まりを累積し、収束・発散の兆候を見る | `reports/nippo-*.md` |

### 期間・プロジェクト・source を絞る

```text
/nippo 3                  # 今日を含む過去3日
/nippo insight 30         # 過去30日の振り返り
/nippo insight 30 nippo   # nippo プロジェクトだけを対象にする
/nippo review 180         # 過去180日の自己評価
/nippo daily claude       # Claude Code の履歴だけを使う
/nippo daily codex        # Codex の履歴だけを使う
/nippo daily all          # Claude Code と Codex の履歴をまとめる
/nippo daily yesterday all  # 昨日分を Claude Code と Codex からまとめる
$nippo daily yesterday all  # 同じことを Codex 側から
```

`yesterday` の他に `today` / `this-week` / `last-week` / `week-before-last` /
`this-month` / `last-month` / `month-before-last` を渡すと、`--period` として
扱われます。

source を指定しない場合は `auto` です。出力先は常にコマンドを実行した
ディレクトリの `reports/` で、同じ日付・同じモードのファイルは最新の結果で
上書きされます。

## 詰まりを継続して見る

日報の `## Unclear points` には、詰まった場面、その原因、次回に使える対処ルールが
記録されます。`/nippo ledger` は複数の日報を時系列で読み、同じ詰まりが減っているか、
増え続けているかを `reports/ledger.yaml` にまとめます。

```text
/nippo ledger
```

繰り返し現れた対処ルールを AGENTS.md / CLAUDE.md の候補として書き出す場合は、
Rust CLI を直接実行します。出力は自動採用せず、内容を確認してから使ってください。

```bash
nippo ledger --export
```

対話画面を使う場合は `tui` feature を有効にしてインストールします。

```bash
cargo install nippo --features tui --force
nippo ledger --tui
```

判定方法の背景は [`docs/reflection-theory.md`](docs/reflection-theory.md) にあります。

## 定期実行

セッション履歴はローカルファイルにあるため、日報の定期生成にはローカルプロジェクトを
扱えるタスクを使います。最初に手動で `/nippo daily` または `$nippo daily` を実行し、
期待する `reports/` に保存されることを確認してください。

### Codex

ChatGPT デスクトップアプリで対象プロジェクトを開き、次のように依頼します。

```text
毎日18:00に、このプロジェクトで $nippo daily codex を実行する定期タスクを作成してください。
ローカルプロジェクトで実行し、worktree は使わないでください。
```

作成後はサイドバーの **Scheduled** で対象プロジェクトと実行時刻を確認します。
ローカルファイルを使うタスクでは、実行時にコンピューターが起動し、デスクトップアプリが
動いている必要があります。Codex CLI と IDE 拡張には Scheduled の管理画面がありません。

詳細は [OpenAI の Scheduled tasks](https://learn.chatgpt.com/docs/automations) を
参照してください。

### Claude Code

Claude Code Desktop の **Code** タブで **Routines** → **New routine** → **Local**
を選び、次の内容を設定します。

- Instructions: `/nippo daily claude`
- Folder: `reports/` を作成したいプロジェクト
- Schedule: `Daily` と任意の時刻
- Worktree: オフ

**Run now** で一度実行し、権限と出力先を確認してください。ローカルタスクは
デスクトップアプリが起動し、コンピューターがスリープしていない間に動きます。

詳細は
[Claude Code Desktop の定期タスク](https://code.claude.com/docs/en/desktop-scheduled-tasks)
を参照してください。開いている CLI セッションの中だけで短期間繰り返す場合は
[`/loop`](https://code.claude.com/docs/en/scheduled-tasks) も使えます。

### 定期実行の注意

- `reports/` はタスクの作業フォルダに作成されます。
- 普段使う checkout に日報を残す場合は worktree を無効にします。
- 両方の source を使う場合は、どちらか一方で `daily all` を実行します。
- 同じ日付の日報を複数回生成すると、後の結果で上書きされます。

## Rust CLI

skill を介さず、collector の結果を直接確認することもできます。既定の出力は JSON です。

```bash
nippo collect --period today
nippo collect --days 7 --format summary
nippo collect --source codex --period today
nippo collect --source opencode --period today
nippo collect --source all --days 7
nippo collect --period last-week
nippo collect --from 2026-08-01 --to 2026-08-31
nippo collect --project nippo
```

調査時だけ使うオプション:

```bash
nippo collect --include-prompt-noise  # 定型通知なども含める
nippo collect --include-self          # 実行中のセッションも含める
nippo collect --days 0                # 全期間
```

日付境界は実行環境のローカルタイムゾーンを使います。collector は同じ session ID の
分割記録を統合し、既定では実行中のセッションと定型的なプロンプトノイズを除外します。

保存場所、JSON の項目、source の解決順は
[`docs/data-sources.md`](docs/data-sources.md) を参照してください。利用可能な全オプションは
`nippo collect --help` で確認できます。

## 仕組み

```text
Claude Code / Codex のローカル履歴
                  |
                  v
          nippo collect (Rust)
                  |
                  v
             collector JSON
                  |
                  v
       nippo skill + docs/templates
                  |
                  v
              reports/*.md
```

collector はデータ収集と決定的な集計だけを担当します。文章の要約や振り返りは、
各モードのテンプレートを読む skill が担当します。

データ収集先:

- Claude Code: `~/.claude/projects/**/*.jsonl`
- Codex: `~/.codex/history.jsonl`、`state_5.sqlite`、thread が参照する rollout JSONL
- `logs_2.sqlite` は診断用で、日報の主データソースには使いません

## テンプレートを変更する

[`docs/templates/`](docs/templates/) に各モードのテンプレートがあります。

- `nippo-template.md`: 日報
- `reflection-template.md`: 振り返りの問い
- `plan-template.md`: 今日の実験候補
- `guide-template.md`: 学習ガイド
- `report-template.md`: 進捗報告
- `review-template.md`: 自己評価
- `insight-template.md`: 期間の振り返り
- `trend-template.md`: 三分割の変化分析

checkout へのシンボリックリンク方式で skill を配置している場合、テンプレートの変更に
再ビルドは不要です。埋め込みファイルを書き出した環境では、変更を反映するために
`nippo skill install --force` を再実行してください。

## 設計方針

nippo は、記録の収集と、人が行う振り返りを分けています。

| nippo が支援すること | 自分で行うこと |
| --- | --- |
| 作業時間・プロジェクト・ツール使用の集計 | 判断の背景を自分の言葉にする |
| 判断ポイントや詰まりの抽出 | 問いに答えて経験を振り返る |
| 期間ごとの傾向の整理 | 次に試すことを選ぶ |

`/nippo reflection` は回答を埋めず、問いと空欄を出力します。振り返りに使っている理論と
各モードの役割は [`docs/reflection-theory.md`](docs/reflection-theory.md) にまとめています。

### Claude Code の `/insights` との違い

`/nippo insight` は、判断・行動・思考など、自分の仕事の進め方を振り返るための機能です。
Claude Code 組み込みの `/insights` は、設定やフックなど Claude Code の使い方を
改善するための機能です。目的に応じて使い分けてください。

## データの扱い

- collector はローカルの履歴を読み取り、使用中の Claude Code / Codex がレポートへ
  まとめて `reports/` に保存します。
- `reports/*.md` と `reports/ledger.yaml` は `.gitignore` の対象です。
- レポートには作業内容や内省が含まれるため、共有前に内容を確認してください。

## 開発

主な変更場所:

- [`crates/collector/src/main.rs`](crates/collector/src/main.rs): CLI と source 選択
- [`crates/collector/src/filter.rs`](crates/collector/src/filter.rs): 日付・期間フィルタ
- [`crates/collector/src/session.rs`](crates/collector/src/session.rs): source 共通のセッション表現
- [`crates/collector/src/output.rs`](crates/collector/src/output.rs): JSON と summary の生成
- [`crates/collector/src/sources/`](crates/collector/src/sources/): 履歴パーサ
- [`.claude/skills/nippo/SKILL.md`](.claude/skills/nippo/SKILL.md): Claude Code 用 skill
- [`.agents/skills/nippo/SKILL.md`](.agents/skills/nippo/SKILL.md): Codex 用 skill

検証コマンド:

```bash
cargo fmt --all
cargo clippy -p nippo -- -D warnings
cargo test -p nippo
```

新しいモードや source を追加する場合は、collector だけでなく README、
[`docs/data-sources.md`](docs/data-sources.md)、関連する両方の skill も更新してください。

リリース用 package は `crates/collector/assets/` のシンボリックリンクをたどって
skill とテンプレートを埋め込みます。Windows で package を作る場合は、git checkout で
シンボリックリンクを有効にしてください。

## ライセンス

MIT
