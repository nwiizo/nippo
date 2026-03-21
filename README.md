# nippo

Claude Code のスキルとして動作する、**日報自動生成 + リフレクション支援ツール**。
何も記録しなくていい。`/nippo` を実行するだけで、今日の事実が整理される。
その先の内省は、あなた自身が行う。

---

## コンセプト

### 「書く」をやめる、「考える」は自分で

```
Before: 終業時に各種ツールを漁って15分かけて日報を書く
After : /nippo で事実を自動収集。/nippo reflection で内省の足場を得る。
```

Claude Code で作業した記録はすべて `~/.claude/projects/` 以下の JSONL に残っている。
**あなたが Claude に投げたプロンプトは、そのまま作業ログになる。**
これを Rust で高速収集し、Claude が日報に変換する。

**ただし、思考・努力・内省まで外注しない。**

ツールがやること:
- 事実の収集（作業時間、プロジェクト、ツール使用、意思決定ポイント）
- 用語・コミュニケーションの正確さのレビュー
- リフレクションのための問いの提示

ツールがやらないこと:
- あなたの代わりに振り返ること
- あなたの代わりに反省すること
- あなたの代わりに学びを言語化すること

### 「反省」ではなく「リフレクション」

日報はともすれば「ダメ出しの記録」になりがちだが、それは **反省** であってリフレクションではない。

| | 反省 | リフレクション |
|---|---|---|
| 視点 | 過去の悪かった点を責める | 良い点も含めフラットに見つめる |
| 目的 | 同じ失敗をしないこと | 経験から学びを抽出し未来に活かす |
| 方向 | 過去完結 | 未来志向 |

本ツールは、**コルブの経験学習サイクル** ・ **ギブスのリフレクティブサイクル** ・ **ALACTモデル** の理論に基づき、深いリフレクションを自然に促す。

---

## 使い方

```bash
/nippo                # 今日の日報（事実 + 意思決定 + 用語レビュー）
/nippo brief          # 端的な日報（統計と要点のみ）
/nippo reflection     # リフレクション足場（問いのみ、自分で書く）
/nippo guide          # ガイド（問い + 改善提案 + 学ぶべき概念）
/nippo insight        # 過去7日分の深い振り返り（回答付き）
/nippo report         # 上司・メンター向けの進捗報告（過去7日）
/nippo review         # 評価面談・自己評価用の成果まとめ（過去90日）
/nippo trend 90       # 過去90日を三分割して変化の推移を分析
```

全コマンドで期間指定・プロジェクト指定が可能:

```bash
/nippo 3                          # 過去3日分の日報
/nippo reflection 5               # 過去5日分のリフレクション
/nippo insight 30                 # 過去30日分の深い振り返り
/nippo insight 30 nippo           # nippo プロジェクトのみ
/nippo guide 7 oitoriaezu-owarasero  # 特定プロジェクトのガイド
/nippo trend 90                   # 過去90日の変化分析
```

出力先:

```
reports/
├── nippo-YYYY-MM-DD.md         # 日報（事実）
├── brief-YYYY-MM-DD.md         # 端的な日報
├── reflection-YYYY-MM-DD.md    # リフレクション足場（問い）
├── guide-YYYY-MM-DD.md         # ガイド（学習支援付き）
├── insight-YYYY-MM-DD.md       # 深い振り返り
├── report-YYYY-MM-DD.md        # 上司・メンター向け報告
├── review-YYYY-MM-DD.md        # 評価面談・自己評価用
└── trend-YYYY-MM-DD.md         # 変化の推移分析
```

### コマンドの関係

```
/nippo（事実の自動収集）── 日々の記録
    │
    ├─→ /nippo brief（要点だけ見たいとき）
    │
    ├─→ /nippo reflection（自分で考えたいとき → 問いのみ）
    │
    ├─→ /nippo guide（初学者 → 回答 + 学ぶべき概念 + 多角的フィードバック）
    │
    ├─→ /nippo report（上司・メンターに見せるとき → 成果 + 課題 + 相談事項）
    │
    ├─→ /nippo review（評価面談 → 四半期の成果 + 成長 + 次期目標）
    │
    ├─→ /nippo insight（期間を俯瞰したいとき → ALACT 分析）
    │
    └─→ /nippo trend（長期の変化を見たいとき → 三分割比較）
```

---

## コマンド体系

