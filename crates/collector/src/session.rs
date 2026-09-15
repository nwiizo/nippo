use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Serialize;

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

#[derive(Clone, Debug)]
pub struct RawSession {
    pub session_id: String,
    pub project: String,
    pub project_path: String,
    pub git_branch: Option<String>,
    pub user_entries: Vec<ParsedUserEntry>,
    pub assistant_entries: Vec<ParsedAssistantEntry>,
}

#[derive(Clone, Debug)]
pub struct ParsedUserEntry {
    pub timestamp: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAssistantEntry {
    pub timestamp: String,
    pub message_count: usize,
    pub tool_uses: Vec<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub file_paths: Vec<String>,
}

pub fn assistant_message_count(entries: &[ParsedAssistantEntry]) -> usize {
    entries.iter().map(|entry| entry.message_count).sum()
}

pub(crate) fn merge_sessions_by_id(sessions: Vec<RawSession>) -> Vec<RawSession> {
    let mut merged = BTreeMap::<String, RawSession>::new();
    let mut sessions_without_id = Vec::new();

    for session in sessions {
        if session.session_id.is_empty() {
            sessions_without_id.push(session);
            continue;
        }

        match merged.entry(session.session_id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(session);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                merge_session(entry.get_mut(), session);
            }
        }
    }

    let mut sessions: Vec<RawSession> = merged.into_values().collect();
    sessions.extend(sessions_without_id);
    for session in &mut sessions {
        deduplicate_user_entries(&mut session.user_entries);
        session.assistant_entries.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tool_uses.cmp(&right.tool_uses))
                .then_with(|| left.input_tokens.cmp(&right.input_tokens))
                .then_with(|| left.output_tokens.cmp(&right.output_tokens))
                .then_with(|| left.file_paths.cmp(&right.file_paths))
                .then_with(|| left.message_count.cmp(&right.message_count))
        });
        session.assistant_entries.dedup();
    }
    sessions
}

pub(crate) fn deduplicate_user_entries(entries: &mut Vec<ParsedUserEntry>) {
    entries.sort_by(|left, right| {
        left.timestamp
            .cmp(&right.timestamp)
            .then_with(|| left.text.cmp(&right.text))
    });
    entries.dedup_by(|left, right| left.timestamp == right.timestamp && left.text == right.text);
}

fn merge_session(target: &mut RawSession, mut source: RawSession) {
    let source_recency = latest_timestamp(&source).cmp(latest_timestamp(target));
    if prefers_project_metadata(&source, target) {
        target.project = source.project;
        target.project_path = source.project_path;
    }

    if let Some(source_branch) = source.git_branch.take()
        && !source_branch.is_empty()
        && target.git_branch.as_ref().is_none_or(|target_branch| {
            target_branch.is_empty()
                || source_recency.is_gt()
                || (source_recency.is_eq() && source_branch < *target_branch)
        })
    {
        target.git_branch = Some(source_branch);
    }

    target.user_entries.append(&mut source.user_entries);
    target
        .assistant_entries
        .append(&mut source.assistant_entries);
}

fn prefers_project_metadata(candidate: &RawSession, current: &RawSession) -> bool {
    let quality = |session: &RawSession| {
        usize::from(!session.project_path.is_empty())
            + usize::from(!session.project.is_empty() && session.project != "unknown")
    };
    let candidate_quality = quality(candidate);
    let current_quality = quality(current);

    candidate_quality > current_quality
        || (candidate_quality == current_quality
            && (candidate.project_path.as_str(), candidate.project.as_str())
                < (current.project_path.as_str(), current.project.as_str()))
}

