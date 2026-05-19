use std::fs;
use std::path::{Path, PathBuf};

use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;

const NO_MATCH_PREVIEW_LINES: usize = 20;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileReadParams {
    /// Absolute path to the file to read.
    pub path: String,
    /// Line number to start reading from (1-based).
    #[schemars(range(min = 1))]
    pub line: Option<u32>,
    /// Maximum number of lines to read.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileWriteParams {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileEditParams {
    pub path: String,
    pub before: String,
    pub after: String,
}

pub struct EditTools;

impl EditTools {
    pub fn new() -> Self {
        Self
    }

    pub fn file_write(&self, params: FileWriteParams) -> CallToolResult {
        self.file_write_with_cwd(params, None)
    }

    pub fn file_write_with_cwd(
        &self,
        params: FileWriteParams,
        working_dir: Option<&Path>,
    ) -> CallToolResult {
        if let Some(err) = reject_uri_path(&params.path) {
            return err;
        }
        let path = resolve_path(&params.path, working_dir);

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if let Err(error) = fs::create_dir_all(parent) {
                    return CallToolResult::error(vec![Content::text(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        error
                    ))
                    .with_priority(0.0)]);
                }
            }
        }

        let is_new = !path.exists();

        match fs::write(path, &params.content) {
            Ok(()) => {
                let line_count = params.content.lines().count();
                let action = if is_new { "Created" } else { "Wrote" };
                CallToolResult::success(vec![Content::text(format!(
                    "{} {} ({} lines)",
                    action, params.path, line_count
                ))
                .with_priority(0.0)])
            }
            Err(error) => CallToolResult::error(vec![Content::text(format!(
                "Failed to write {}: {}",
                params.path, error
            ))
            .with_priority(0.0)]),
        }
    }

    pub fn file_edit(&self, params: FileEditParams) -> CallToolResult {
        self.file_edit_with_cwd(params, None)
    }

    pub fn file_edit_with_cwd(
        &self,
        params: FileEditParams,
        working_dir: Option<&Path>,
    ) -> CallToolResult {
        if let Some(err) = reject_uri_path(&params.path) {
            return err;
        }
        let path = resolve_path(&params.path, working_dir);

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(error) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Failed to read {}: {}",
                    params.path, error
                ))
                .with_priority(0.0)]);
            }
        };

        let new_content = match string_replace(&content, &params.before, &params.after) {
            Ok(c) => c,
            Err(msg) => {
                return CallToolResult::error(vec![Content::text(msg).with_priority(0.0)]);
            }
        };
        match fs::write(&path, &new_content) {
            Ok(()) => {
                let old_lines = params.before.lines().count();
                let new_lines = params.after.lines().count();
                CallToolResult::success(vec![Content::text(format!(
                    "Edited {} ({} lines -> {} lines)",
                    params.path, old_lines, new_lines
                ))
                .with_priority(0.0)])
            }
            Err(error) => CallToolResult::error(vec![Content::text(format!(
                "Failed to write {}: {}",
                params.path, error
            ))
            .with_priority(0.0)]),
        }
    }
}

impl Default for EditTools {
    fn default() -> Self {
        Self::new()
    }
}

pub fn string_replace(content: &str, before: &str, after: &str) -> Result<String, String> {
    let matches: Vec<_> = content.match_indices(before).collect();

    match matches.len() {
        0 => {
            let suggestion = find_similar_context(content, before);
            let mut msg = "No match found for the specified text.".to_string();
            if let Some(hint) = suggestion {
                msg.push_str(&format!("\n\nDid you mean:\n```\n{}\n```", hint));
            }
            let preview = build_file_preview(content, NO_MATCH_PREVIEW_LINES);
            msg.push_str(&format!("\n\nFile preview:\n```\n{}\n```", preview));
            Err(msg)
        }
        1 => Ok(content.replacen(before, after, 1)),
        n => {
            let mut msg = format!(
                "Found {} matches. Please provide more context to identify a unique match:\n",
                n
            );

            for (i, (pos, _)) in matches.iter().enumerate().take(2) {
                let line_num = count_lines_before(content, *pos);
                let context = get_line_context(content, line_num, 1);
                msg.push_str(&format!(
                    "\nMatch {} (line {}):\n```\n{}\n```",
                    i + 1,
                    line_num,
                    context
                ));
            }

            if n > 2 {
                msg.push_str(&format!("\n\n...and {} more", n - 2));
            }

            Err(msg)
        }
    }
}

