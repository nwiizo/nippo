//! opencode 履歴のコレクター。
//!
//! `~/.local/share/opencode/opencode.db` の SQLite (`session` / `message` /
//! `part` / `project` テーブル) から、日報生成に必要なセッション一覧へ変換する。
//! opencode は生の JSON を part.data に格納するため、tool 使用と file path は
//! `type=tool` の part を解釈して抽出する。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::filter::DateFilter;
use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession};

use super::{extract_project_from_cwd, truncate_prompt};

/// opencode が使う可能性のある DB ファイル名を優先度順に並べる。
/// prod は `opencode.db`、dev build は `opencode-dev.db`。
const DB_CANDIDATES: &[&str] = &["opencode.db", "opencode-dev.db"];

pub fn discover_history_files(opencode_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = DB_CANDIDATES
        .iter()
        .map(|name| opencode_dir.join(name))
        .filter(|path| path.exists())
        .collect();

    if files.is_empty() {
        anyhow::bail!(
            "opencode の履歴データが見つかりません: {}\n\n\
             opencode を使用すると、セッションは opencode.db (または opencode-dev.db)\n\
             の SQLite に保存されます。\n\
             カスタムディレクトリを指定する場合は --opencode-dir オプションを使用してください。",
            opencode_dir.display()
        );
    }

    files.sort();
    Ok(files)
}

pub fn collect_sessions(opencode_dir: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let db_paths = discover_history_files(opencode_dir)?;
    let mut sessions: Vec<RawSession> = Vec::new();
    for db_path in db_paths {
        sessions.extend(collect_from_db(&db_path, filter)?);
    }
    Ok(sessions)
}

