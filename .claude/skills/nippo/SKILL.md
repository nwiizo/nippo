---
name: nippo
description: >
  日報・リフレクション・インサイトを生成する。
  `/nippo` で今日の事実を自動収集、`/nippo reflection` で内省の足場を生成、
  `/nippo insight` で週次・月次の深い振り返りを生成する。
disable-model-invocation: true
allowed-tools: Read, Write, Bash, Glob, Grep
context: fork
---

# nippo スキル

**最初に必ず `$ARGUMENTS` を確認し、モードと期間を決定してから処理を開始すること。**

`$ARGUMENTS` の値: `$ARGUMENTS`

上記の値を以下のルールで解析する:
- `insight 30` → **insight モード、30日間**（日報ではない）
- `reflection` → **reflection モード**（日報ではない）
- `guide 5` → **guide モード、5日間**（日報ではない）
- `report` → **report モード、7日間**（日報ではない）
- `review` → **review モード、90日間**（日報ではない）
- `trend 90` → **trend モード、90日間**（日報ではない）
- `brief` → **brief モード**（日報ではない）
- `(空)` → **日報モード、1日間**

**引数がある場合はデフォルトの日報モードで実行してはならない。**

---

Claude Code の作業ログ（JSONL）から日報を自動生成し、利用者の内省を促すツール。

**設計思想**: 事実の収集は自動化するが、リフレクション自体は利用者が行う。
思考・努力・内省を外注させない。ツールがやるのは「事実の提示」と「問いの提示」まで。

**このスキルは自己完結している。**
- データ収集は本リポジトリの **Rust バイナリ (`nippo`)** のみで行う
- Python スクリプトは使わない
- 他のスキルのスクリプトやデータを参照・実行しない
- 外部ツールへの依存は一切ない

## 引数の処理（最初に必ず実行）

`$ARGUMENTS` を解析する。全コマンドで期間（日数）を指定できる。

### コマンド一覧

| 引数 | デフォルト期間 | 動作 |
|------|-------------|------|
| `(空)` | 1日 | 日報（事実 + 意思決定 + 用語レビュー） |
| `brief` | 1日 | 端的な日報（統計と要点のみ） |
| `reflection` | 1日 | リフレクション足場（問いのみ、回答は書かない） |
| `guide` | 1日 | ガイド（問い + Claude の回答付き） |
| `insight` | 7日 | 深い振り返り（ALACT モデル、回答付き） |
| `report` | 7日 | 上司・メンター向けの進捗報告 |
| `review` | 90日 | 評価面談・自己評価用の成果まとめ |
| `trend` | 90日 | 期間を三分割し、変化の推移を分析（最低45日） |

### 期間指定

全コマンドに数値を付けると期間を変更できる:

| 例 | 動作 |
|----|------|
| `/nippo 3` | 過去3日分の日報 |
| `/nippo brief 7` | 過去7日分の端的な日報 |
| `/nippo reflection 3` | 過去3日分のリフレクション |
| `/nippo guide 5` | 過去5日分のガイド |
| `/nippo insight 30` | 過去30日分の insight |
| `/nippo trend 90` | 過去90日分のトレンド分析 |

### 引数パースルール

1. `$ARGUMENTS` を空白で分割する
2. 先頭トークンがコマンド名（`brief`, `reflection`, `guide`, `report`, `review`, `insight`, `trend`）ならそのモード
3. トークンに数値があれば期間（日数）として使う
4. 数値でもコマンド名でもない文字列があれば**プロジェクト名フィルタ**として `--project` に渡す
5. コマンド名がなければデフォルトは日報（`nippo`）モード
6. 数値がなければ各コマンドのデフォルト期間を使う

例:
- `/nippo insight 30 nippo` → 過去30日、nippo プロジェクトのみ
- `/nippo insight ccswarm` → 過去7日（デフォルト）、ccswarm プロジェクトのみ
- `/nippo guide 5 oitoriaezu-owarasero` → 過去5日、指定プロジェクトのみ

## ステップ1: データ収集

上で決定したモードと期間に応じて `nippo collect` を実行する。

### 引数からコレクターオプションを組み立てる