pub fn resolve_path(path: &str, working_dir: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        working_dir
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
            .join(path)
    }
}

/// Guards file-reading / editing paths against URIs being passed in.
///
/// Hosts that expose both a filesystem reader and an MCP resource reader
/// can trip over `skill://…`, `github://…`, and other scheme-prefixed
/// strings: the model sometimes tries the filesystem reader on them, and
/// `PathBuf::from` silently produces a bogus relative path under cwd.
/// See `Skills SEP host implementation guidelines.md` for the recorded
/// pitfall. Detect the `<scheme>://` shape and redirect the model toward
/// `read_resource` / `load_skill` with a clear error.
pub fn reject_uri_path(path: &str) -> Option<CallToolResult> {
    if !crate::agents::platform_extensions::looks_like_uri(path) {
        return None;
    }

    Some(CallToolResult::error(vec![Content::text(format!(
        "'{}' is an MCP resource URI, not a filesystem path. Use the read_resource tool (it takes server + uri) for raw URIs, or load_skill for named skills.",
        path
    ))
    .with_priority(0.0)]))
}

fn count_lines_before(content: &str, byte_pos: usize) -> usize {
    content
        .char_indices()
        .take_while(|(i, _)| *i < byte_pos)
        .filter(|(_, c)| *c == '\n')
        .count()
        + 1
}

fn get_line_context(content: &str, target_line: usize, context: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = target_line.saturating_sub(context + 1);
    let end = (target_line + context).min(lines.len());

    lines[start..end].join("\n")
}

fn find_similar_context(content: &str, search: &str) -> Option<String> {
    let first_line = search.lines().next()?.trim();
    if first_line.is_empty() {
        return None;
    }

    for (i, line) in content.lines().enumerate() {
        if line.contains(first_line) || first_line.contains(line.trim()) {
            return Some(get_line_context(content, i + 1, 2));
        }
    }

    None
}

