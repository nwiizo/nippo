use std::collections::HashMap;

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
    pub tool_uses: Vec<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub file_paths: Vec<String>,
}

/// RawSession から出力用の SessionSummary を構築する
pub fn summarize_session(session: &RawSession) -> SessionSummary {
    let mut tool_usage: HashMap<String, u32> = HashMap::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut all_file_paths: Vec<String> = Vec::new();

    for entry in &session.assistant_entries {
        for tool in &entry.tool_uses {
            *tool_usage.entry(tool.clone()).or_insert(0) += 1;
        }
        total_input_tokens += entry.input_tokens;
        total_output_tokens += entry.output_tokens;
        all_file_paths.extend(entry.file_paths.iter().cloned());
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
            assistant: session.assistant_entries.len(),
        },
        total_input_tokens,
        total_output_tokens,
        files_touched: all_file_paths,
    }
}
