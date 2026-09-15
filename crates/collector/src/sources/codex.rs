//! Codex 履歴のコレクター。
//!
//! `~/.codex/history.jsonl` と rollout JSONL の user prompt を
//! `~/.codex/state_5.sqlite` の thread メタデータと結合して、
//! 日報生成に必要なセッション一覧へ変換する。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::filter::DateFilter;
use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession, deduplicate_user_entries};

const MAX_PROMPT_LEN: usize = 500;

#[derive(Deserialize)]
struct HistoryEntry {
    session_id: String,
    ts: i64,
    text: String,
}

struct ThreadMeta {
    cwd: String,
    git_branch: Option<String>,
    rollout_path: Option<PathBuf>,
}

pub fn discover_history_files(codex_dir: &Path) -> Result<Vec<PathBuf>> {
    let files: Vec<PathBuf> = ["history.jsonl", "state_5.sqlite"]
        .into_iter()
        .map(|name| codex_dir.join(name))
        .filter(|path| path.exists())
        .collect();
    if files.is_empty() {
        anyhow::bail!(
            "Codex の履歴データが見つかりません: {}\n\n\
             history.jsonl または state_5.sqlite が必要です。\n\
             カスタムディレクトリを指定する場合は --codex-dir オプションを使用してください。",
            codex_dir.display()
        );
    }

    Ok(files)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}...")
    }
}

fn extract_project_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| cwd.to_string())
}

fn load_thread_metadata(state_path: &Path) -> Result<HashMap<String, ThreadMeta>> {
    if !state_path.exists() {
        return Ok(HashMap::new());
    }

    let conn = Connection::open(state_path)
        .with_context(|| format!("Failed to open {}", state_path.display()))?;
    let columns = thread_columns(&conn)?;
    let rollout_column = if columns.contains("rollout_path") {
        "rollout_path"
    } else {
        "NULL AS rollout_path"
    };
    let query = format!("SELECT id, cwd, git_branch, {rollout_column} FROM threads");
    let mut stmt = conn
        .prepare(&query)
        .context("Failed to prepare Codex thread metadata query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ThreadMeta {
                    cwd: row.get::<_, String>(1)?,
                    git_branch: row.get::<_, Option<String>>(2)?,
                    rollout_path: row
                        .get::<_, Option<String>>(3)?
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from),
                },
            ))
        })
        .context("Failed to read Codex thread metadata")?;

    let mut metadata = HashMap::new();
    for row in rows {
        let (id, meta) = row.context("Failed to decode Codex thread metadata row")?;
        metadata.insert(id, meta);
    }

    Ok(metadata)
}

fn thread_columns(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(threads)")
        .context("Failed to inspect Codex thread metadata schema")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .context("Failed to read Codex thread metadata schema")?;

    let mut columns = HashSet::new();
    for row in rows {
        columns.insert(row.context("Failed to decode Codex thread metadata schema row")?);
    }

    Ok(columns)
}

fn unix_seconds_to_rfc3339(timestamp: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(timestamp, 0).map(|dt| dt.to_rfc3339())
}

fn collect_rollout_entries(
    rollout_path: &Path,
    filter: &DateFilter,
) -> (Vec<ParsedUserEntry>, Vec<ParsedAssistantEntry>) {
    let file = match File::open(rollout_path) {
        Ok(file) => file,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    let reader = BufReader::new(file);
    let mut user_entries = Vec::new();
    let mut assistant_entries = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(value) => value,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
            continue;
        };
        if !filter.matches(timestamp) {
            continue;
        }

        let Some(item_type) = value.get("type").and_then(Value::as_str) else {
            continue;
        };
        if !matches!(item_type, "response_item" | "event_msg") {
            continue;
        }
        let Some(payload) = value.get("payload") else {
            continue;
        };
        if item_type == "response_item"
            && let Some(entry) = extract_rollout_user_entry(timestamp, payload)
        {
            user_entries.push(entry);
        }
        if let Some(entry) = extract_rollout_entry(timestamp, item_type, payload) {
            assistant_entries.push(entry);
        }
    }

    (user_entries, assistant_entries)
}

fn extract_rollout_user_entry(timestamp: &str, payload: &Value) -> Option<ParsedUserEntry> {
    if payload.get("type").and_then(Value::as_str) != Some("message")
        || payload.get("role").and_then(Value::as_str) != Some("user")
    {
        return None;
    }
    let text = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Use history's second precision and UTC spelling for user prompts.
    let timestamp = DateTime::parse_from_rfc3339(timestamp).ok()?.timestamp();
    Some(ParsedUserEntry {
        timestamp: unix_seconds_to_rfc3339(timestamp)?,
        text: trimmed.to_string(),
    })
}

