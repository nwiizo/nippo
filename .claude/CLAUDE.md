# nippo

Claude Code の JSONL セッションログから日報・リフレクションを生成するツール。
Rust バイナリ（データ収集）+ Claude Code スキル（レポート生成）の2層構成。

## 開発

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test   # 品質チェック
cargo build --release -p nippo                            # ビルド
cargo install --path crates/collector                     # ローカルインストール
```

- Rust edition 2024、Rust 1.85+ が必要
- `.unwrap()` は使わない。`anyhow::Result` + `?` で処理する

## 変更時の注意

### Rust バイナリを変更するとき
- `crates/collector/src/` 以下を編集
- `main.rs`: CLI 引数、`filter.rs`: 日付フィルタ、`output.rs`: JSON/summary 出力、`sources/claude_code.rs`: JSONL パーサ
- 変更後は `cargo fmt && cargo clippy -- -D warnings && cargo test` を実行

### スキルを変更するとき
- `.claude/skills/nippo/SKILL.md`: コマンドルーティング・実行手順
- `docs/templates/`: 各モードのテンプレート
- `docs/`: リフレクション理論・データソース仕様
- テンプレートファイル名とモード名の対応:
  - `docs/templates/nippo-template.md` → `/nippo`
  - `docs/templates/reflection-template.md` → `/nippo reflection`
  - `docs/templates/guide-template.md` → `/nippo guide`
  - `docs/templates/report-template.md` → `/nippo report`
  - `docs/templates/review-template.md` → `/nippo review`
  - `docs/templates/insight-template.md` → `/nippo insight`
  - `docs/templates/trend-template.md` → `/nippo trend`
  - `docs/reflection-theory.md` → 全リフレクション系モードが参照
  - `docs/data-sources.md` → JSONL データソース仕様

### 新しいモードを追加するとき
1. `docs/templates/` にテンプレートファイルを作成
2. `SKILL.md` の引数パースルール・コマンド一覧・ステップ2・出力ルール・参照リソースを更新
3. `README.md` のコマンド一覧を更新
4. この `CLAUDE.md` のテンプレート対応表を更新

## 制約

- データ収集は Rust バイナリのみ。**Python スクリプトを使わない**
- 他のスキルのスクリプトやデータを参照・実行しない
- 書籍・URL を紹介しない（ハルシネーションリスク）。概念名・検索キーワードを示す
- `reports/` はコマンドを実行した cwd に出力する（nippo リポジトリ固定ではない）
- `reports/*.md` は `.gitignore` 済み