| コマンド | 何を出すか | 回答 | デフォルト期間 |
|---------|-----------|------|-------------|
| `/nippo` | 事実 + 意思決定 + 用語レビュー | - | 1日 |
| `/nippo brief` | 統計と要点のみ | - | 1日 |
| `/nippo reflection` | 問いのみ | **書かない**（自分で書く） | 1日 |
| `/nippo guide` | 問い + 回答 + 学ぶべき概念 | **書く**（学習支援） | 1日 |
| `/nippo report` | 成果 + 課題 + 相談事項 | **書く**（報告用） | 7日 |
| `/nippo review` | 成果 + 成長 + 次期目標 | **書く**（評価用） | 90日 |
| `/nippo insight` | 傾向分析 + 振り返り | **書く**（分析結果） | 7日 |
| `/nippo trend` | 三分割して変化の推移を比較 | **書く**（変化分析） | 90日（最低45日） |

---

## 生成される日報（`/nippo`）

事実と意思決定のみ。リフレクションは含めない。

```markdown
# 日報 2026年03月21日（金）

## 今日の作業
- 作業時間帯: 09:14 〜 18:32
- プロジェクト: nippo
- セッション数: 6

**やったこと**
- Rust 収集エンジンの基本設計と実装
- JSONL パーサの実装（rayon による並列処理）
- スキル定義の作成

## 意思決定とトレードオフ
| 場面 | 選んだこと | 他の選択肢・捨てたこと |
|------|-----------|---------------------|
| パーサの実装言語 | Rust（rayon並列） | Python（GIL制約） |
| エントリ型の設計 | serde tagged enum | 手動パース |

## 用語・コミュニケーションレビュー

**用語の正確さ**
| 使った表現 | 文脈 | より正確な表現 | 補足 |
|-----------|------|--------------|------|
| lifetime エラー | rayon での並列処理 | 借用チェッカーエラー | lifetime はその一種 |

**コミュニケーション改善案**
- 「いい感じにして」→ 具体的な制約を明示すると精度が上がる
- 効果的だったパターン: 「〜ではなく〜にして」は意図が明確に伝わった

## 統計
- メッセージ数: user 27 / assistant 150
- ツール使用: Read(46), Bash(121), Edit(19), Write(9), Agent(20)
```

---

## 生成されるリフレクション足場（`/nippo reflection`）

あなたが考えるための問い。回答欄は空白。

```markdown
# リフレクション 2026年03月21日（金）

> 答えはあなた自身の言葉で書いてください。

## ① 省察的観察 × 感情
- rayon の lifetime エラーに3時間かかったとき、
  どの時点で「別のアプローチを試そう」と思いましたか？
  >
- serde の tagged enum を選んだとき、確信がありましたか？
  それとも不安でしたか？
  >

## ② 抽象的概念化
- 「ドキュメントを先に読む vs まず動かす」、
  今日はどちらが多かったですか？ その結果は？
  >

## ③ コミュニケーションの振り返り
- 「いい感じにして」と書いた場面で、
  本当は何を期待していましたか？
  >

## ④ 能動的実験
- 明日、ひとつだけ変えるとしたら何を変えますか？
  >
```

---

## リフレクション理論と設計の対応

### コルブの経験学習サイクル

<https://doi.org/10.1002/j.2333-8504.1976.tb01154.x>

```
① 具体的経験     →  /nippo（今日何が起きたか）
② 省察的観察     →  /nippo reflection（問いを通じて自分で振り返る）
③ 抽象的概念化   →  /nippo reflection（パターンを自分で見つける）
④ 能動的実験     →  /nippo reflection（明日どう試すか自分で決める）
```

### ギブスのリフレクティブサイクル（感情の組み込み）

コルブモデルの弱点（感情プロセスの欠如）を補い、
「どう感じたか」を省察の正式な要素として `/nippo reflection` の問いに組み込む。

### ALACTモデルの「8つの問い」（本質への掘り下げ）

`/nippo insight` で、行為の表面だけでなく
**「行為の奥にある思考・感情・意図」** まで掘り下げる問いを提示する。

---

## アーキテクチャ

### 処理フロー

```
/nippo 実行
    │
    ▼
[Rust] nippo collect --days 1
    ├─ ~/.claude/projects/**/*.jsonl を glob で探索
    ├─ rayon::par_iter() で並列パース
    ├─ 日付フィルタ（mtime プレフィルタ + タイムスタンプフィルタ）
    └─ JSON 出力（セッション情報 + 意思決定ポイント + 統計）
    │
    ▼
[Claude] テンプレートに従い日報を生成
    ├─ 事実の整理
    ├─ 意思決定・トレードオフの抽出
    └─ 用語・コミュニケーションレビュー
    │
    ▼
reports/nippo-YYYY-MM-DD.md に保存
```

