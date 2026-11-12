use std::{fmt, str::FromStr, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const GEMINI_THOUGHT_SIGNATURE_KEY: &str = "thoughtSignature";
const MAX_BUFFERED_THINK_TAG_BYTES: usize = 8 * 1024;

pub fn split_think_blocks(text: &str) -> (String, String) {
    let mut filter = ThinkFilter::new();
    let mut out = filter.push(text);
    let final_out = filter.finish();
    out.content.push_str(&final_out.content);
    out.thinking.push_str(&final_out.thinking);
    (out.content, out.thinking)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FilterOut {
    pub content: String,
    pub thinking: String,
}

pub struct ThinkFilter {
    buffer: String,
    inside_think: bool,
    think_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThinkTag {
    Open,
    Close,
    SelfClosing,
}

enum BufferEvent {
    Tag {
        pos: usize,
        end: usize,
        kind: ThinkTag,
    },
    OversizedTag {
        end: usize,
    },
    Partial(usize),
}

impl ThinkFilter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            inside_think: false,
            think_depth: 0,
        }
    }

    pub fn push(&mut self, chunk: &str) -> FilterOut {
        self.buffer.push_str(chunk);
        self.process_buffer()
    }

    pub fn finish(mut self) -> FilterOut {
        let mut out = self.process_buffer();
        if !self.buffer.is_empty() {
            if self.inside_think {
                out.thinking.push_str(&self.buffer);
            } else {
                out.content.push_str(&self.buffer);
            }
            self.buffer.clear();
        }
        out
    }

    fn process_buffer(&mut self) -> FilterOut {
        let mut out = FilterOut::default();

        loop {
            match next_buffer_event(&self.buffer, self.inside_think) {
                Some(BufferEvent::Tag { pos, end, kind }) => {
                    if pos > 0 {
                        let prefix = self.buffer.get(..pos).unwrap_or_default().to_string();
                        if self.inside_think {
                            out.thinking.push_str(&prefix);
                        } else {
                            out.content.push_str(&prefix);
                        }
                    }

                    self.buffer.drain(..end);

                    match kind {
                        ThinkTag::Open => {
                            self.think_depth += 1;
                            self.inside_think = true;
                        }
                        ThinkTag::Close => {
                            self.think_depth = self.think_depth.saturating_sub(1);
                            self.inside_think = self.think_depth > 0;
                        }
                        ThinkTag::SelfClosing => {}
                    }
                }
                Some(BufferEvent::OversizedTag { end }) => {
                    let malformed = self.buffer.get(..end).unwrap_or_default().to_string();
                    if self.inside_think {
                        out.thinking.push_str(&malformed);
                    } else {
                        out.content.push_str(&malformed);
                    }
                    self.buffer.drain(..end);
                }
                Some(BufferEvent::Partial(pos)) => {
                    if pos > 0 {
                        let prefix = self.buffer.get(..pos).unwrap_or_default().to_string();
                        if self.inside_think {
                            out.thinking.push_str(&prefix);
                        } else {
                            out.content.push_str(&prefix);
                        }
                        self.buffer.drain(..pos);
                    }
                    if self.buffer.len() > MAX_BUFFERED_THINK_TAG_BYTES {
                        let oversized = std::mem::take(&mut self.buffer);
                        if self.inside_think {
                            out.thinking.push_str(&oversized);
                        } else {
                            out.content.push_str(&oversized);
                        }
                    }
                    break;
                }
                None => {
                    if !self.buffer.is_empty() {
                        if self.inside_think {
                            out.thinking.push_str(&self.buffer);
                        } else {
                            out.content.push_str(&self.buffer);
                        }
                        self.buffer.clear();
                    }
                    break;
                }
            }
        }

        if self.buffer.capacity() > MAX_BUFFERED_THINK_TAG_BYTES {
            let mut bounded = String::with_capacity(self.buffer.len());
            bounded.push_str(&self.buffer);
            self.buffer = bounded;
        }

        out
    }
}

impl Default for ThinkFilter {
    fn default() -> Self {
        Self::new()
    }
}

