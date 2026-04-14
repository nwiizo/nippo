# nippo

Claude Code / Codex で作業するだけで日報ができる。

```bash
/nippo
```

これだけで、今日やったこと・判断したこと・改善点が `reports/nippo-20XX-YY-ZZ.md` にまとまる。手動で何も記録する必要はない。Claude Code / Codex の作業ログがそのまま日報になる。

---

## 何が出力されるか

### `/nippo` — 日報

```markdown
# 日報 2026年03月21日（金）

## 今日の作業

- 作業時間帯: 09:14 〜 18:32
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
# 1. Rust バイナリ
cargo install nippo

# 2. リポジトリ取得
git clone https://github.com/nwiizo/nippo && cd nippo

# 3-a. Claude Code スキル（シンボリックリンク推奨）
ln -s "$(pwd)/.claude/skills/nippo" ~/.claude/skills/nippo

# 3-b. Codex スキルをグローバルに使う場合
mkdir -p ~/.agents/skills
ln -s "$(pwd)/.agents/skills/nippo" ~/.agents/skills/nippo
```

Codex は repo 内の `.agents/skills/` も自動検出するので、このリポジトリ内で使うだけなら追加インストールなしでも使える。

シンボリックリンクにすると `git pull` でスキルとテンプレートの更新が自動反映される。
スキルディレクトリ内の `docs` シンボリックリンクにより、どのディレクトリから `/nippo` を実行してもテンプレートが正しく読み込まれる。

コピーする場合は `cp -r .claude/skills/nippo ~/.claude/skills/nippo`（テンプレート更新時は再コピーが必要）。

**要件**: [Claude Code](https://claude.com/claude-code) または Codex + Rust 1.85+

---

## 全コマンド

### 日々の記録

| コマンド       | 何を出すか                           | デフォルト期間 |
| -------------- | ------------------------------------ | -------------- |
| `/nippo`       | 作業内容 + 判断の記録 + 用語レビュー | 1日            |
| `/nippo brief` | 統計と要点のみ                       | 1日            |

### 自分で振り返る

| コマンド            | 何を出すか                   | デフォルト期間 |
| ------------------- | ---------------------------- | -------------- |
| `/nippo reflection` | 問いのみ（回答は自分で書く） | 1日            |

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

### 期間指定・プロジェクト指定

全コマンド共通:

```bash
/nippo 3                      # 過去3日分
/nippo insight 30              # 過去30日分
/nippo insight 30 nippo        # nippo プロジェクトのみ
/nippo review 180              # 過去半年の自己評価
```

出力先はコマンドを実行したディレクトリの `reports/` 配下。

---

## Rust CLI（単体でも使える）

スキルを介さずに、Rust バイナリを直接実行してデータを確認できる。

```bash
nippo collect --days 1                           # JSON 出力
nippo collect --days 7 --format summary          # テキストサマリー
nippo collect --source codex --days 1            # Codex 履歴のみ
nippo collect --source all --days 7              # Claude Code + Codex
nippo collect --period last-week                 # 先週
nippo collect --from 2026-03-01 --to 2026-03-15  # 日付範囲
nippo collect --project ccswarm                  # プロジェクトフィルタ
```

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
    ├─ ~/.codex/history.jsonl + state_5.sqlite を収集
    ├─ mtime プレフィルタ + 2パスデシリアライズ
    └─ JSON 出力
    │
    ▼
[Claude] テンプレートに従いレポート生成
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
│       └── codex.rs          # Codex 履歴パーサ
├── .agents/skills/nippo/
│   └── SKILL.md              # Codex 用 skill
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
