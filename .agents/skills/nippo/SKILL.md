---
name: "nippo"
description: "Generate Japanese daily reports, reflection prompts, guides, reviews, and trend reports from Claude Code or Codex work logs. Use when the user asks for nippo, 日報, reflection, guide, report, review, insight, trend, or wants to summarize recent Claude Code/Codex work."
---

# Nippo

Use this skill when the user wants to turn recent Claude Code or Codex work into a report under `reports/`.

## Inputs

- mode: default, `brief`, `reflection`, `guide`, `report`, `review`, `insight`, `trend`
- optional days
- optional project filter
- optional source override: `claude`, `codex`, `all`

## Workflow

1. Prefer `nippo collect ...` when `nippo` is already installed.
2. If `nippo` is not on PATH and the current workspace is this repository, run `cargo run -q -p nippo -- collect ...`.
3. Default to `--source auto`. Override only when the user explicitly asks for `claude`, `codex`, or merged history.
4. For `brief`, save the summary output directly and stop.
5. For other modes, read the collected JSON and the matching template:
   - [docs/templates/nippo-template.md](docs/templates/nippo-template.md)
   - [docs/templates/reflection-template.md](docs/templates/reflection-template.md)
   - [docs/templates/guide-template.md](docs/templates/guide-template.md)
   - [docs/templates/report-template.md](docs/templates/report-template.md)
   - [docs/templates/review-template.md](docs/templates/review-template.md)
   - [docs/templates/insight-template.md](docs/templates/insight-template.md)
   - [docs/templates/trend-template.md](docs/templates/trend-template.md)
6. For `reflection`, `guide`, and `insight`, also read [docs/reflection-theory.md](docs/reflection-theory.md).
7. Save to `reports/{mode}-YYYY-MM-DD.md`. Append `-Nd` when days > 1.

## Mode Defaults

- default: `--days 1`
- `brief`: `--days 1 --format summary`
- `reflection`: `--days 1`
- `guide`: `--days 1`
- `report`: `--days 7 --stats-only`
- `review`: `--days 90 --stats-only`
- `insight`: `--days 7`
- `trend`: split the time window into 3 ranges and run 3 summary collections

## Rules

- Data collection must go through `nippo collect`. Do not reimplement parsing in ad-hoc scripts.
- Do not use Python for data collection.
- Use `stats` as-is. Do not recalculate counters in prose.
- Write reports in Japanese.
- `Codex` report data comes from `history.jsonl` and `state_5.sqlite`. Treat `logs_2.sqlite` as diagnostics only.
- Codex-derived reports may have sparse assistant/tool metrics. State that explicitly instead of inventing numbers.
