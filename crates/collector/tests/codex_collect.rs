use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::{TempDir, tempdir};

const PROMPT: &str = "履歴の収集対象を確認してください";

fn user_message(timestamp: &str, text: &str) -> Value {
    json!({
        "timestamp": timestamp,
        "type": "response_item",
        "payload": {
            "type": "message", "role": "user",
            "content": [{"type": "input_text", "text": text}]
        }
    })
}

fn rollout() -> Vec<Value> {
    vec![
        user_message("2026-09-04T12:00:00Z", PROMPT),
        json!({
            "timestamp": "2026-09-04T12:00:01Z", "type": "response_item",
            "payload": {
                "type": "message", "role": "assistant",
                "content": [{"type": "output_text", "text": "収集対象を確認します"}]
            }
        }),
    ]
}

fn write_jsonl(path: &Path, entries: &[Value]) -> Result<()> {
    let lines: Vec<String> = entries.iter().map(Value::to_string).collect();
    fs::write(path, lines.join("\n"))?;
    Ok(())
}

fn fixture(history: Option<&[Value]>, rollout: &[Value]) -> Result<TempDir> {
    let dir = tempdir()?;
    if let Some(history) = history {
        write_jsonl(&dir.path().join("history.jsonl"), history)?;
    }
    let rollout_path = dir.path().join("rollout.jsonl");
    write_jsonl(&rollout_path, rollout)?;
    let conn = Connection::open(dir.path().join("state_5.sqlite"))?;
    conn.execute_batch(
        "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, git_branch TEXT, rollout_path TEXT);",
    )?;
    conn.execute(
        "INSERT INTO threads VALUES (?1, ?2, ?3, ?4)",
        (
            "test-thread",
            "/tmp/nippo-repro",
            "main",
            rollout_path.to_str(),
        ),
    )?;
    Ok(dir)
}

