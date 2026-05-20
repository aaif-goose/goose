//! Streaming markdown buffer for safe incremental rendering.
//!
//! This module provides a buffer that accumulates streaming markdown chunks
//! and determines safe points to flush content for rendering. It tracks
//! open markdown constructs (code blocks, bold, links, etc.) to ensure
//! we only output complete, well-formed markdown.
//!
//! # Example
//!
//! ```
//! use goose_cli::session::streaming_buffer::MarkdownBuffer;
//!
//! let mut buf = MarkdownBuffer::new();
//!
//! // Partial bold - buffers until closed
//! assert_eq!(buf.push("Hello **wor"), Some("Hello ".to_string()));
//! assert_eq!(buf.push("ld**!"), Some("**world**!".to_string()));
//!
//! // At end of stream, flush remaining content
//! let remaining = buf.flush();
//! ```

use regex::Regex;
use std::io::Write;
use std::sync::LazyLock;

const DEFAULT_MAX_CODE_BLOCK_LINES: usize = 50;
const DEFAULT_TRUNCATED_SHOW_LINES: usize = 20;

/// Parse a line-count env value, rejecting anything that isn't a positive
/// integer. Zero is invalid because it would hide every non-empty code block
/// behind a temp-file pointer.
fn parse_positive_lines(value: &str) -> Option<usize> {
    value.parse::<usize>().ok().filter(|&n| n > 0)
}

fn max_code_block_lines() -> Option<usize> {
    static VALUE: LazyLock<Option<usize>> = LazyLock::new(|| {
        if std::env::var("GOOSE_NO_CODE_TRUNCATION")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return None;
        }
        Some(
            std::env::var("GOOSE_MAX_CODE_BLOCK_LINES")
                .ok()
                .and_then(|v| parse_positive_lines(&v))
                .unwrap_or(DEFAULT_MAX_CODE_BLOCK_LINES),
        )
    });
    *VALUE
}

fn truncated_show_lines() -> usize {
    static VALUE: LazyLock<usize> = LazyLock::new(|| {
        std::env::var("GOOSE_TRUNCATED_SHOW_LINES")
            .ok()
            .and_then(|v| parse_positive_lines(&v))
            .unwrap_or(DEFAULT_TRUNCATED_SHOW_LINES)
    });
    *VALUE
}

fn truncate_code_blocks(content: &str) -> String {
    let Some(max_lines) = max_code_block_lines() else {
        return content.to_string();
    };
    truncate_code_blocks_with(content, max_lines, truncated_show_lines())
}

fn truncate_code_blocks_with(content: &str, max_lines: usize, show_lines: usize) -> String {
    let Some((open_pos, fence_char, fence_len)) = find_opening_fence(content) else {
        return content.to_string();
    };

    let after_fence = open_pos + fence_len;
    let Some(after_open) = content.get(after_fence..) else {
        return content.to_string();
    };
    let Some(newline_pos) = after_open.find('\n') else {
        return content.to_string();
    };
    let code_start = after_fence + newline_pos + 1;

    let Some(code_region) = content.get(code_start..) else {
        return content.to_string();
    };
    let Some(close_offset) = find_closing_fence(code_region, fence_char, fence_len) else {
        return content.to_string();
    };

    let Some(code_content) = code_region.get(..close_offset) else {
        return content.to_string();
    };
    let lines: Vec<&str> = code_content.lines().collect();

    if lines.len() <= max_lines {
        return content.to_string();
    }

    let show_lines = show_lines.min(max_lines).min(lines.len());
    let truncated: String = lines
        .iter()
        .take(show_lines)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let remaining = lines.len() - show_lines;

    let file_msg = save_to_temp_file(code_content)
        .map(|p| format!(" → {}", p))
        .unwrap_or_default();

    let close_pos = code_start + close_offset + 1; // +1 to include the \n
    let prefix = content.get(..code_start).unwrap_or("");
    let suffix = content.get(close_pos..).unwrap_or("");
    format!(
        "{}{}\n... ({} more lines{})\n{}",
        prefix, truncated, remaining, file_msg, suffix
    )
}

