use std::fs;
use std::path::PathBuf;

// TODO(security): no sandboxing today — the agent can read or write any path
// the desktop user can. Mirrors the dev extension's prior behavior so this
// PR is a pure bug fix. Once we want to scope agent fs access (project root,
// prompt-on-write-outside, etc.), the check goes here.

#[tauri::command]
pub fn acp_read_text_file(
    path: String,
    line: Option<u32>,
    limit: Option<u32>,
) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let content = fs::read_to_string(&p)
        .map_err(|e| format!("Failed to read file '{}': {}", p.display(), e))?;
    Ok(apply_line_limit(&content, line, limit))
}

#[tauri::command]
pub fn acp_write_text_file(path: String, content: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
    }
    fs::write(&path, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))
}

// Match the slicing semantics of the dev extension's `apply_line_limit` in
// crates/goose/src/agents/platform_extensions/developer/edit.rs so the `read`
// tool produces identical output whether routed through ACP or the legacy path.
fn apply_line_limit(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return content.to_string();
    }
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let start = line
        .map(|l| (l as usize).saturating_sub(1))
        .unwrap_or(0)
        .min(lines.len());
    let end = limit
        .map(|l| start + l as usize)
        .unwrap_or(lines.len())
        .min(lines.len());
    lines[start..end].concat()
}

#[cfg(test)]
mod tests {
    use super::apply_line_limit;

    const SAMPLE: &str = "a\nb\nc\nd\ne\n";

    #[test]
    fn no_line_no_limit_returns_full_content() {
        assert_eq!(apply_line_limit(SAMPLE, None, None), SAMPLE);
    }

    #[test]
    fn line_only_skips_to_offset() {
        assert_eq!(apply_line_limit(SAMPLE, Some(3), None), "c\nd\ne\n");
    }

    #[test]
    fn limit_only_truncates_from_start() {
        assert_eq!(apply_line_limit(SAMPLE, None, Some(2)), "a\nb\n");
    }

    #[test]
    fn line_and_limit_combine() {
        assert_eq!(apply_line_limit(SAMPLE, Some(2), Some(2)), "b\nc\n");
    }

    #[test]
    fn line_one_is_same_as_no_offset() {
        assert_eq!(apply_line_limit(SAMPLE, Some(1), Some(3)), "a\nb\nc\n");
    }

    #[test]
    fn line_past_end_returns_empty() {
        assert_eq!(apply_line_limit(SAMPLE, Some(99), None), "");
    }

    #[test]
    fn limit_past_end_clamps() {
        assert_eq!(apply_line_limit(SAMPLE, Some(4), Some(100)), "d\ne\n");
    }

    #[test]
    fn handles_missing_trailing_newline() {
        let content = "a\nb\nc";
        assert_eq!(apply_line_limit(content, Some(2), Some(2)), "b\nc");
    }
}
