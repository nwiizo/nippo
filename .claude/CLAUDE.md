# nippo

Claude Code の作業ログから日報・リフレクション・インサイトを生成するツール。

## インストール

```bash
cargo install nippo                                        # バイナリ
ln -s /path/to/nippo/.claude/skills/nippo ~/.claude/skills/nippo  # スキル
```

## 開発

- ビルド: `cargo build --release -p nippo`
- 品質チェック: `cargo fmt && cargo clippy -- -D warnings && cargo test`
- ローカルインストール: `cargo install --path crates/collector`

## スキル

- `/nippo` — 日報（事実 + 意思決定 + 用語レビュー）
- `/nippo brief` — 端的な日報
- `/nippo reflection` — リフレクション足場（問いのみ）
- `/nippo guide` — 学習支援付きガイド（回答 + 学ぶべき概念）
- `/nippo report` — 上司・メンター向け進捗報告
- `/nippo review` — 評価面談・自己評価用の成果まとめ
- `/nippo insight` — 深い振り返り（ALACT モデル、回答付き）
- `/nippo trend` — 三分割変化分析（最低45日）

全コマンドで期間指定（`/nippo 7`）・プロジェクト指定（`/nippo insight 30 nippo`）が可能。

## 制約

- データ収集は Rust バイナリのみ。Python スクリプトは使わない
- 他のスキルのスクリプトやデータを参照・実行しない
- 書籍・URL の紹介はしない（ハルシネーションリスク）。概念名・検索キーワードを示す
- `reports/*.md` は `.gitignore` 済み（個人データ）
