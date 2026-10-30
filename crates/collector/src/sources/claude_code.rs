//! Claude Code JSONL セッションファイルのパーサ。
//!
//! ~/.claude/projects/ 以下に保存される JSONL ファイルを読み取り、
//! ユーザーのプロンプト・アシスタントの応答・ツール使用状況を抽出する。
//! rayon による並列パースと、2パスデシリアライズによる高速化を行う。

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{extract_project_from_cwd, truncate_prompt};
use crate::filter::DateFilter;
use crate::session::{ParsedAssistantEntry, ParsedUserEntry, RawSession};

// ---------------------------------------------------------------------------
// JSONL エントリ型（実データの構造に基づく）
//
// トップレベル type: user, assistant, queue-operation, progress,
//                    file-history-snapshot, system, last-prompt
// このうち日報生成に必要なのは user と assistant のみ。
// ---------------------------------------------------------------------------

/// JSONL 1行ごとのエントリ。type フィールドで判別する。
#[derive(Deserialize)]
#[serde(tag = "type")]
enum JournalEntry {
    #[serde(rename = "user")]
    User(UserEntry),
    #[serde(rename = "assistant")]
    Assistant(AssistantEntry),
    /// queue-operation, progress 等は構造を見ないため unit で受ける
    #[serde(other)]
    Other,
}

/// ユーザーメッセージ
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserEntry {
    timestamp: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    #[serde(default)]
    is_sidechain: Option<bool>,
    /// hook の出力・system-reminder・他セッションからのメッセージ等、ハーネスが差し込んだ行
    #[serde(default)]
    is_meta: Option<bool>,
    is_synthetic: Option<bool>,
    origin: Option<MessageOrigin>,
    message: Option<UserMessage>,
}

#[derive(Deserialize)]
struct MessageOrigin {
    kind: Option<String>,
}

impl UserEntry {
    fn is_meta(&self) -> bool {
        self.is_meta.unwrap_or(false)
            || self.is_synthetic.unwrap_or(false)
            || matches!(
                self.origin
                    .as_ref()
                    .and_then(|origin| origin.kind.as_deref()),
                Some(
                    "peer"
                        | "task-notification"
                        | "coordinator"
                        | "auto-continuation"
                        | "unclassified"
                )
            )
    }
}

#[derive(Deserialize)]
struct UserMessage {
    content: MessageContent,
}

/// message.content は文字列またはブロック配列のどちらかが来る
#[derive(Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// コンテンツブロックの種別
#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    /// tool_result, thinking 等は中身を使わない
    #[serde(other)]
    Unknown,
}

/// アシスタントメッセージ
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssistantEntry {
    timestamp: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    message: Option<AssistantMessage>,
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: Option<Vec<ContentBlock>>,
    usage: Option<TokenUsage>,
}

#[derive(Deserialize)]
struct TokenUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

/// 2パスデシリアライズ用の軽量ヘッダ。
/// 1パス目で type と timestamp だけ読み、フィルタを通過したものだけ2パス目でフル展開する。
#[derive(Deserialize)]
struct EntryHeader {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    timestamp: Option<String>,
}

// ---------------------------------------------------------------------------
// パース結果の中間表現
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// セッションファイルの探索
// ---------------------------------------------------------------------------

pub struct SessionFile {
    pub path: PathBuf,
    pub mtime: SystemTime,
}

/// ~/.claude/projects/ 以下の全 JSONL ファイルを探索する
pub fn discover_session_files(claude_dir: &Path) -> Result<Vec<SessionFile>> {
    let projects_dir = claude_dir.join("projects");
    if !projects_dir.exists() {
        anyhow::bail!(
            "Claude Code のセッションデータが見つかりません: {}\n\n\
             Claude Code（CLI または VS Code 拡張）を使用すると、\n\
             セッションデータが自動的にこのディレクトリに保存されます。\n\
             カスタムディレクトリを指定する場合は --claude-dir オプションを使用してください。",
            projects_dir.display()
        );
    }

    let pattern = format!("{}/**/*.jsonl", projects_dir.display());
    let mut files = Vec::new();

    for entry in glob::glob(&pattern).context("Failed to read glob pattern")? {
        let path = match entry {
            Ok(p) => p,
            Err(_) => continue,
        };
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        files.push(SessionFile { path, mtime });
    }

    Ok(files)
}