fn collect_from_db(db_path: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("Failed to open {}", db_path.display()))?;

    let project_worktree = load_project_worktree(&conn)?;
    let session_meta = load_session_meta(&conn, &project_worktree)?;
    if session_meta.is_empty() {
        return Ok(Vec::new());
    }

    let mut sessions: HashMap<String, RawSession> = session_meta
        .into_iter()
        .map(|(id, meta)| {
            (
                id.clone(),
                RawSession {
                    session_id: id,
                    project: meta.project,
                    project_path: meta.project_path,
                    git_branch: None,
                    user_entries: Vec::new(),
                    assistant_entries: Vec::new(),
                },
            )
        })
        .collect();

    // Map message_id → session_id, role, timestamp, tokens (populated as we
    // read `message`) so we can attribute parts back to the assistant entry.
    let mut message_index: HashMap<String, MessageIndexEntry> = HashMap::new();

    {
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, time_created, data \
                 FROM message ORDER BY session_id, time_created, id",
            )
            .context("Failed to prepare opencode message query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .context("Failed to read opencode messages")?;

        for row in rows {
            let (message_id, session_id, time_created_ms, data_json) =
                row.context("Failed to decode opencode message row")?;
            if !sessions.contains_key(&session_id) {
                continue;
            }
            if !filter.matches_unix_seconds(time_created_ms / 1_000) {
                continue;
            }
            let Some(timestamp) = unix_ms_to_rfc3339(time_created_ms) else {
                continue;
            };
            let data: Value = match serde_json::from_str(&data_json) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let role = data.get("role").and_then(Value::as_str).unwrap_or("");

            match role {
                "assistant" => {
                    let (input_tokens, output_tokens) = extract_tokens(&data);
                    let session = sessions
                        .get_mut(&session_id)
                        .expect("session was validated above");
                    let entry_index = session.assistant_entries.len();
                    session.assistant_entries.push(ParsedAssistantEntry {
                        timestamp: timestamp.clone(),
                        message_count: 1,
                        tool_uses: Vec::new(),
                        input_tokens,
                        output_tokens,
                        file_paths: Vec::new(),
                    });
                    message_index.insert(
                        message_id,
                        MessageIndexEntry {
                            session_id,
                            role: MessageRole::Assistant { entry_index },
                        },
                    );
                }
                "user" => {
                    let session = sessions
                        .get_mut(&session_id)
                        .expect("session was validated above");
                    let entry_index = session.user_entries.len();
                    session.user_entries.push(ParsedUserEntry {
                        timestamp,
                        text: String::new(),
                        is_meta: false,
                    });
                    message_index.insert(
                        message_id,
                        MessageIndexEntry {
                            session_id,
                            role: MessageRole::User { entry_index },
                        },
                    );
                }
                _ => {
                    // Unknown roles (e.g. system, tool) are not counted as
                    // user turns; their parts are ignored so summary stats
                    // stay accurate.
                    continue;
                }
            }
        }
    }

    {
        let mut stmt = conn
            .prepare(
                "SELECT message_id, data \
                 FROM part ORDER BY session_id, time_created, id",
            )
            .context("Failed to prepare opencode part query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("Failed to read opencode parts")?;

        for row in rows {
            let (message_id, data_json) = row.context("Failed to decode opencode part row")?;
            let Some(index_entry) = message_index.get(&message_id) else {
                continue;
            };
            let data: Value = match serde_json::from_str(&data_json) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(session) = sessions.get_mut(&index_entry.session_id) else {
                continue;
            };
            let part_type = data.get("type").and_then(Value::as_str).unwrap_or("");

            match (&index_entry.role, part_type) {
                (MessageRole::User { entry_index }, "text") => {
                    let is_synthetic = data
                        .get("synthetic")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let is_ignored = data
                        .get("ignored")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if is_synthetic || is_ignored {
                        continue;
                    }
                    if let Some(text) = data.get("text").and_then(Value::as_str) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty()
                            && let Some(entry) = session.user_entries.get_mut(*entry_index)
                        {
                            if !entry.text.is_empty() {
                                entry.text.push('\n');
                            }
                            entry.text.push_str(trimmed);
                        }
                    }
                }
                (MessageRole::Assistant { entry_index }, "tool") => {
                    let tool_name = data
                        .get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if tool_name.is_empty() {
                        continue;
                    }
                    let file_paths = extract_file_paths(&tool_name, &data);
                    if let Some(entry) = session.assistant_entries.get_mut(*entry_index) {
                        entry.tool_uses.push(tool_name);
                        entry.file_paths.extend(file_paths);
                    }
                }
                _ => {}
            }
        }
    }

    let mut sessions: Vec<RawSession> = sessions.into_values().collect();
    for session in &mut sessions {
        session
            .user_entries
            .retain(|entry| !entry.text.trim().is_empty());
        for entry in &mut session.user_entries {
            entry.text = truncate_prompt(entry.text.trim());
        }
        session
            .user_entries
            .sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        for entry in session.assistant_entries.iter_mut() {
            entry.file_paths.sort();
            entry.file_paths.dedup();
        }
    }
    sessions.retain(|session| {
        !session.user_entries.is_empty() || !session.assistant_entries.is_empty()
    });

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

struct SessionMeta {
    project: String,
    project_path: String,
}

struct MessageIndexEntry {
    session_id: String,
    role: MessageRole,
}

enum MessageRole {
    User { entry_index: usize },
    Assistant { entry_index: usize },
}

