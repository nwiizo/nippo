use chrono::{DateTime, Local, Timelike, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::session::{RawSession, assistant_message_count, summarize_session};

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
}

#[derive(Serialize)]
pub struct OutputMeta {
    pub generated_at: String,
    pub filter_label: String,
    pub source: SourceMeta,
    pub total_sessions: usize,
    pub total_files_scanned: usize,
}

#[derive(Serialize)]
pub struct SourceMeta {
    pub requested: String,
    pub resolved: Vec<String>,
}

#[derive(Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub project: String,
    pub project_path: String,
    pub git_branch: Option<String>,
    pub time_range: DateRange,
    pub user_prompts: Vec<PromptSummary>,
    pub tool_usage: HashMap<String, u32>,
    pub message_counts: MessageCounts,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub files_touched: Vec<String>,
}

#[derive(Serialize)]
pub struct DateRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Serialize)]
pub struct PromptSummary {
    pub text: String,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct MessageCounts {
    pub user: usize,
    pub assistant: usize,
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
    total_files_scanned: usize,
    stats_only: bool,
    source: SourceMeta,
) -> CollectorOutput {
    sessions.sort_by(|a, b| {
        let left = latest_timestamp(a);
        let right = latest_timestamp(b);
        right
            .cmp(left)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    let decisions = extract_decisions(&sessions);
    let stats = compute_stats(&sessions, &decisions);

    let session_summaries = if stats_only {
        Vec::new()
    } else {
        sessions.iter().map(summarize_session).collect()
    };

    CollectorOutput {
        meta: OutputMeta {
            generated_at: chrono::Utc::now().to_rfc3339(),
            filter_label: filter_label.to_string(),
            source,
            total_sessions: sessions.len(),
            total_files_scanned,
        },
        sessions: session_summaries,
        decisions,
        stats,
    }
}

fn latest_timestamp(session: &RawSession) -> &str {
    let user = session
        .user_entries
        .last()
        .map(|entry| entry.timestamp.as_str())
        .unwrap_or_default();
    let assistant = session
        .assistant_entries
        .last()
        .map(|entry| entry.timestamp.as_str())
        .unwrap_or_default();

    if assistant > user { assistant } else { user }
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

fn compute_stats(sessions: &[RawSession], decisions: &[DecisionPoint]) -> AggregateStats {
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

    let avg_length = if total_prompts > 0 {
        total_prompt_chars / total_prompts
    } else {
        0
    };

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession};

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
                }],
                assistant_entries: Vec::<ParsedAssistantEntry>::new(),
            },
        ];

        let output = build_output(
            sessions,
            "today",
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
            }],
            assistant_entries: Vec::<ParsedAssistantEntry>::new(),
        }];

        let output = build_output(
            sessions,
            "today",
            1,
            false,
            SourceMeta {
                requested: "auto".to_string(),
                resolved: vec!["codex".to_string()],
            },
        );
        let summary = format_summary(&output);

        assert!(summary.contains("ソース: requested auto | resolved codex"));
        assert_eq!(output.meta.source.requested, "auto");
        assert_eq!(output.meta.source.resolved, vec!["codex".to_string()]);
    }
}