| コマンド | コレクター実行 |
|---------|--------------|
| `/nippo` | `nippo collect --days 1` |
| `/nippo brief` | `nippo collect --days 1 --format summary` |
| `/nippo reflection` | `nippo collect --days 1` |
| `/nippo guide` | `nippo collect --days 1` |
| `/nippo insight` | `nippo collect --days 7` |
| `/nippo insight nippo` | `nippo collect --days 7 --project nippo` |
| `/nippo report` | `nippo collect --days 7` |
| `/nippo review` | `nippo collect --days 90` |
| `/nippo trend` | 3回実行（期間を三分割。後述） |
| 期間指定あり | `--days N` に置き換え |

`--format` オプション:
- `json`（デフォルト）: 構造化 JSON。`/nippo`, `/nippo reflection`, `/nippo guide`, `/nippo insight` で使用
- `summary`: テキストサマリー。`/nippo brief` で使用（Claude による加工不要でそのまま出力）

実行:

```bash
nippo collect --days N > /tmp/nippo-data.json
```

### 出力の読み取り

`/tmp/nippo-data.json` を Read で読み込む。

出力JSON構造:
```json
{
  "meta": {
    "generated_at": "...",
    "filter_days": 1,
    "total_sessions": 5,
    "total_files_scanned": 1839
  },
  "sessions": [
    {
      "session_id": "...",
      "project": "nippo",
      "project_path": "/path/to/project",
      "git_branch": "main",
      "time_range": {"start": "...", "end": "..."},
      "user_prompts": [{"text": "...", "timestamp": "..."}],
      "tool_usage": {"Read": 5, "Edit": 3},
      "message_counts": {"user": 10, "assistant": 15},
      "total_input_tokens": 50000,
      "total_output_tokens": 10000,
      "files_touched": ["/path/to/file.rs"]
    }
  ],
  "decisions": [
    {
      "timestamp": "...",
      "project": "nippo",
      "context": "判断の文脈",
      "user_prompt": "ユーザーの指示"
    }
  ],
  "stats": {
    "projects_worked_on": [...],
    "total_user_messages": 27,
    "total_assistant_messages": 150,
    "total_tool_uses": 200,
    "tool_frequency": {"Read": 50, "Edit": 30},
    "total_input_tokens": 100000,
    "total_output_tokens": 30000
  }
}
```

## ステップ2: レポート生成

**ステップ1で決定したモードに応じて、対応するセクションだけを実行する。**
モードが `insight` なら「`/nippo insight`」セクションへ進む。`reflection` なら「`/nippo reflection`」セクションへ進む。
**`$ARGUMENTS` が空でない場合、`/nippo`（日報）セクションを実行してはならない。**

### `/nippo`（日報）

[docs/templates/nippo-template.md](docs/templates/nippo-template.md) のテンプレートに従い、日本語でレポートを生成する。

**重要**:
- 「今日の作業」「意思決定とトレードオフ」「用語・コミュニケーションレビュー」「統計」はデータから**自動的に埋める**
- 用語レビューでは、ユーザーのプロンプトを分析し、不正確な用語や曖昧な表現を指摘し、改善案を提示する
- リフレクション（振り返りの問い）は含めない。それは `/nippo reflection` で行う

### `/nippo brief`（端的な日報）

日報の簡潔版。Rust バイナリの `--format summary` 出力をそのまま使用する。

**重要**:
- コレクターは `nippo collect --days N --format summary` で実行する
- Rust バイナリが生成するテキストサマリーをそのままファイルに保存する
- Claude による追加の加工は不要

### `/nippo reflection`（リフレクション足場）

[docs/templates/reflection-template.md](docs/templates/reflection-template.md) のテンプレートに従い生成する。

**重要**:
- 同日の日報 `reports/nippo-YYYY-MM-DD.md` が存在すればそれも Read で読み込み、事実に基づいた**具体的な問い**を生成する
- 問いは固定テンプレートではなく、**その日の作業内容に即して Claude が生成する**
- [docs/reflection-theory.md](docs/reflection-theory.md) を参照し、コルブ・ギブスの理論に基づく問いを出す
- **回答は絶対に書かない**。回答欄は空白（`>` のみ）で出力する
- 利用者自身が考え、書くことに意味がある