fn load_project_worktree(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn
        .prepare("SELECT id, worktree FROM project")
        .context("Failed to prepare opencode project query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("Failed to read opencode projects")?;

    let mut map = HashMap::new();
    for row in rows {
        let (id, worktree) = row.context("Failed to decode opencode project row")?;
        map.insert(id, worktree);
    }
    Ok(map)
}

fn load_session_meta(
    conn: &Connection,
    project_worktree: &HashMap<String, String>,
) -> Result<HashMap<String, SessionMeta>> {
    let mut stmt = conn
        .prepare("SELECT id, project_id, directory FROM session")
        .context("Failed to prepare opencode session query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to read opencode sessions")?;

    let mut map = HashMap::new();
    for row in rows {
        let (id, project_id, directory) = row.context("Failed to decode opencode session row")?;
        // Prefer session.directory (per-session cwd) and fall back to the
        // project worktree for the "global" pseudo-project or when directory
        // was left empty.
        let project_path = if !directory.is_empty() && directory != "/" {
            directory
        } else {
            project_worktree
                .get(&project_id)
                .cloned()
                .unwrap_or_default()
        };
        let project = if project_path.is_empty() {
            "unknown".to_string()
        } else {
            extract_project_from_cwd(&project_path)
        };
        map.insert(
            id,
            SessionMeta {
                project,
                project_path,
            },
        );
    }
    Ok(map)
}

fn unix_ms_to_rfc3339(timestamp_ms: i64) -> Option<String> {
    let seconds = timestamp_ms.div_euclid(1_000);
    let nanos = (timestamp_ms.rem_euclid(1_000) * 1_000_000) as u32;
    DateTime::<Utc>::from_timestamp(seconds, nanos).map(|dt| dt.to_rfc3339())
}

fn extract_tokens(message_data: &Value) -> (u64, u64) {
    let tokens = message_data.get("tokens");
    let input = tokens
        .and_then(|value| value.get("input"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = tokens
        .and_then(|value| value.get("output"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (input, output)
}

fn extract_file_paths(tool_name: &str, part_data: &Value) -> Vec<String> {
    let input = match part_data.get("state").and_then(|value| value.get("input")) {
        Some(value) => value,
        None => return Vec::new(),
    };

    let mut paths = Vec::new();
    match tool_name {
        "read" | "edit" | "write" => {
            if let Some(path) = input.get("filePath").and_then(Value::as_str)
                && !path.is_empty()
            {
                paths.push(path.to_string());
            }
        }
        _ => {}
    }

    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::summarize_session;
    use chrono::Duration;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn create_schema(conn: &Connection) {
        conn.execute_batch(
            "
            CREATE TABLE project (
                id TEXT PRIMARY KEY,
                worktree TEXT NOT NULL
            );
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                directory TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL DEFAULT 0,
                data TEXT NOT NULL
            );
            ",
        )
        .expect("create schema");
    }

    fn write_db(path: &Path) {
        let conn = Connection::open(path).expect("open db");
        create_schema(&conn);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES (?1, ?2)",
            ("proj-1", "/tmp/nippo"),
        )
        .expect("insert project");
        conn.execute(
            "INSERT INTO session (id, project_id, directory, title, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                "ses-1",
                "proj-1",
                "/tmp/nippo",
                "test",
                1_776_144_000_000_i64,
                1_776_144_500_000_i64,
            ),
        )
        .expect("insert session");

        // user message
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-u",
                "ses-1",
                1_776_144_000_000_i64,
                r#"{"role":"user","time":{"created":1776144000000}}"#,
            ),
        )
        .expect("insert user message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-u1",
                "msg-u",
                "ses-1",
                1_776_144_000_000_i64,
                r#"{"type":"text","text":"最初の指示"}"#,
            ),
        )
        .expect("insert user text part");

        // assistant message with tokens + tool part
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-a",
                "ses-1",
                1_776_144_100_000_i64,
                r#"{"role":"assistant","tokens":{"input":11,"output":7},"time":{"created":1776144100000}}"#,
            ),
        )
        .expect("insert assistant message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-a1",
                "msg-a",
                "ses-1",
                1_776_144_101_000_i64,
                r#"{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/tmp/nippo/crates/collector/src/main.rs"}}}"#,
            ),
        )
        .expect("insert read tool");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-a2",
                "msg-a",
                "ses-1",
                1_776_144_102_000_i64,
                r#"{"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/tmp/nippo/README.md","oldString":"a","newString":"b"}}}"#,
            ),
        )
        .expect("insert edit tool");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-a3",
                "msg-a",
                "ses-1",
                1_776_144_103_000_i64,
                r#"{"type":"text","text":"作業しました"}"#,
            ),
        )
        .expect("insert assistant text");

        // second assistant message with a bash tool (no file path)
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-a2",
                "ses-1",
                1_776_144_200_000_i64,
                r#"{"role":"assistant","tokens":{"input":5,"output":2}}"#,
            ),
        )
        .expect("insert assistant2");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-b1",
                "msg-a2",
                "ses-1",
                1_776_144_201_000_i64,
                r#"{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"ls"}}}"#,
            ),
        )
        .expect("insert bash tool");
    }

    #[test]
    fn collects_opencode_sessions_from_sqlite() {
        let dir = tempdir().expect("tempdir");
        write_db(&dir.path().join("opencode.db"));

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect sessions");

        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.session_id, "ses-1");
        assert_eq!(session.project, "nippo");
        assert_eq!(session.project_path, "/tmp/nippo");
        assert_eq!(session.user_entries.len(), 1);
        assert_eq!(session.user_entries[0].text, "最初の指示");
        assert_eq!(session.assistant_entries.len(), 2);

        let summary = summarize_session(session);
        assert_eq!(summary.message_counts.user, 1);
        assert_eq!(summary.message_counts.assistant, 2);
        assert_eq!(summary.tool_usage.get("read"), Some(&1));
        assert_eq!(summary.tool_usage.get("edit"), Some(&1));
        assert_eq!(summary.tool_usage.get("bash"), Some(&1));
        assert_eq!(summary.total_input_tokens, 16);
        assert_eq!(summary.total_output_tokens, 9);
        assert_eq!(
            summary.files_touched,
            vec!["README.md", "crates/collector/src/main.rs"]
        );
    }

    #[test]
    fn filters_by_local_day_bounds() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("opencode.db");
        let conn = Connection::open(&db_path).expect("open");
        create_schema(&conn);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES (?1, ?2)",
            ("proj-1", "/tmp/nippo"),
        )
        .expect("insert project");

        let filter = DateFilter::from_days(1);
        let cutoff = filter.mtime_cutoff().expect("cutoff");
        let cutoff_utc = DateTime::<Utc>::from(cutoff);
        let inside_ms = cutoff_utc.timestamp_millis();
        let outside_ms = (cutoff_utc - Duration::seconds(1)).timestamp_millis();

        conn.execute(
            "INSERT INTO session (id, project_id, directory, title, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            ("ses-1", "proj-1", "/tmp/nippo", "t", outside_ms, inside_ms),
        )
        .expect("insert session");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-old",
                "ses-1",
                outside_ms,
                r#"{"role":"user","time":{"created":0}}"#,
            ),
        )
        .expect("insert old message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-old",
                "msg-old",
                "ses-1",
                outside_ms,
                r#"{"type":"text","text":"too old"}"#,
            ),
        )
        .expect("insert old part");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            ("msg-new", "ses-1", inside_ms, r#"{"role":"user"}"#),
        )
        .expect("insert new message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-new",
                "msg-new",
                "ses-1",
                inside_ms,
                r#"{"type":"text","text":"kept"}"#,
            ),
        )
        .expect("insert new part");

        let sessions = collect_sessions(dir.path(), &filter).expect("collect");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "kept");
    }

    #[test]
    fn discover_history_files_returns_prod_and_dev_sorted() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("opencode.db"), b"").expect("touch prod");
        std::fs::write(dir.path().join("opencode-dev.db"), b"").expect("touch dev");

        let files = discover_history_files(dir.path()).expect("discover");
        // Both databases are returned and the order is deterministic
        // (files.sort() yields dev before prod because `-` < `.` in ASCII).
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("opencode-dev.db"));
        assert!(files[1].ends_with("opencode.db"));
    }

    #[test]
    fn collects_sessions_from_prod_and_dev_when_both_present() {
        let dir = tempdir().expect("tempdir");
        write_db(&dir.path().join("opencode.db"));

        // A separate dev database holds a distinct session; the collector
        // must read both files rather than stopping at the first match.
        let dev_path = dir.path().join("opencode-dev.db");
        let conn = Connection::open(&dev_path).expect("open dev");
        create_schema(&conn);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES (?1, ?2)",
            ("proj-dev", "/tmp/dev"),
        )
        .expect("insert dev project");
        conn.execute(
            "INSERT INTO session (id, project_id, directory, title, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                "ses-dev",
                "proj-dev",
                "/tmp/dev",
                "dev",
                1_776_144_000_000_i64,
                1_776_144_500_000_i64,
            ),
        )
        .expect("insert dev session");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-dev",
                "ses-dev",
                1_776_144_000_000_i64,
                r#"{"role":"user"}"#,
            ),
        )
        .expect("insert dev message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-dev",
                "msg-dev",
                "ses-dev",
                1_776_144_000_000_i64,
                r#"{"type":"text","text":"dev build"}"#,
            ),
        )
        .expect("insert dev part");

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect");
        let session_ids: Vec<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(
            session_ids.contains(&"ses-1"),
            "prod session ses-1 missing: {session_ids:?}"
        );
        assert!(
            session_ids.contains(&"ses-dev"),
            "dev session ses-dev missing: {session_ids:?}"
        );
    }

    #[test]
    fn skips_messages_with_unknown_role() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("opencode.db");
        let conn = Connection::open(&db_path).expect("open");
        create_schema(&conn);
        conn.execute(
            "INSERT INTO project (id, worktree) VALUES (?1, ?2)",
            ("proj-1", "/tmp/nippo"),
        )
        .expect("insert project");
        conn.execute(
            "INSERT INTO session (id, project_id, directory, title, time_created, time_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                "ses-1",
                "proj-1",
                "/tmp/nippo",
                "t",
                1_776_144_000_000_i64,
                1_776_144_500_000_i64,
            ),
        )
        .expect("insert session");
        // A `system` message and its part must not contribute to user_entries.
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-sys",
                "ses-1",
                1_776_144_000_000_i64,
                r#"{"role":"system"}"#,
            ),
        )
        .expect("insert system message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-sys",
                "msg-sys",
                "ses-1",
                1_776_144_000_000_i64,
                r#"{"type":"text","text":"system prompt"}"#,
            ),
        )
        .expect("insert system part");
        // A real user message so the session is not dropped as empty.
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            (
                "msg-u",
                "ses-1",
                1_776_144_100_000_i64,
                r#"{"role":"user"}"#,
            ),
        )
        .expect("insert user message");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-u",
                "msg-u",
                "ses-1",
                1_776_144_100_000_i64,
                r#"{"type":"text","text":"real user"}"#,
            ),
        )
        .expect("insert user part");

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect");
        assert_eq!(sessions.len(), 1);
        let session = &sessions[0];
        assert_eq!(session.user_entries.len(), 1);
        assert_eq!(session.user_entries[0].text, "real user");
    }

    #[test]
    fn skips_synthetic_and_ignored_user_text_parts() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("opencode.db");
        write_db(&db_path);
        let conn = Connection::open(&db_path).expect("open");

        for (message_id, part_id, timestamp, flag) in [
            (
                "msg-synthetic",
                "part-synthetic",
                1_776_144_300_000_i64,
                r#""synthetic":true"#,
            ),
            (
                "msg-ignored",
                "part-ignored",
                1_776_144_400_000_i64,
                r#""ignored":true"#,
            ),
        ] {
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                (message_id, "ses-1", timestamp, r#"{"role":"user"}"#),
            )
            .expect("insert user message");
            let data = format!(r#"{{"type":"text","text":"internal context",{flag}}}"#);
            conn.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                (part_id, message_id, "ses-1", timestamp, data),
            )
            .expect("insert user text part");
        }

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "最初の指示");
    }

    #[test]
    fn joins_user_text_parts_into_one_message() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("opencode.db");
        write_db(&db_path);
        let conn = Connection::open(&db_path).expect("open");
        conn.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "part-u2",
                "msg-u",
                "ses-1",
                1_776_144_000_001_i64,
                r#"{"type":"text","text":"次の指示"}"#,
            ),
        )
        .expect("insert second user text part");

        let filter = DateFilter::from_days(0);
        let sessions = collect_sessions(dir.path(), &filter).expect("collect");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].user_entries.len(), 1);
        assert_eq!(sessions[0].user_entries[0].text, "最初の指示\n次の指示");
    }

    #[test]
    fn discover_history_files_errors_when_missing() {
        let dir = tempdir().expect("tempdir");
        let err = discover_history_files(dir.path()).unwrap_err();
        assert!(err.to_string().contains("opencode の履歴データ"));
    }
}