/// Find the first opening code fence in `content`.
///
/// Returns the byte offset of the fence, the fence character (`` ` `` or `~`),
/// and the actual run length (≥ 3 consecutive characters). The run length is
/// needed so the matching closing fence can be located even when an inner
/// fence of a shorter length appears inside the block.
// SAFETY: `pos` comes from `str::find`, which returns a char-boundary byte
// offset. Fence chars (`` ` `` and `~`) are ASCII, so the slice always starts
// on a char boundary.
#[allow(clippy::string_slice)]
fn find_opening_fence(content: &str) -> Option<(usize, char, usize)> {
    let (pos, ch) = match (content.find("```"), content.find("~~~")) {
        (Some(a), Some(b)) if a <= b => (a, '`'),
        (Some(a), None) => (a, '`'),
        (None, Some(b)) => (b, '~'),
        (Some(_), Some(b)) => (b, '~'),
        (None, None) => return None,
    };
    let len = content[pos..].chars().take_while(|&c| c == ch).count();
    Some((pos, ch, len))
}

/// Find the closing fence for a block opened with `min_len` `fence_char`
/// characters. A closing fence is a line whose only non-whitespace content
/// is a run of at least `min_len` matching fence characters.
///
/// Returns the offset (within `region`) of the newline preceding the closing
/// fence line, matching the offset semantics that the rest of
/// `truncate_code_blocks_with` expects.
// SAFETY: All slice indices are at char boundaries:
// - `search_from` starts at 0 and only advances to `line_start = nl_pos + 1`,
//   where `nl_pos` is a `\n` byte (ASCII, single byte).
// - `fence_count` is `chars().take_while(== fence_char).count()`; fence chars
//   are ASCII, so the char count equals the byte length.
// - `line_end` comes from `find('\n')` (ASCII boundary) or `after_fence.len()`.
#[allow(clippy::string_slice)]
fn find_closing_fence(region: &str, fence_char: char, min_len: usize) -> Option<usize> {
    let mut search_from = 0;
    while let Some(nl_rel) = region[search_from..].find('\n') {
        let nl_pos = search_from + nl_rel;
        let line_start = nl_pos + 1;
        let line = region.get(line_start..)?;
        let fence_count = line.chars().take_while(|&c| c == fence_char).count();
        if fence_count >= min_len {
            let after_fence = &line[fence_count..];
            let line_end = after_fence.find('\n').unwrap_or(after_fence.len());
            if after_fence[..line_end].trim().is_empty() {
                return Some(nl_pos);
            }
        }
        search_from = line_start;
    }
    None
}

fn save_to_temp_file(content: &str) -> Option<String> {
    let mut file = tempfile::Builder::new()
        .prefix("goose-")
        .suffix(".txt")
        .tempfile()
        .ok()?;

    file.write_all(content.as_bytes()).ok()?;

    // Keep the file (don't delete on drop) and get the path
    let (_, path) = file.keep().ok()?;
    Some(path.display().to_string())
}

/// Regex that tokenizes markdown inline elements.
/// Order matters: longer/more-specific patterns first.
static INLINE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(",
        r"\\.",                 // Escaped char (highest priority)
        r"|`+",                 // Inline code (variable length backticks)
        r"|\*\*\*",             // Bold+italic
        r"|\*\*",               // Bold
        r"|\*",                 // Italic
        r"|___",                // Bold+italic (underscore)
        r"|__",                 // Bold (underscore)
        r"|_",                  // Italic (underscore)
        r"|~~",                 // Strikethrough
        r"|\!\[",               // Image start
        r"|\]\(",               // Link URL start
        r"|\[",                 // Link text start
        r"|\]",                 // Bracket close (without following paren)
        r"|\)",                 // Link URL end
        r"|[^\\\*_`~\[\]!()]+", // Plain text (no special chars)
        r"|.",                  // Any other single char
        r")"
    ))
    .unwrap()
});

/// A streaming markdown buffer that tracks open constructs.
///
/// Accumulates chunks and returns content that is safe to render,
/// holding back any incomplete markdown constructs. Large code blocks
/// are automatically truncated with full content saved to a temp file.
#[derive(Default)]
pub struct MarkdownBuffer {
    buffer: String,
}

