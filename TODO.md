# TODO

- `daily` レポート生成の回帰を防ぐ仕組みを追加する
  - 新しく生成した `reports/nippo-YYYY-MM-DD.md` が `meta.source` `meta.total_sessions` `stats.projects_worked_on` と食い違わないことを golden test か smoke test で確認する
  - 既存レポートの継ぎ足しではなく、毎回 fresh collect から再生成されることを検証する

- 判断ログを表向けに構造化する
  - 現状の `decisions` は `context` と `user_prompt` が中心で、`場面` / `選んだこと` / `他の選択肢` をそのまま持っていない
  - レポート生成時の推測を減らすため、collector 側で構造化できる余地を検討する

- 日報本文のプロジェクト要約を collector 側でも補助できるようにする
  - `stats.projects_worked_on` は量的な並びだけなので、「その日に何をしたか」の代表文はまだ LLM 側の推測が大きい
  - 上位プロジェクトごとの代表 prompt / touched files / decisions を report 用に束ねる補助データを検討する
