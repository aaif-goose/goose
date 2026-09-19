use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::conversation::{Conversation, EffectiveRole};
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult, CLIENT_LOG,
};
use crate::config::Config;
use crate::session::Session;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-1.13.0";
const DECISION: &str = "decision";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EffortDecision {
    effort: Option<ThinkingEffort>,
    model: Option<String>,
    confidence: Option<f32>,
    probabilities: HashMap<String, f32>,
}

impl EffortDecision {
    fn fallback() -> Self {
        Self {
            effort: None,
            model: None,
            confidence: None,
            probabilities: HashMap::new(),
        }
    }
}

#[derive(Deserialize)]
struct JevResponse {
    model: String,
    answers: JevAnswers,
}

#[derive(Deserialize)]
struct JevAnswers {
    effort: JevChoice,
}

#[derive(Deserialize)]
struct JevChoice {
    choice: ThinkingEffort,
    confidence: f32,
    probabilities: HashMap<String, f32>,
}

pub struct AutoEffortOperation {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
}

impl AutoEffortOperation {
    pub(super) fn new(api_key: String, endpoint: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            endpoint,
        }
    }

    pub fn from_config(model_config: &ModelConfig) -> Option<Self> {
        let config = Config::global();
        if !model_config.is_reasoning_model()
            || !config
                .get_param::<bool>("GOOSE_AUTO_EFFORT_ENABLED")
                .unwrap_or(false)
        {
            return None;
        }

        let api_key = config
            .get_secret::<String>("TYPESAFE_API_KEY")
            .ok()?
            .trim()
            .to_string();
        if api_key.is_empty() {
            return None;
        }

        Some(Self::new(api_key, ENDPOINT.to_string()))
    }

    async fn classify(&self, request: &str) -> Result<EffortDecision> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .timeout(Duration::from_secs(2))
            .json(&json!({
                "model": MODEL,
                "state": request,
                "questions": {
                    "effort": {
                        "type": "choice",
                        "instructions": "Choose the least thinking effort that can reliably handle this request.",
                        "criteria": {
                            "off": "No reasoning is needed, such as a greeting or a direct factual response.",
                            "low": "A small amount of reasoning is enough for a simple, well-scoped task.",
                            "medium": "The task needs several reasoning steps or ordinary coding work.",
                            "high": "The task is complex, ambiguous, or needs careful planning and verification.",
                            "max": "The task is exceptionally difficult or high stakes and benefits from the deepest available reasoning."
                        }
                    }
                }
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<JevResponse>()
            .await?;

        Ok(EffortDecision {
            effort: Some(response.answers.effort.choice),
            model: Some(response.model),
            confidence: Some(response.answers.effort.confidence),
            probabilities: response.answers.effort.probabilities,
        })
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for AutoEffortOperation {
    fn name(&self) -> &'static str {
        "auto_effort"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let kickoff = &messages[0];
        if self.message_meta(kickoff, DECISION).is_some()
            || !matches!(
                last_effective_role(messages)?,
                EffectiveRole::User | EffectiveRole::Tool
            )
        {
            return not_applicable();
        }

        let request = kickoff.user_visible_content().as_concat_text();
        if request.trim().is_empty() {
            return not_applicable();
        }

        let decision = tokio::select! {
            _ = emit.cancelled() => return not_applicable(),
            result = self.classify(&request) => match result {
                Ok(decision) => decision,
                Err(error) => {
                    tracing::warn!(%error, "Automatic effort selection failed; using configured effort");
                    EffortDecision::fallback()
                }
            }
        };
        let Some(message_id) = kickoff.id.clone() else {
            return not_applicable();
        };

        let mut effects = Vec::new();
        if let Some(effort) = decision.effort {
            let model_config = session
                .model_config
                .clone()
                .ok_or_else(|| anyhow::anyhow!("session has no model config"))?
                .with_thinking_effort(effort);
            effects.push(GooseEffect::SetModelConfig(model_config));
        }
        let client_log = decision
            .effort
            .map(|effort| format!("Thinking effort: {effort}"));
        effects.push(
            ConversationEffect::SetMessageOperationNote {
                message_id: message_id.clone(),
                operation: self.name().to_string(),
                key: DECISION.to_string(),
                value: serde_json::to_value(decision)?,
            }
            .into(),
        );
        if let Some(client_log) = client_log {
            effects.push(
                ConversationEffect::SetMessageOperationNote {
                    message_id,
                    operation: self.name().to_string(),
                    key: CLIENT_LOG.to_string(),
                    value: client_log.into(),
                }
                .into(),
            );
        }

        applied(effects)
    }
}
