use lru::LruCache;
use rmcp::model::Tool;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tiktoken_rs::CoreBPE;
use tokio::sync::OnceCell;

use crate::conversation::message::Message;

static TOKENIZER: OnceCell<Arc<CoreBPE>> = OnceCell::const_new();

const MAX_TOKEN_CACHE_SIZE: usize = 1_024;

pub(crate) const TOKENS_PER_MESSAGE: usize = 4;
const REPLY_PRIMER_TOKENS: usize = 3;

// token use for various bits of a tool calls:
const FUNC_INIT: usize = 7;
const PROP_INIT: usize = 3;
const PROP_KEY: usize = 3;
const ENUM_INIT: isize = -3;
const ENUM_ITEM: usize = 3;
const FUNC_END: usize = 12;

pub struct TokenCounter {
    tokenizer: Arc<CoreBPE>,
    token_cache: Mutex<LruCache<TokenCacheKey, usize>>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct TokenCacheKey {
    len: usize,
    hash: [u8; 32],
}

impl TokenCacheKey {
    fn from_text(text: &str) -> Self {
        Self {
            len: text.len(),
            hash: *blake3::hash(text.as_bytes()).as_bytes(),
        }
    }
}

impl TokenCounter {
    pub async fn new() -> Result<Self, String> {
        let tokenizer = get_tokenizer().await?;
        let cache_capacity =
            NonZeroUsize::new(MAX_TOKEN_CACHE_SIZE).expect("token cache capacity must be non-zero");
        Ok(Self {
            tokenizer,
            token_cache: Mutex::new(LruCache::new(cache_capacity)),
        })
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        let cache_key = TokenCacheKey::from_text(text);
        if let Some(count) = self
            .token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .get(&cache_key)
            .copied()
        {
            return count;
        }

        let tokens = self.tokenizer.encode_with_special_tokens(text);
        let count = tokens.len();

        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .put(cache_key, count);
        count
    }

