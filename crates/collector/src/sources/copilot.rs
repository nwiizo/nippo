//! GitHub Copilot CLI session collector.
//!
//! `~/.copilot/session-state/<session-id>/events.jsonl` is the resumable session
//! record. Persisted user, assistant, and tool events are normalized into the
//! source-independent `RawSession` representation used by nippo.

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::filter::DateFilter;
use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession};

const MAX_PROMPT_LEN: usize = 500;

#[derive(Debug)]
pub struct SessionFile {
    pub path: PathBuf,
    pub mtime: SystemTime,
}

#[derive(Default)]
struct SessionMetadata {
    session_id: String,
    cwd: String,
    git_branch: Option<String>,
    repository: Option<String>,
}

pub fn discover_session_files(copilot_dir: &Path) -> Result<Vec<SessionFile>> {
    let session_state_dir = copilot_dir.join("session-state");
    if !session_state_dir.is_dir() {
        anyhow::bail!(
            "GitHub Copilot CLI の履歴データが見つかりません: {}\n\n\
             Copilot CLI のセッションは session-state/<session-id>/events.jsonl に保存されます。\n\
             カスタムディレクトリを指定する場合は --copilot-dir オプションを使用してください。",
            session_state_dir.display()
        );
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(&session_state_dir)
        .with_context(|| format!("Failed to read {}", session_state_dir.display()))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path().join("events.jsonl");
        let metadata = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => continue,
        };
        files.push(SessionFile {
            path,
            mtime: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max).collect::<String>())
    }
}

fn extract_project(metadata: &SessionMetadata) -> String {
    if !metadata.cwd.is_empty() {
        return Path::new(&metadata.cwd)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| metadata.cwd.clone());
    }
    metadata
        .repository
        .as_deref()
        .and_then(|repository| repository.rsplit('/').next())
        .filter(|project| !project.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn yaml_string(value: &serde_yaml::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for key in keys {
                if let Some(value) = mapping.get(serde_yaml::Value::String((*key).to_string()))
                    && let Some(value) = value.as_str()
                    && !value.is_empty()
                {
                    return Some(value.to_string());
                }
            }
            mapping.values().find_map(|value| yaml_string(value, keys))
        }
        serde_yaml::Value::Sequence(values) => {
            values.iter().find_map(|value| yaml_string(value, keys))
        }
        _ => None,
    }
}

fn load_workspace_metadata(session_dir: &Path) -> SessionMetadata {
    let path = session_dir.join("workspace.yaml");
    let Ok(contents) = fs::read_to_string(path) else {
        return SessionMetadata::default();
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&contents) else {
        return SessionMetadata::default();
    };

    SessionMetadata {
        cwd: yaml_string(&value, &["cwd", "workingDirectory", "gitRoot"]).unwrap_or_default(),
        git_branch: yaml_string(&value, &["branch", "gitBranch"]),
        repository: yaml_string(&value, &["repository"]),
        ..SessionMetadata::default()
    }
}

fn update_context(metadata: &mut SessionMetadata, context: &Value) {
    if let Some(cwd) = context
        .get("cwd")
        .or_else(|| context.get("gitRoot"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.cwd = cwd.to_string();
    }
    if let Some(branch) = context
        .get("branch")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.git_branch = Some(branch.to_string());
    }
    if let Some(repository) = context
        .get("repository")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        metadata.repository = Some(repository.to_string());
    }
}

fn extract_argument_paths(arguments: &Value) -> Vec<String> {
    const PATH_KEYS: &[&str] = &["fileName", "file_path", "path", "paths", "possiblePaths"];

    fn visit(value: &Value, current_key: Option<&str>, paths: &mut Vec<String>) {
        match value {
            Value::String(value)
                if current_key.is_some_and(|key| PATH_KEYS.contains(&key)) && !value.is_empty() =>
            {
                paths.push(value.clone());
            }
            Value::Array(values) if current_key.is_some_and(|key| PATH_KEYS.contains(&key)) => {
                for value in values {
                    if let Some(value) = value.as_str()
                        && !value.is_empty()
                    {
                        paths.push(value.to_string());
                    }
                }
            }
            Value::Object(object) => {
                for (key, value) in object {
                    visit(value, Some(key), paths);
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, current_key, paths);
                }
            }
            _ => {}
        }
    }

    let mut paths = Vec::new();
    visit(arguments, None, &mut paths);
    paths.sort();
    paths.dedup();
    paths
}