fn empty_assistant_entry(timestamp: &str) -> ParsedAssistantEntry {
    ParsedAssistantEntry {
        timestamp: timestamp.to_string(),
        message_count: 0,
        tool_uses: Vec::new(),
        input_tokens: 0,
        output_tokens: 0,
        file_paths: Vec::new(),
    }
}

fn extract_rollout_entry(
    timestamp: &str,
    item_type: &str,
    payload: &Value,
) -> Option<ParsedAssistantEntry> {
    match (item_type, payload.get("type").and_then(Value::as_str)) {
        // response_item payloads
        ("response_item", Some("message"))
            if payload.get("role").and_then(Value::as_str) == Some("assistant") =>
        {
            Some(ParsedAssistantEntry {
                message_count: 1,
                ..empty_assistant_entry(timestamp)
            })
        }
        ("response_item", Some("function_call" | "custom_tool_call")) => payload
            .get("name")
            .and_then(Value::as_str)
            .map(|name| ParsedAssistantEntry {
                tool_uses: vec![name.to_string()],
                ..empty_assistant_entry(timestamp)
            }),
        ("response_item", Some("web_search_call")) => Some(ParsedAssistantEntry {
            tool_uses: vec!["web_search".to_string()],
            ..empty_assistant_entry(timestamp)
        }),
        // event_msg payloads
        ("event_msg", Some("token_count")) => payload
            .get("info")
            .and_then(|value| value.get("last_token_usage"))
            .and_then(Value::as_object)
            .and_then(|usage| {
                let input_tokens = usage
                    .get("input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let output_tokens = usage
                    .get("output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                (input_tokens != 0 || output_tokens != 0).then(|| ParsedAssistantEntry {
                    input_tokens,
                    output_tokens,
                    ..empty_assistant_entry(timestamp)
                })
            }),
        ("event_msg", Some("exec_command_end")) => {
            let file_paths = payload
                .get("parsed_cmd")
                .and_then(Value::as_array)
                .map(|commands| extract_paths_from_commands(commands))
                .unwrap_or_default();
            (!file_paths.is_empty()).then(|| ParsedAssistantEntry {
                file_paths,
                ..empty_assistant_entry(timestamp)
            })
        }
        ("event_msg", Some("patch_apply_end")) => {
            let file_paths = payload
                .get("changes")
                .and_then(Value::as_object)
                .map(|changes| {
                    let mut paths: Vec<String> = changes.keys().cloned().collect();
                    paths.sort();
                    paths.dedup();
                    paths
                })
                .unwrap_or_default();
            (!file_paths.is_empty()).then(|| ParsedAssistantEntry {
                file_paths,
                ..empty_assistant_entry(timestamp)
            })
        }
        _ => None,
    }
}

fn extract_paths_from_commands(commands: &[Value]) -> Vec<String> {
    let mut paths = Vec::new();

    for command in commands {
        let command_type = command.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(command_type, "read" | "write") {
            continue;
        }
        if let Some(path) = command.get("path").and_then(Value::as_str)
            && !path.is_empty()
        {
            paths.push(path.to_string());
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

fn collect_history_entries(
    history_path: &Path,
    filter: &DateFilter,
) -> Result<HashMap<String, Vec<ParsedUserEntry>>> {
    let file = match File::open(history_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to open {}", history_path.display()));
        }
    };
    let reader = BufReader::new(file);

    let mut grouped_entries: HashMap<String, Vec<ParsedUserEntry>> = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(value) => value,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        let entry: HistoryEntry = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let trimmed = entry.text.trim();
        if trimmed.is_empty() || !filter.matches_unix_seconds(entry.ts) {
            continue;
        }

        let Some(timestamp) = unix_seconds_to_rfc3339(entry.ts) else {
            continue;
        };

        grouped_entries
            .entry(entry.session_id)
            .or_default()
            .push(ParsedUserEntry {
                timestamp,
                text: trimmed.to_string(),
            });
    }

    Ok(grouped_entries)
}

fn merge_rollout_user_entries(
    history_entries: &mut Vec<ParsedUserEntry>,
    mut rollout_entries: Vec<ParsedUserEntry>,
) {
    deduplicate_user_entries(history_entries);
    deduplicate_user_entries(&mut rollout_entries);
    // nwiizo-coding-style: history and rollout have no shared message ID, so pair
    // identical text one-to-one by the nearest timestamp; use IDs if available later.
    for history_entry in history_entries.iter() {
        let Ok(history_time) = DateTime::parse_from_rfc3339(&history_entry.timestamp) else {
            continue;
        };
        let closest = rollout_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.text == history_entry.text)
            .filter_map(|(index, entry)| {
                let time = DateTime::parse_from_rfc3339(&entry.timestamp).ok()?;
                Some((time.timestamp().abs_diff(history_time.timestamp()), index))
            })
            .min();
        if let Some((_, index)) = closest {
            rollout_entries.remove(index);
        }
    }
    history_entries.extend(rollout_entries);
}

