use chrono::{DateTime, Local, Timelike, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::session::{
    DateRange, RawSession, SessionSummary, assistant_message_count, sort_sessions_by_recency,
    summarize_session,
};

/// UTC タイムスタンプからローカル時間の時（HH）を抽出する
fn extract_local_hour(timestamp: &str) -> Option<String> {
    let dt = DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|d| d.with_timezone(&Utc))?;
    let local = dt.with_timezone(&Local);
    Some(format!("{:02}", local.hour()))
}

// ---------------------------------------------------------------------------
// Output structures (serialized to JSON for Claude)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CollectorOutput {
    pub meta: OutputMeta,
    pub sessions: Vec<SessionSummary>,
    pub decisions: Vec<DecisionPoint>,
    pub stats: AggregateStats,
    pub render_helpers: RenderHelpers,
}

#[derive(Serialize)]
pub struct OutputMeta {
    pub generated_at: String,
    pub filter_label: String,
    pub period: OutputPeriod,
    pub source: SourceMeta,
    pub total_sessions: usize,
    pub total_files_scanned: usize,
}

#[derive(Serialize)]
pub struct OutputPeriod {
    pub from: Option<String>,
    pub to: Option<String>,
    pub timezone: String,
}

#[derive(Serialize)]
pub struct SourceMeta {
    pub requested: String,
    pub resolved: Vec<String>,
}

#[derive(Serialize)]
pub struct DecisionPoint {
    pub timestamp: String,
    pub project: String,
    pub context: String,
    pub user_prompt: String,
}

