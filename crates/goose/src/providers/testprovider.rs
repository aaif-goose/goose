use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::base::stream_from_single_message;
use super::base::{MessageStream, Provider, ProviderDef, ProviderMetadata};
use crate::conversation::message::{Message, ToolResponse};
use crate::utils::bytes_to_hex;
use futures::future::BoxFuture;
use goose_providers::conversation::token_usage::ProviderUsage;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::{CallToolResult, Tool};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestInput {
    system: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestOutput {
    message: Message,
    usage: ProviderUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestRecord {
    input: TestInput,
    output: TestOutput,
}

pub struct TestProvider {
    inner: Option<Arc<dyn Provider>>,
    records: Arc<Mutex<HashMap<String, TestRecord>>>,
    file_path: String,
    name: String,
}

impl TestProvider {
    const PROVIDER_NAME: &str = "test";

    pub fn new_recording(inner: Arc<dyn Provider>, file_path: impl Into<String>) -> Self {
        Self {
            inner: Some(inner),
            records: Arc::new(Mutex::new(HashMap::new())),
            file_path: file_path.into(),
            name: Self::PROVIDER_NAME.to_string(),
        }
    }

    pub fn new_replaying(file_path: impl Into<String>) -> Result<Self> {
        let file_path = file_path.into();
        let records = Self::load_records(&file_path)?;

        Ok(Self {
            inner: None,
            records: Arc::new(Mutex::new(records)),
            file_path,
            name: Self::PROVIDER_NAME.to_string(),
        })
    }

    pub fn finish_recording(self) -> Result<()> {
        if self.inner.is_some() {
            self.save_records()?;
        }
        Ok(())
    }

    fn hash_input(messages: &[Message]) -> String {
        use crate::conversation::message::MessageContent;

        // Strip internal metadata (e.g. tool_meta/_meta) from content before hashing.
        // This metadata is used for internal routing (like goose_extension ownership)
        // and isn't part of the semantic input the LLM sees, so it shouldn't affect
        // replay matching.
        let stable_messages: Vec<_> = messages
            .iter()
            .map(|msg| {
                let mut cleaned_content: Vec<_> = msg.content.to_vec();

                for content in &mut cleaned_content {
                    match content {
                        MessageContent::ToolRequest(ref mut req) => {
                            req.tool_meta = None;
                        }
                        MessageContent::ToolResponse(ToolResponse {
                            tool_result:
                                Ok(
                                    ref mut result @ CallToolResult {
                                        is_error: Some(false),
                                        ..
                                    },
                                ),
                            ..
                        }) => {
                            result.is_error = None;
                            result.result_type = None;
                        }
                        _ => {}
                    }
                }
                (msg.role.clone(), cleaned_content)
            })
            .collect();
        let serialized = serde_json::to_string(&stable_messages).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        bytes_to_hex(hasher.finalize())
    }

    fn load_records(file_path: &str) -> Result<HashMap<String, TestRecord>> {
        if !Path::new(file_path).exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(file_path)?;
        let records: HashMap<String, TestRecord> = serde_json::from_str(&content)?;
        Ok(records)
    }

    pub fn save_records(&self) -> Result<()> {
        let records = self.records.lock().unwrap();
        let content = serde_json::to_string_pretty(&*records)?;
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    pub fn get_record_count(&self) -> usize {
        self.records.lock().unwrap().len()
    }
}

impl goose_providers::base::ProviderDescriptor for TestProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            Self::PROVIDER_NAME,
            "Test Provider",
            "Provider for testing that can record/replay interactions",
            "test-model",
            vec!["test-model"],
            "",
            vec![],
        )
    }
}

impl ProviderDef for TestProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async { Err(anyhow!("TestProvider must be constructed explicitly")) })
    }
}

#[async_trait]
impl Provider for TestProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let hash = Self::hash_input(messages);

        if let Some(inner) = &self.inner {
            // Call inner provider's stream and collect it
            let stream = inner.stream(model_config, system, messages, tools).await?;
            let (message, usage) = super::base::collect_stream(stream).await?;

            let record = TestRecord {
                input: TestInput {
                    system: system.to_string(),
                    messages: messages.to_vec(),
                    tools: tools.to_vec(),
                },
                output: TestOutput {
                    message: message.clone(),
                    usage: usage.clone(),
                },
            };

            {
                let mut records = self.records.lock().unwrap();
                records.insert(hash, record);
            }

            Ok(super::base::stream_from_single_message(message, usage))
        } else {
            let records = self.records.lock().unwrap();
            if let Some(record) = records.get(&hash) {
                let message = record.output.message.clone();
                let usage = record.output.usage.clone();
                Ok(super::base::stream_from_single_message(message, usage))
            } else {
                Err(ProviderError::ExecutionError(format!(
                    "No recorded response found for input hash: {}",
                    hash
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use chrono::Utc;
    use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
    use rmcp::model::{Role, TextContent};
    use std::env;

    #[derive(Clone)]
    struct MockProvider {
        response: String,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn get_name(&self) -> &str {
            "mock-testprovider"
        }

        async fn stream(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            let message = Message::new(
                Role::Assistant,
                Utc::now().timestamp(),
                vec![MessageContent::Text(TextContent::new(
                    self.response.clone(),
                ))],
            );
            let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
            Ok(stream_from_single_message(message, usage))
        }
    }

    #[tokio::test]
    async fn test_record_and_replay() {
        let temp_file = format!(
            "{}/test_records_{}.json",
            env::temp_dir().display(),
            std::process::id()
        );

        let mock = Arc::new(MockProvider {
            response: "Hello, world!".to_string(),
        });

        {
            let test_provider = TestProvider::new_recording(mock, &temp_file);
            let model_config = ModelConfig::new("test-model");

            let result = test_provider
                .complete(&model_config, "You are helpful", &[], &[])
                .await;

            assert!(result.is_ok());
            let (message, _) = result.unwrap();

            if let MessageContent::Text(content) = &message.content[0] {
                assert_eq!(content.text, "Hello, world!");
            }

            assert_eq!(test_provider.get_record_count(), 1);
            test_provider.finish_recording().unwrap();
        }

        {
            let replay_provider = TestProvider::new_replaying(&temp_file).unwrap();
            let model_config = ModelConfig::new("test-model");

            let result = replay_provider
                .complete(&model_config, "You are helpful", &[], &[])
                .await;

            assert!(result.is_ok());
            let (message, _) = result.unwrap();

            if let MessageContent::Text(content) = &message.content[0] {
                assert_eq!(content.text, "Hello, world!");
            }
        }

        let _ = fs::remove_file(temp_file);
    }

    #[tokio::test]
    async fn test_replay_missing_record() {
        let temp_file = format!(
            "{}/test_missing_{}.json",
            env::temp_dir().display(),
            std::process::id()
        );

        let replay_provider = TestProvider::new_replaying(&temp_file).unwrap();
        let model_config = ModelConfig::new("test-model");

        let result = replay_provider
            .complete(&model_config, "Different system prompt", &[], &[])
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No recorded response found"));

        let _ = fs::remove_file(temp_file);
    }
}
