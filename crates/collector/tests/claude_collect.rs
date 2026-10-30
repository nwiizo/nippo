use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn collect(dir: &Path, extra_args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_nippo"))
        .args(["collect", "--source", "claude", "--days", "0"])
        .arg("--claude-dir")
        .arg(dir)
        .args(extra_args)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("parse collect JSON")
}

fn write_session(dir: &Path, name: &str, entries: &[Value]) -> Result<()> {
    fs::create_dir_all(dir.join("projects/demo"))?;
    let lines = entries
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(dir.join("projects/demo").join(name), lines)?;
    Ok(())
}

fn user(text: Value, is_meta: Option<bool>) -> Value {
    let mut entry = json!({
        "type": "user", "sessionId": "s1", "cwd": "/tmp/demo",
        "timestamp": "2026-10-07T03:00:00Z", "message": {"content": text}
    });
    if let Some(is_meta) = is_meta {
        entry["isMeta"] = json!(is_meta);
    }
    entry
}

fn assistant() -> Value {
    json!({
        "type": "assistant", "sessionId": "s1", "cwd": "/tmp/demo",
        "timestamp": "2026-10-07T03:01:00Z",
        "message": {
            "content": [{"type": "tool_use", "name": "Edit", "input": {"file_path": "/tmp/demo/main.rs"}}],
            "usage": {"input_tokens": 100, "output_tokens": 10}
        }
    })
}

#[test]
fn meta_prompts_are_optional_but_never_decisions() -> Result<()> {
    let dir = tempdir()?;
    write_session(
        dir.path(),
        "session.jsonl",
        &[
            user(json!("Rust にする"), None),
            user(json!("テストを追加する"), Some(false)),
            user(json!("Go にする"), Some(true)),
            user(
                json!([{"type": "text", "text": "Python にする"}]),
                Some(true),
            ),
            user(
                json!("Another Claude session sent a message: Java にする"),
                None,
            ),
            user(json!("[Cross-session idle notice] Go にする"), None),
            assistant(),
        ],
    )?;
    let normal = collect(dir.path(), &[])?;
    assert_eq!(normal["stats"]["total_user_messages"], 2);
    assert_eq!(normal["stats"]["total_assistant_messages"], 1);
    assert_eq!(normal["decisions"].as_array().map(Vec::len), Some(1));
    assert_eq!(normal["decisions"][0]["user_prompt"], "Rust にする");
    for prompt in normal["sessions"][0]["user_prompts"]
        .as_array()
        .context("prompts")?
    {
        assert!(prompt.get("is_meta").is_none());
    }

    let diagnostic = collect(dir.path(), &["--include-prompt-noise"])?;
    assert_eq!(diagnostic["stats"]["total_user_messages"], 6);
    assert_eq!(diagnostic["stats"]["total_assistant_messages"], 1);
    assert_eq!(diagnostic["decisions"], normal["decisions"]);
    let prompts = diagnostic["sessions"][0]["user_prompts"]
        .as_array()
        .context("prompts")?;
    assert_eq!(
        prompts
            .iter()
            .filter(|prompt| prompt["is_meta"] == true)
            .count(),
        2
    );
    assert_eq!(
        diagnostic["sessions"][0]["files_touched"],
        json!(["main.rs"])
    );
    Ok(())
}

#[test]
fn meta_only_sessions_are_available_for_diagnostics() -> Result<()> {
    let dir = tempdir()?;
    write_session(
        dir.path(),
        "session.jsonl",
        &[user(json!("Rust にする"), Some(true)), assistant()],
    )?;
    assert_eq!(collect(dir.path(), &[])?["meta"]["total_sessions"], 0);
    let diagnostic = collect(dir.path(), &["--include-prompt-noise"])?;
    assert_eq!(diagnostic["meta"]["total_sessions"], 1);
    assert_eq!(diagnostic["stats"]["total_user_messages"], 1);
    assert_eq!(diagnostic["stats"]["total_assistant_messages"], 1);
    assert_eq!(
        diagnostic["sessions"][0]["user_prompts"][0]["text"],
        "Rust にする"
    );
    assert_eq!(diagnostic["decisions"], json!([]));
    Ok(())
}

#[test]
fn merging_keeps_identical_human_and_meta_prompts_distinct() -> Result<()> {
    let dir = tempdir()?;
    write_session(
        dir.path(),
        "meta.jsonl",
        &[user(json!("Rust にする"), Some(true))],
    )?;
    write_session(
        dir.path(),
        "human.jsonl",
        &[user(json!("Rust にする"), None)],
    )?;
    let normal = collect(dir.path(), &[])?;
    assert_eq!(normal["stats"]["total_user_messages"], 1);
    assert_eq!(normal["decisions"].as_array().map(Vec::len), Some(1));
    let diagnostic = collect(dir.path(), &["--include-prompt-noise"])?;
    assert_eq!(diagnostic["stats"]["total_user_messages"], 2);
    assert_eq!(diagnostic["decisions"], normal["decisions"]);
    Ok(())
}

#[test]
fn recognizes_explicit_synthetic_origins_without_dropping_legacy_input() -> Result<()> {
    let dir = tempdir()?;
    let mut entries = Vec::new();
    for kind in [
        "human",
        "channel",
        "future-origin",
        "peer",
        "task-notification",
        "coordinator",
        "auto-continuation",
        "unclassified",
    ] {
        let mut entry = user(json!(format!("{kind} にする")), None);
        entry["origin"] = json!({"kind": kind});
        entries.push(entry);
    }
    let mut synthetic = user(json!("synthetic にする"), None);
    synthetic["isSynthetic"] = json!(true);
    entries.push(synthetic);
    entries.push(user(json!("legacy にする"), None));
    write_session(dir.path(), "session.jsonl", &entries)?;
    let normal = collect(dir.path(), &[])?;
    assert_eq!(normal["stats"]["total_user_messages"], 4);
    assert_eq!(normal["decisions"].as_array().map(Vec::len), Some(4));
    let diagnostic = collect(dir.path(), &["--include-prompt-noise"])?;
    assert_eq!(diagnostic["stats"]["total_user_messages"], 10);
    assert_eq!(diagnostic["decisions"], normal["decisions"]);
    Ok(())
}

#[test]
fn truncates_only_prompts_exceeding_five_hundred_characters() -> Result<()> {
    for character in ["a", "あ", "🦀"] {
        for length in [0, 499, 500, 501] {
            let dir = tempdir()?;
            let prompt = character.repeat(length);
            write_session(dir.path(), "session.jsonl", &[user(json!(prompt), None)])?;
            let output = collect(dir.path(), &[])?;
            if length == 0 {
                assert_eq!(output["meta"]["total_sessions"], 0);
            } else {
                let expected = if length > 500 {
                    format!("{}...", character.repeat(500))
                } else {
                    prompt
                };
                assert_eq!(output["sessions"][0]["user_prompts"][0]["text"], expected);
            }
        }
    }
    Ok(())
}
