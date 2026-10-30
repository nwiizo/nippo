use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn fixture(dir: &Path, source: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    if source == "claude" {
        fs::create_dir_all(dir.join("projects/custom-project-name"))?;
        fs::write(
            dir.join("projects/custom-project-name/session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/demo","timestamp":"2026-10-07T03:00:00Z","message":{"content":"収集先を確認する"}}"#,
        )?;
    } else {
        fs::write(
            dir.join("history.jsonl"),
            r#"{"session_id":"s1","ts":1791342000,"text":"収集先を確認する"}"#,
        )?;
    }
    Ok(())
}

#[test]
fn collection_directories_follow_cli_then_environment_then_home() -> Result<()> {
    for (source, variable, default_name, argument) in [
        ("claude", "CLAUDE_CONFIG_DIR", ".claude", "--claude-dir"),
        ("codex", "CODEX_HOME", ".codex", "--codex-dir"),
    ] {
        for mode in ["default", "empty-env", "environment", "cli"] {
            let dir = tempdir()?;
            let custom = dir.path().join("custom");
            let explicit = dir.path().join("explicit");
            let expected = match mode {
                "environment" => custom.clone(),
                "cli" => explicit.clone(),
                _ => dir.path().join(default_name),
            };
            fixture(&expected, source)?;
            let mut command = Command::new(env!("CARGO_BIN_EXE_nippo"));
            command
                .args(["collect", "--source", "auto", "--days", "0"])
                .env("HOME", dir.path())
                .env("USERPROFILE", dir.path())
                .env_remove("CODEX_HOME")
                .env_remove("CLAUDE_CONFIG_DIR")
                .env_remove("CODEX_THREAD_ID")
                .env_remove("CLAUDE_CODE_SESSION_ID")
                .arg("--opencode-dir")
                .arg(dir.path().join("absent-opencode"));
            match mode {
                "empty-env" => {
                    command.env(variable, "");
                }
                "environment" | "cli" => {
                    command.env(variable, &custom);
                }
                _ => {}
            }
            if mode == "cli" {
                command.arg(argument).arg(&explicit);
            }
            let output = command.output()?;
            assert!(
                output.status.success(),
                "{source}/{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: Value = serde_json::from_slice(&output.stdout).context("parse output")?;
            assert_eq!(value["stats"]["total_user_messages"], 1, "{source}/{mode}");
        }
    }
    Ok(())
}