pub fn parse_session_file(path: &Path, filter: &DateFilter) -> Result<Option<RawSession>> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let session_dir = path.parent().unwrap_or_else(|| Path::new(""));
    let mut metadata = load_workspace_metadata(session_dir);
    metadata.session_id = session_dir
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut user_entries = Vec::new();
    let mut assistant_entries = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            _ => continue,
        };
        let event: Value = match serde_json::from_str(&line) {
            Ok(event) => event,
            Err(_) => continue,
        };
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
        let data = event.get("data").unwrap_or(&Value::Null);

        // Sub-agent events share the parent stream. Like Claude sidechains,
        // they are excluded to avoid double-counting delegated work or letting
        // a sub-agent context overwrite the parent session metadata.
        if event.get("agentId").and_then(Value::as_str).is_some() {
            continue;
        }

        match event_type {
            "session.start" => {
                if let Some(session_id) = data
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    metadata.session_id = session_id.to_string();
                }
                if let Some(context) = data.get("context") {
                    update_context(&mut metadata, context);
                }
            }
            "session.context_changed" => update_context(&mut metadata, data),
            _ => {}
        }

        let Some(timestamp) = event.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        if !filter.matches(timestamp) {
            continue;
        }

        match event_type {
            "user.message" => {
                if let Some(content) = data
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|content| !content.is_empty())
                {
                    user_entries.push(ParsedUserEntry {
                        timestamp: timestamp.to_string(),
                        text: truncate(content, MAX_PROMPT_LEN),
                    });
                }
            }
            "assistant.message" => {
                assistant_entries.push(ParsedAssistantEntry {
                    timestamp: timestamp.to_string(),
                    message_count: 1,
                    tool_uses: Vec::new(),
                    input_tokens: 0,
                    output_tokens: data
                        .get("outputTokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    file_paths: Vec::new(),
                });
            }
            "assistant.usage" => {
                let input_tokens = data.get("inputTokens").and_then(Value::as_u64).unwrap_or(0);
                if input_tokens > 0 {
                    assistant_entries.push(ParsedAssistantEntry {
                        timestamp: timestamp.to_string(),
                        message_count: 0,
                        tool_uses: Vec::new(),
                        input_tokens,
                        output_tokens: 0,
                        file_paths: Vec::new(),
                    });
                }
            }
            "tool.execution_start" => {
                if let Some(tool_name) = data
                    .get("toolName")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    assistant_entries.push(ParsedAssistantEntry {
                        timestamp: timestamp.to_string(),
                        message_count: 0,
                        tool_uses: vec![tool_name.to_string()],
                        input_tokens: 0,
                        output_tokens: 0,
                        file_paths: data
                            .get("arguments")
                            .map(extract_argument_paths)
                            .unwrap_or_default(),
                    });
                }
            }
            _ => {}
        }
    }

    if user_entries.is_empty() && assistant_entries.is_empty() {
        return Ok(None);
    }

    user_entries.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
    assistant_entries.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
    let project = extract_project(&metadata);

    Ok(Some(RawSession {
        session_id: metadata.session_id,
        project,
        project_path: metadata.cwd,
        git_branch: metadata.git_branch,
        user_entries,
        assistant_entries,
    }))
}

