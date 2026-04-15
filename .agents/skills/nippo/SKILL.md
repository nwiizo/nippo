---
name: "nippo"
description: "Generate Japanese daily reports, reflection prompts, guides, reviews, and trend reports from Claude Code or Codex work logs. Use when the user asks for nippo, 日報, daily, reflection, guide, report, review, insight, trend, or wants to summarize recent Claude Code/Codex work."
---

# Nippo

Use this skill when the user wants to turn recent Claude Code or Codex work into a report under `reports/`.

## Inputs

- mode: default, `daily`, `brief`, `reflection`, `guide`, `report`, `review`, `insight`, `trend`
- optional days
- optional project filter
- optional source override: `claude`, `codex`, `all`

Examples:

- `$nippo daily`
- `$nippo daily codex`
- `$nippo daily claude`
- `$nippo daily all`
- `$nippo review 90 codex`

## Workflow

1. If the current workspace is this repository, prefer `cargo run -q -p nippo -- collect ...` so the skill uses the checked-out implementation rather than a potentially stale globally installed `nippo`.
2. Otherwise prefer `nippo collect ...` when `nippo` is already installed.
3. If neither is available, stop and tell the user to install `nippo`.
4. Treat `daily` as an alias of the default daily report mode.
5. Default to `--source auto`. Override when the user explicitly asks for `claude`, `codex`, or `all`.
6. For `brief`, save the summary output directly and stop.
7. For other modes, read the collected JSON and the matching template:
   - [docs/templates/nippo-template.md](docs/templates/nippo-template.md)
   - [docs/templates/reflection-template.md](docs/templates/reflection-template.md)
   - [docs/templates/guide-template.md](docs/templates/guide-template.md)
   - [docs/templates/report-template.md](docs/templates/report-template.md)
   - [docs/templates/review-template.md](docs/templates/review-template.md)
   - [docs/templates/insight-template.md](docs/templates/insight-template.md)
   - [docs/templates/trend-template.md](docs/templates/trend-template.md)
8. For `reflection`, `guide`, and `insight`, also read [docs/reflection-theory.md](docs/reflection-theory.md).
9. Save daily reports, including `daily`, to `reports/nippo-YYYY-MM-DD.md`. Other modes keep `reports/{mode}-YYYY-MM-DD.md`. Append `-Nd` when days > 1.
10. In daily mode, treat the freshly collected JSON as the only source of truth. Do not read an existing `reports/nippo-YYYY-MM-DD.md` as input; overwrite it with the new report.
11. Ground the daily report header and stats directly in `meta` and `stats`, and choose project sections from `stats.projects_worked_on` in order of `message_count`.

## Mode Defaults

- default: `--period today`
- `daily`: alias of default, `--period today`
- `brief`: `--period today --format summary`
- `reflection`: `--period today`
- `guide`: `--period today`
- `report`: `--days 7 --stats-only`
- `review`: `--days 90 --stats-only`
- `insight`: `--days 7`
- `trend`: split the time window into 3 ranges and run 3 summary collections

## Rules

- Data collection must go through `nippo collect`. Do not reimplement parsing in ad-hoc scripts.
- Do not use Python for data collection.
- Use `stats` as-is. Do not recalculate counters in prose.
- Write reports in Japanese.
- Date boundaries follow the machine's local timezone. `--days 1` and `daily` mean the current local calendar day.
- Treat one token matching `claude`, `codex`, or `all` as the source selector and pass it through to `--source`.
- `Codex` report data comes from `history.jsonl`, `state_5.sqlite`, and rollout data referenced by `rollout_path`. Treat `logs_2.sqlite` as diagnostics only.
- Codex-derived reports may have sparse assistant/tool metrics. State that explicitly instead of inventing numbers.
- For daily reports, copy `meta.source`, `meta.total_sessions`, `stats.projects_worked_on`, and `stats.tool_frequency` from the collected JSON instead of inferring them from an older report.
- Cover the activity-heavy projects first. Use `stats.projects_worked_on` order and give the top 3-5 projects their own section before collapsing anything into `その他`.
- If you show only a subset of `decisions`, explicitly state `全N件中M件を記載`.
- If the current report generation itself appears as a tiny `nippo` project, keep it in the header stats but label it explicitly as report generation/editing and keep the body treatment minimal.
- Sanitize reference links before writing them. Do not copy malformed URL fragments with trailing Japanese text or punctuation.
