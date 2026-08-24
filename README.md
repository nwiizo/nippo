# nippo

Claude Code / Codex / GitHub Copilot で作業するだけで日報ができる。

```bash
# Claude Code
/nippo

# Codex
$nippo

# GitHub Copilot CLI
/nippo
```

これだけで、今日やったこと・判断したこと・改善点が `reports/nippo-20XX-YY-ZZ.md` にまとまる。手動で何も記録する必要はない。各エージェントの作業ログがそのまま日報になる。
Claude Code と GitHub Copilot では `/nippo`、Codex では `$nippo` で同じ日報生成フローを実行できる。

---

## 何が出力されるか

### `/nippo` — 日報

```markdown
# 日報 2026年03月21日（金）

## 今日の作業

- 作業時間帯: 09:14 〜 18:32（ローカルタイムゾーン）
- ソース: requested all | resolved claude, codex, copilot
- プロジェクト: nippo, oitoriaezu-owarasero
- セッション数: 35

**やったこと**

- Rust 製コレクターの設計・実装
- 全章の論理フローレビュー（16並列エージェント）

## 判断の記録

| 場面     | 選んだこと        | 他の選択肢        |
| -------- | ----------------- | ----------------- |
| 実装言語 | Rust（rayon並列） | Python（GIL制約） |

## 用語レビュー

| 使った表現      | より正確な表現       | 補足                |
| --------------- | -------------------- | ------------------- |
| lifetime エラー | 借用チェッカーエラー | lifetime はその一種 |

## 統計

- メッセージ: user 82 / assistant 958
- ツール使用: Bash(234), Read(198), Edit(70)
```

### `/nippo reflection` — 自分で考えるための問い

回答は書かれない。空欄のまま出力される。書くのはあなた。

```markdown
- rayon の lifetime エラーに3時間かかったとき、
  どの時点で「別のアプローチを試そう」と思いましたか？
  >
- 明日、ひとつだけ変えるとしたら何を変えますか？
  >
```

---

## インストール

```bash
cargo install nippo
nippo skill install
```

これで Claude Code 用の `~/.claude/skills/nippo`、Codex 用の
`~/.agents/skills/nippo`、GitHub Copilot 用の `~/.copilot/skills/nippo` がセットアップされる。
一つだけ入れる場合は `--target claude`、`--target codex`、`--target copilot` を指定する。既存のインストールを
置き換える場合は `--force` を付ける。

通常はバイナリに埋め込まれたスキルとテンプレートを書き出す。nippo の
リポジトリ内（またはその子ディレクトリ）で実行した場合はリポジトリを自動検出し、
各スキルへのシンボリックリンクを作るため、checkout の更新がそのまま反映される。

**アップグレード時の注意**: 埋め込み書き出し方式でインストールした場合、
`cargo install nippo` でバイナリを更新してもディスク上のスキルとテンプレートは
自動更新されない。バイナリ更新後は `nippo skill install --force` を再実行して
スキル定義を揃えること（シンボリックリンク方式なら不要）。

Windows の git checkout ではクレート内の埋め込み用シンボリックリンクが
通常ファイルになることがあるため、シンボリックリンクを有効にした環境で package を作成する。