fn next_buffer_event(buffer: &str, inside_think: bool) -> Option<BufferEvent> {
    let mut search_from = 0;

    while let Some(rel_pos) = buffer.get(search_from..).and_then(|rest| rest.find('<')) {
        let pos = search_from + rel_pos;
        let suffix = buffer.get(pos..).unwrap_or_default();

        if let Some((kind, end)) = parse_think_tag(buffer, pos) {
            if end - pos > MAX_BUFFERED_THINK_TAG_BYTES {
                return Some(BufferEvent::OversizedTag { end });
            }
            if inside_think || matches!(kind, ThinkTag::Open | ThinkTag::SelfClosing) {
                return Some(BufferEvent::Tag { pos, end, kind });
            }
        } else if !contains_unquoted_gt(suffix) && is_possible_partial_think_tag(suffix) {
            return Some(BufferEvent::Partial(pos));
        }

        search_from = pos + 1;
    }

    None
}

fn parse_think_tag(buffer: &str, start: usize) -> Option<(ThinkTag, usize)> {
    let bytes = buffer.as_bytes();
    if bytes.get(start) != Some(&b'<') {
        return None;
    }

    let mut idx = start + 1;
    let is_close = if bytes.get(idx) == Some(&b'/') {
        idx += 1;
        true
    } else {
        false
    };

    let name_start = idx;
    while bytes.get(idx).is_some_and(u8::is_ascii_alphabetic) {
        idx += 1;
    }

    if idx == name_start {
        return None;
    }

    let name = buffer.get(name_start..idx).unwrap_or_default();
    let is_think = name.eq_ignore_ascii_case("think") || name.eq_ignore_ascii_case("thinking");
    if !is_think {
        return None;
    }

    if is_close {
        while bytes.get(idx).is_some_and(u8::is_ascii_whitespace) {
            idx += 1;
        }
        if bytes.get(idx) == Some(&b'>') {
            return Some((ThinkTag::Close, idx + 1));
        }
        return None;
    }

    // Require a real tag boundary immediately after the name (>, /, or whitespace).
    // Without this, `<thinking-mode>` or `<thinking123>` would be classified as a
    // think tag and stripped from normal content.
    let valid_open_boundary = match bytes.get(idx) {
        Some(&b) => b == b'>' || b == b'/' || b.is_ascii_whitespace(),
        None => false,
    };
    if !valid_open_boundary {
        return None;
    }

    let mut quote: Option<u8> = None;
    let mut last_non_ws: Option<u8> = None;
    while let Some(&byte) = bytes.get(idx) {
        match quote {
            Some(quote_byte) => {
                if byte == quote_byte {
                    quote = None;
                }
            }
            None if matches!(byte, b'"' | b'\'') => {
                quote = Some(byte);
                last_non_ws = Some(byte);
            }
            None if byte == b'>' => {
                let kind = if last_non_ws == Some(b'/') {
                    ThinkTag::SelfClosing
                } else {
                    ThinkTag::Open
                };
                return Some((kind, idx + 1));
            }
            None if !byte.is_ascii_whitespace() => {
                last_non_ws = Some(byte);
            }
            None => {}
        }
        idx += 1;
    }

    None
}

fn is_possible_partial_think_tag(suffix: &str) -> bool {
    if contains_unquoted_gt(suffix) {
        return false;
    }

    // Allow a trailing `/` so a chunk boundary that lands between `<think` and
    // `>` in a self-closing `<think/>` (or `<thinking/>`) is still recognised
    // as a partial tag and buffered until the `>` arrives in the next chunk.
    static OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)^<(?:t(?:h(?:i(?:n(?:k(?:i(?:n(?:g)?)?)?)?)?)?)?)(?:\s.*|/)?$").unwrap()
    });
    static CLOSE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)^</(?:t(?:h(?:i(?:n(?:k(?:i(?:n(?:g)?)?)?)?)?)?)?)(?:\s*)?$").unwrap()
    });

    OPEN_RE.is_match(suffix) || CLOSE_RE.is_match(suffix)
}

fn contains_unquoted_gt(text: &str) -> bool {
    let mut quote: Option<u8> = None;
    for &byte in text.as_bytes() {
        match quote {
            Some(quote_byte) => {
                if byte == quote_byte {
                    quote = None;
                }
            }
            None if matches!(byte, b'"' | b'\'') => quote = Some(byte),
            None if byte == b'>' => return true,
            None => {}
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingEffort {
    Off,
    Low,
    Medium,
    High,
    Max,
}

impl FromStr for ThinkingEffort {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "off" | "disabled" | "none" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" | "xhigh" => Ok(Self::Max),
            other => Err(format!("unknown thinking effort: '{other}'")),
        }
    }
}

impl fmt::Display for ThinkingEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Max => write!(f, "max"),
        }
    }
}