    pub fn count_tokens_for_tools(&self, tools: &[Tool]) -> usize {
        let mut func_token_count = 0;
        if !tools.is_empty() {
            for tool in tools {
                func_token_count += FUNC_INIT;
                let name = &tool.name;
                let description = &tool
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .trim_end_matches('.');

                let line = format!("{}:{}", name, description);
                func_token_count += self.count_tokens(&line);

                if let Some(serde_json::Value::Object(properties)) =
                    tool.input_schema.get("properties")
                {
                    if !properties.is_empty() {
                        func_token_count += PROP_INIT;
                        for (key, value) in properties {
                            func_token_count += PROP_KEY;
                            let p_name = key;
                            let p_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            let p_desc = value
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .trim_end_matches('.');

                            let line = format!("{}:{}:{}", p_name, p_type, p_desc);
                            func_token_count += self.count_tokens(&line);

                            if let Some(enum_values) = value.get("enum").and_then(|v| v.as_array())
                            {
                                func_token_count =
                                    func_token_count.saturating_add_signed(ENUM_INIT);
                                for item in enum_values {
                                    if let Some(item_str) = item.as_str() {
                                        func_token_count += ENUM_ITEM;
                                        func_token_count += self.count_tokens(item_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            func_token_count += FUNC_END;
        }

        func_token_count
    }

    /// Agent-invisible messages are never sent, so they count as zero.
    pub fn count_message_tokens(&self, message: &Message) -> usize {
        if !message.metadata.agent_visible {
            return 0;
        }
        let mut num_tokens = TOKENS_PER_MESSAGE;
        for content in &message.content {
            if let Some(content_text) = content.as_text() {
                num_tokens += self.count_tokens(content_text);
            } else if let Some(tool_request) = content.as_tool_request() {
                if let Ok(tool_call) = tool_request.tool_call.as_ref() {
                    let text = format!(
                        "{}:{}:{:?}",
                        tool_request.id, tool_call.name, tool_call.arguments
                    );
                    num_tokens += self.count_tokens(&text);
                }
            } else if let Some(tool_response_text) = content.as_tool_response_text() {
                num_tokens += self.count_tokens(&tool_response_text);
            }
        }
        num_tokens
    }

    pub fn count_chat_tokens(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> usize {
        let mut num_tokens = 0;

        if !system_prompt.is_empty() {
            num_tokens += self.count_tokens(system_prompt) + TOKENS_PER_MESSAGE;
        }

        for message in messages {
            num_tokens += self.count_message_tokens(message);
        }

        if !tools.is_empty() {
            num_tokens += self.count_tokens_for_tools(tools);
        }

        num_tokens += REPLY_PRIMER_TOKENS;

        num_tokens
    }

    pub fn count_everything(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
        resources: &[String],
    ) -> usize {
        let mut num_tokens = self.count_chat_tokens(system_prompt, messages, tools);

        if !resources.is_empty() {
            for resource in resources {
                num_tokens += self.count_tokens(resource);
            }
        }
        num_tokens
    }

    pub fn clear_cache(&self) {
        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .clear();
    }

    pub fn cache_size(&self) -> usize {
        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .len()
    }
}

async fn get_tokenizer() -> Result<Arc<CoreBPE>, String> {
    Ok(TOKENIZER
        .get_or_init(|| async {
            let bpe = tiktoken_rs::o200k_base().expect("Failed to initialize o200k_base tokenizer");
            Arc::new(bpe)
        })
        .await
        .clone())
}

pub async fn create_token_counter() -> Result<TokenCounter, String> {
    TokenCounter::new().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_caching() {
        let counter = create_token_counter().await.unwrap();

        let text = "This is a test for caching functionality";

        let count1 = counter.count_tokens(text);
        assert_eq!(counter.cache_size(), 1);

        let count2 = counter.count_tokens(text);
        assert_eq!(count1, count2);
        assert_eq!(counter.cache_size(), 1);

        let count3 = counter.count_tokens("Different text");
        assert_eq!(counter.cache_size(), 2);
        assert_ne!(count1, count3);
    }

    #[tokio::test]
    async fn test_cache_management() {
        let counter = create_token_counter().await.unwrap();

        counter.count_tokens("First text");
        counter.count_tokens("Second text");
        counter.count_tokens("Third text");

        assert_eq!(counter.cache_size(), 3);

        counter.clear_cache();
        assert_eq!(counter.cache_size(), 0);

        let count = counter.count_tokens("First text");
        assert!(count > 0);
        assert_eq!(counter.cache_size(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_token_counter_creation() {
        let handles: Vec<_> = (0..10)
            .map(|_| tokio::spawn(async { create_token_counter().await.unwrap() }))
            .collect();

        let counters: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let text = "Test concurrent creation";
        let expected_count = counters[0].count_tokens(text);

        for counter in &counters {
            assert_eq!(counter.count_tokens(text), expected_count);
        }
    }

    #[tokio::test]
    async fn test_cache_eviction_behavior() {
        let counter = create_token_counter().await.unwrap();

        let mut cached_texts = Vec::new();
        for i in 0..=MAX_TOKEN_CACHE_SIZE {
            let text = format!("Test string number {}", i);
            counter.count_tokens(&text);
            cached_texts.push(text);
        }

        assert_eq!(counter.cache_size(), MAX_TOKEN_CACHE_SIZE);

        let recent_text = &cached_texts[cached_texts.len() - 1];
        let start_size = counter.cache_size();

        counter.count_tokens(recent_text);
        assert_eq!(counter.cache_size(), start_size);
    }

    #[tokio::test]
    async fn test_concurrent_cache_operations() {
        let counter = std::sync::Arc::new(create_token_counter().await.unwrap());

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let counter_clone = counter.clone();
                tokio::spawn(async move {
                    let text = format!("Concurrent test {}", i % 5);
                    counter_clone.count_tokens(&text)
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        for result in results {
            assert!(result > 0);
        }

        assert!(counter.cache_size() > 0);
        assert!(counter.cache_size() <= MAX_TOKEN_CACHE_SIZE);
    }
}