// ---------------------------------------------------------------------------
// JSONL パース
// ---------------------------------------------------------------------------

/// ユーザーメッセージからテキスト部分を抽出する
fn extract_user_text(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::Text(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(truncate_prompt(trimmed))
            }
        }
        MessageContent::Blocks(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(truncate_prompt(&texts.join("\n")))
            }
        }
    }
}

/// アシスタント応答からツール名とファイルパスを抽出する
fn extract_tool_info(blocks: &[ContentBlock]) -> (Vec<String>, Vec<String>) {
    let mut tool_names = Vec::new();
    let mut file_paths = Vec::new();

    for block in blocks {
        if let ContentBlock::ToolUse {
            name: Some(n),
            input,
        } = block
        {
            tool_names.push(n.clone());

            // ファイル操作系ツールからパスを抽出
            if let Some(input_val) = input
                && matches!(n.as_str(), "Read" | "Write" | "Edit" | "Glob" | "Grep")
            {
                if let Some(fp) = input_val.get("file_path").and_then(|v| v.as_str()) {
                    file_paths.push(fp.to_string());
                }
                if let Some(fp) = input_val.get("path").and_then(|v| v.as_str()) {
                    file_paths.push(fp.to_string());
                }
            }
        }
    }

    (tool_names, file_paths)
}

/// 1つの JSONL ファイルをパースし、RawSession を返す。
/// エントリが1件もフィルタを通過しなければ None。
pub fn parse_session_file(path: &Path, filter: &DateFilter) -> Result<Option<RawSession>> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut user_entries = Vec::new();
    let mut assistant_entries = Vec::new();
    let mut session_id = String::new();
    let mut project = String::new();
    let mut project_path = String::new();
    let mut git_branch: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        // 1パス目: type と timestamp だけ確認してフィルタ
        let header: EntryHeader = match serde_json::from_str(&line) {
            Ok(h) => h,
            Err(_) => continue,
        };

        let entry_type = match &header.entry_type {
            Some(t) => t.as_str(),
            None => continue,
        };

        if !matches!(entry_type, "user" | "assistant") {
            continue;
        }

        if let Some(ts) = &header.timestamp {
            if !filter.matches(ts) {
                continue;
            }
        } else {
            continue;
        }

        // 2パス目: フィルタを通過したエントリのみフルデシリアライズ
        let entry: JournalEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        match entry {
            JournalEntry::User(user) => {
                let is_meta = user.is_meta();
                // サブエージェントの内部メッセージはスキップ
                if user.is_sidechain.unwrap_or(false) {
                    continue;
                }

                if session_id.is_empty()
                    && let Some(sid) = &user.session_id
                {
                    session_id = sid.clone();
                }
                if project.is_empty()
                    && let Some(cwd) = &user.cwd
                {
                    project = extract_project_from_cwd(cwd);
                    project_path = cwd.clone();
                }
                if git_branch.is_none() {
                    git_branch = user.git_branch.clone();
                }

                if let Some(msg) = &user.message
                    && let Some(text) = extract_user_text(&msg.content)
                {
                    user_entries.push(ParsedUserEntry {
                        timestamp: user.timestamp.unwrap_or_default(),
                        text,
                        is_meta,
                    });
                }
            }
            JournalEntry::Assistant(assistant) => {
                if session_id.is_empty()
                    && let Some(sid) = &assistant.session_id
                {
                    session_id = sid.clone();
                }
                if project.is_empty()
                    && let Some(cwd) = &assistant.cwd
                {
                    project = extract_project_from_cwd(cwd);
                    project_path = cwd.clone();
                }
                if git_branch.is_none() {
                    git_branch = assistant.git_branch.clone();
                }

                if let Some(msg) = &assistant.message {
                    let blocks = msg.content.as_deref().unwrap_or(&[]);
                    let (tool_uses, file_paths) = extract_tool_info(blocks);
                    let (input_tokens, output_tokens) = msg
                        .usage
                        .as_ref()
                        .map(|u| (u.input_tokens.unwrap_or(0), u.output_tokens.unwrap_or(0)))
                        .unwrap_or((0, 0));

                    assistant_entries.push(ParsedAssistantEntry {
                        timestamp: assistant.timestamp.unwrap_or_default(),
                        message_count: 1,
                        tool_uses,
                        input_tokens,
                        output_tokens,
                        file_paths,
                    });
                }
            }
            JournalEntry::Other => {}
        }
    }

    if user_entries.is_empty() && assistant_entries.is_empty() {
        return Ok(None);
    }

    // ファイル名をセッションIDのフォールバックに使う
    if session_id.is_empty() {
        session_id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }

    Ok(Some(RawSession {
        session_id,
        project,
        project_path,
        git_branch,
        user_entries,
        assistant_entries,
    }))
}

