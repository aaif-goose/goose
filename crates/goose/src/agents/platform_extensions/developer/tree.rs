use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use ignore::{DirEntry, WalkBuilder};
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LineCount {
    lines: usize,
    is_partial: bool,
}

#[derive(Default)]
struct DirectoryNode {
    dirs: BTreeMap<String, DirectoryNode>,
    files: BTreeMap<String, FileMeasure>,
    total_lines: LineCount,
}

/// What a listing knows about a file: a line count from reading it, or - for a cloud placeholder
/// that was deliberately not read - the size the directory entry already advertised.
enum FileMeasure {
    Lines(LineCount),
    Placeholder(u64),
}

impl DirectoryNode {
    fn insert_dir(&mut self, components: &[String]) {
        let mut node = self;
        for component in components {
            node = node.dirs.entry(component.clone()).or_default();
        }
    }

    fn insert_file(&mut self, components: &[String], measure: FileMeasure) {
        if components.is_empty() {
            return;
        }

        let mut node = self;
        for component in &components[..components.len() - 1] {
            node = node.dirs.entry(component.clone()).or_default();
        }

        let filename = components[components.len() - 1].clone();
        node.files.insert(filename, measure);
    }

    fn compute_total_lines(&mut self) -> LineCount {
        let mut total = LineCount::default();
        for dir in self.dirs.values_mut() {
            let count = dir.compute_total_lines();
            total.lines += count.lines;
            total.is_partial |= count.is_partial;
        }
        for measure in self.files.values() {
            if let FileMeasure::Lines(count) = measure {
                total.lines += count.lines;
                total.is_partial |= count.is_partial;
            }
        }
        self.total_lines = total;
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

        for (name, measure) in &self.files {
            let rendered = match measure {
                FileMeasure::Lines(lines) => format_lines(*lines),
                FileMeasure::Placeholder(bytes) => format!("[cloud-only, {}]", format_size(*bytes)),
            };
            out.push_str(&format!("{}{}  {}\n", indent, name, rendered));
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
            tree.insert_file(&components, measure_file(&entry));
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

/// Procfs pseudo-files such as `pagemap` often report size 0 but yield a huge
/// stream when read, so line counting must have a byte limit.
const LINE_COUNT_MAX_BYTES: usize = 1024 * 1024;
const LINE_COUNT_BUF_SIZE: usize = 64 * 1024;
const UTF8_MAX_EXTRA_BYTES: usize = 3;

fn count_file_lines(path: &Path) -> LineCount {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return LineCount::default(),
    };

    let read_limit = (LINE_COUNT_MAX_BYTES + UTF8_MAX_EXTRA_BYTES) as u64;
    let mut reader = io::BufReader::with_capacity(LINE_COUNT_BUF_SIZE, file.take(read_limit));
    let mut bytes = Vec::new();
    if reader.read_to_end(&mut bytes).is_err() {
        return LineCount::default();
    }

    if bytes.len() <= LINE_COUNT_MAX_BYTES {
        return std::str::from_utf8(&bytes)
            .map(|content| LineCount {
                lines: content.lines().count(),
                is_partial: false,
            })
            .unwrap_or_default();
    }

    let prefix = &bytes[..LINE_COUNT_MAX_BYTES];
    let content = match std::str::from_utf8(prefix) {
        Ok(content) => content,
        Err(err) if err.error_len().is_some() => return LineCount::default(),
        Err(err) => {
            let valid_up_to = err.valid_up_to();
            let character_width = match bytes[valid_up_to] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return LineCount::default(),
            };
            let character_end = valid_up_to + character_width;
            if character_end > bytes.len()
                || std::str::from_utf8(&bytes[valid_up_to..character_end]).is_err()
            {
                return LineCount::default();
            }
            std::str::from_utf8(&bytes[..valid_up_to]).unwrap()
        }
    };

    LineCount {
        lines: content.lines().count(),
        is_partial: true,
    }
}

// OneDrive Files On-Demand, iCloud Drive and other File Provider backends leave "cloud-only"
// files on disk as placeholders that hold no data. Enumerating a directory and stat-ing its
// entries sees them for free, but OPENING one for data blocks while the provider downloads the
// whole file. Counting lines opens every file in the walk, so listing a synced folder used to
// pull the entire remote tree down - unprompted, since `tree` is annotated read-only and so
// never asks for approval. Report the size the placeholder already advertises instead.
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
const SF_DATALESS: u32 = 0x4000_0000;

// Both predicates compile everywhere so both stay unit-testable on any host; only the accessor
// that feeds them is platform-gated.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_attributes_are_placeholder(attributes: u32) -> bool {
    let mask = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
    attributes & mask != 0
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn macos_flags_are_placeholder(flags: u32) -> bool {
    flags & SF_DATALESS != 0
}

#[cfg(windows)]
fn is_cloud_placeholder(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    windows_attributes_are_placeholder(metadata.file_attributes())
}

#[cfg(target_os = "macos")]
fn is_cloud_placeholder(metadata: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    macos_flags_are_placeholder(metadata.st_flags())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn is_cloud_placeholder(_metadata: &fs::Metadata) -> bool {
    false
}

fn measure_file(entry: &DirEntry) -> FileMeasure {
    measure_file_with(entry, is_cloud_placeholder)
}

// The placeholder verdict is a parameter so a test can assert the read is skipped: a stub that
// answers "placeholder" can only produce Placeholder if nothing opened the file.
fn measure_file_with(
    entry: &DirEntry,
    is_placeholder: impl Fn(&fs::Metadata) -> bool,
) -> FileMeasure {
    let Ok(metadata) = entry.metadata() else {
        return FileMeasure::Lines(LineCount::default());
    };

    if is_placeholder(&metadata) {
        return FileMeasure::Placeholder(metadata.len());
    }

    FileMeasure::Lines(count_file_lines(entry.path()))
}

fn format_lines(count: LineCount) -> String {
    let partial = if count.is_partial { "+" } else { "" };
    if count.lines >= 1000 {
        format!("[{}K{}]", count.lines / 1000, partial)
    } else {
        format!("[{}{}]", count.lines, partial)
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
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
        // The bracket a local file renders is part of the contract, so assert it here and not
        // just the name: a placeholder verdict that answered "yes" for every file would print
        // `[cloud-only, ...]` on each line and zero every directory total.
        assert!(text.contains("main.rs  [1]"), "{text}");
        assert!(text.contains("src/  [2]"), "{text}");
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
    fn placeholder_predicates_match_each_provider_flag() {
        // A cloud-only file carries at least one of these; a normal file carries none of them.
        assert!(windows_attributes_are_placeholder(
            FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
        ));
        assert!(windows_attributes_are_placeholder(FILE_ATTRIBUTE_OFFLINE));
        assert!(windows_attributes_are_placeholder(
            FILE_ATTRIBUTE_RECALL_ON_OPEN
        ));
        assert!(macos_flags_are_placeholder(SF_DATALESS));

        // FILE_ATTRIBUTE_ARCHIVE | FILE_ATTRIBUTE_NORMAL - an ordinary local file.
        assert!(!windows_attributes_are_placeholder(0x20 | 0x80));
        assert!(!windows_attributes_are_placeholder(0));
        // Archive | SparseFile | ReparsePoint: a reparse point alone is not a placeholder.
        assert!(!windows_attributes_are_placeholder(0x620));
        // The word measured on a OneDrive cloud-only file: those three plus OFFLINE and
        // RECALL_ON_DATA_ACCESS.
        assert!(windows_attributes_are_placeholder(0x40_1620));
        assert!(!macos_flags_are_placeholder(0));
        // UF_HIDDEN is not SF_DATALESS.
        assert!(!macos_flags_are_placeholder(0x8000));
        // UF_COMPRESSED | UF_TRACKED: the word a locally-present file carries inside an
        // iCloud root (a dataless one there measured 0x40000060, these plus SF_DATALESS).
        assert!(!macos_flags_are_placeholder(0x60));
    }

    #[test]
    fn cloud_only_placeholders_render_as_size() {
        // Constructed directly: creating a real placeholder needs a File Provider backend.
        let mut node = DirectoryNode::default();
        node.insert_file(
            &["archive.zip".to_string()],
            FileMeasure::Placeholder(323 * 1024 * 1024),
        );
        node.compute_total_lines();

        let mut out = String::new();
        node.render_into(0, &mut out);

        assert!(out.contains("archive.zip  [cloud-only, 323.0 MB]"), "{out}");
        assert_eq!(node.total_lines, LineCount::default());
    }

    #[test]
    fn a_file_judged_a_placeholder_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("three_lines.txt");
        fs::write(&path, "a\nb\nc\n").unwrap();

        let entry = WalkBuilder::new(&path)
            .build()
            .flatten()
            .find(|entry| entry.file_type().is_some_and(|t| t.is_file()))
            .expect("the walk yields the file");

        // Stubbing the verdict is what makes the skip observable without a File Provider
        // backend: had measure_file_with reached the read, it would count three lines.
        match measure_file_with(&entry, |_| true) {
            FileMeasure::Placeholder(bytes) => assert_eq!(bytes, 6),
            FileMeasure::Lines(count) => panic!("the file was read: counted {} lines", count.lines),
        }

        // The same entry with a local-file verdict still gets its line count, so the gate skips
        // placeholders rather than everything.
        match measure_file_with(&entry, |_| false) {
            FileMeasure::Lines(count) => {
                assert_eq!(count.lines, 3);
                assert!(!count.is_partial);
            }
            FileMeasure::Placeholder(bytes) => panic!("a local file was skipped: {bytes} B"),
        }
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
        assert_eq!(
            count_file_lines(&path),
            LineCount {
                lines: 3,
                is_partial: false
            }
        );
        fs::write(&path, "a\nb\nc").unwrap();
        assert_eq!(
            count_file_lines(&path),
            LineCount {
                lines: 3,
                is_partial: false
            }
        );
        fs::write(&path, "").unwrap();
        assert_eq!(count_file_lines(&path), LineCount::default());
    }

    #[test]
    fn count_file_lines_invalid_utf8_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        fs::write(&path, [0xff, 0xfe, 0xfd, b'\n']).unwrap();
        assert_eq!(count_file_lines(&path), LineCount::default());
    }

    #[test]
    fn count_file_lines_incomplete_utf8_below_cap_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.txt");
        fs::write(&path, b"a\n\xc3").unwrap();
        assert_eq!(count_file_lines(&path), LineCount::default());
    }

