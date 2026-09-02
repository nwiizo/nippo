---
name: "nippo"
description: "Generate Japanese daily reports, reflection prompts, guides, reviews, and trend reports from Claude Code or Codex work logs. Use when the user asks for nippo, 日報, daily, reflection, guide, report, review, insight, trend, plan, ledger, or wants to summarize recent Claude Code/Codex work."
---

# Nippo

Use this skill when the user wants to turn recent Claude Code or Codex work into a report under `reports/`.

## Inputs

- mode: default, `daily`, `brief`, `reflection`, `guide`, `report`, `review`, `insight`, `trend`, `plan`, `ledger`
- optional days
- optional project filter
- optional source override: `claude`, `codex`, `opencode`, `all`
- optional period selector: `today`, `yesterday`, `this-week`, `last-week`, `week-before-last`, `this-month`, `last-month`, `month-before-last`

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
5. Default to `--source auto`. Override when the user explicitly asks for `claude`, `codex`, `opencode`, or `all`. The collector removes deterministic prompt noise by default; do not pass `--include-prompt-noise` during report generation.
6. For `brief`, save the summary output directly and stop.
7. For `ledger`, do NOT call `nippo collect`. Run `nippo ledger` (or `cargo run -q -p nippo -- ledger`) which scans `reports/nippo-*.md` (daily reports) for the `## Unclear points` section, folds them into `reports/ledger.yaml`, and prints a CONVERGED / DIVERGENCE-SIGNAL / CONTINUE verdict. Relay that verdict back to the user verbatim; do not over-interpret. If the user asks for agent-config candidates, run `nippo ledger --export`: it writes recurring General Fix Rules to `reports/ledger-export.md`. Tell the user these are candidates only — a human must curate them before copying anything into CLAUDE.md / AGENTS.md.
8. For `plan`, do NOT call `nippo collect`. Find the newest `reports/nippo-*.md` by the date in its filename and read it. Also read `reports/ledger.yaml` if present and use unresolved recurring General Fix Rules as candidate material. Read [docs/templates/plan-template.md](docs/templates/plan-template.md) and [docs/reflection-theory.md](docs/reflection-theory.md), present only 1-3 experiment candidates, leave the choice / reason / first-step fields blank for the user, save to `reports/plan-YYYY-MM-DD.md`, and report the path.
9. For other modes, read the collected JSON and the matching template:
   - [docs/templates/nippo-template.md](docs/templates/nippo-template.md)
   - [docs/templates/reflection-template.md](docs/templates/reflection-template.md)
   - [docs/templates/guide-template.md](docs/templates/guide-template.md)
   - [docs/templates/report-template.md](docs/templates/report-template.md)
   - [docs/templates/review-template.md](docs/templates/review-template.md)
   - [docs/templates/insight-template.md](docs/templates/insight-template.md)
   - [docs/templates/trend-template.md](docs/templates/trend-template.md)
   - [docs/templates/plan-template.md](docs/templates/plan-template.md)
10. For `reflection`, `guide`, `insight`, and `plan`, also read [docs/reflection-theory.md](docs/reflection-theory.md).
11. Save daily reports, including `daily`, to `reports/nippo-YYYY-MM-DD.md`. Other modes keep `reports/{mode}-YYYY-MM-DD.md`. Append `-Nd` when days > 1.
12. In daily mode, treat the freshly collected JSON as the only source of truth. Do not read an existing `reports/nippo-YYYY-MM-DD.md` as input; overwrite it with the new report.
13. Ground the daily report header and stats directly in `meta` and `stats`, use `meta.period` for the requested date range, and choose project sections from `stats.projects_worked_on` in order of `message_count`.
14. After generating a daily report (which emits an `## Unclear points` section), suggest the user run `/nippo ledger` to fold today's stuck points into the cumulative streak signal — but do NOT run it automatically.
15. Finish with the saved path and a concise report summary. Do not suggest collector changes for values already available directly or by simple derivation from the JSON.

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
- `plan`: no collection. Reads the newest `reports/nippo-*.md` and `reports/ledger.yaml`, writes `reports/plan-YYYY-MM-DD.md`
- `ledger`: no time window. Reads `reports/nippo-*.md`, writes `reports/ledger.yaml`, prints a streak verdict. `--export` writes recurring rules to `reports/ledger-export.md`

## Rules

- Data collection must go through `nippo collect`. Do not reimplement parsing in ad-hoc scripts.
- Do not use Python for data collection.
- Use `stats` as-is. Do not recalculate counters in prose.
- Write reports in Japanese.
- Do not recommend books or URLs (hallucination risk). Give concept names and search keywords instead.
- For `reflection` and `guide`, also read the same-day `reports/nippo-YYYY-MM-DD.md` if it exists.
- If the first token is neither a mode name, a number, a source selector, nor a period selector, treat it as a project filter and run the default daily mode with `--project`.
- Date boundaries follow the machine's local timezone. `--days 1` and `daily` mean the current local calendar day.
- Treat one token matching `claude`, `codex`, `opencode`, or `all` as the source selector and pass it through to `--source`.
- Treat one token matching `today`, `yesterday`, `this-week`, `last-week`, `week-before-last`, `this-month`, `last-month`, or `month-before-last` as a period selector and pass it through to `--period`. In that case do not add the mode's default `--days` or default `--period today`; the explicit `--period` is the sole time-window flag. For `daily`, use `meta.period.to` from the collected JSON as the output filename date instead of computing it from the execution date.
- `Codex` report data comes from `history.jsonl`, `state_5.sqlite`, and rollout data referenced by `rollout_path`. Treat `logs_2.sqlite` as diagnostics only.
- Codex-derived reports may have sparse assistant/tool metrics. State that explicitly instead of inventing numbers.
- For daily reports, copy `meta.source`, `meta.total_sessions`, `stats.projects_worked_on`, and `stats.tool_frequency` from the collected JSON instead of inferring them from an older report.
- In every mode that prints a date range, use `meta.period.from` and `meta.period.to`; do not recalculate the range from the execution date.
- Cover the activity-heavy projects first. Use `stats.projects_worked_on` order (= `message_count` descending) and give the top 3-5 projects their own section before collapsing anything into `その他`.
- If you show only a subset of `decisions`, explicitly state `全N件中M件を記載`.
- The current Claude Code or Codex session is excluded by the collector. Do not manually subtract another session from `meta` or `stats`.
- Sanitize reference links before writing them. Do not copy malformed URL fragments with trailing Japanese text or punctuation.

## Improvement Issues

After the report is complete, an Issue may be proposed when report generation repeatedly performs the same deterministic aggregation or formatting, or when one structural problem affects multiple modes. Group symptoms with the same root cause into one coherent Issue instead of proposing one field at a time. Propose at most two Issues per report.

Do not propose an Issue for values already available directly or by simple derivation:

- Total projects: the length of `stats.projects_worked_on`
- Reporting period: `meta.period`
- Top tools and main work window: `render_helpers`
- Prose or summarization that belongs in a report template

Before proposing, run `gh issue list --repo nwiizo/nippo --state all` and skip anything already covered or solvable by reading existing fields correctly. If a new Issue is warranted, present a title and body that separate verified facts, affected modes, the shared cause, and the processing that the change would eliminate. Create it with `gh issue create --repo nwiizo/nippo` only after the user explicitly agrees.