#[derive(Serialize)]
pub struct AggregateStats {
    pub projects_worked_on: Vec<ProjectStat>,
    pub total_user_messages: usize,
    pub total_assistant_messages: usize,
    pub total_tool_uses: usize,
    pub tool_frequency: HashMap<String, u32>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub decisions_by_project: Vec<DecisionsByProject>,
    pub total_decisions: usize,
    pub sessions_by_hour: HashMap<String, u32>,
    pub overall_time_range: DateRange,
    pub prompt_stats: PromptStats,
    pub(crate) focus: FocusStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct FocusStats {
    pub(crate) context_switches: usize,
    pub(crate) longest_focus_minutes: i64,
    pub(crate) focus_blocks: usize,
}

#[derive(Serialize)]
pub struct ProjectStat {
    pub name: String,
    pub session_count: usize,
    pub message_count: usize,
    pub time_range: DateRange,
    pub tool_usage: HashMap<String, u32>,
    pub files_touched: Vec<String>,
}

#[derive(Serialize)]
pub struct PromptStats {
    pub avg_length: usize,
    pub short_prompts: usize,
    pub total_prompts: usize,
}

#[derive(Serialize)]
pub struct DecisionsByProject {
    pub project: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct RenderHelpers {
    pub sessions_local: Vec<LocalSession>,
    pub main_work_window_local: Option<MainWorkWindowLocal>,
    pub tool_frequency_top5: Vec<ToolFrequencyItem>,
}

#[derive(Serialize)]
pub struct LocalSession {
    pub session_id: String,
    pub project: String,
    pub start_local: Option<String>,
    pub end_local: Option<String>,
}

#[derive(Serialize)]
pub struct MainWorkWindowLocal {
    pub start: String,
    pub end: String,
    pub method: &'static str,
}

#[derive(Serialize)]
pub struct ToolFrequencyItem {
    pub name: String,
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Decision extraction
// ---------------------------------------------------------------------------

/// Signal words that indicate a user made a decision or chose between alternatives.
const DECISION_SIGNALS_JA: &[&str] = &[
    "にする",
    "を選ぶ",
    "の方がいい",
    "ではなく",
    "より",
    "じゃなくて",
    "そうじゃなくて",
    "いや、",
    "やっぱり",
    "を使う",
    "に変える",
    "にして",
    "に変更",
    "のほうが",
];

const DECISION_SIGNALS_EN: &[&str] = &[
    "instead",
    "rather than",
    "go with",
    "let's use",
    "prefer",
    "switch to",
    "change to",
    "not that",
    "actually,",
    "no,",
];

fn extract_decisions(sessions: &[RawSession]) -> Vec<DecisionPoint> {
    let mut decisions = Vec::new();
    let mut seen = HashSet::new();

    for session in sessions {
        for entry in &session.user_entries {
            if !entry.is_meaningful() {
                continue;
            }
            let text_lower = entry.text.to_lowercase();

            let is_decision = DECISION_SIGNALS_JA.iter().any(|s| entry.text.contains(s))
                || DECISION_SIGNALS_EN.iter().any(|s| text_lower.contains(s));

            if is_decision {
                let key = (entry.timestamp.clone(), entry.text.clone());
                if !seen.insert(key) {
                    continue;
                }

                // Try to extract context from the first ~50 chars
                let context = entry.text.chars().take(80).collect::<String>();

                decisions.push(DecisionPoint {
                    timestamp: entry.timestamp.clone(),
                    project: session.project.clone(),
                    context,
                    user_prompt: entry.text.clone(),
                });
            }
        }
    }

    decisions
}

// ---------------------------------------------------------------------------
// Build output
// ---------------------------------------------------------------------------

pub fn build_output(
    mut sessions: Vec<RawSession>,
    filter_label: &str,
    period: OutputPeriod,
    total_files_scanned: usize,
    stats_only: bool,
    source: SourceMeta,
) -> CollectorOutput {
    sort_sessions_by_recency(&mut sessions);

    let decisions = extract_decisions(&sessions);
    let focus = compute_focus(&sessions);
    let stats = compute_stats(&sessions, &decisions, focus.stats);
    let render_helpers = build_render_helpers(&sessions, &stats, &focus, stats_only);

    let session_summaries = if stats_only {
        Vec::new()
    } else {
        sessions.iter().map(summarize_session).collect()
    };

    CollectorOutput {
        meta: OutputMeta {
            generated_at: chrono::Utc::now().to_rfc3339(),
            filter_label: filter_label.to_string(),
            period,
            source,
            total_sessions: sessions.len(),
            total_files_scanned,
        },
        sessions: session_summaries,
        decisions,
        stats,
        render_helpers,
    }
}

fn build_render_helpers(
    sessions: &[RawSession],
    stats: &AggregateStats,
    focus: &FocusCalculation,
    stats_only: bool,
) -> RenderHelpers {
    let sessions_local = if stats_only {
        Vec::new()
    } else {
        sessions
            .iter()
            .map(|session| {
                let range = parsed_session_range(session);
                LocalSession {
                    session_id: session.session_id.clone(),
                    project: session.project.clone(),
                    start_local: range.as_ref().map(|(start, _)| format_local_time(start)),
                    end_local: range.as_ref().map(|(_, end)| format_local_time(end)),
                }
            })
            .collect()
    };

    let main_work_window_local =
        focus
            .main_work_window
            .as_ref()
            .map(|(start, end)| MainWorkWindowLocal {
                start: format_local_time(start),
                end: format_local_time(end),
                method: "longest_block_with_gaps_under_30_minutes",
            });

    let mut tools: Vec<_> = stats.tool_frequency.iter().collect();
    tools.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let tool_frequency_top5 = tools
        .into_iter()
        .take(5)
        .map(|(name, count)| ToolFrequencyItem {
            name: name.clone(),
            count: *count,
        })
        .collect();

    RenderHelpers {
        sessions_local,
        main_work_window_local,
        tool_frequency_top5,
    }
}

fn format_local_time(timestamp: &DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// 人間が読みやすいサマリーテキストを生成する
pub fn format_summary(output: &CollectorOutput) -> String {
    let mut buf = String::new();
    let s = &output.stats;

    if output.meta.total_sessions == 0 {
        writeln!(
            buf,
            "指定期間（{}）にセッションデータが見つかりませんでした。",
            output.meta.filter_label
        )
        .ok();
        writeln!(buf, "ソース: {}", format_source_line(&output.meta.source)).ok();
        writeln!(buf).ok();
        writeln!(buf, "ヒント:").ok();
        writeln!(buf, "  - 期間を広げてみてください: --days 7 や --days 30").ok();
        writeln!(
            buf,
            "  - プロジェクトフィルタを外してみてください（--project を省略）"
        )
        .ok();
        writeln!(buf, "  - 全期間を確認: --days 0").ok();
        return buf;
    }

    writeln!(buf, "ソース: {}", format_source_line(&output.meta.source)).ok();
    writeln!(
        buf,
        "期間: {} | セッション: {} | プロジェクト: {} | 意思決定: {}",
        output.meta.filter_label,
        output.meta.total_sessions,
        s.projects_worked_on.len(),
        output.decisions.len(),
    )
    .ok();
    writeln!(
        buf,
        "メッセージ: user {} / assistant {} | ツール使用: {}",
        s.total_user_messages, s.total_assistant_messages, s.total_tool_uses,
    )
    .ok();
    writeln!(
        buf,
        "トークン: input {} / output {}",
        s.total_input_tokens, s.total_output_tokens,
    )
    .ok();
    writeln!(
        buf,
        "集中: {} ブロック | 最長: {} 分 | プロジェクト切替: {} 回",
        s.focus.focus_blocks, s.focus.longest_focus_minutes, s.focus.context_switches,
    )
    .ok();

    if !s.projects_worked_on.is_empty() {
        writeln!(buf).ok();
        writeln!(buf, "プロジェクト:").ok();
        for p in &s.projects_worked_on {
            writeln!(
                buf,
                "  {:<30} {:>3} セッション  {:>6} メッセージ",
                p.name, p.session_count, p.message_count,
            )
            .ok();
        }
    }

    if !s.tool_frequency.is_empty() {
        writeln!(buf).ok();
        writeln!(buf, "ツール:").ok();
        let mut tools: Vec<_> = s.tool_frequency.iter().collect();
        tools.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let total = s.total_tool_uses.max(1) as f64;
        for (name, count) in tools.iter().take(8) {
            let pct = (**count as f64 / total) * 100.0;
            writeln!(buf, "  {:<12} {:>5} ({:.1}%)", name, count, pct).ok();
        }
    }

    if !output.decisions.is_empty() {
        writeln!(buf).ok();
        writeln!(buf, "意思決定 ({}):", output.decisions.len()).ok();
        for d in output.decisions.iter().take(5) {
            let ctx: String = d.context.chars().take(60).collect();
            writeln!(buf, "  [{}] {}", d.project, ctx).ok();
        }
        if output.decisions.len() > 5 {
            writeln!(buf, "  ... 他 {} 件", output.decisions.len() - 5).ok();
        }
    }

    buf
}

fn format_source_line(source: &SourceMeta) -> String {
    let resolved = if source.resolved.is_empty() {
        "なし".to_string()
    } else {
        source.resolved.join(", ")
    };

    format!("requested {} | resolved {}", source.requested, resolved)
}

struct FocusEvent<'a> {
    timestamp: DateTime<Utc>,
    session_id: &'a str,
}

struct FocusInterval<'a> {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    project: &'a str,
    session_id: &'a str,
}

struct FocusCalculation {
    stats: FocusStats,
    main_work_window: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// Aggregate uninterrupted work blocks and context switches.
///
/// - Focus blocks and the main work window are detected from individual
///   message timestamps (event-based). Splitting on events instead of on
///   whole-session ranges lets a single long-running session with internal
///   idle gaps show up as multiple focus blocks, so a background session
///   that stays open all day does not silently pin the "longest block" to
///   the full day.
/// - Context switches stay session-based. Every parallel session generates
///   its own timestamp stream, so counting switches per interleaved event
///   would explode with the number of parallel sessions. Session-boundary
///   counting keeps the metric interpretable across single- and
///   multi-source setups.
///
/// A gap of 30 minutes or more starts a new block (or ends a session's
/// contribution to switch counting).
fn compute_focus(sessions: &[RawSession]) -> FocusCalculation {
    let (events, intervals) = collect_focus_inputs(sessions);
    let main_work_window = compute_focus_blocks(&events);
    let (focus_blocks, longest_window) = match main_work_window {
        Some(window) => (count_focus_blocks(&events), window),
        None => {
            return FocusCalculation {
                stats: FocusStats::default(),
                main_work_window: None,
            };
        }
    };
    let context_switches = count_context_switches(&intervals);

    FocusCalculation {
        stats: FocusStats {
            context_switches,
            longest_focus_minutes: (longest_window.1 - longest_window.0).num_minutes(),
            focus_blocks,
        },
        main_work_window: Some(longest_window),
    }
}

fn collect_focus_inputs<'a>(
    sessions: &'a [RawSession],
) -> (Vec<FocusEvent<'a>>, Vec<FocusInterval<'a>>) {
    let mut events: Vec<FocusEvent<'a>> = Vec::new();
    let mut intervals: Vec<FocusInterval<'a>> = Vec::new();

    for session in sessions {
        let timestamps = session
            .user_entries
            .iter()
            .map(|entry| entry.timestamp.as_str())
            .chain(
                session
                    .assistant_entries
                    .iter()
                    .map(|entry| entry.timestamp.as_str()),
            );
        let mut collected: Vec<DateTime<Utc>> = Vec::new();
        let mut parsed_all = true;
        for timestamp in timestamps {
            match DateTime::parse_from_rfc3339(timestamp) {
                Ok(dt) => collected.push(dt.with_timezone(&Utc)),
                Err(_) => {
                    parsed_all = false;
                    break;
                }
            }
        }
        if !parsed_all || collected.is_empty() {
            continue;
        }

        let min = *collected.iter().min().expect("collected is non-empty");
        let max = *collected.iter().max().expect("collected is non-empty");
        intervals.push(FocusInterval {
            start: min,
            end: max,
            project: &session.project,
            session_id: &session.session_id,
        });
        for timestamp in collected {
            events.push(FocusEvent {
                timestamp,
                session_id: &session.session_id,
            });
        }
    }

    events.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.session_id.cmp(right.session_id))
    });
    intervals.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.session_id.cmp(right.session_id))
    });

    (events, intervals)
}

