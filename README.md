# nippo

Claude Code で作業するだけで日報ができる。

```bash
/nippo
```

これだけで、今日の作業・意思決定・コミュニケーションの振り返りが `reports/nippo-2026-03-21.md` に出力される。手動で何も記録する必要はない。Claude Code の作業ログがそのまま日報の素材になる。

---

## 何が出力されるか

### `/nippo` — 今日の事実

```markdown
# 日報 2026年03月21日（金）

## 今日の作業
- 作業時間帯: 09:14 〜 18:32
- プロジェクト: nippo, oitoriaezu-owarasero
- セッション数: 35

**やったこと**
- Rust 製コレクターの設計・実装
- 全章の論理フローレビュー（16並列エージェント）

## 意思決定とトレードオフ
| 場面 | 選んだこと | 捨てたこと |
|------|-----------|----------|
| 実装言語 | Rust（rayon並列） | Python（GIL制約） |

## 用語・コミュニケーションレビュー
| 使った表現 | より正確な表現 | 補足 |
|-----------|--------------|------|
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

# 2. スキル（シンボリックリンク推奨。git pull で更新が反映される）
git clone https://github.com/nwiizo/nippo && cd nippo
ln -s "$(pwd)/.claude/skills/nippo" ~/.claude/skills/nippo
```

コピーする場合は `cp -r .claude/skills/nippo ~/.claude/skills/nippo`。

**要件**: [Claude Code](https://claude.com/claude-code) + Rust 1.85+

---

## 全コマンド

### 日々の記録

| コマンド | 何を出すか | デフォルト期間 |
|---------|-----------|-------------|
| `/nippo` | 事実 + 意思決定 + 用語レビュー | 1日 |
| `/nippo brief` | 統計と要点のみ | 1日 |

### 自分で振り返る

| コマンド | 何を出すか | デフォルト期間 |
|---------|-----------|-------------|
| `/nippo reflection` | 問いのみ（回答は自分で書く） | 1日 |

### 学びを得る

| コマンド | 何を出すか | デフォルト期間 |
|---------|-----------|-------------|
| `/nippo guide` | 回答 + 学ぶべき概念 + シニア・スタッフ・CTO・ビジネスの視点 | 1日 |

### 他者に見せる

| コマンド | 何を出すか | デフォルト期間 |
|---------|-----------|-------------|
| `/nippo report` | 成果 + 課題 + 相談事項（上司・メンター向け） | 7日 |
| `/nippo review` | 成果 + 成長 + 次期目標（評価面談用） | 90日 |

### 期間を俯瞰する

| コマンド | 何を出すか | デフォルト期間 |
|---------|-----------|-------------|
| `/nippo insight` | ALACT モデルに基づく深い振り返り | 7日 |
| `/nippo trend` | 期間を三分割して変化の推移を比較 | 90日（最低45日） |

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

### 事実と内省の分離

| 自動化する | 自動化しない |
|-----------|------------|
| 作業時間・プロジェクト・統計 | 「なぜそう判断したか」 |
| 意思決定ポイントの抽出 | 判断の振り返り |
| 用語レビュー | 感情の記録 |

日報ツールに「今日の学び」を自動生成させていた時期がある。楽だった。楽すぎた。書いてあることは正しいのに、何も残らなかった。

**思考・努力・内省まで外注してはいけない。** `/nippo reflection` の問いに答える5分間は、自動生成された振り返りの100行より価値がある。

### リフレクション理論

| 理論 | 提唱者 | 活用箇所 |
|------|-------|---------|
| 経験学習サイクル | コルブ（1984） | `/nippo reflection` の問い構造 |
| リフレクティブサイクル | ギブス（1988） | 感情を含む問いの生成 |
| ALACT モデル | コルトハーヘン（2001） | `/nippo insight` の深掘り |
| 経験の連続性 | デューイ（1938） | 日報の蓄積が成長につながる設計 |

---

## アーキテクチャ

```
/nippo 実行
    │
    ▼
[Rust] nippo collect
    ├─ ~/.claude/projects/**/*.jsonl を rayon で並列パース
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
│   ├── output.rs             # JSON / summary 出力
│   └── sources/claude_code.rs # JSONL パーサ
├── .claude/skills/nippo/
│   └── SKILL.md              # スキル定義
├── docs/
│   ├── templates/            # 各モードのテンプレート
│   ├── reflection-theory.md  # リフレクション理論
│   └── data-sources.md       # JSONL 仕様
└── .github/workflows/ci.yml  # CI
```

## 制約

- データ収集は Rust バイナリのみ。Python は使わない
- 他のスキルのスクリプトを参照しない
- 書籍・URL は紹介しない（ハルシネーションリスク）。概念名と検索キーワードを示す
- `reports/` は `.gitignore` 済み（個人データ）

## ライセンス

MIT
