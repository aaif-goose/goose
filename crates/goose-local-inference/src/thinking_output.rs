use goose_provider_types::thinking::{FilterOut, ThinkFilter};

pub(crate) struct ThinkingOutputFilter {
    enabled: bool,
    saw_structured_reasoning: bool,
    think_filter: ThinkFilter,
    pending_inline_thinking: String,
    accumulated_thinking: String,
}

impl ThinkingOutputFilter {
    pub(crate) fn new(enable_thinking: bool, generation_prompt: &str) -> Self {
        let mut think_filter = ThinkFilter::new();
        if enable_thinking && !generation_prompt.is_empty() {
            let _ = think_filter.push(generation_prompt);
        }

        Self {
            enabled: enable_thinking,
            saw_structured_reasoning: false,
            think_filter,
            pending_inline_thinking: String::new(),
            accumulated_thinking: String::new(),
        }
    }

    pub(crate) fn push_structured_reasoning(&mut self, reasoning: &str) -> Option<String> {
        if reasoning.is_empty() {
            return None;
        }

        self.saw_structured_reasoning = true;
        self.pending_inline_thinking.clear();
        self.think_filter = ThinkFilter::new();
        self.accumulated_thinking.push_str(reasoning);
        Some(reasoning.to_string())
    }

    pub(crate) fn push_text(&mut self, text: &str) -> FilterOut {
        if !self.enabled {
            return FilterOut {
                content: text.to_string(),
                thinking: String::new(),
            };
        }

        let mut filtered = self.think_filter.push(text);
        if self.saw_structured_reasoning {
            filtered.thinking.clear();
        } else if !filtered.thinking.is_empty() {
            self.pending_inline_thinking.push_str(&filtered.thinking);
            filtered.thinking.clear();
        }
        filtered
    }

    pub(crate) fn finish(&mut self) -> FilterOut {
        let mut filtered = if self.enabled && !self.saw_structured_reasoning {
            std::mem::take(&mut self.think_filter).finish()
        } else {
            FilterOut::default()
        };

        if !self.saw_structured_reasoning {
            let mut thinking = std::mem::take(&mut self.pending_inline_thinking);
            thinking.push_str(&filtered.thinking);
            if !thinking.is_empty() {
                self.accumulated_thinking.push_str(&thinking);
            }
            filtered.thinking = thinking;
        } else {
            filtered.thinking.clear();
        }

        filtered
    }

    pub(crate) fn accumulated_thinking(&self) -> &str {
        &self.accumulated_thinking
    }
}
