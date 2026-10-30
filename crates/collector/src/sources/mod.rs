pub mod claude_code;
pub mod codex;
pub mod opencode;

use std::path::Path;

/// Limit stored prompts by Unicode scalar count, adding a suffix only when cut.
pub(super) fn truncate_prompt(text: &str) -> String {
    match text.char_indices().nth(500) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text.to_string(),
    }
}

pub(super) fn extract_project_from_cwd(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| cwd.to_string())
}