### `/nippo guide`（学習支援付きガイド）

[docs/templates/guide-template.md](docs/templates/guide-template.md) のテンプレートに従い生成する。

ジュニアエンジニアやシニアなりたて、初学者向け。
問いに対する回答を先に提示し、**改善方法と学ぶべき概念・技術への導線**を提供する。

**重要**:
- 同日の日報 `reports/nippo-YYYY-MM-DD.md` が存在すればそれも Read で読み込む
- [docs/reflection-theory.md](docs/reflection-theory.md) を参照する
- 問い + 回答 + 改善提案 + 学ぶべき概念を生成する
- **書籍の紹介はしない**（ハルシネーションリスク）。概念名・技術名・検索キーワードを示す
- 学習リソースそのものは提示せず、**概念の名前と検索の手がかり**を示して自分でたどり着けるようにする
- 「今日の作業」に紐づけて「この概念を知っていると解決が速くなる」と示す
- アクションプランは3つ以内

### `/nippo insight`（深い振り返り・回答付き）

[docs/templates/insight-template.md](docs/templates/insight-template.md) のテンプレートに従い生成する。

insight は振り返りそのものなので、Claude が問いに対する**回答も書く**。

**重要**:
- 「期間の事実」「意思決定の傾向」は自動生成する
- ALACT モデルの問いを生成し、**Claude がデータに基づいて回答する**
- 回答は事実と推測を区別する（「〜と推測されます」等）
- [docs/reflection-theory.md](docs/reflection-theory.md) を参照する
- ファイル末尾に「あなたの番」セクションを設け、利用者が自分の視点を書き足せるようにする

### `/nippo report`（上司・メンター向け進捗報告）

[docs/templates/report-template.md](docs/templates/report-template.md) のテンプレートに従い生成する。

1on1 やメンター面談、上司への報告に使う。自分向けの振り返りではなく、**他者に見せる**前提。

**重要**:
- 「やったこと」を「完了した成果」に再構成する
- 課題は「自分の対処案」とセットで出す
- 意思決定データから「相談事項」を抽出する（判断が分かれた場面）
- 感情・内省は含めない
- 読み手が文脈を持っていない前提で書く

### `/nippo review`（評価面談・自己評価用）

[docs/templates/review-template.md](docs/templates/review-template.md) のテンプレートに従い生成する。

四半期・半期の評価面談や自己評価シートの記入に使う。成果の定量化と成長の可視化に焦点を当てる。

**重要**:
- プロジェクトは上位3〜5に絞り、各プロジェクトの成果・規模・技術的ハイライト・インパクトを記述
- 定量データ（セッション数、メッセージ数、ツール使用比率）を含める
- 技術的な成長（新しく取り組んだ技術、深化した領域）を記述
- 次期の目標は測定可能な形で3つ以内に
- `/nippo report` とは異なり、週次進捗ではなく**期間全体の成果**を俯瞰する

### `/nippo trend`（変化の推移分析）

[docs/templates/trend-template.md](docs/templates/trend-template.md) のテンプレートに従い生成する。

指定期間を三等分し、各期間のデータを個別に収集して**変化の推移**を分析する。

**重要**:
- 最低45日以上の期間が必要。45日未満の場合はエラーメッセージを出して `/nippo insight` を推奨する
- コレクターを3回実行する。例: 90日の場合
  - 期間1: `nippo collect --from YYYY-MM-DD --to YYYY-MM-DD`（1〜30日目）
  - 期間2: `nippo collect --from YYYY-MM-DD --to YYYY-MM-DD`（31〜60日目）
  - 期間3: `nippo collect --from YYYY-MM-DD --to YYYY-MM-DD`（61〜90日目）
- 3回分の JSON を読み込み、各期間の統計を比較する
- 比較の観点:
  - プロジェクトの変化（何に取り組んでいたか）
  - ツール使用比率の変化（Read/Edit 比率の変化は作業スタイルの変化を示す）
  - メッセージ量の変化（活動量の推移）
  - 意思決定パターンの変化
  - コミュニケーションスタイルの変化