pub(crate) fn is_meaningful_prompt(text: &str) -> bool {
    const NOISE_PREFIXES: &[&str] = &[
        "# AGENTS.md instructions for ",
        "<environment_context>",
        "<command-name>",
        "<command-message>",
        "<command-args>",
        "<task-notification>",
        "<local-command-caveat>",
        "<local-command-stdout>",
        "<bash-input>",
        "<bash-stdout>",
        "[Image #",
        "[Image: source:",
        "[Request interrupted by user]",
        "This session is being continued from a previous conversation",
        "Base directory for this skill:",
        "(Re-invocation of /",
    ];

    let trimmed = text.trim();
    if trimmed.is_empty()
        || NOISE_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
    {
        return false;
    }

    let normalized = trimmed
        .trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '.' | ',' | '!' | '?' | '。' | '、' | '！' | '？')
        })
        .to_lowercase();
    !matches!(
        normalized.as_str(),
        "y" | "yes" | "ok" | "okay" | "done" | "はい"
    )
}

pub(crate) fn retain_meaningful_prompts(sessions: &mut Vec<RawSession>) {
    for session in sessions.iter_mut() {
        session
            .user_entries
            .retain(|entry| is_meaningful_prompt(&entry.text));
    }
    sessions.retain(|session| !session.user_entries.is_empty());
}