/// A single selectable effort value advertised by a provider-managed harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingEffortOption {
    pub value: String,
    pub label: String,
}

/// A harness-advertised effort config option mirrored verbatim into goose's
/// `thinking_effort` session option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingEffortCapability {
    /// The harness's config option id, e.g. "effort".
    pub option_id: String,
    pub values: Vec<ThinkingEffortOption>,
    pub current: Option<String>,
}

/// How a provider participates in thinking-effort selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkingEffortSupport {
    /// The provider doesn't manage effort; callers keep the model-name-based path.
    Unspecified,
    /// The provider manages reasoning itself but has no effort knob for the current model.
    Unsupported,
    /// The provider passes through a harness-advertised effort option.
    Options(ThinkingEffortCapability),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_think_blocks_extracts_inline_reasoning() {
        assert_eq!(
            split_think_blocks("<think>x</think>y"),
            ("y".to_string(), "x".to_string())
        );
    }

    #[test]
    fn test_split_think_blocks_is_case_insensitive() {
        assert_eq!(
            split_think_blocks("<THINK>x</think>y"),
            ("y".to_string(), "x".to_string())
        );
    }

    #[test]
    fn test_split_think_blocks_handles_multiple_blocks() {
        assert_eq!(
            split_think_blocks("<think>a</think>b<think>c</think>d"),
            ("bd".to_string(), "ac".to_string())
        );
    }

    #[test]
    fn test_split_think_blocks_without_tags() {
        assert_eq!(
            split_think_blocks("plain content"),
            ("plain content".to_string(), String::new())
        );
    }

    #[test]
    fn test_split_think_blocks_handles_attributes() {
        assert_eq!(
            split_think_blocks(r#"<think class="x">a</think>b"#),
            ("b".to_string(), "a".to_string())
        );
    }

    #[test]
    fn test_split_think_blocks_handles_quoted_gt_in_self_closing_attributes() {
        for input in [
            r#"<think data="a>b"/>Visible"#,
            "<think data='a>b'/>Visible",
        ] {
            assert_eq!(
                split_think_blocks(input),
                ("Visible".to_string(), String::new()),
                "mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn test_split_think_blocks_handles_quoted_gt_in_open_attributes() {
        assert_eq!(
            split_think_blocks(r#"<think data="a>b">Hidden</think>Visible"#),
            ("Visible".to_string(), "Hidden".to_string())
        );
    }

    #[test]
    fn test_split_think_blocks_handles_thinking_variant() {
        assert_eq!(
            split_think_blocks("<thinking>a</thinking>b"),
            ("b".to_string(), "a".to_string())
        );
    }

    #[test]
    fn test_think_filter_streaming_across_partial_tags() {
        let mut filter = ThinkFilter::new();
        let mut out = FilterOut::default();

        for chunk in ["<thi", "nk>x</thi", "nk>y"] {
            let partial = filter.push(chunk);
            out.content.push_str(&partial.content);
            out.thinking.push_str(&partial.thinking);
        }

        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "y");
        assert_eq!(out.thinking, "x");
    }

    #[test]
    fn test_think_filter_preserves_non_think_tags() {
        let mut filter = ThinkFilter::new();
        let mut out = filter.push("<table>");
        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "<table>");
        assert!(out.thinking.is_empty());
    }

    #[test]
    fn test_think_filter_finish_treats_unterminated_think_as_thinking() {
        let mut filter = ThinkFilter::new();
        let mut out = filter.push("<think>unfinished");
        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert!(out.content.is_empty());
        assert_eq!(out.thinking, "unfinished");
    }

    #[test]
    fn test_think_filter_tracks_generation_prompt_open_block() {
        let mut filter = ThinkFilter::new();
        let _ = filter.push("<|assistant|><think>\n");
        let mut out = filter.push("hidden reasoning</think>visible answer");
        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "visible answer");
        assert_eq!(out.thinking, "hidden reasoning");
    }

    #[test]
    fn test_think_filter_preserves_tags_with_think_prefix() {
        for input in [
            "<thinking-mode>hello</thinking-mode>",
            "<thinking123>payload</thinking123>",
            "<thinker>note</thinker>",
        ] {
            let mut filter = ThinkFilter::new();
            let mut out = filter.push(input);
            let final_out = filter.finish();
            out.content.push_str(&final_out.content);
            out.thinking.push_str(&final_out.thinking);

            assert_eq!(out.content, input, "content mismatch for {input:?}");
            assert!(
                out.thinking.is_empty(),
                "unexpected thinking for {input:?}: {:?}",
                out.thinking
            );
        }
    }

    #[test]
    fn test_think_filter_accepts_think_with_attributes() {
        let mut filter = ThinkFilter::new();
        let mut out = filter.push("<think data-source=\"x\">hidden</think>visible");
        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "visible");
        assert_eq!(out.thinking, "hidden");
    }

    #[test]
    fn test_think_filter_treats_self_closing_as_noop() {
        // `<think/>` carries no reasoning payload. It must not flip the filter
        // into "inside_think" mode, and the tag itself must not leak into
        // visible content.
        for input in [
            "before <think/> after",
            "before <think /> after",
            "before <thinking/> after",
            "before <think data-source=\"x\"/> after",
        ] {
            let mut filter = ThinkFilter::new();
            let mut out = filter.push(input);
            let final_out = filter.finish();
            out.content.push_str(&final_out.content);
            out.thinking.push_str(&final_out.thinking);

            assert_eq!(
                out.content, "before  after",
                "content mismatch for {input:?}"
            );
            assert!(
                out.thinking.is_empty(),
                "unexpected thinking for {input:?}: {:?}",
                out.thinking
            );
        }
    }

    #[test]
    fn test_think_filter_self_closing_does_not_swallow_following_content() {
        // Regression: a self-closing `<think/>` used to be classified as an
        // Open tag, which incremented think_depth and routed everything after
        // it into the thinking bucket for the rest of the stream.
        let mut filter = ThinkFilter::new();
        let mut out = filter.push("<think/>visible chunk 1");
        let final_out = filter.push("visible chunk 2");
        let tail_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);
        out.content.push_str(&tail_out.content);
        out.thinking.push_str(&tail_out.thinking);

        assert_eq!(out.content, "visible chunk 1visible chunk 2");
        assert!(out.thinking.is_empty());
    }

    #[test]
    fn test_think_filter_streaming_across_self_closing_boundary() {
        // Regression: a chunk boundary between `<think` and `>` in a
        // self-closing `<think/>` used to fall out of the partial-tag regex
        // (which only allowed `<think<ws>...`), so the `<think/` prefix leaked
        // into visible content before the `>` arrived.
        for (a, b) in [
            ("before <think/", "> after"),
            ("before <thinking/", "> after"),
            ("head <think ", "/> tail"),
        ] {
            let mut filter = ThinkFilter::new();
            let mut out = filter.push(a);
            let second = filter.push(b);
            let final_out = filter.finish();
            out.content.push_str(&second.content);
            out.content.push_str(&final_out.content);
            out.thinking.push_str(&second.thinking);
            out.thinking.push_str(&final_out.thinking);

            assert!(
                !out.content.contains('<'),
                "partial tag leaked into content for ({a:?}, {b:?}): {:?}",
                out.content
            );
            assert!(
                out.thinking.is_empty(),
                "unexpected thinking for ({a:?}, {b:?}): {:?}",
                out.thinking
            );
        }
    }

    #[test]
    fn test_think_filter_streaming_across_quoted_attribute_boundary() {
        let mut filter = ThinkFilter::new();
        let mut out = filter.push(r#"<think data="a>b"#);
        assert!(out.content.is_empty());
        assert!(out.thinking.is_empty());

        let second = filter.push(r#""/>Visible"#);
        let final_out = filter.finish();
        out.content.push_str(&second.content);
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&second.thinking);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "Visible");
        assert!(out.thinking.is_empty());
    }

    #[test]
    fn test_think_filter_self_closing_inside_open_block_closes_nothing() {
        // `<think/>` inside an open `<think>` block is still a no-op: depth
        // should stay at 1 until the real `</think>` arrives.
        let mut filter = ThinkFilter::new();
        let mut out = filter.push("before <think>hidden1 <think/> hidden2</think>visible");
        let final_out = filter.finish();
        out.content.push_str(&final_out.content);
        out.thinking.push_str(&final_out.thinking);

        assert_eq!(out.content, "before visible");
        assert_eq!(out.thinking, "hidden1  hidden2");
    }

    #[test]
    fn test_think_filter_bounds_unterminated_quoted_tag_candidates() {
        for quote in ['"', '\''] {
            let payload = format!(
                "<think data={quote}{}",
                "a".repeat(MAX_BUFFERED_THINK_TAG_BYTES * 2)
            );
            let mut filter = ThinkFilter::new();
            let mut content = String::new();

            for chunk in payload.as_bytes().chunks(17) {
                let out = filter.push(std::str::from_utf8(chunk).unwrap());
                content.push_str(&out.content);
                assert!(filter.buffer.len() <= MAX_BUFFERED_THINK_TAG_BYTES);
            }

            content.push_str(&filter.finish().content);
            assert_eq!(content, payload);
        }
    }

    #[test]
    fn test_think_filter_bounds_partial_close_tag_candidates() {
        let payload = format!("</think{}", " ".repeat(MAX_BUFFERED_THINK_TAG_BYTES * 2));
        let mut filter = ThinkFilter::new();
        let mut content = String::new();

        for chunk in payload.as_bytes().chunks(19) {
            let out = filter.push(std::str::from_utf8(chunk).unwrap());
            content.push_str(&out.content);
            assert!(filter.buffer.len() <= MAX_BUFFERED_THINK_TAG_BYTES);
        }

        content.push_str(&filter.finish().content);
        assert_eq!(content, payload);
    }

    #[test]
    fn test_think_filter_releases_single_oversized_candidate_allocation() {
        let payload = format!(
            "<think data=\"{}",
            "a".repeat(MAX_BUFFERED_THINK_TAG_BYTES * 16)
        );
        let mut filter = ThinkFilter::new();
        let out = filter.push(&payload);

        assert_eq!(out.content, payload);
        assert!(out.thinking.is_empty());
        assert!(filter.buffer.is_empty());
        assert_eq!(filter.buffer.capacity(), 0);
    }

    #[test]
    fn test_think_filter_preserves_and_releases_completed_oversized_tag() {
        let payload = format!(
            "<think data=\"{}\"/>",
            "a".repeat(MAX_BUFFERED_THINK_TAG_BYTES * 16)
        );
        let mut filter = ThinkFilter::new();
        let out = filter.push(&payload);

        assert_eq!(out.content, payload);
        assert!(out.thinking.is_empty());
        assert!(filter.buffer.is_empty());
        assert_eq!(filter.buffer.capacity(), 0);
    }

    #[test]
    fn test_think_filter_releases_oversized_prefix_capacity() {
        let prefix = "a".repeat(MAX_BUFFERED_THINK_TAG_BYTES * 16);
        let payload = format!("{prefix}<thi");
        let mut filter = ThinkFilter::new();
        let out = filter.push(&payload);

        assert_eq!(out.content, prefix);
        assert!(out.thinking.is_empty());
        assert_eq!(filter.buffer, "<thi");
        assert!(filter.buffer.capacity() <= MAX_BUFFERED_THINK_TAG_BYTES);
    }

    #[test]
    fn test_think_filter_accepts_bounded_streamed_attributes() {
        let prefix = "<think data=\"";
        let attribute = "a".repeat(MAX_BUFFERED_THINK_TAG_BYTES - prefix.len() - 2);
        let mut filter = ThinkFilter::new();
        let first = filter.push(&format!("{prefix}{attribute}"));

        assert!(first.content.is_empty());
        assert!(first.thinking.is_empty());
        assert_eq!(filter.buffer.len(), MAX_BUFFERED_THINK_TAG_BYTES - 2);

        let second = filter.push("\">hidden</think>visible");
        let final_out = filter.finish();
        assert_eq!(second.content, "visible");
        assert_eq!(second.thinking, "hidden");
        assert_eq!(final_out, FilterOut::default());
    }
}