- 変化を「成長」「停滞」「変化」として中立的に記述する。良い/悪いの評価はしない
- [docs/reflection-theory.md](docs/reflection-theory.md) を参照する

## ステップ3: 出力

### 出力ルール

1. **現在の作業ディレクトリ（`cwd`）の `reports/` 配下に出力する**。nippo リポジトリではなく、スキルを実行したディレクトリが基準
   - 例: `/Users/nwiizo/ghq/github.com/nwiizo/oitoriaezu-owarasero/` で実行 → `reports/` はそのプロジェクト内に作成
   - `reports/` ディレクトリが存在しない場合は `mkdir -p reports` を実行
2. **ステップ1で決定したモードに対応するファイル名** で Write ツールを使って書き出す:
   - 日報モード → `reports/nippo-YYYY-MM-DD.md`
   - brief モード → `reports/brief-YYYY-MM-DD.md`
   - reflection モード → `reports/reflection-YYYY-MM-DD.md`
   - guide モード → `reports/guide-YYYY-MM-DD.md`
   - insight モード → `reports/insight-YYYY-MM-DD.md`
   - report モード → `reports/report-YYYY-MM-DD.md`
   - review モード → `reports/review-YYYY-MM-DD.md`
   - trend モード → `reports/trend-YYYY-MM-DD.md`
   - 期間指定時（N > 1）はファイル名に期間を付与: 例 `reports/insight-YYYY-MM-DD-30d.md`
3. ファイルパスをユーザーに通知する

### 記述上の注意

- レポートは**必ず日本語**で出力する
- ファイルパスの個人名部分は `<user>` にマスクする
- プロジェクト固有の機密情報は伏せる
- 用語レビューは批判ではなく改善提案として書く
- エビデンスのないセクションは省略してよい

## 参照リソース

- **[docs/templates/nippo-template.md](docs/templates/nippo-template.md)** — 日報テンプレート
- **[docs/templates/reflection-template.md](docs/templates/reflection-template.md)** — リフレクション足場テンプレート
- **[docs/templates/guide-template.md](docs/templates/guide-template.md)** — ガイドテンプレート（回答付きリフレクション）
- **[docs/templates/insight-template.md](docs/templates/insight-template.md)** — insight テンプレート
- **[docs/templates/report-template.md](docs/templates/report-template.md)** — report テンプレート（上司・メンター向け報告）
- **[docs/templates/review-template.md](docs/templates/review-template.md)** — review テンプレート（評価面談・自己評価用）
- **[docs/templates/trend-template.md](docs/templates/trend-template.md)** — trend テンプレート（三分割変化分析）
- **[docs/reflection-theory.md](docs/reflection-theory.md)** — リフレクション理論まとめ
- **[docs/data-sources.md](docs/data-sources.md)** — JSONL データソースの仕様

## ステップ4: Rust バイナリへの改善提案

レポート生成中に、以下のような処理を Claude 側で行っていることに気づいた場合、
**GitHub Issue を立てて Rust バイナリ側への実装移行を提案する**。

### Issue を立てる基準

- Claude がデータの加工・集計・フィルタリングを自前で行っている（本来は Rust 側でやるべき）
- JSON 出力に含まれていないが、レポート生成に必要なデータがある
- 同じ前処理を毎回繰り返している
- パフォーマンス上 Rust で処理すべきと判断される重い処理

### Issue のフォーマット

```bash
gh issue create \
  --repo nwiizo/nippo \
  --title "feat(collector): <処理内容を端的に>" \
  --label "enhancement" \
  --body "$(cat <<'EOF'
## 背景

`/nippo` 実行時に Claude 側で以下の処理を行っている:
- <現在 Claude が行っている処理の説明>

## 提案

この処理を Rust バイナリ (`nippo collect`) に移行する。

## 期待する出力の変化

<JSON 出力にどのフィールドを追加・変更するか>

## 理由

- <なぜ Rust 側でやるべきか: パフォーマンス / 再利用性 / 一貫性 等>
EOF
)"
```

### 注意

- Issue を立てるのはレポート生成後（ユーザーの作業を止めない）
- 同じ内容の Issue が既に存在しないか `gh issue list` で確認してから立てる
- 1回の実行につき最大2件まで