fn compute_focus_blocks(events: &[FocusEvent<'_>]) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let first = events.first()?;
    let mut block_start = first.timestamp;
    let mut block_end = first.timestamp;
    let mut longest_window = (block_start, block_end);
    for event in events.iter().skip(1) {
        let gap = event.timestamp - block_end;
        if gap < chrono::Duration::minutes(30) {
            if event.timestamp > block_end {
                block_end = event.timestamp;
            }
        } else {
            if block_end - block_start > longest_window.1 - longest_window.0 {
                longest_window = (block_start, block_end);
            }
            block_start = event.timestamp;
            block_end = event.timestamp;
        }
    }
    if block_end - block_start > longest_window.1 - longest_window.0 {
        longest_window = (block_start, block_end);
    }
    Some(longest_window)
}

fn count_focus_blocks(events: &[FocusEvent<'_>]) -> usize {
    if events.is_empty() {
        return 0;
    }
    let mut blocks = 1usize;
    let mut previous = events[0].timestamp;
    for event in events.iter().skip(1) {
        if event.timestamp - previous >= chrono::Duration::minutes(30) {
            blocks += 1;
        }
        if event.timestamp > previous {
            previous = event.timestamp;
        }
    }
    blocks
}

fn count_context_switches(intervals: &[FocusInterval<'_>]) -> usize {
    let mut context_switches = 0usize;
    let mut previous: Option<&FocusInterval<'_>> = None;
    let mut block_end: Option<DateTime<Utc>> = None;

    for interval in intervals {
        if let (Some(prev), Some(prev_end)) = (previous, block_end) {
            let gap = interval.start - prev_end;
            if gap < chrono::Duration::minutes(30) && interval.project != prev.project {
                context_switches += 1;
            }
        }
        block_end = Some(match block_end {
            Some(existing) if existing > interval.end => existing,
            _ => interval.end,
        });
        previous = Some(interval);
    }

    context_switches
}

fn parsed_session_range(session: &RawSession) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let timestamps = session
        .user_entries
        .iter()
        .map(|entry| entry.timestamp.as_str())
        .chain(
            session
                .assistant_entries
                .iter()
                .map(|entry| entry.timestamp.as_str()),
        );
    let mut range: Option<(DateTime<Utc>, DateTime<Utc>)> = None;

    for timestamp in timestamps {
        // A session with any unparseable timestamp is excluded from focus
        // aggregation; mixing partial ranges would distort gaps and switches.
        let parsed = DateTime::parse_from_rfc3339(timestamp)
            .ok()?
            .with_timezone(&Utc);
        match range.as_mut() {
            Some((start, end)) => {
                if parsed < *start {
                    *start = parsed;
                } else if parsed > *end {
                    *end = parsed;
                }
            }
            None => {
                let end = parsed.to_owned();
                range = Some((parsed, end));
            }
        }
    }

    range
}

fn compute_stats(
    sessions: &[RawSession],
    decisions: &[DecisionPoint],
    focus: FocusStats,
) -> AggregateStats {
    let mut total_user = 0usize;
    let mut total_assistant = 0usize;
    let mut total_tool_uses = 0usize;
    let mut tool_freq: HashMap<String, u32> = HashMap::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut hour_counts: HashMap<String, u32> = HashMap::new();
    let mut all_timestamps: Vec<&str> = Vec::new();
    let mut total_prompt_chars: usize = 0;
    let mut short_prompts: usize = 0;
    let mut total_prompts: usize = 0;

    // プロジェクト別集約用
    struct ProjectAccum {
        session_count: usize,
        message_count: usize,
        timestamps: Vec<String>,
        tool_usage: HashMap<String, u32>,
        files: Vec<String>,
    }
    let mut project_accum: HashMap<String, ProjectAccum> = HashMap::new();

    for session in sessions {
        let assistant_messages = assistant_message_count(&session.assistant_entries);
        total_user += session.user_entries.len();
        total_assistant += assistant_messages;

        let msg_count = session.user_entries.len() + assistant_messages;
        let pa = project_accum
            .entry(session.project.clone())
            .or_insert_with(|| ProjectAccum {
                session_count: 0,
                message_count: 0,
                timestamps: Vec::new(),
                tool_usage: HashMap::new(),
                files: Vec::new(),
            });
        pa.session_count += 1;
        pa.message_count += msg_count;

        for ue in &session.user_entries {
            all_timestamps.push(&ue.timestamp);
            pa.timestamps.push(ue.timestamp.clone());

            // 時間帯別集計
            if let Some(hour) = extract_local_hour(&ue.timestamp) {
                *hour_counts.entry(hour).or_insert(0) += 1;
            }

            // プロンプト統計
            total_prompt_chars += ue.text.len();
            total_prompts += 1;
            if ue.text.len() < 20 {
                short_prompts += 1;
            }
        }

        for ae in &session.assistant_entries {
            all_timestamps.push(&ae.timestamp);
            pa.timestamps.push(ae.timestamp.clone());

            for tool in &ae.tool_uses {
                *tool_freq.entry(tool.clone()).or_insert(0) += 1;
                *pa.tool_usage.entry(tool.clone()).or_insert(0) += 1;
                total_tool_uses += 1;
            }
            pa.files.extend(ae.file_paths.iter().cloned());
            total_input_tokens += ae.input_tokens;
            total_output_tokens += ae.output_tokens;
        }
    }

    // 全体の時間範囲
    all_timestamps.sort();
    let overall_time_range = DateRange {
        start: all_timestamps.first().map(|s| s.to_string()),
        end: all_timestamps.last().map(|s| s.to_string()),
    };

    // プロジェクト別集約
    let mut projects_worked_on: Vec<ProjectStat> = project_accum
        .into_iter()
        .map(|(name, mut pa)| {
            pa.timestamps.sort();
            pa.files.sort();
            pa.files.dedup();
            ProjectStat {
                name,
                session_count: pa.session_count,
                message_count: pa.message_count,
                time_range: DateRange {
                    start: pa.timestamps.first().cloned(),
                    end: pa.timestamps.last().cloned(),
                },
                tool_usage: pa.tool_usage,
                files_touched: pa.files,
            }
        })
        .collect();
    projects_worked_on.sort_by(|a, b| {
        b.message_count
            .cmp(&a.message_count)
            .then_with(|| a.name.cmp(&b.name))
    });

    // decisions のプロジェクト別集計
    let mut dec_counts: HashMap<String, usize> = HashMap::new();
    for d in decisions {
        *dec_counts.entry(d.project.clone()).or_insert(0) += 1;
    }
    let mut decisions_by_project: Vec<DecisionsByProject> = dec_counts
        .into_iter()
        .map(|(project, count)| DecisionsByProject { project, count })
        .collect();
    decisions_by_project.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.project.cmp(&b.project))
    });

    let avg_length = total_prompt_chars.checked_div(total_prompts).unwrap_or(0);
    AggregateStats {
        projects_worked_on,
        total_user_messages: total_user,
        total_assistant_messages: total_assistant,
        total_tool_uses,
        tool_frequency: tool_freq,
        total_input_tokens,
        total_output_tokens,
        decisions_by_project,
        total_decisions: decisions.len(),
        sessions_by_hour: hour_counts,
        overall_time_range,
        prompt_stats: PromptStats {
            avg_length,
            short_prompts,
            total_prompts,
        },
        focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession};

    fn timed_session(project: &str, session_id: &str, start: &str, end: &str) -> RawSession {
        RawSession {
            session_id: session_id.to_string(),
            project: project.to_string(),
            project_path: format!("/tmp/{project}"),
            git_branch: Some("main".to_string()),
            user_entries: vec![ParsedUserEntry {
                timestamp: start.to_string(),
                text: "prompt".to_string(),
                is_meta: false,
            }],
            assistant_entries: vec![ParsedAssistantEntry {
                timestamp: end.to_string(),
                message_count: 1,
                tool_uses: Vec::new(),
                input_tokens: 0,
                output_tokens: 0,
                file_paths: Vec::new(),
            }],
        }
    }

    #[test]
    fn counts_project_switches_in_start_time_order() {
        let sessions = vec![
            timed_session("alpha", "a", "2026-04-01T09:00:00Z", "2026-04-01T09:10:00Z"),
            timed_session("beta", "b", "2026-04-01T09:20:00Z", "2026-04-01T09:30:00Z"),
            timed_session("beta", "c", "2026-04-01T09:40:00Z", "2026-04-01T09:50:00Z"),
            timed_session("alpha", "d", "2026-04-01T10:00:00Z", "2026-04-01T10:10:00Z"),
        ];

        assert_eq!(compute_focus(&sessions).stats.context_switches, 2);
    }

    #[test]
    fn does_not_count_project_switches_after_a_thirty_minute_gap() {
        let sessions = vec![
            timed_session("alpha", "a", "2026-04-01T09:00:00Z", "2026-04-01T09:10:00Z"),
            timed_session("beta", "b", "2026-04-01T09:40:00Z", "2026-04-01T10:00:00Z"),
        ];

        assert_eq!(compute_focus(&sessions).stats.context_switches, 0);
    }

    #[test]
    fn merges_blocks_when_the_gap_is_under_thirty_minutes() {
        let sessions = vec![
            timed_session("alpha", "a", "2026-04-01T09:00:00Z", "2026-04-01T09:10:00Z"),
            timed_session("alpha", "b", "2026-04-01T09:39:00Z", "2026-04-01T10:00:00Z"),
            timed_session("alpha", "c", "2026-04-01T11:00:00Z", "2026-04-01T11:20:00Z"),
        ];

        let focus = compute_focus(&sessions).stats;

        assert_eq!(focus.focus_blocks, 2);
        assert_eq!(focus.longest_focus_minutes, 60);
    }

    #[test]
    fn starts_a_new_block_at_the_thirty_minute_boundary() {
        let sessions = vec![
            timed_session("alpha", "a", "2026-04-01T09:00:00Z", "2026-04-01T09:10:00Z"),
            timed_session("alpha", "b", "2026-04-01T09:40:00Z", "2026-04-01T10:00:00Z"),
        ];

        let focus = compute_focus(&sessions).stats;

        assert_eq!(focus.focus_blocks, 2);
        assert_eq!(focus.longest_focus_minutes, 20);
    }

    #[test]
    fn splits_focus_blocks_by_internal_gaps_within_a_single_session() {
        // A session that spans many hours but goes idle for 30+ minutes in
        // the middle must not report the full span as one focus block. Old
        // session-range logic wrongly collapsed the whole range into one
        // 15-hour block; event-based logic breaks at the internal gap.
        let session = RawSession {
            session_id: "long".to_string(),
            project: "nippo".to_string(),
            project_path: "/tmp/nippo".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: vec![
                ParsedUserEntry {
                    timestamp: "2026-08-24T03:33:00Z".to_string(),
                    text: "early morning".to_string(),
                    is_meta: false,
                },
                ParsedUserEntry {
                    timestamp: "2026-08-24T19:00:00Z".to_string(),
                    text: "evening".to_string(),
                    is_meta: false,
                },
            ],
            assistant_entries: vec![
                ParsedAssistantEntry {
                    timestamp: "2026-08-24T03:45:00Z".to_string(),
                    message_count: 1,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    file_paths: Vec::new(),
                },
                ParsedAssistantEntry {
                    timestamp: "2026-08-24T19:01:00Z".to_string(),
                    message_count: 1,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    file_paths: Vec::new(),
                },
            ],
        };

        let focus = compute_focus(&[session]).stats;

        assert_eq!(focus.focus_blocks, 2);
        // Longest block is the morning stretch 03:33 → 03:45 = 12 minutes.
        // Not 927 minutes.
        assert_eq!(focus.longest_focus_minutes, 12);
    }

    #[test]
    fn context_switches_stay_session_based_when_focus_blocks_split_internally() {
        // Guard rail: focus block detection moved to event-based splitting,
        // but context_switches must still be counted per session boundary
        // so parallel/interleaved event streams do not inflate the metric.
        // First session's internal messages straddle a 30+ minute gap;
        // event-based focus_blocks splits it, session-based
        // context_switches counts only the one alpha→beta transition.
        let long_session = RawSession {
            session_id: "alpha-long".to_string(),
            project: "alpha".to_string(),
            project_path: "/tmp/alpha".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: vec![
                ParsedUserEntry {
                    timestamp: "2026-04-01T09:00:00Z".to_string(),
                    text: "morning".to_string(),
                    is_meta: false,
                },
                ParsedUserEntry {
                    timestamp: "2026-04-01T13:00:00Z".to_string(),
                    text: "afternoon".to_string(),
                    is_meta: false,
                },
            ],
            assistant_entries: vec![
                ParsedAssistantEntry {
                    timestamp: "2026-04-01T09:05:00Z".to_string(),
                    message_count: 1,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    file_paths: Vec::new(),
                },
                ParsedAssistantEntry {
                    timestamp: "2026-04-01T13:05:00Z".to_string(),
                    message_count: 1,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    file_paths: Vec::new(),
                },
            ],
        };
        let sessions = vec![
            long_session,
            timed_session("beta", "b", "2026-04-01T13:10:00Z", "2026-04-01T13:20:00Z"),
        ];

        let focus = compute_focus(&sessions).stats;

        assert!(focus.focus_blocks >= 2);
        assert_eq!(focus.context_switches, 1);
    }

    #[test]
    fn excludes_sessions_with_unparseable_timestamps() {
        let sessions = vec![
            timed_session(
                "alpha",
                "valid",
                "2026-04-01T09:00:00Z",
                "2026-04-01T09:20:00Z",
            ),
            timed_session("beta", "invalid", "not-a-timestamp", "2026-04-01T09:30:00Z"),
        ];

        let focus = compute_focus(&sessions).stats;

        assert_eq!(focus.focus_blocks, 1);
        assert_eq!(focus.context_switches, 0);
        assert_eq!(focus.longest_focus_minutes, 20);
    }

    #[test]
    fn deduplicates_decisions_by_timestamp_and_prompt() {
        let sessions = vec![
            RawSession {
                session_id: "session-a".to_string(),
                project: "nippo".to_string(),
                project_path: "/tmp/nippo".to_string(),
                git_branch: Some("main".to_string()),
                user_entries: vec![ParsedUserEntry {
                    timestamp: "2026-04-01T10:00:00Z".to_string(),
                    text: "Rust にする".to_string(),
                    is_meta: false,
                }],
                assistant_entries: Vec::<ParsedAssistantEntry>::new(),
            },
            RawSession {
                session_id: "session-b".to_string(),
                project: "nippo".to_string(),
                project_path: "/tmp/nippo".to_string(),
                git_branch: Some("main".to_string()),
                user_entries: vec![ParsedUserEntry {
                    timestamp: "2026-04-01T10:00:00Z".to_string(),
                    text: "Rust にする".to_string(),
                    is_meta: false,
                }],
                assistant_entries: Vec::<ParsedAssistantEntry>::new(),
            },
        ];

        let output = build_output(
            sessions,
            "today",
            OutputPeriod {
                from: Some("2026-04-01".to_string()),
                to: Some("2026-04-01".to_string()),
                timezone: "Asia/Tokyo".to_string(),
            },
            2,
            false,
            SourceMeta {
                requested: "all".to_string(),
                resolved: vec!["claude".to_string(), "codex".to_string()],
            },
        );

        assert_eq!(output.decisions.len(), 1);
        assert_eq!(output.stats.total_decisions, 1);
    }

    #[test]
    fn includes_source_metadata_in_summary() {
        let sessions = vec![RawSession {
            session_id: "session-a".to_string(),
            project: "nippo".to_string(),
            project_path: "/tmp/nippo".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: vec![ParsedUserEntry {
                timestamp: "2026-04-01T10:00:00Z".to_string(),
                text: "進める".to_string(),
                is_meta: false,
            }],
            assistant_entries: Vec::<ParsedAssistantEntry>::new(),
        }];

        let output = build_output(
            sessions,
            "today",
            OutputPeriod {
                from: Some("2026-04-01".to_string()),
                to: Some("2026-04-01".to_string()),
                timezone: "Asia/Tokyo".to_string(),
            },
            1,
            false,
            SourceMeta {
                requested: "auto".to_string(),
                resolved: vec!["codex".to_string()],
            },
        );
        let summary = format_summary(&output);

        assert!(summary.contains("ソース: requested auto | resolved codex"));
        assert!(summary.contains("集中: 1 ブロック | 最長: 0 分 | プロジェクト切替: 0 回"));
        assert_eq!(output.meta.source.requested, "auto");
        assert_eq!(output.meta.source.resolved, vec!["codex".to_string()]);
    }

    #[test]
    fn excludes_skill_preambles_from_decisions() {
        let sessions = vec![RawSession {
            session_id: "session-a".to_string(),
            project: "nippo".to_string(),
            project_path: "/tmp/nippo".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: vec![
                ParsedUserEntry {
                    timestamp: "2026-08-13T10:00:00Z".to_string(),
                    text: "Base directory for this skill: /tmp/instead-of-this".to_string(),
                    is_meta: false,
                },
                ParsedUserEntry {
                    timestamp: "2026-08-13T10:01:00Z".to_string(),
                    text: "(Re-invocation of /switch-to-example)".to_string(),
                    is_meta: false,
                },
                ParsedUserEntry {
                    timestamp: "2026-08-13T10:02:00Z".to_string(),
                    text: "Python ではなく Rust にする".to_string(),
                    is_meta: false,
                },
            ],
            assistant_entries: Vec::new(),
        }];

        let output = build_output(
            sessions,
            "today",
            OutputPeriod {
                from: Some("2026-08-13".to_string()),
                to: Some("2026-08-13".to_string()),
                timezone: "Asia/Tokyo".to_string(),
            },
            1,
            false,
            SourceMeta {
                requested: "claude".to_string(),
                resolved: vec!["claude".to_string()],
            },
        );

        assert_eq!(output.decisions.len(), 1);
        assert_eq!(
            output.decisions[0].user_prompt,
            "Python ではなく Rust にする"
        );
    }

    #[test]
    fn builds_deterministic_render_helpers() {
        let sessions = vec![
            timed_session("alpha", "a", "2026-05-05T03:17:00Z", "2026-05-05T03:45:00Z"),
            timed_session("alpha", "b", "2026-05-05T04:00:00Z", "2026-05-05T04:17:00Z"),
        ];
        let mut sessions = sessions;
        sessions[0].assistant_entries[0].tool_uses = vec!["Read".to_string(), "Edit".to_string()];
        sessions[1].assistant_entries[0].tool_uses = vec!["Edit".to_string()];

        let output = build_output(
            sessions,
            "today",
            OutputPeriod {
                from: Some("2026-05-05".to_string()),
                to: Some("2026-05-05".to_string()),
                timezone: "Asia/Tokyo".to_string(),
            },
            2,
            false,
            SourceMeta {
                requested: "claude".to_string(),
                resolved: vec!["claude".to_string()],
            },
        );
        let value = serde_json::to_value(&output).expect("serialize output");
        assert_eq!(value["meta"]["period"]["from"], "2026-05-05");
        assert_eq!(value["meta"]["period"]["to"], "2026-05-05");
        assert_eq!(value["meta"]["period"]["timezone"], "Asia/Tokyo");
        let helpers = &value["render_helpers"];

        assert_eq!(helpers["sessions_local"].as_array().map(Vec::len), Some(2));
        assert_eq!(helpers["tool_frequency_top5"][0]["name"], "Edit");
        assert_eq!(helpers["tool_frequency_top5"][0]["count"], 2);
        assert_eq!(
            helpers["main_work_window_local"]["method"],
            "longest_block_with_gaps_under_30_minutes"
        );

        let start = helpers["main_work_window_local"]["start"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .expect("local start");
        let end = helpers["main_work_window_local"]["end"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .expect("local end");
        assert_eq!((end - start).num_minutes(), 60);
    }
}