// ---------------------------------------------------------------------------
// 並列収集
// ---------------------------------------------------------------------------

/// 全セッションファイルを並列でパースし、フィルタ済みの RawSession を返す
pub fn collect_sessions(claude_dir: &Path, filter: &DateFilter) -> Result<Vec<RawSession>> {
    let files = discover_session_files(claude_dir)?;

    // ファイルの更新日時で大半をスキップ（mtime プレフィルタ）
    let cutoff = filter.mtime_cutoff();
    let candidates: Vec<&SessionFile> = files
        .iter()
        .filter(|f| cutoff.map(|c| f.mtime >= c).unwrap_or(true))
        .collect();

    let sessions: Vec<RawSession> = candidates
        .par_iter()
        .filter_map(|sf| parse_session_file(&sf.path, filter).ok().flatten())
        .collect();

    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn preserves_meta_user_entries_for_optional_filtering() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"user\",\"timestamp\":\"2026-10-07T03:00:00Z\",\"sessionId\":\"s1\",\"cwd\":\"/tmp/articles\",\"message\":{\"role\":\"user\",\"content\":\"記事はです・ます調にする\"}}\n",
                "{\"type\":\"user\",\"isMeta\":true,\"timestamp\":\"2026-10-07T03:01:00Z\",\"sessionId\":\"s1\",\"cwd\":\"/tmp/articles\",\"message\":{\"role\":\"user\",\"content\":\"Stop hook feedback:\\n[bash ~/.claude/hooks/stop-bye-trigger.sh]: ⚠️ 文脈が 301k トークンに達した\"}}\n",
                "{\"type\":\"user\",\"isMeta\":true,\"timestamp\":\"2026-10-07T03:02:00Z\",\"sessionId\":\"s1\",\"cwd\":\"/tmp/articles\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"Another Claude session sent a message: <teammate-message teammate_id=\\\"x\\\">ではなく</teammate-message>\"}]}}\n",
                "{\"type\":\"assistant\",\"timestamp\":\"2026-10-07T03:03:00Z\",\"sessionId\":\"s1\",\"cwd\":\"/tmp/articles\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}\n"
            ),
        )
        .expect("write session");

        let filter =
            DateFilter::from_range(Some("2026-10-07"), Some("2026-10-07")).expect("filter");
        let session = parse_session_file(&path, &filter)
            .expect("parse")
            .expect("session");

        let texts: Vec<&str> = session
            .user_entries
            .iter()
            .filter(|entry| !entry.is_meta)
            .map(|entry| entry.text.as_str())
            .collect();
        assert_eq!(texts, vec!["記事はです・ます調にする"]);
        assert_eq!(session.user_entries.len(), 3);
        assert!(session.user_entries[1].is_meta);
        assert!(session.user_entries[2].is_meta);
        assert_eq!(session.assistant_entries.len(), 1);
    }
}
