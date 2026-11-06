use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use ignore::WalkBuilder;
use rmcp::model::{CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeParams {
    pub path: String,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    2
}

pub struct TreeTool;

impl TreeTool {
    pub fn new() -> Self {
        Self
    }

    pub fn tree(&self, params: TreeParams) -> CallToolResult {
        let root = PathBuf::from(&params.path);
        self.tree_at(root, params.depth)
    }

    pub fn tree_with_cwd(&self, params: TreeParams, working_dir: Option<&Path>) -> CallToolResult {
        let path = PathBuf::from(&params.path);
        let root = if path.is_absolute() {
            path
        } else {
            working_dir
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."))
                .join(path)
        };
        self.tree_at(root, params.depth)
    }

    fn tree_at(&self, root: PathBuf, depth: u32) -> CallToolResult {
        if !root.exists() {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Path does not exist: {}",
                root.display()
            ))]);
        }

        if !root.is_dir() {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Path is not a directory: {}",
                root.display()
            ))]);
        }

        let max_depth = if depth == 0 {
            None
        } else {
            Some(depth as usize)
        };

        let mut tree = collect_tree(&root, max_depth);
        tree.compute_total_lines();

        let mut output = String::new();
        tree.render_into(0, &mut output);
        if output.is_empty() {
            output.push_str("(empty directory)");
        }

        CallToolResult::success(vec![ContentBlock::text(output)])
    }
}

impl Default for TreeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct DirectoryNode {
    dirs: BTreeMap<String, DirectoryNode>,
    files: BTreeMap<String, usize>,
    total_lines: usize,
}

impl DirectoryNode {
    fn insert_dir(&mut self, components: &[String]) {
        let mut node = self;
        for component in components {
            node = node.dirs.entry(component.clone()).or_default();
        }
    }

    fn insert_file(&mut self, components: &[String], line_count: usize) {
        if components.is_empty() {
            return;
        }

        let mut node = self;
        for component in &components[..components.len() - 1] {
            node = node.dirs.entry(component.clone()).or_default();
        }

        let filename = components[components.len() - 1].clone();
        node.files.insert(filename, line_count);
    }

    fn compute_total_lines(&mut self) -> usize {
        let dir_lines: usize = self
            .dirs
            .values_mut()
            .map(DirectoryNode::compute_total_lines)
            .sum();
        let file_lines: usize = self.files.values().copied().sum();
        self.total_lines = dir_lines + file_lines;
        self.total_lines
    }

    fn render_into(&self, depth: usize, out: &mut String) {
        let indent = "  ".repeat(depth);

        for (name, dir) in &self.dirs {
            out.push_str(&format!(
                "{}{}/  {}\n",
                indent,
                name,
                format_lines(dir.total_lines)
            ));
            dir.render_into(depth + 1, out);
        }

        for (name, line_count) in &self.files {
            out.push_str(&format!(
                "{}{}  {}\n",
                indent,
                name,
                format_lines(*line_count)
            ));
        }
    }
}

fn collect_tree(root: &Path, max_depth: Option<usize>) -> DirectoryNode {
    let mut builder = WalkBuilder::new(root);
    builder.git_ignore(true);
    builder.git_exclude(true);
    builder.git_global(true);
    builder.require_git(false);
    builder.ignore(true);
    builder.hidden(true);

    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth + 1));
    }

    let mut tree = DirectoryNode::default();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }

        let rel = match path.strip_prefix(root) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        let components = match relative_components(rel) {
            Some(components) => components,
            None => continue,
        };

        if entry.file_type().is_some_and(|t| t.is_dir()) {
            tree.insert_dir(&components);
        } else if entry.file_type().is_some_and(|t| t.is_file()) {
            tree.insert_file(&components, count_file_lines(path));
        }
    }

    tree
}

fn relative_components(path: &Path) -> Option<Vec<String>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(value.to_string_lossy().into_owned()),
            _ => return None,
        }
    }

    if components.is_empty() {
        None
    } else {
        Some(components)
    }
}

/// Max bytes read when counting lines for one file.
///
/// Procfs pseudo-files such as `pagemap` often report size 0 but yield a huge
/// stream when read. An uncapped `read_to_string` can OOM the process; a buffer
/// alone can still stall for minutes. Cap the read so `tree` stays bounded.
const LINE_COUNT_MAX_BYTES: u64 = 1024 * 1024; // 1 MiB
const LINE_COUNT_BUF_SIZE: usize = 64 * 1024; // 64 KiB