/// Tracks the current parsing state for markdown constructs.
#[derive(Default, Debug, Clone, PartialEq)]
struct ParseState {
    in_code_block: bool,
    code_fence_char: char,
    code_fence_len: usize,
    in_table: bool,
    pending_heading: bool,
    in_inline_code: bool,
    inline_code_len: usize,
    in_bold: bool,
    in_italic: bool,
    in_strikethrough: bool,
    in_link_text: bool,
    in_link_url: bool,
    in_image_alt: bool,
}

impl ParseState {
    /// Returns true if no markdown constructs are currently open.
    fn is_clean(&self) -> bool {
        !self.in_code_block
            && !self.in_table
            && !self.pending_heading
            && !self.in_inline_code
            && !self.in_bold
            && !self.in_italic
            && !self.in_strikethrough
            && !self.in_link_text
            && !self.in_link_url
            && !self.in_image_alt
    }
}

// SAFETY: All string slicing in this impl is safe because:
// - We only slice at positions derived from ASCII characters (newlines, #, |, etc.)
// - The regex tokenizer operates on valid UTF-8 and returns byte positions at char boundaries
// - Code fence detection uses chars().take_while() which respects UTF-8
#[allow(clippy::string_slice)]
impl MarkdownBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a chunk of markdown text to the buffer.
    ///
    /// Returns any content that is safe to render, or None if the buffer
    /// contains only incomplete constructs. Large code blocks are automatically
    /// truncated with full content saved to a temp file.
    pub fn push(&mut self, chunk: &str) -> Option<String> {
        self.buffer.push_str(chunk);
        let safe_end = self.find_safe_end();

        if safe_end > 0 {
            // SAFETY: safe_end is always at a valid UTF-8 char boundary because:
            // - We only set it after processing complete regex tokens (which match
            //   valid UTF-8 sequences) or at newline positions (ASCII, single byte)
            // - The regex tokenizer operates on &str which guarantees UTF-8
            let to_render = self.buffer[..safe_end].to_string();
            self.buffer = self.buffer[safe_end..].to_string();
            Some(truncate_code_blocks(&to_render))
        } else {
            None
        }
    }

    /// Flush any remaining content from the buffer.
    ///
    /// Call this at the end of a stream to get any buffered content,
    /// even if markdown constructs are unclosed.
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.buffer)
    }

    /// Find the last byte position where the parse state is "clean".
    fn find_safe_end(&self) -> usize {
        let mut state = ParseState::default();
        let mut last_safe: usize = 0;
        let bytes = self.buffer.as_bytes();
        let len = bytes.len();
        let mut pos: usize = 0;

        while pos < len {
            let at_line_start = pos == 0 || bytes[pos - 1] == b'\n';

            if at_line_start {
                if let Some(new_pos) = self.process_line_start(&mut state, pos) {
                    pos = new_pos;
                    if state.is_clean() {
                        last_safe = pos;
                    }
                    continue;
                }
            }

            if state.in_code_block {
                while pos < len && bytes[pos] != b'\n' {
                    pos += 1;
                }
                if pos < len {
                    pos += 1;
                }
                continue;
            }

            let remaining = &self.buffer[pos..];
            let line_end = remaining.find('\n').map(|i| pos + i + 1).unwrap_or(len);
            let line_content = &self.buffer[pos..line_end];

            for cap in INLINE_TOKEN_RE.find_iter(line_content) {
                let token = cap.as_str();
                let token_end = pos + cap.end();

                self.process_inline_token(&mut state, token);

                if state.is_clean() {
                    last_safe = token_end;
                }
            }

            if line_end <= len && line_end > pos && bytes[line_end - 1] == b'\n' {
                state.pending_heading = false;
                if state.is_clean() {
                    last_safe = line_end;
                }
            }

            pos = line_end;
        }

        last_safe
    }

    /// Process block-level constructs at the start of a line.
    ///
    /// Returns the new position after processing, or None if no block construct found.
    fn process_line_start(&self, state: &mut ParseState, pos: usize) -> Option<usize> {
        let remaining = &self.buffer[pos..];

        if state.pending_heading {
            state.pending_heading = false;
        }

        if let Some(fence_result) = self.check_code_fence(remaining, state) {
            return Some(pos + fence_result);
        }

        if state.in_code_block {
            return None;
        }

        if remaining.starts_with('#') {
            let hashes = remaining.chars().take_while(|&c| c == '#').count();
            if hashes <= 6 {
                let after_hashes = &remaining[hashes..];
                if after_hashes.is_empty()
                    || after_hashes.starts_with(' ')
                    || after_hashes.starts_with('\n')
                {
                    state.pending_heading = true;
                    return None;
                }
            }
        }

        if remaining.starts_with('|') {
            state.in_table = true;
            return None;
        }

        if (remaining.starts_with('\n') || remaining.is_empty()) && state.in_table {
            state.in_table = false;
            return Some(pos + 1);
        }

        if state.in_table && !remaining.starts_with('|') {
            state.in_table = false;
        }

        None
    }

    /// Check for a code fence and update state accordingly.
    ///
    /// Returns the position after the fence line if found, None otherwise.
    fn check_code_fence(&self, line: &str, state: &mut ParseState) -> Option<usize> {
        let trimmed = line.trim_start();

        let fence_char = trimmed.chars().next()?;
        if fence_char != '`' && fence_char != '~' {
            return None;
        }

        let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
        if fence_len < 3 {
            return None;
        }

        let after_fence = &trimmed[fence_len..];

        if state.in_code_block {
            if fence_char == state.code_fence_char
                && fence_len >= state.code_fence_len
                && (after_fence.is_empty()
                    || after_fence.starts_with('\n')
                    || after_fence.trim().is_empty())
            {
                state.in_code_block = false;
                state.code_fence_char = '\0';
                state.code_fence_len = 0;

                if let Some(newline_pos) = line.find('\n') {
                    return Some(newline_pos + 1);
                } else {
                    return Some(line.len());
                }
            }
        } else {
            state.in_code_block = true;
            state.code_fence_char = fence_char;
            state.code_fence_len = fence_len;

            if let Some(newline_pos) = line.find('\n') {
                return Some(newline_pos + 1);
            } else {
                return Some(line.len());
            }
        }

        None
    }

    /// Process an inline token and update state.
    fn process_inline_token(&self, state: &mut ParseState, token: &str) {
        if token.starts_with('\\') && token.len() == 2 {
            return;
        }

        if token.starts_with('`') {
            let tick_count = token.len();
            if state.in_inline_code {
                if tick_count == state.inline_code_len {
                    state.in_inline_code = false;
                    state.inline_code_len = 0;
                }
            } else {
                state.in_inline_code = true;
                state.inline_code_len = tick_count;
            }
            return;
        }

        if state.in_inline_code {
            return;
        }

        match token {
            "***" | "___" => {
                if state.in_bold && state.in_italic {
                    state.in_bold = false;
                    state.in_italic = false;
                } else if state.in_bold {
                    state.in_italic = !state.in_italic;
                } else if state.in_italic {
                    state.in_bold = !state.in_bold;
                } else {
                    state.in_bold = true;
                    state.in_italic = true;
                }
            }
            "**" | "__" => {
                state.in_bold = !state.in_bold;
            }
            "*" | "_" => {
                state.in_italic = !state.in_italic;
            }
            "~~" => {
                state.in_strikethrough = !state.in_strikethrough;
            }
            "![" => {
                state.in_image_alt = true;
            }
            "[" => {
                if !state.in_link_text && !state.in_image_alt {
                    state.in_link_text = true;
                }
            }
            "](" => {
                if state.in_link_text {
                    state.in_link_text = false;
                    state.in_link_url = true;
                } else if state.in_image_alt {
                    state.in_image_alt = false;
                    state.in_link_url = true;
                }
            }
            "]" => {}
            ")" => {
                if state.in_link_url {
                    state.in_link_url = false;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    /// Process chunks through the buffer and return all outputs (skipping None, including flush)
    fn stream(chunks: &[&str]) -> Vec<String> {
        let mut buf = MarkdownBuffer::new();
        let mut results: Vec<String> = chunks.iter().filter_map(|chunk| buf.push(chunk)).collect();
        let remaining = buf.flush();
        if !remaining.is_empty() {
            results.push(remaining);
        }
        results
    }

    // ===========================================
    // Realistic LLM streaming scenarios
    // ===========================================

    #[test_case(
        &["I'll", " help", " you", " with", " that", "!"],
        &["I'll", " help", " you", " with", " that", "!"]
        ; "simple sentence streams through immediately without markdown"
    )]
    #[test_case(
        &["Here's the **important", "** part."],
        &["Here's the ", "**important** part."]
        ; "bold split mid-word"
    )]
    #[test_case(
        &["Use the `println!", "` macro."],
        &["Use the ", "`println!` macro."]
        ; "inline code split"
    )]
    #[test_case(
        &["Check [the docs](https://doc", "s.rs) for more."],
        &["Check ", "[the docs](https://docs.rs) for more."]
        ; "link url split"
    )]
    fn test_inline_streaming(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Code blocks (most important for bat rendering)
    // ===========================================

    #[test_case(
        &["```rust\n", "fn main() {\n", "    println!(\"hello\");\n", "}\n", "```\n"],
        &["```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n"]
        ; "rust code block streamed line by line"
    )]
    #[test_case(
        &["Here's an exa", "mple:\n\n```python\nprint(\"``", "`nested```\")\n```\n\nNice!"],
        &["Here's an exa", "mple:\n", "\n```python\nprint(\"```nested```\")\n```\n\nNice!"]
        ; "code block with backticks in string literal"
    )]
    #[test_case(
        &["````md\n", "```\ninner\n```\n", "````\n"],
        &["````md\n```\ninner\n```\n````\n"]
        ; "nested code fence with longer outer fence"
    )]
    #[test_case(
        &["~~~bash\n", "echo 'hello'\n", "~", "~~\n"],
        &["~~~bash\necho 'hello'\n~~~\n"]
        ; "tilde code fence"
    )]
    #[test_case(
        &["```\ncode"],
        &["```\ncode"]
        ; "unclosed code block flushes at end"
    )]
    fn test_code_blocks(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Headings
    // ===========================================

    #[test_case(
        &["# Getting St", "arted\n\nFirst, install..."],
        &["# Getting Started\n\nFirst, install..."]
        ; "heading split mid-word"
    )]
    #[test_case(
        &["## API Reference\n\n###", " Methods\n\n"],
        &["## API Reference\n\n", "### Methods\n\n"]
        ; "multiple headings in one chunk"
    )]
    fn test_headings(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Tables
    // ===========================================

    #[test_case(
        &["| Name | Value |\n", "|------|-------|\n", "| foo  | 42    |\n", "\nMore text"],
        &["| Name | Value |\n|------|-------|\n| foo  | 42    |\n\nMore text"]
        ; "table streamed row by row"
    )]
    #[test_case(
        &["| A | B |\n|---|---|\n| 1 | 2 |\n\n"],
        &["| A | B |\n|---|---|\n| 1 | 2 |\n\n"]
        ; "table followed by blank line"
    )]
    fn test_tables(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Mixed formatting (realistic assistant responses)
    // ===========================================

    #[test_case(
        &[
            "Here's how to do it:\n\n",
            "1. First, run `cargo", " build`\n",
            "2. Then check the **out", "put**\n\n",
            "```rust\n",
            "fn main() {}\n",
            "```\n"
        ],
        &[
            "Here's how to do it:\n\n",
            "1. First, run ",
            "`cargo build`\n",
            "2. Then check the ",
            "**output**\n\n",
            "```rust\nfn main() {}\n```\n"
        ]
        ; "typical assistant response with list code and formatting"
    )]
    #[test_case(
        &[
            "See the [**Rust Book**](https://doc.rust-l",
            "ang.org/book/) for more info.\n\n",
            "Key points:\n- Use `Result` for errors\n- Prefer `Option` over null"
        ],
        &[
            "See the ",
            "[**Rust Book**](https://doc.rust-lang.org/book/) for more info.\n\n",
            "Key points:\n- Use `Result` for errors\n- Prefer `Option` over null"
        ]
        ; "link with nested bold and list"
    )]
    #[test_case(
        &[
            "![screenshot](./img/sc",
            "reen.png)\n\nAs shown above..."
        ],
        &[
            "![screenshot](./img/screen.png)\n\nAs shown above..."
        ]
        ; "image with split url"
    )]
    fn test_mixed_content(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Edge cases and escapes
    // ===========================================

    #[test_case(
        &["Use \\* for bullet points, not \\`code\\`"],
        &["Use \\* for bullet points, not \\`code\\`"]
        ; "escaped markdown characters"
    )]
    #[test_case(
        &["Price: $100 * 2 = $200"],
        &["Price: $100 ", "* 2 = $200"]
        ; "asterisk in math context treated as italic marker"
    )]
    #[test_case(
        &[""],
        &[] as &[&str]
        ; "empty input produces no output"
    )]
    #[test_case(
        &["Hello 世界! Here's some **太字** text."],
        &["Hello 世界! Here's some **太字** text."]
        ; "unicode content"
    )]
    #[test_case(
        &["**bold *and italic* together**"],
        &["**bold *and italic* together**"]
        ; "nested bold and italic"
    )]
    #[test_case(
        &["***bold italic***"],
        &["***bold italic***"]
        ; "combined bold italic marker"
    )]
    #[test_case(
        &["~~stri", "ke~~ and **bo", "ld**"],
        &["~~strike~~ and ", "**bold**"]
        ; "strikethrough and bold split"
    )]
    fn test_edge_cases(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Incomplete constructs at stream end
    // ===========================================

    #[test_case(
        &["This is **incomplete bold"],
        &["This is ", "**incomplete bold"]
        ; "unclosed bold flushes"
    )]
    #[test_case(
        &["Check [broken link](http://"],
        &["Check ", "[broken link](http://"]
        ; "unclosed link flushes"
    )]
    #[test_case(
        &["Start of `code"],
        &["Start of ", "`code"]
        ; "unclosed inline code flushes"
    )]
    fn test_incomplete_constructs(chunks: &[&str], expected: &[&str]) {
        assert_eq!(stream(chunks), expected);
    }

    // ===========================================
    // Code-block truncation
    // ===========================================

    #[test]
    fn truncation_preserves_longer_outer_backtick_fence() {
        let content = "````md\n```\nline1\nline2\nline3\n```\n````\n";
        let out = truncate_code_blocks_with(content, 2, 1);

        assert!(
            out.starts_with("````md\n"),
            "outer fence should be preserved at the open: {out:?}"
        );
        assert!(
            out.contains("\n````\n"),
            "outer fence should still close the block: {out:?}"
        );
        assert!(
            out.contains("... (4 more lines"),
            "all 4 inner lines (including the inner ``` fences) should count toward truncation: {out:?}"
        );
    }

    #[test]
    fn truncation_preserves_longer_outer_tilde_fence() {
        let content = "~~~~md\n~~~\nline1\nline2\nline3\n~~~\n~~~~\n";
        let out = truncate_code_blocks_with(content, 2, 1);

        assert!(out.starts_with("~~~~md\n"), "{out:?}");
        assert!(out.contains("\n~~~~\n"), "{out:?}");
        assert!(out.contains("... (4 more lines"), "{out:?}");
    }

    #[test]
    fn truncation_ignores_non_fence_lines_containing_backticks() {
        // A code line that begins with `````` but also has trailing text should
        // not be treated as a closing fence.
        let content = "````\nline1\n``` not a fence\nline3\nline4\nline5\n````\n";
        let out = truncate_code_blocks_with(content, 2, 1);

        assert!(out.starts_with("````\n"), "{out:?}");
        assert!(out.contains("\n````\n"), "{out:?}");
        assert!(out.contains("... (4 more lines"), "{out:?}");
    }

    #[test]
    fn truncation_skips_when_block_is_within_limit() {
        let content = "```\nline1\nline2\n```\n";
        let out = truncate_code_blocks_with(content, 10, 5);
        assert_eq!(out, content);
    }

    #[test]
    fn parse_positive_lines_rejects_invalid_inputs() {
        assert_eq!(parse_positive_lines("50"), Some(50));
        assert_eq!(parse_positive_lines("1"), Some(1));
        assert_eq!(parse_positive_lines("0"), None);
        assert_eq!(parse_positive_lines("-1"), None);
        assert_eq!(parse_positive_lines(""), None);
        assert_eq!(parse_positive_lines("not-a-number"), None);
        assert_eq!(parse_positive_lines("3.14"), None);
    }
}