    #[test]
    fn count_file_lines_incomplete_utf8_at_exact_cap_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.txt");
        let mut data = vec![b'a'; LINE_COUNT_MAX_BYTES];
        data[LINE_COUNT_MAX_BYTES - 1] = 0xc3;
        fs::write(&path, data).unwrap();
        assert_eq!(count_file_lines(&path), LineCount::default());
    }

    #[test]
    fn count_file_lines_cap_splitting_multibyte_counts_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("split.txt");
        let cap = LINE_COUNT_MAX_BYTES;
        let mut data = Vec::with_capacity(cap + 4);
        while data.len() + 2 < cap {
            data.extend_from_slice(b"x\n");
        }
        while data.len() < cap - 1 {
            data.push(b'x');
        }
        let expected = data
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .count();
        data.extend_from_slice("🦀\n".as_bytes());
        fs::write(&path, &data).unwrap();
        assert_eq!(
            count_file_lines(&path),
            LineCount {
                lines: expected,
                is_partial: true
            }
        );
    }

    #[test]
    fn count_file_lines_invalid_utf8_across_cap_returns_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.txt");
        let mut data = vec![b'a'; LINE_COUNT_MAX_BYTES - 1];
        data.extend_from_slice(b"\xc3x");
        fs::write(&path, data).unwrap();
        assert_eq!(count_file_lines(&path), LineCount::default());
    }

    #[test]
    fn count_file_lines_respects_byte_cap() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        let line = b"x\n";
        let lines_to_write = (LINE_COUNT_MAX_BYTES / line.len()) + 500;
        {
            let mut f = fs::File::create(&path).unwrap();
            for _ in 0..lines_to_write {
                f.write_all(line).unwrap();
            }
        }

        let counted = count_file_lines(&path);
        let max_lines = LINE_COUNT_MAX_BYTES / line.len();
        assert!(counted.lines > 0, "expected some lines within the cap");
        assert!(
            counted.lines <= max_lines,
            "counted {} lines, cap allows at most {max_lines}",
            counted.lines
        );
        assert!(
            counted.lines < lines_to_write,
            "cap should stop before reading the whole {lines_to_write}-line file"
        );
        assert!(counted.is_partial);
    }

    #[test]
    fn tree_marks_capped_counts_partial() {
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("src");
        fs::create_dir(&source_dir).unwrap();
        fs::write(
            source_dir.join("big.txt"),
            "x\n".repeat((LINE_COUNT_MAX_BYTES / 2) + 1),
        )
        .unwrap();

        let result = TreeTool::new().tree(TreeParams {
            path: dir.path().display().to_string(),
            depth: 2,
        });

        let text = extract_text(&result);
        assert!(text.contains("src/  [524K+]"));
        assert!(text.contains("big.txt  [524K+]"));
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