fn count_file_lines(path: &Path) -> usize {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };

    let mut reader =
        io::BufReader::with_capacity(LINE_COUNT_BUF_SIZE, file.take(LINE_COUNT_MAX_BYTES));
    let mut bytes = Vec::new();
    if reader.read_to_end(&mut bytes).is_err() {
        return 0;
    }

    match std::str::from_utf8(&bytes) {
        Ok(content) => content.lines().count(),
        Err(err) => {
            // Match prior `read_to_string` behavior: invalid UTF-8 → 0 lines.
            // When the byte cap cuts a multi-byte sequence, `error_len` is None;
            // count lines in the valid prefix instead of treating it as binary.
            if err.error_len().is_some() {
                return 0;
            }
            match std::str::from_utf8(&bytes[..err.valid_up_to()]) {
                Ok(content) => content.lines().count(),
                Err(_) => 0,
            }
        }
    }
}

fn format_lines(lines: usize) -> String {
    if lines >= 1000 {
        format!("[{}K]", lines / 1000)
    } else {
        format!("[{}]", lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ContentBlock;
    use tempfile::TempDir;

    fn extract_text(result: &CallToolResult) -> &str {
        match &result.content[0] {
            ContentBlock::Text(t) => &t.text,
            _ => panic!("expected text"),
        }
    }

    fn setup_tree() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
        fs::write(dir.path().join("tests/test.rs"), "#[test]\nfn t() {}\n").unwrap();
        dir
    }

    #[test]
    fn tree_lists_files_and_directories() {
        let dir = setup_tree();
        let tool = TreeTool::new();

        let result = tool.tree(TreeParams {
            path: dir.path().display().to_string(),
            depth: 2,
        });

        let text = extract_text(&result);
        assert!(text.contains("src/"));
        assert!(text.contains("tests/"));
        assert!(text.contains("main.rs"));
    }

    #[test]
    fn tree_respects_depth() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.rs"), "fn deep() {}\n").unwrap();

        let tool = TreeTool::new();
        let result = tool.tree(TreeParams {
            path: dir.path().display().to_string(),
            depth: 1,
        });

        let text = extract_text(&result);
        assert!(text.contains("a/"));
        assert!(text.contains("b/"));
        assert!(!text.contains("deep.rs"));
    }

    #[test]
    fn tree_uses_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored/\n*.log\n").unwrap();
        fs::create_dir_all(dir.path().join("ignored")).unwrap();
        fs::write(dir.path().join("ignored/secret.rs"), "fn secret() {}\n").unwrap();
        fs::write(dir.path().join("visible.rs"), "fn visible() {}\n").unwrap();
        fs::write(dir.path().join("debug.log"), "hidden\n").unwrap();

        let tool = TreeTool::new();
        let result = tool.tree(TreeParams {
            path: dir.path().display().to_string(),
            depth: 2,
        });

        let text = extract_text(&result);
        assert!(text.contains("visible.rs"));
        assert!(!text.contains("ignored"));
        assert!(!text.contains("debug.log"));
    }
    #[test]
    fn count_file_lines_counts_small_utf8_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.txt");
        fs::write(&path, "a\nb\nc\n").unwrap();
        assert_eq!(count_file_lines(&path), 3);
        fs::write(&path, "a\nb\nc").unwrap();
        assert_eq!(count_file_lines(&path), 3);
        fs::write(&path, "").unwrap();
        assert_eq!(count_file_lines(&path), 0);
    }

    #[test]
    fn count_file_lines_invalid_utf8_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        fs::write(&path, [0xff, 0xfe, 0xfd, b'\n']).unwrap();
        assert_eq!(count_file_lines(&path), 0);
    }

    #[test]
    fn count_file_lines_respects_byte_cap() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let line = b"x\n";
        let lines_to_write = (LINE_COUNT_MAX_BYTES as usize / line.len()) + 500;
        {
            let mut f = fs::File::create(&path).unwrap();
            for _ in 0..lines_to_write {
                f.write_all(line).unwrap();
            }
        }

        let counted = count_file_lines(&path);
        let max_lines = (LINE_COUNT_MAX_BYTES as usize) / line.len();
        assert!(counted > 0, "expected some lines within the cap");
        assert!(
            counted <= max_lines,
            "counted {counted} lines, cap allows at most {max_lines}"
        );
        assert!(
            counted < lines_to_write,
            "cap should stop before reading the whole {lines_to_write}-line file"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn count_file_lines_proc_pagemap_finishes_quickly() {
        use std::time::{Duration, Instant};

        let path = Path::new("/proc/self/pagemap");
        if !path.exists() {
            return;
        }

        let start = Instant::now();
        let _count = count_file_lines(path);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "pagemap line count took {:?}; expected a capped fast read",
            start.elapsed()
        );
    }
}