**要件**: [Claude Code](https://claude.com/claude-code)、Codex、または GitHub Copilot CLI + Rust 1.88+

---

## 全コマンド

Claude Code と GitHub Copilot では `/nippo ...`、Codex では `$nippo ...` を使う。引数と挙動は同じ。

### 日々の記録

| コマンド       | 何を出すか                           | デフォルト期間 |
| -------------- | ------------------------------------ | -------------- |
| `/nippo`       | 作業内容 + 判断の記録 + 用語レビュー | 1日            |
| `/nippo daily` | `/nippo` の明示エイリアス           | 1日            |
| `/nippo brief` | 統計と要点のみ                       | 1日            |

### 自分で振り返る

| コマンド            | 何を出すか                   | デフォルト期間 |
| ------------------- | ---------------------------- | -------------- |
| `/nippo reflection` | 問いのみ（回答は自分で書く） | 1日            |

### 次の行動につなげる

| コマンド      | 何を出すか                                   | 入力                                |
| ------------- | -------------------------------------------- | ----------------------------------- |
| `/nippo plan` | 前日の振り返りから今日の実験候補を1〜3個提示 | 最新の日報 + 任意の `ledger.yaml`   |

### 学びを得る

| コマンド       | 何を出すか                             | デフォルト期間 |
| -------------- | -------------------------------------- | -------------- |
| `/nippo guide` | 学習ガイド（多角的フィードバック付き） | 1日            |

### 他者に見せる

| コマンド        | 何を出すか                     | デフォルト期間 |
| --------------- | ------------------------------ | -------------- |
| `/nippo report` | 進捗報告（上司・メンター向け） | 7日            |
| `/nippo review` | 自己評価（評価面談用）         | 90日           |

### 期間を俯瞰する

| コマンド         | 何を出すか                   | デフォルト期間   |
| ---------------- | ---------------------------- | ---------------- |
| `/nippo insight` | 週・月単位の振り返り         | 7日              |
| `/nippo trend`   | 長期の変化分析（三分割比較） | 90日（最低45日） |

### 詰まりを累積で追う

| コマンド        | 何を出すか                                  | 入力                  |
| --------------- | ------------------------------------------- | --------------------- |
| `/nippo ledger` | `## Unclear points` を横断集計して収束/発散判定 | `reports/nippo-*.md` (期間なし) |

過去の日報の `## Unclear points` セクション
（`Issue / Cause / General Fix Rule` の三項組）を時系列で読み込み、
`reports/ledger.yaml` に正規化されたルール集合を累積する。各日について
新規ルール数を観察し、2 日連続でゼロなら `[CONVERGED]`（この種の詰まりは
学習済み）、3 日連続で非減少なら `[DIVERGENCE-SIGNAL]`（戦術的修正の限界、
作業環境・道具・タスク選定そのものを変える合図）を表示する。
詳細は [docs/reflection-theory.md](docs/reflection-theory.md) の「推論時
Alignment / 自然言語による勾配降下」節を参照。

`nippo ledger --tui` は、同じ集計結果を対話的な読み取り専用ダッシュボードで
表示する（収束/発散を色分けしたバッジ、レポートごとの new/reseen、新規ルール数の
スパークライン、再出現ルールの一覧）。`tui` フィーチャ付きのビルドが必要:
`cargo install --path crates/collector --features tui`。`q` / `Esc` で終了、`j`/`k`
でレポートを移動する。

`nippo ledger --export` は通常の集計後、複数回出現した General Fix Rule を
CLAUDE.md / AGENTS.md 追記候補として `reports/ledger-export.md` に出力する。
各候補には出現回数と初出・最終出現日が付く。自動採用するルールではないため、
設定へコピーする前に人間が取捨選択する。

### 期間指定・プロジェクト指定

全コマンド共通:

```bash
/nippo daily                  # /nippo と同じ（日報）
/nippo daily codex            # Codex 履歴だけで日報
/nippo daily claude           # Claude Code 履歴だけで日報
/nippo daily copilot          # GitHub Copilot CLI 履歴だけで日報
/nippo daily all              # 利用可能な全ソースを混ぜて日報
/nippo 3                      # 過去3日分
/nippo insight 30              # 過去30日分
/nippo insight 30 nippo        # nippo プロジェクトのみ
/nippo review 180              # 過去半年の自己評価
```

出力先はコマンドを実行したディレクトリの `reports/` 配下。同じ日付・同じモードのレポートがある場合は、最新の収集結果で上書きする。

---

## 定期実行

NIPPO は端末内の Claude Code / Codex / GitHub Copilot CLI の作業履歴を読むため、ローカルファイルへ
アクセスできる定期タスクを使う。クラウド上で動くタスクはローカル履歴を読めない。
最初に対象プロジェクトで `/nippo daily` または `$nippo daily` を手動実行し、
`reports/` に日報が作られることを確認しておく。

### Codex（ChatGPT デスクトップアプリ）

対象プロジェクトを開いた Codex のチャットで、たとえば次のように依頼する。

```text
毎日18:00に、このプロジェクトで $nippo daily codex を実行する定期タスクを作成してください。
ローカルプロジェクトで実行し、worktree は使わないでください。
```

作成後はサイドバーの `Scheduled` から実行時刻と対象プロジェクトを確認し、最初の
数回は結果を確認する。ローカル履歴を読むには、実行時にコンピューターが起動していて、
ChatGPT デスクトップアプリが動いている必要がある。Codex CLI と IDE 拡張には定期タスクの
管理画面がないため、作成と管理はデスクトップアプリで行う。

詳しくは [OpenAI の Scheduled tasks ドキュメント](https://learn.chatgpt.com/docs/automations) を参照。

### Claude Code Desktop

`Code` タブの `Routines` から `New routine` → `Local` を選び、次のように設定する。

- `Instructions`: `/nippo daily claude`
- `Folder`: 日報の `reports/` を作りたいプロジェクト
- `Schedule`: `Daily` と任意の時刻
- `Worktree`: オフ

作成後に `Run now` で一度実行し、必要な権限と出力先を確認する。ローカル定期タスクは
Claude Code Desktop が起動し、コンピューターがスリープしていない間に動く。
`Routines` が表示されない場合は Desktop アプリを更新する。

詳しくは [Claude Code Desktop の定期タスク](https://code.claude.com/docs/en/desktop-scheduled-tasks) を参照。

CLI セッションを開いたまま一時的に繰り返すだけなら、`/loop` も使える。

```text
/loop 1d /nippo daily claude
```

`/loop` は現在のセッション内でのみ動き、固定間隔のタスクは最長 7 日で終了する。
常設の日報生成には Desktop のローカル定期タスクを使う。詳細は
[Claude Code の `/loop` ドキュメント](https://code.claude.com/docs/en/scheduled-tasks) を参照。

### 定期実行時の注意

- `reports/` は定期タスクの作業フォルダに作られる。Git worktree を使うと日報もその
  worktree 内に出力されるため、普段使うフォルダへ残したい場合は worktree を無効にする。
- 複数のエージェントを使う場合は、どれか一つで `/nippo daily all` または
  `$nippo daily all` を定期実行する。同じ日付の日報を両方から作ると、後の実行結果で上書きされる。
- 定期タスクの指示やスキル展開だけで終わるセッションは、既定のプロンプトノイズ除外の
  対象になる。日報生成時は `--include-prompt-noise` や `--include-self` を付けない。

---

## Rust CLI（単体でも使える）

スキルを介さずに、Rust バイナリを直接実行してデータを確認できる。

```bash
nippo collect --period today                     # 今日の JSON 出力
nippo collect --days 1                           # 今日の JSON 出力（ローカル日付基準）
nippo collect --days 7 --format summary          # テキストサマリー
nippo collect --source codex --period today      # Codex 履歴のみ
nippo collect --source copilot --period today    # GitHub Copilot CLI 履歴のみ
nippo collect --source all --days 7              # 利用可能な全ソース
nippo collect --period last-week                 # 先週
nippo collect --from 2026-03-01 --to 2026-03-15  # 日付範囲
nippo collect --project ccswarm                  # プロジェクトフィルタ
nippo collect --include-prompt-noise             # 除外前のプロンプトも確認
nippo collect --include-self                     # 実行中のセッションも含める
```

`--days` / `--from` / `--to` / `--period` の日付境界は、コマンドを実行した環境のローカルタイムゾーン基準。`--days 1` は「今日のローカル日付」を意味する。

JSON の `meta.period` は、指定した日付範囲の両端と境界の基準になったタイムゾーンを返す。
プロジェクト総数は重複する項目を持たず、`stats.projects_worked_on` の要素数から求める。

```json
{
  "meta": {
    "filter_label": "90 days",
    "period": {
      "from": "2026-05-27",
      "to": "2026-08-24",
      "timezone": "Asia/Tokyo"
    }
  }
}
```

`--days 0` では両端が `null` になる。`--from` だけを指定した場合の終了日は今日、
`--to` だけを指定した場合の開始日は `null` になる。

同じ `session_id` の記録が複数ファイルに分かれている場合は、プロンプトを重複除去して
1 セッションに統合する。Claude Code / Codex / GitHub Copilot 内から実行したときは、ホストが公開する
現在のセッション ID と完全一致する記録を既定で除外する。確認目的で含めたい場合だけ
`--include-self` を指定する。

既定では、スラッシュコマンド展開、ハーネス通知、画像プレースホルダ、中断通知、
compact の導入文、短い肯定応答を `user_prompts` と user 側の集計から除外する。
統合後に意味のある user prompt が一つも残らないセッションは、assistant 応答やツール使用を
含めて集計対象から外す。同じ ID に通常の依頼がある分割記録は先に統合されるため残る。
除外前のプロンプトを調査したい場合だけ `--include-prompt-noise` を指定する。

JSON の `render_helpers` には、既存の `sessions` と `stats` から機械的に算出した表示補助が入る。

```json
{
  "render_helpers": {
    "sessions_local": [
      {
        "session_id": "abc123",
        "project": "nippo",
        "start_local": "2026-05-05T12:17:00+09:00",
        "end_local": "2026-05-05T12:55:00+09:00"
      }
    ],
    "main_work_window_local": {
      "start": "2026-05-05T12:17:00+09:00",
      "end": "2026-05-05T13:55:00+09:00",
      "method": "longest_block_with_gaps_under_30_minutes"
    },
    "tool_frequency_top5": [{ "name": "Edit", "count": 3 }]
  }
}
```

`--stats-only` では `render_helpers.sessions_local` は空配列になり、集計用の主要時間帯と
上位ツールだけを返す。文章の要約や判断の選択肢の推測は Rust 側では行わず、引き続き
テンプレートを読む skill が担当する。

```
期間: today | セッション: 48 | プロジェクト: 4 | 意思決定: 8
メッセージ: user 115 / assistant 1234 | ツール使用: 794
トークン: input 19146 / output 422103

プロジェクト:
  nippo                           24 セッション    1185 メッセージ
  oitoriaezu-owarasero            17 セッション      99 メッセージ
```

---

## なぜ作ったか

### 記録と振り返りの分離

| ツールがやること             | 自分でやること             |
| ---------------------------- | -------------------------- |
| 作業時間・プロジェクトの集計 | なぜそう判断したかの言語化 |
| 判断ポイントの抽出           | 振り返り                   |
| 用語の正確さのチェック       | 感情の記録                 |

日報ツールに「今日の学び」を自動生成させていた時期がある。楽だった。楽すぎた。書いてあることは正しいのに、何も残らなかった。

**思考・努力・内省まで外注してはいけない。** `/nippo reflection` の問いに答える5分間は、自動生成された振り返りの100行より価値がある。

### リフレクション理論

| 理論                   | 提唱者                 | 活用箇所                       |
| ---------------------- | ---------------------- | ------------------------------ |
| 経験学習サイクル       | コルブ（1984）         | `/nippo reflection` の問い構造 |
| リフレクティブサイクル | ギブス（1988）         | 感情を含む問いの生成           |
| ALACT モデル           | コルトハーヘン（2001） | `/nippo insight` の深掘り      |
| 経験の連続性           | デューイ（1938）       | 日報の蓄積が成長につながる設計 |

---

## アーキテクチャ

```
/nippo 実行
    │
    ▼
[Rust] nippo collect
    ├─ ~/.claude/projects/**/*.jsonl を rayon で並列パース
    ├─ ~/.codex/history.jsonl + state_5.sqlite + rollout データを収集
    ├─ ~/.copilot/session-state/*/events.jsonl + workspace.yaml を収集
    ├─ mtime プレフィルタ + 2パスデシリアライズ
    └─ JSON 出力
    │
    ▼
[Agent] テンプレートに従いレポート生成
    │
    ▼
reports/ に保存
```

```
nippo/
├── crates/collector/src/     # Rust コレクター
│   ├── main.rs               # CLI (clap)
│   ├── filter.rs             # 日付・期間フィルタ
│   ├── session.rs            # source 共通セッション表現
│   ├── output.rs             # JSON / summary 出力
│   └── sources/
│       ├── claude_code.rs    # Claude Code JSONL パーサ
│       ├── codex.rs          # Codex 履歴パーサ
│       └── copilot.rs        # GitHub Copilot CLI イベントパーサ
├── .agents/skills/nippo/
│   └── SKILL.md              # Codex / GitHub Copilot 共通 skill
├── .claude/skills/nippo/
│   ├── SKILL.md              # スキル定義
│   └── docs -> ../../../docs # テンプレートへのシンボリックリンク
├── AGENTS.md                 # Codex 用 repo ガイド
├── docs/
│   ├── templates/            # 各モードのテンプレート
│   ├── reflection-theory.md  # リフレクション理論
│   └── data-sources.md       # JSONL 仕様
└── .github/workflows/ci.yml  # CI
```

## テンプレートのカスタマイズ

[`docs/templates/`](docs/templates/) のテンプレートを編集すると、各コマンドの出力形式を変更できる。

| ファイル                 | 変更できること                                       |
| ------------------------ | ---------------------------------------------------- |
| `nippo-template.md`      | 日報の項目（セクションの追加・削除）                 |
| `reflection-template.md` | 問いの生成ルール・理論フレームワーク                 |
| `plan-template.md`       | 前日の経験を今日の行動実験へつなぐ足場               |
| `guide-template.md`      | フィードバックの視点（シニア・CTO 等の変更・追加）   |
| `report-template.md`     | 進捗報告のフォーマット（社内テンプレートに合わせる） |
| `review-template.md`     | 自己評価の構造                                       |
| `insight-template.md`    | 振り返りの分析フレーム                               |
| `trend-template.md`      | 変化分析の比較観点                                   |
| `reflection-theory.md`   | 参照するリフレクション理論                           |

テンプレートの編集に Rust の再ビルドは不要。シンボリックリンクで配置していれば、ファイルを編集するだけで即反映される。

## Claude Code `/insights` との関係

Claude Code には組み込みの `/insights` コマンドがある。両者は同じセッションログを使うが、**片付けたい用事（ジョブ）が違う**。

| | `/nippo insight` | `/insights` |
|---|---|---|
| ジョブ | 自分の仕事パターンを理解して成長する | Claude Code の使い方を最適化する |
| 焦点 | 人（判断・行動・思考の傾向） | ツール（設定・フック・摩擦ポイント） |
| 出力 | ALACT ベースの内省 + 行動実験 | HTML ダッシュボード + CLAUDE.md 提案 |

使い分け:
- **仕事の振り返り**をしたいなら → `/nippo insight`
- **ツール設定を改善**したいなら → `/insights`
- 両方やると補完的に効く

## 制約

- データ収集は Rust バイナリのみ。Python は使わない
- 他のスキルのスクリプトを参照しない
- 書籍・URL は紹介しない（ハルシネーションリスク）。概念名と検索キーワードを示す
- `reports/` は `.gitignore` 済み（個人データ）

## ライセンス

MIT