pub fn collect_sessions(copilot_dir: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let files = discover_session_files(copilot_dir)?;
    let cutoff = filter.mtime_cutoff();
    let candidates: Vec<&SessionFile> = files
        .iter()
        .filter(|file| cutoff.map(|cutoff| file.mtime >= cutoff).unwrap_or(true))
        .collect();

    Ok(candidates
        .par_iter()
        .filter_map(|file| parse_session_file(&file.path, filter).ok().flatten())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::summarize_session;
    use tempfile::tempdir;

    fn all_time_filter() -> DateFilter {
        DateFilter::from_days(0)
    }

    #[test]
    fn collects_copilot_events_and_workspace_metadata() -> Result<()> {
        let dir = tempdir().context("tempdir")?;
        let session_dir = dir.path().join("session-state/session-1");
        fs::create_dir_all(&session_dir)?;
        fs::write(
            session_dir.join("workspace.yaml"),
            "workspace:\n  cwd: /tmp/nippo\n  branch: feat/copilot\n",
        )?;
        fs::write(
            session_dir.join("events.jsonl"),
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-08-24T01:00:00Z\",\"data\":{\"sessionId\":\"session-1\",\"context\":{\"cwd\":\"/tmp/nippo\",\"branch\":\"feat/copilot\"}}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-08-24T01:01:00Z\",\"data\":{\"content\":\"Add Copilot support\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-08-24T01:02:00Z\",\"data\":{\"content\":\"Working on it\",\"outputTokens\":12}}\n",
                "{\"type\":\"assistant.usage\",\"timestamp\":\"2026-08-24T01:02:01Z\",\"data\":{\"inputTokens\":30,\"outputTokens\":12}}\n",
                "{\"type\":\"tool.execution_start\",\"timestamp\":\"2026-08-24T01:03:00Z\",\"data\":{\"toolName\":\"edit\",\"arguments\":{\"fileName\":\"/tmp/nippo/src/main.rs\"}}}\n",
                "{\"type\":\"session.context_changed\",\"timestamp\":\"2026-08-24T01:03:30Z\",\"agentId\":\"subagent-1\",\"data\":{\"cwd\":\"/tmp/wrong-subagent-project\",\"branch\":\"feat/subagent\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-08-24T01:04:00Z\",\"agentId\":\"subagent-1\",\"data\":{\"content\":\"delegated\",\"outputTokens\":99}}\n"
            ),
        )?;

        let sessions = collect_sessions(dir.path(), &all_time_filter())?;
        assert_eq!(sessions.len(), 1);
        let summary = summarize_session(&sessions[0]);
        assert_eq!(summary.session_id, "session-1");
        assert_eq!(summary.project, "nippo");
        assert_eq!(summary.project_path, "/tmp/nippo");
        assert_eq!(summary.git_branch.as_deref(), Some("feat/copilot"));
        assert_eq!(summary.message_counts.user, 1);
        assert_eq!(summary.message_counts.assistant, 1);
        assert_eq!(summary.tool_usage.get("edit"), Some(&1));
        assert_eq!(summary.total_input_tokens, 30);
        assert_eq!(summary.total_output_tokens, 12);
        assert_eq!(summary.files_touched, vec!["src/main.rs"]);
        Ok(())
    }

    #[test]
    fn filters_events_by_local_date_range() -> Result<()> {
        let dir = tempdir().context("tempdir")?;
        let session_dir = dir.path().join("session-state/session-2");
        fs::create_dir_all(&session_dir)?;
        fs::write(
            session_dir.join("events.jsonl"),
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-08-20T00:00:00Z\",\"data\":{\"context\":{\"repository\":\"owner/project\"}}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-08-20T00:01:00Z\",\"data\":{\"content\":\"old\"}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-08-24T00:01:00Z\",\"data\":{\"content\":\"current\"}}\n"
            ),
        )?;

        let filter = DateFilter::from_range(Some("2026-08-24"), Some("2026-08-24"))?;
        let sessions = collect_sessions(dir.path(), &filter)?;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project, "project");
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "current");
        Ok(())
    }

    #[test]
    fn discovery_requires_session_state_directory() {
        let dir = tempdir().expect("tempdir");
        let error = discover_session_files(dir.path()).expect_err("missing data must fail");
        assert!(error.to_string().contains("session-state"));
    }
}
