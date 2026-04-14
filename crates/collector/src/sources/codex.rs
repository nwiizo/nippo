//! Codex 履歴のコレクター。
//!
//! `~/.codex/history.jsonl` の user prompt と
//! `~/.codex/state_5.sqlite` の thread メタデータを結合して、
//! 日報生成に必要なセッション一覧へ変換する。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::filter::DateFilter;
use crate::session::{ParsedUserEntry, RawSession};

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
}

pub fn discover_history_files(codex_dir: &Path) -> Result<Vec<PathBuf>> {
    let history_path = codex_dir.join("history.jsonl");
    if !history_path.exists() {
        anyhow::bail!(
            "Codex の履歴データが見つかりません: {}\n\n\
             Codex を使用すると、user prompt 履歴は history.jsonl に保存されます。\n\
             カスタムディレクトリを指定する場合は --codex-dir オプションを使用してください。",
            history_path.display()
        );
    }

    let mut files = vec![history_path];
    let state_path = codex_dir.join("state_5.sqlite");
    if state_path.exists() {
        files.push(state_path);
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
    let mut stmt = conn
        .prepare("SELECT id, cwd, git_branch FROM threads")
        .context("Failed to prepare Codex thread metadata query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ThreadMeta {
                    cwd: row.get::<_, String>(1)?,
                    git_branch: row.get::<_, Option<String>>(2)?,
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

fn unix_seconds_to_rfc3339(timestamp: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(timestamp, 0).map(|dt| dt.to_rfc3339())
}

pub fn collect_sessions(codex_dir: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let history_path = codex_dir.join("history.jsonl");
    let state_path = codex_dir.join("state_5.sqlite");
    let thread_metadata = load_thread_metadata(&state_path)?;

    let file = File::open(&history_path)
        .with_context(|| format!("Failed to open {}", history_path.display()))?;
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
                text: truncate(trimmed, MAX_PROMPT_LEN),
            });
    }

    let mut sessions: Vec<RawSession> = grouped_entries
        .into_iter()
        .map(|(session_id, mut user_entries)| {
            user_entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

            let meta = thread_metadata.get(&session_id);
            let project_path = meta.map(|value| value.cwd.clone()).unwrap_or_default();
            let project = if project_path.is_empty() {
                "unknown".to_string()
            } else {
                extract_project_from_cwd(&project_path)
            };

            RawSession {
                session_id,
                project,
                project_path,
                git_branch: meta.and_then(|value| value.git_branch.clone()),
                user_entries,
                assistant_entries: Vec::new(),
            }
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

        let conn = Connection::open(dir.path().join("state_5.sqlite")).expect("open sqlite");
        conn.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, git_branch TEXT)",
            [],
        )
        .expect("create table");
        conn.execute(
            "INSERT INTO threads (id, cwd, git_branch) VALUES ('thread-1', '/tmp/nippo', 'main')",
            [],
        )
        .expect("insert thread 1");
        conn.execute(
            "INSERT INTO threads (id, cwd, git_branch) VALUES ('thread-2', '/tmp/other', 'feat/test')",
            [],
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
    }
}
