use std::collections::HashMap;
use std::path::Path;

use crate::output::{DateRange, MessageCounts, PromptSummary, SessionSummary};

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

#[derive(Clone, Debug)]
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
}