pub(crate) fn sort_sessions_by_recency(sessions: &mut [RawSession]) {
    sessions.sort_by(|left, right| {
        latest_timestamp(right)
            .cmp(latest_timestamp(left))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

pub(crate) fn latest_timestamp(session: &RawSession) -> &str {
    session
        .user_entries
        .iter()
        .map(|entry| entry.timestamp.as_str())
        .chain(
            session
                .assistant_entries
                .iter()
                .map(|entry| entry.timestamp.as_str()),
        )
        .max()
        .unwrap_or_default()
}

/// RawSession から出力用の SessionSummary を構築する
pub fn summarize_session(session: &RawSession) -> SessionSummary {
    let mut tool_usage: HashMap<String, u32> = HashMap::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut all_file_paths: Vec<String> = Vec::new();
    let assistant_messages = assistant_message_count(&session.assistant_entries);

    for entry in &session.assistant_entries {
        for tool in &entry.tool_uses {
            *tool_usage.entry(tool.clone()).or_insert(0) += 1;
        }
        total_input_tokens += entry.input_tokens;
        total_output_tokens += entry.output_tokens;
        all_file_paths.extend(
            entry
                .file_paths
                .iter()
                .filter_map(|path| normalize_file_path(path, &session.project_path)),
        );
    }

    all_file_paths.sort();
    all_file_paths.dedup();

    let user_prompts: Vec<PromptSummary> = session
        .user_entries
        .iter()
        .map(|entry| PromptSummary {
            text: entry.text.clone(),
            timestamp: entry.timestamp.clone(),
        })
        .collect();

    let mut timestamps: Vec<&str> = Vec::new();
    for entry in &session.user_entries {
        timestamps.push(&entry.timestamp);
    }
    for entry in &session.assistant_entries {
        timestamps.push(&entry.timestamp);
    }
    timestamps.sort();

    let time_range = DateRange {
        start: timestamps.first().map(|value| value.to_string()),
        end: timestamps.last().map(|value| value.to_string()),
    };

    SessionSummary {
        session_id: session.session_id.clone(),
        project: session.project.clone(),
        project_path: session.project_path.clone(),
        git_branch: session.git_branch.clone(),
        time_range,
        user_prompts,
        tool_usage,
        message_counts: MessageCounts {
            user: session.user_entries.len(),
            assistant: assistant_messages,
        },
        total_input_tokens,
        total_output_tokens,
        files_touched: all_file_paths,
    }
}

fn normalize_file_path(path: &str, project_path: &str) -> Option<String> {
    if path.is_empty() || matches!(path, "." | ".." | "~" | "/") {
        return None;
    }

    if !project_path.is_empty()
        && let Ok(stripped) = Path::new(path).strip_prefix(project_path)
    {
        let stripped = stripped.to_string_lossy().to_string();
        if !stripped.is_empty() && stripped != "." {
            return Some(stripped);
        }
        return None;
    }

    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_session(
        session_id: &str,
        project: &str,
        user_entries: Vec<ParsedUserEntry>,
        assistant_entries: Vec<ParsedAssistantEntry>,
    ) -> RawSession {
        RawSession {
            session_id: session_id.to_string(),
            project: project.to_string(),
            project_path: format!("/tmp/{project}"),
            git_branch: Some("main".to_string()),
            user_entries,
            assistant_entries,
        }
    }

    fn user_entry(timestamp: &str, text: &str) -> ParsedUserEntry {
        ParsedUserEntry {
            timestamp: timestamp.to_string(),
            text: text.to_string(),
        }
    }

    fn assistant_entry(timestamp: &str, tool: &str) -> ParsedAssistantEntry {
        ParsedAssistantEntry {
            timestamp: timestamp.to_string(),
            message_count: 1,
            tool_uses: vec![tool.to_string()],
            input_tokens: 10,
            output_tokens: 5,
            file_paths: vec!["src/main.rs".to_string()],
        }
    }

    #[test]
    fn normalize_file_path_filters_directory_noise() {
        let project_path = "/tmp/nippo";

        assert_eq!(normalize_file_path("", project_path), None);
        assert_eq!(normalize_file_path(".", project_path), None);
        assert_eq!(normalize_file_path("..", project_path), None);
        assert_eq!(normalize_file_path("~", project_path), None);
        assert_eq!(normalize_file_path("/", project_path), None);
        assert_eq!(normalize_file_path("/tmp/nippo", project_path), None);
        assert_eq!(
            normalize_file_path("/tmp/nippo/README.md", project_path),
            Some("README.md".to_string())
        );
        assert_eq!(
            normalize_file_path("reports/nippo-2026-04-14.md", project_path),
            Some("reports/nippo-2026-04-14.md".to_string())
        );
    }

    #[test]
    fn summarize_session_normalizes_and_dedups_file_paths() {
        let session = RawSession {
            session_id: "thread-1".to_string(),
            project: "nippo".to_string(),
            project_path: "/tmp/nippo".to_string(),
            git_branch: Some("main".to_string()),
            user_entries: vec![ParsedUserEntry {
                timestamp: "2026-04-14T05:26:39Z".to_string(),
                text: "prompt".to_string(),
            }],
            assistant_entries: vec![
                ParsedAssistantEntry {
                    timestamp: "2026-04-14T05:26:40Z".to_string(),
                    message_count: 1,
                    tool_uses: vec!["exec_command".to_string()],
                    input_tokens: 10,
                    output_tokens: 3,
                    file_paths: vec![
                        ".".to_string(),
                        "/tmp/nippo".to_string(),
                        "/tmp/nippo/crates/collector/src/main.rs".to_string(),
                        "crates/collector/src/main.rs".to_string(),
                        "/tmp/nippo/README.md".to_string(),
                    ],
                },
                ParsedAssistantEntry {
                    timestamp: "2026-04-14T05:26:41Z".to_string(),
                    message_count: 0,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: 0,
                    file_paths: vec!["/tmp/nippo/crates/collector/src/main.rs".to_string()],
                },
            ],
        };

        let summary = summarize_session(&session);

        assert_eq!(summary.message_counts.assistant, 1);
        assert_eq!(
            summary.files_touched,
            vec!["README.md", "crates/collector/src/main.rs"]
        );
        assert_eq!(summary.tool_usage.get("exec_command"), Some(&1));
        assert_eq!(summary.total_input_tokens, 10);
        assert_eq!(summary.total_output_tokens, 3);
    }

    #[test]
    fn merges_split_records_with_the_same_session_id() {
        let sessions = vec![
            raw_session(
                "session-1",
                "nippo",
                vec![user_entry("2026-08-02T10:00:00Z", "最初の指示")],
                vec![assistant_entry("2026-08-02T10:03:00Z", "Read")],
            ),
            raw_session(
                "session-1",
                "nippo",
                vec![
                    user_entry("2026-08-02T10:00:00Z", "最初の指示"),
                    user_entry("2026-08-02T10:02:00Z", "次の指示"),
                ],
                vec![assistant_entry("2026-08-02T10:03:00Z", "Read")],
            ),
        ];

        let merged = merge_sessions_by_id(sessions);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].user_entries.len(), 2);
        assert_eq!(merged[0].assistant_entries.len(), 1);
        let summary = summarize_session(&merged[0]);
        assert_eq!(
            summary.time_range.start.as_deref(),
            Some("2026-08-02T10:00:00Z")
        );
        assert_eq!(
            summary.time_range.end.as_deref(),
            Some("2026-08-02T10:03:00Z")
        );
        assert_eq!(summary.tool_usage.get("Read"), Some(&1));
        assert_eq!(summary.total_input_tokens, 10);
    }

    #[test]
    fn keeps_the_latest_branch_when_merging_session_fragments() {
        let mut older = raw_session(
            "session-1",
            "nippo",
            vec![user_entry("2026-08-02T10:00:00Z", "最初の指示")],
            vec![],
        );
        older.git_branch = Some("a-old".to_string());
        let mut newer = raw_session(
            "session-1",
            "nippo",
            vec![user_entry("2026-08-02T11:00:00Z", "次の指示")],
            vec![],
        );
        newer.git_branch = Some("z-new".to_string());

        let merged = merge_sessions_by_id(vec![older, newer]);

        assert_eq!(merged[0].git_branch.as_deref(), Some("z-new"));
    }

    #[test]
    fn sorts_sessions_by_latest_entry() {
        let mut sessions = vec![
            raw_session(
                "a-old",
                "nippo",
                vec![user_entry("2026-08-02T10:00:00Z", "古い指示")],
                vec![],
            ),
            raw_session(
                "z-new",
                "nippo",
                vec![user_entry("2026-08-02T11:00:00Z", "新しい指示")],
                vec![],
            ),
        ];

        sort_sessions_by_recency(&mut sessions);

        assert_eq!(sessions[0].session_id, "z-new");
    }

    #[test]
    fn identifies_only_deterministic_prompt_noise() {
        for text in [
            "<command-name>/loop</command-name>",
            "<task-notification>agent finished</task-notification>",
            "<local-command-caveat>generated</local-command-caveat>",
            "<local-command-stdout>done</local-command-stdout>",
            "<bash-input>cargo test</bash-input>",
            "<bash-stdout>ok</bash-stdout>",
            "[Image #1]",
            "[Image: source: screenshot.png]",
            "[Request interrupted by user]",
            "This session is being continued from a previous conversation that ran out of context.",
            "Base directory for this skill: /tmp/example",
            "(Re-invocation of /nippo)",
            "y",
            "はい",
            "OK",
            "done",
        ] {
            assert!(!is_meaningful_prompt(text), "expected noise: {text}");
        }

        for text in [
            "画像の余白を狭くしてください",
            "done の状態を一覧に表示する",
            "Rust にする",
        ] {
            assert!(
                is_meaningful_prompt(text),
                "expected meaningful prompt: {text}"
            );
        }
    }

    #[test]
    fn removes_noise_only_when_meaningful_filter_is_applied() {
        let mut sessions = vec![
            raw_session(
                "meaningful",
                "nippo",
                vec![
                    user_entry("2026-08-02T10:00:00Z", "<command-name>/loop</command-name>"),
                    user_entry("2026-08-02T10:01:00Z", "Rust にする"),
                ],
                vec![assistant_entry("2026-08-02T10:02:00Z", "Bash")],
            ),
            raw_session(
                "scheduled-work",
                "nippo",
                vec![user_entry(
                    "2026-08-02T11:00:00Z",
                    "<task-notification>scheduled</task-notification>",
                )],
                vec![assistant_entry("2026-08-02T11:01:00Z", "Bash")],
            ),
            raw_session(
                "noise-only",
                "nippo",
                vec![user_entry("2026-08-02T12:00:00Z", "OK")],
                vec![],
            ),
        ];

        retain_meaningful_prompts(&mut sessions);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "Rust にする");
        assert_eq!(sessions[0].assistant_entries.len(), 1);
    }
}