fn build_file_preview(content: &str, max_lines: usize) -> String {
    if content.is_empty() {
        return "(file is empty)".to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    let preview_end = lines.len().min(max_lines);
    let mut preview = lines[..preview_end]
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{:>4}: {}", index + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    if lines.len() > preview_end {
        preview.push_str(&format!("\n... ({} more lines)", lines.len() - preview_end));
    }

    preview
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawContent;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn extract_text(result: &CallToolResult) -> &str {
        match &result.content[0].raw {
            RawContent::Text(text) => &text.text,
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_reject_uri_path_rejects_schemed_inputs() {
        // Recognizes the canonical `skill://` scheme as well as domain-
        // native schemes that the SEP explicitly permits.
        assert!(reject_uri_path("skill://pull-requests/SKILL.md").is_some());
        assert!(reject_uri_path("github://owner/repo/skills/x/SKILL.md").is_some());
        assert!(reject_uri_path("repo://foo").is_some());
        assert!(reject_uri_path("https://example.com/a").is_some());
        // RFC 3986 allows +, -, . in scheme; all should be recognized.
        assert!(reject_uri_path("svn+ssh://host/path").is_some());
    }

    #[test]
    fn test_reject_uri_path_passes_through_filesystem_paths() {
        assert!(reject_uri_path("/absolute/path").is_none());
        assert!(reject_uri_path("C:\\Users\\me\\file.txt").is_none());
        assert!(reject_uri_path("relative/path.md").is_none());
        // A literal colon in the name is not enough — we require `://`.
        assert!(reject_uri_path("my:file").is_none());
        // `://` without a valid scheme prefix is passed through.
        assert!(reject_uri_path("://foo").is_none());
        assert!(reject_uri_path("1bad://foo").is_none());
    }

    #[test]
    fn test_file_write_new() {
        let dir = setup();
        let path = dir.path().join("new_file.txt");
        let tools = EditTools::new();

        let result = tools.file_write(FileWriteParams {
            path: path.to_string_lossy().to_string(),
            content: "Hello, world!\nLine 2".to_string(),
        });

        assert!(!result.is_error.unwrap_or(false));
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "Hello, world!\nLine 2");
    }

    #[test]
    fn test_file_write_overwrite() {
        let dir = setup();
        let path = dir.path().join("existing.txt");
        fs::write(&path, "old content").unwrap();
        let tools = EditTools::new();

        let result = tools.file_write(FileWriteParams {
            path: path.to_string_lossy().to_string(),
            content: "new content".to_string(),
        });

        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    #[test]
    fn test_file_write_creates_dirs() {
        let dir = setup();
        let path = dir.path().join("a/b/c/file.txt");
        let tools = EditTools::new();

        let result = tools.file_write(FileWriteParams {
            path: path.to_string_lossy().to_string(),
            content: "nested".to_string(),
        });

        assert!(!result.is_error.unwrap_or(false));
        assert!(path.exists());
    }

    #[test]
    fn test_file_edit_single_match() {
        let dir = setup();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "fn foo() {\n    println!(\"hello\");\n}").unwrap();
        let tools = EditTools::new();

        let result = tools.file_edit(FileEditParams {
            path: path.to_string_lossy().to_string(),
            before: "println!(\"hello\");".to_string(),
            after: "println!(\"world\");".to_string(),
        });

        assert!(!result.is_error.unwrap_or(false));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("println!(\"world\");"));
        assert!(!content.contains("println!(\"hello\");"));
    }

    #[test]
    fn test_file_edit_no_match() {
        let dir = setup();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "some content").unwrap();
        let tools = EditTools::new();

        let result = tools.file_edit(FileEditParams {
            path: path.to_string_lossy().to_string(),
            before: "nonexistent".to_string(),
            after: "replacement".to_string(),
        });

        assert!(result.is_error.unwrap_or(false));
        let text = extract_text(&result);
        assert!(text.contains("No match found"));
        assert!(text.contains("File preview:"));
        assert!(text.contains("some content"));
    }

    #[test]
    fn test_file_edit_multiple_matches() {
        let dir = setup();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "foo\nbar\nfoo\nbaz").unwrap();
        let tools = EditTools::new();

        let result = tools.file_edit(FileEditParams {
            path: path.to_string_lossy().to_string(),
            before: "foo".to_string(),
            after: "qux".to_string(),
        });

        assert!(result.is_error.unwrap_or(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "foo\nbar\nfoo\nbaz");
    }

    #[test]
    fn test_file_edit_delete() {
        let dir = setup();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "keep\ndelete me\nkeep").unwrap();
        let tools = EditTools::new();

        let result = tools.file_edit(FileEditParams {
            path: path.to_string_lossy().to_string(),
            before: "\ndelete me".to_string(),
            after: "".to_string(),
        });

        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep\nkeep");
    }

    #[test]
    fn test_file_write_resolves_relative_paths_from_working_dir() {
        let dir = setup();
        let tools = EditTools::new();

        let result = tools.file_write_with_cwd(
            FileWriteParams {
                path: "relative.txt".to_string(),
                content: "relative write".to_string(),
            },
            Some(dir.path()),
        );

        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(
            fs::read_to_string(dir.path().join("relative.txt")).unwrap(),
            "relative write"
        );
    }

    #[test]
    fn test_file_edit_resolves_relative_paths_from_working_dir() {
        let dir = setup();
        fs::write(dir.path().join("relative-edit.txt"), "before").unwrap();
        let tools = EditTools::new();

        let result = tools.file_edit_with_cwd(
            FileEditParams {
                path: "relative-edit.txt".to_string(),
                before: "before".to_string(),
                after: "after".to_string(),
            },
            Some(dir.path()),
        );

        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(
            fs::read_to_string(dir.path().join("relative-edit.txt")).unwrap(),
            "after"
        );
    }
}