### ファイル構成

```
nippo/
├── README.md
├── Cargo.toml                          # workspace root
├── .gitignore
│
├── .claude/
│   └── skills/
│       └── nippo/
│           ├── SKILL.md
│           └── references/
│               ├── data-sources.md
│               ├── nippo-template.md
│               ├── reflection-template.md
│               ├── guide-template.md
│               ├── insight-template.md
│               ├── trend-template.md
│               └── reflection-theory.md
│
├── crates/
│   └── collector/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── sources/
│           │   ├── mod.rs
│           │   └── claude_code.rs
│           ├── filter.rs
│           └── output.rs
│
└── reports/
    └── .gitkeep
```

---

## Rust コアエンジン

### なぜ Rust か

Claude Code は作業するほどセッションファイルが増える。
毎日数千メッセージをパースするなら速度が重要。

```
Python : 2,000件 → 3〜5秒
Rust   : 2,000件 → 0.1秒未満（rayon 並列）
```

単一バイナリ配布のため、Python 環境も pip も不要。

### CLI

```bash
nippo collect --days 1                           # 今日分（JSON 出力）
nippo collect --days 7                           # 過去7日
nippo collect --days 0                           # 全期間
nippo collect --days 7 --stats-only              # 統計のみ
nippo collect --days 1 --format summary          # テキストサマリー
nippo collect --project nippo                    # プロジェクト名でフィルタ
nippo collect --from 2026-03-01 --to 2026-03-15  # 日付範囲
nippo collect --period last-week                 # 先週
nippo collect --period last-month                # 先月
```

`--format`: `json`（デフォルト） / `summary`（テキスト）
`--period`: `today` / `yesterday` / `this-week` / `last-week` / `this-month` / `last-month` 等

### 依存クレート

```toml
[dependencies]
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
rayon       = "1"
glob        = "0.3"
chrono      = { version = "0.4", features = ["serde"] }
clap        = { version = "4", features = ["derive"] }
anyhow      = "1"
```

---

## セットアップ

### 1. Rust バイナリのインストール

```bash
# crates.io からインストール
cargo install nippo

# または、リポジトリからインストール
cargo install --git https://github.com/nwiizo/nippo nippo
```

### 2. Claude Code スキルの配置

```bash
# リポジトリをクローン
git clone https://github.com/nwiizo/nippo
cd nippo
```

#### 方法A: シンボリックリンク（推奨）

```bash
ln -s "$(pwd)/.claude/skills/nippo" ~/.claude/skills/nippo
```

`git pull` するだけでスキルの更新が反映される。

#### 方法B: コピー

```bash
cp -r .claude/skills/nippo ~/.claude/skills/nippo
```

更新時は再度 `cp -r` が必要。

## 要件

- Claude Code（CLI または VS Code 拡張）
- Rust 1.85+（`cargo install` 時のみ）

---

## JSONL を解析してできること（将来の拡張案）

現在のコレクターが収集するデータからは、以下のような分析も可能:

- **作業パターンの可視化**: 時間帯別の活動量ヒートマップ、曜日別の傾向
- **ツール使用比率の推移**: 日ごとの Read/Edit/Bash 比率の変化から、コードリーディング中心の日か実装中心の日かを判別
- **プロジェクト間のコンテキストスイッチ回数**: 1日に何プロジェクト触ったか、切り替え頻度
- **プロンプト長の推移**: 指示の具体性が日々改善しているかの指標
- **セッション継続時間の分布**: 集中力の持続パターン
- **エラー・リトライ率**: tool_result の is_error フラグから、試行錯誤の度合いを計測
- **ファイル変更の頻度分析**: よく触るファイル、一度も読まれないファイルの可視化
- **トークン効率**: 入力トークンあたりの成果物量の推定

---

## 参考理論

| 理論 | 提唱者 | 本ツールでの活用箇所 |
|---|---|---|
| 経験学習サイクル | コルブ（1984） | `/nippo reflection` の問い構造 |
| リフレクティブサイクル | ギブス（1988） | 感情を含む問いの生成 |
| ALACTモデル | コルトハーヘン（2001） | `/nippo insight` の深掘り構造 |
| 経験の連続性 | デューイ（1938） | 日報の蓄積が成長につながる設計思想 |

---

## ライセンス

MIT