pub fn collect_sessions(codex_dir: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let thread_metadata = load_thread_metadata(&codex_dir.join("state_5.sqlite"))?;
    let mut grouped_entries = collect_history_entries(&codex_dir.join("history.jsonl"), filter)?;
    for session_id in thread_metadata.keys() {
        grouped_entries.entry(session_id.clone()).or_default();
    }

    let mut sessions: Vec<RawSession> = grouped_entries
        .into_iter()
        .filter_map(|(session_id, mut user_entries)| {
            let meta = thread_metadata.get(&session_id);
            let project_path = meta.map(|value| value.cwd.clone()).unwrap_or_default();
            let (rollout_user_entries, assistant_entries) = meta
                .and_then(|value| value.rollout_path.as_deref())
                .map(|path| collect_rollout_entries(path, filter))
                .unwrap_or_default();
            merge_rollout_user_entries(&mut user_entries, rollout_user_entries);
            if user_entries.is_empty() {
                return None;
            }
            for entry in &mut user_entries {
                entry.text = truncate(&entry.text, MAX_PROMPT_LEN);
            }
            user_entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            let project = if project_path.is_empty() {
                "unknown".to_string()
            } else {
                extract_project_from_cwd(&project_path)
            };

            Some(RawSession {
                session_id,
                project,
                project_path,
                git_branch: meta.and_then(|value| value.git_branch.clone()),
                user_entries,
                assistant_entries,
            })
        })
        .collect();

    sessions.sort_by(|a, b| {
        let left = a
            .user_entries
            .last()
            .map(|entry| entry.timestamp.as_str())
            .unwrap_or_default();
        let right = b
            .user_entries
            .last()
            .map(|entry| entry.timestamp.as_str())
            .unwrap_or_default();
        right.cmp(left)
    });

    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::summarize_session;
    use chrono::Duration;
    use rusqlite::Connection;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn collects_codex_history_with_thread_metadata() {
        let dir = tempdir().expect("tempdir");
        let history_path = dir.path().join("history.jsonl");
        fs::write(
            &history_path,
            concat!(
                "{\"session_id\":\"thread-1\",\"ts\":1776144399,\"text\":\"first prompt\"}\n",
                "{\"session_id\":\"thread-1\",\"ts\":1776144499,\"text\":\"second prompt\"}\n",
                "{\"session_id\":\"thread-2\",\"ts\":1776144599,\"text\":\"other project\"}\n"
            ),
        )
        .expect("write history");
        let rollout_path = dir.path().join("rollout-thread-1.jsonl");
        fs::write(
            &rollout_path,
            concat!(
                "{\"timestamp\":\"2026-04-14T05:26:40Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"working\"}]}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:41Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"arguments\":\"{}\",\"call_id\":\"call-1\"}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:42Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"exec_command_end\",\"call_id\":\"call-1\",\"parsed_cmd\":[{\"type\":\"read\",\"path\":\"crates/collector/src/main.rs\"}]}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:43Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call\",\"name\":\"apply_patch\",\"status\":\"completed\",\"call_id\":\"call-2\",\"input\":\"*** Begin Patch\"}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:44Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"patch_apply_end\",\"call_id\":\"call-2\",\"success\":true,\"changes\":{\"/tmp/nippo/crates/collector/src/main.rs\":{\"type\":\"update\"}}}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:45Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":11,\"output_tokens\":7}}}}\n",
                "{\"timestamp\":\"2026-04-14T05:26:46Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"web_search_call\",\"status\":\"completed\",\"action\":{\"type\":\"open_page\",\"url\":\"https://example.com\"}}}\n"
            ),
        )
        .expect("write rollout");

        let conn = Connection::open(dir.path().join("state_5.sqlite")).expect("open sqlite");
        conn.execute(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                cwd TEXT NOT NULL,
                git_branch TEXT,
                rollout_path TEXT
            )",
            [],
        )
        .expect("create table");
        conn.execute(
            "INSERT INTO threads (id, cwd, git_branch, rollout_path)
             VALUES (?1, ?2, ?3, ?4)",
            (
                "thread-1",
                "/tmp/nippo",
                "main",
                rollout_path.to_string_lossy().as_ref(),
            ),
        )
        .expect("insert thread 1");
        conn.execute(
            "INSERT INTO threads (id, cwd, git_branch, rollout_path)
             VALUES (?1, ?2, ?3, NULL)",
            ("thread-2", "/tmp/other", "feat/test"),
        )
        .expect("insert thread 2");

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect sessions");

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "thread-2");
        assert_eq!(sessions[0].project, "other");
        assert_eq!(sessions[1].session_id, "thread-1");
        assert_eq!(sessions[1].project, "nippo");
        assert_eq!(sessions[1].git_branch.as_deref(), Some("main"));
        assert_eq!(sessions[1].user_entries.len(), 2);
        assert_eq!(sessions[1].user_entries[0].text, "first prompt");
        assert_eq!(sessions[1].user_entries[1].text, "second prompt");

        let summary = summarize_session(&sessions[1]);
        assert_eq!(summary.message_counts.assistant, 1);
        assert_eq!(summary.tool_usage.get("exec_command"), Some(&1));
        assert_eq!(summary.tool_usage.get("apply_patch"), Some(&1));
        assert_eq!(summary.tool_usage.get("web_search"), Some(&1));
        assert_eq!(summary.total_input_tokens, 11);
        assert_eq!(summary.total_output_tokens, 7);
        assert_eq!(summary.files_touched, vec!["crates/collector/src/main.rs"]);
    }

    #[test]
    fn collects_codex_history_using_local_day_bounds() {
        let dir = tempdir().expect("tempdir");
        let filter = DateFilter::from_days(1);
        let cutoff = filter.mtime_cutoff().expect("cutoff");
        let cutoff_utc = DateTime::<Utc>::from(cutoff);
        let inside = cutoff_utc.to_rfc3339();
        let outside = (cutoff_utc - Duration::seconds(1)).to_rfc3339();

        fs::write(
            dir.path().join("history.jsonl"),
            format!(
                concat!(
                    "{{\"session_id\":\"thread-1\",\"ts\":{},\"text\":\"too old\"}}\n",
                    "{{\"session_id\":\"thread-1\",\"ts\":{},\"text\":\"kept\"}}\n"
                ),
                cutoff_utc.timestamp() - 1,
                cutoff_utc.timestamp()
            ),
        )
        .expect("write history");
        let rollout_path = dir.path().join("rollout-thread-1.jsonl");
        fs::write(
            &rollout_path,
            format!(
                concat!(
                    "{{\"timestamp\":\"{}\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"ignored\"}}]}}}}\n",
                    "{{\"timestamp\":\"{}\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"kept\"}}]}}}}\n",
                    "{{\"timestamp\":\"{}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"exec_command_end\",\"call_id\":\"call-1\",\"parsed_cmd\":[{{\"type\":\"read\",\"path\":\".\"}},{{\"type\":\"read\",\"path\":\"/tmp/nippo/crates/collector/src/main.rs\"}},{{\"type\":\"write\",\"path\":\"crates/collector/src/main.rs\"}},{{\"type\":\"write\",\"path\":\"/tmp/nippo\"}}]}}}}\n"
                ),
                outside, inside, inside
            ),
        )
        .expect("write rollout");

        let conn = Connection::open(dir.path().join("state_5.sqlite")).expect("open sqlite");
        conn.execute(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                cwd TEXT NOT NULL,
                git_branch TEXT,
                rollout_path TEXT
            )",
            [],
        )
        .expect("create table");
        conn.execute(
            "INSERT INTO threads (id, cwd, git_branch, rollout_path)
             VALUES (?1, ?2, ?3, ?4)",
            (
                "thread-1",
                "/tmp/nippo",
                "main",
                rollout_path.to_string_lossy().as_ref(),
            ),
        )
        .expect("insert thread");

        let sessions = collect_sessions(dir.path(), &filter).expect("collect sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "kept");

        let summary = summarize_session(&sessions[0]);
        assert_eq!(summary.message_counts.assistant, 1);
        assert_eq!(summary.files_touched, vec!["crates/collector/src/main.rs"]);
    }
}