fn collect(dir: &Path, source: &str, extra_args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_nippo"))
        .args([
            "collect",
            "--source",
            source,
            "--from",
            "2026-09-04",
            "--to",
            "2026-09-04",
        ])
        .arg("--codex-dir")
        .arg(dir)
        .arg("--claude-dir")
        .arg(dir.join("absent-claude"))
        .arg("--opencode-dir")
        .arg(dir.join("absent-opencode"))
        .args(extra_args)
        .env("TZ", "Asia/Tokyo")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()?;
    assert!(
        output.status.success(),
        "collect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("parse collect JSON")
}

#[test]
fn collects_rollout_session_with_empty_history() -> Result<()> {
    let dir = fixture(Some(&[]), &rollout())?;
    let output = collect(dir.path(), "codex", &[])?;

    assert_eq!(output["meta"]["total_sessions"], 1);
    assert_eq!(output["stats"]["total_user_messages"], 1);
    assert_eq!(output["stats"]["total_assistant_messages"], 1);
    assert_eq!(output["sessions"][0]["session_id"], "test-thread");
    assert_eq!(output["sessions"][0]["project"], "nippo-repro");
    assert_eq!(output["sessions"][0]["git_branch"], "main");
    assert_eq!(output["sessions"][0]["user_prompts"][0]["text"], PROMPT);
    Ok(())
}

fn usage_event(
    timestamp: &str,
    modern: bool,
    input: u64,
    output: u64,
    total_input: u64,
    total_output: u64,
) -> Value {
    let usage = json!({"input_tokens": input, "output_tokens": output});
    let total = json!({"input_tokens": total_input, "output_tokens": total_output});
    if modern {
        json!({"timestamp": timestamp, "type": "token_usage_record", "payload": {
            "response_id": format!("response-{total_input}-{total_output}"),
            "usage": usage, "thread_token_usage": total
        }})
    } else {
        json!({"timestamp": timestamp, "type": "event_msg", "payload": {
            "type": "token_count", "info": {"last_token_usage": usage, "total_token_usage": total}
        }})
    }
}

#[test]
fn counts_modern_usage_records_without_legacy_notifications() -> Result<()> {
    let mut entries = rollout();
    entries.push(usage_event("2026-09-04T12:00:02Z", true, 11, 7, 11, 7));
    entries.push(usage_event("2026-09-04T12:00:03Z", true, 11, 7, 22, 14));
    let dir = fixture(None, &entries)?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_input_tokens"], 22);
    assert_eq!(output["stats"]["total_output_tokens"], 14);
    Ok(())
}

#[test]
fn deduplicates_usage_updates_and_keeps_period_boundaries() -> Result<()> {
    let mut entries = vec![usage_event("2026-09-03T12:00:00Z", false, 100, 20, 100, 20)];
    entries.extend(rollout());
    entries.extend([
        // A rate-limit update repeats usage from outside the requested day.
        usage_event("2026-09-04T12:00:02Z", false, 100, 20, 100, 20),
        usage_event("2026-09-04T12:00:03Z", true, 11, 7, 111, 27),
        usage_event("2026-09-04T12:00:04Z", false, 11, 7, 111, 27),
        // A later response may have exactly the same per-response usage.
        usage_event("2026-09-04T12:00:05Z", true, 11, 7, 122, 34),
        usage_event("2026-09-04T12:00:06Z", false, 11, 7, 122, 34),
        usage_event("2026-09-04T12:00:07Z", true, 11, 7, 111, 27),
    ]);
    let dir = fixture(None, &entries)?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_input_tokens"], 22);
    assert_eq!(output["stats"]["total_output_tokens"], 14);
    Ok(())
}

#[test]
fn separates_harness_context_from_user_text_using_provenance() -> Result<()> {
    let mut mixed = user_message("2026-09-04T12:00:00Z", "unused");
    mixed["payload"]["content"] = json!([
        {"type": "input_text", "text": "Rust にする"},
        {"type": "input_text", "text": "Go にする"},
        {"type": "input_text", "text": "Python にする"}
    ]);
    mixed["payload"]["internal_chat_message_metadata_passthrough"] = json!({
        "content_item_metadata": [
            {"provenance": {"type": "user"}},
            {"provenance": {"type": "harness", "hook": true}},
            {"provenance": {"type": "agents_md"}}
        ]
    });
    let mut entries = rollout();
    entries[0] = mixed;
    let dir = fixture(None, &entries)?;
    let normal = collect(dir.path(), "codex", &[])?;
    assert_eq!(
        normal["sessions"][0]["user_prompts"][0]["text"],
        "Rust にする"
    );
    assert_eq!(normal["decisions"][0]["user_prompt"], "Rust にする");
    let diagnostic = collect(dir.path(), "codex", &["--include-prompt-noise"])?;
    let prompts = diagnostic["sessions"][0]["user_prompts"]
        .as_array()
        .context("prompts")?;
    assert_eq!(prompts.len(), 2);
    assert!(prompts.iter().any(|prompt| prompt["is_meta"] == true && prompt["text"] == "Go にする\nPython にする"));
    assert_eq!(diagnostic["decisions"], normal["decisions"]);
    Ok(())
}

#[test]
fn unknown_or_misaligned_provenance_does_not_discard_user_input() -> Result<()> {
    for metadata in [
        json!({"content_item_metadata": [{"provenance": {"type": "future-source"}}]}),
        json!({"content_item_metadata": [{"provenance": {"type": "harness"}}, {}]}),
        json!({"content_item_metadata": "unexpected"}),
    ] {
        let mut entries = rollout();
        entries[0]["payload"]["internal_chat_message_metadata_passthrough"] = metadata;
        let dir = fixture(None, &entries)?;
        let output = collect(dir.path(), "codex", &[])?;
        assert_eq!(output["stats"]["total_user_messages"], 1);
        assert_eq!(output["sessions"][0]["user_prompts"][0]["text"], PROMPT);
    }
    Ok(())
}

#[test]
fn matching_history_does_not_consume_a_synthetic_prompt() -> Result<()> {
    let timestamp = "2026-09-04T12:00:00Z";
    let mut meta = user_message(timestamp, PROMPT);
    meta["payload"]["internal_chat_message_metadata_passthrough"] = json!({
        "content_item_metadata": [{"provenance": {"type": "harness"}}]
    });
    let history = [json!({"session_id": "test-thread", "ts": 1788523200, "text": PROMPT})];
    let dir = fixture(Some(&history), &[meta, user_message(timestamp, PROMPT)])?;
    let output = collect(dir.path(), "codex", &["--include-prompt-noise"])?;
    assert_eq!(output["stats"]["total_user_messages"], 2);
    let prompts = output["sessions"][0]["user_prompts"]
        .as_array()
        .context("prompts")?;
    assert_eq!(
        prompts
            .iter()
            .filter(|prompt| prompt["is_meta"] == true)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn usage_counter_reset_and_legacy_records_keep_single_request_usage() -> Result<()> {
    let mut entries = rollout();
    entries.extend([
        usage_event("2026-09-04T12:00:02Z", false, 11, 7, 100, 50),
        usage_event("2026-09-04T12:00:03Z", false, 3, 2, 3, 2),
        usage_event("2026-09-04T12:00:04Z", false, 3, 2, 3, 2),
        json!({"timestamp": "2026-09-04T12:00:05Z", "type": "event_msg", "payload": {
            "type": "token_count", "info": {"last_token_usage": {"output_tokens": 4}}
        }}),
    ]);
    let dir = fixture(None, &entries)?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_input_tokens"], 14);
    assert_eq!(output["stats"]["total_output_tokens"], 13);
    Ok(())
}

#[test]
fn discovers_codex_without_a_history_file() -> Result<()> {
    let dir = fixture(None, &rollout())?;
    for source in ["codex", "auto", "all"] {
        let output = collect(dir.path(), source, &[])?;
        assert_eq!(output["meta"]["total_sessions"], 1, "source {source}");
        assert_eq!(output["stats"]["total_user_messages"], 1);
        assert_eq!(output["stats"]["total_assistant_messages"], 1);
    }
    Ok(())
}

#[test]
fn merges_partial_history_and_rollout_without_double_counting() -> Result<()> {
    let history = [
        json!({"session_id": "test-thread", "ts": 1788523200_i64, "text": PROMPT}),
        json!({"session_id": "test-thread", "ts": 1788523260_i64, "text": "history only"}),
    ];
    let mut entries = rollout();
    entries[0]["timestamp"] = json!("2026-09-04T21:00:01.123+09:00");
    entries.push(user_message("2026-09-04T12:02:00Z", "rollout only"));
    // Repeating the same words later is a separate prompt.
    entries.push(user_message("2026-09-04T12:03:00Z", PROMPT));
    // The event notification must not count the response_item a second time.
    entries.push(json!({
        "timestamp": "2026-09-04T12:00:00.124Z", "type": "event_msg",
        "payload": {"type": "user_message", "message": PROMPT}
    }));
    entries.push(json!({
        "timestamp": "2026-09-04T12:00:02Z", "type": "response_item",
        "payload": {"type": "function_call", "name": "exec_command", "arguments": "{}"}
    }));
    entries.push(json!({
        "timestamp": "2026-09-04T12:00:03Z", "type": "event_msg",
        "payload": {"type": "token_count", "info": {
            "last_token_usage": {"input_tokens": 11, "output_tokens": 7}
        }}
    }));
    let dir = fixture(Some(&history), &entries)?;
    let output = collect(dir.path(), "codex", &[])?;

    assert_eq!(output["meta"]["total_sessions"], 1);
    assert_eq!(output["stats"]["total_user_messages"], 4);
    assert_eq!(output["stats"]["total_assistant_messages"], 1);
    assert_eq!(output["stats"]["total_tool_uses"], 1);
    assert_eq!(output["stats"]["total_input_tokens"], 11);
    assert_eq!(output["stats"]["total_output_tokens"], 7);
    let prompts = &output["sessions"][0]["user_prompts"];
    assert_eq!(prompts[0]["text"], PROMPT);
    assert_eq!(prompts[1]["text"], "history only");
    assert_eq!(prompts[2]["text"], "rollout only");
    assert_eq!(prompts[3]["text"], PROMPT);
    Ok(())
}

#[test]
fn matches_repeated_prompts_to_the_nearest_history_entry() -> Result<()> {
    let history = [
        json!({"session_id": "test-thread", "ts": 1788523380_i64, "text": PROMPT}),
        json!({"session_id": "test-thread", "ts": 1788523380_i64, "text": PROMPT}),
    ];
    let mut entries = rollout();
    entries.push(user_message("2026-09-04T12:03:01.123Z", PROMPT));
    entries.push(user_message("2026-09-04T12:03:01.123Z", PROMPT));
    let dir = fixture(Some(&history), &entries)?;
    let output = collect(dir.path(), "codex", &[])?;

    assert_eq!(output["stats"]["total_user_messages"], 2);
    let prompts = &output["sessions"][0]["user_prompts"];
    assert_eq!(prompts[0]["timestamp"], "2026-09-04T12:00:00+00:00");
    assert_eq!(prompts[1]["timestamp"], "2026-09-04T12:03:00+00:00");
    Ok(())
}

#[test]
fn compares_full_prompts_before_truncating_output() -> Result<()> {
    let prefix = "あ".repeat(500);
    let history = [json!({
        "session_id": "test-thread", "ts": 1788523200_i64,
        "text": format!("{prefix} history")
    })];
    let entries = [user_message(
        "2026-09-04T12:00:01Z",
        &format!("{prefix} rollout"),
    )];
    let dir = fixture(Some(&history), &entries)?;
    let output = collect(dir.path(), "codex", &[])?;

    assert_eq!(output["stats"]["total_user_messages"], 2);
    assert_eq!(
        output["sessions"][0]["user_prompts"][0]["text"],
        format!("{prefix}...")
    );
    Ok(())
}

#[test]
fn filters_rollout_prompts_by_local_day_and_prompt_noise() -> Result<()> {
    let mut entries = rollout();
    entries[0] = user_message("2026-09-03T15:00:00Z", "local midnight");
    entries.extend([
        user_message("2026-09-03T14:59:59Z", "previous day"),
        user_message("2026-09-04T14:59:59.999Z", "end of day"),
        user_message("2026-09-04T15:00:00Z", "next day"),
        user_message(
            "2026-09-04T12:01:00Z",
            "<task-notification>done</task-notification>",
        ),
        user_message("2026-09-04T12:02:00Z", "はい"),
        user_message("2026-09-04T12:03:00Z", "   "),
        user_message(
            "2026-09-04T12:04:00Z",
            "# AGENTS.md instructions for /tmp/nippo\n\n<INSTRUCTIONS>Project rules</INSTRUCTIONS>",
        ),
        user_message(
            "2026-09-04T12:05:00Z",
            "<environment_context>cwd: /tmp/nippo</environment_context>",
        ),
    ]);
    let dir = fixture(Some(&[]), &entries)?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_user_messages"], 2);
    let prompts = &output["sessions"][0]["user_prompts"];
    assert_eq!(prompts[0]["text"], "local midnight");
    assert_eq!(prompts[1]["text"], "end of day");

    let unfiltered = collect(dir.path(), "codex", &["--include-prompt-noise"])?;
    assert_eq!(unfiltered["stats"]["total_user_messages"], 6);
    Ok(())
}

#[test]
fn reads_user_text_blocks_and_skips_invalid_rollout_records() -> Result<()> {
    let entries = [
        json!({
            "timestamp": "2026-09-04T12:00:00Z", "type": "response_item",
            "payload": {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "  first"},
                {"type": "input_image", "image_url": "data:image/png;base64,..."},
                {"type": "input_text", "text": "second  "}
            ]}
        }),
        user_message("invalid", "invalid date"),
        json!({
            "timestamp": "2026-09-04T12:00:01Z", "type": "response_item",
            "payload": {"type": "message", "role": "user", "content": null}
        }),
        json!({
            "timestamp": "2026-09-04T12:00:02Z", "type": "response_item",
            "payload": {"type": "message", "role": "developer", "content": [
                {"type": "input_text", "text": "not a user prompt"}
            ]}
        }),
    ];
    let dir = fixture(Some(&[]), &entries)?;
    let rollout_path = dir.path().join("rollout.jsonl");
    // A broken line must not hide valid records that follow it.
    fs::write(
        &rollout_path,
        format!("not JSON\n{}\n{{", fs::read_to_string(&rollout_path)?),
    )?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_user_messages"], 1);
    assert_eq!(
        output["sessions"][0]["user_prompts"][0]["text"],
        "first\nsecond"
    );
    Ok(())
}

#[test]
fn excludes_sessions_without_meaningful_prompts_in_the_period() -> Result<()> {
    let mut entries = rollout();
    entries[0] = user_message("2026-09-03T14:59:59Z", PROMPT);
    entries.push(user_message("2026-09-04T12:00:00Z", "はい"));
    let dir = fixture(Some(&[]), &entries)?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["meta"]["total_sessions"], 0);

    let unfiltered = collect(dir.path(), "codex", &["--include-prompt-noise"])?;
    assert_eq!(unfiltered["meta"]["total_sessions"], 1);
    assert_eq!(unfiltered["stats"]["total_user_messages"], 1);
    assert_eq!(unfiltered["stats"]["total_assistant_messages"], 1);
    Ok(())
}

#[test]
fn keeps_history_when_rollout_or_metadata_is_missing() -> Result<()> {
    let history = [json!({"session_id": "test-thread", "ts": 1788523200_i64, "text": PROMPT})];
    let dir = fixture(Some(&history), &[])?;
    fs::remove_file(dir.path().join("rollout.jsonl"))?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_user_messages"], 1);
    assert_eq!(output["stats"]["total_assistant_messages"], 0);

    fs::remove_file(dir.path().join("state_5.sqlite"))?;
    let output = collect(dir.path(), "codex", &[])?;
    assert_eq!(output["stats"]["total_user_messages"], 1);
    assert_eq!(output["sessions"][0]["project"], "unknown");
    Ok(())
}
