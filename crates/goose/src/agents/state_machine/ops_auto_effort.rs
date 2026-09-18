use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::conversation::message::Message;
use goose_providers::conversation::{Conversation, EffectiveRole};
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult,
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
}

impl AutoEffortOperation {
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

        Some(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }

    async fn classify(&self, request: &str) -> Result<EffortDecision> {
        let response = self
            .client
            .post(ENDPOINT)
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

pub(super) fn selected_effort(messages: &[Message]) -> Option<ThinkingEffort> {
    messages
        .iter()
        .rfind(|message| {
            message.role == rmcp::model::Role::User
                && message.is_user_visible()
                && !message.is_tool_response()
        })?
        .metadata
        .operation_note("auto_effort", DECISION)
        .and_then(|value| serde_json::from_value::<EffortDecision>(value.clone()).ok())
        .and_then(|decision| decision.effort)
}

#[async_trait]
impl Operation<Session, GooseEffect> for AutoEffortOperation {
    fn name(&self) -> &'static str {
        "auto_effort"
    }

    async fn run(
        &self,
        _session: &Session,
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

        applied([ConversationEffect::SetMessageOperationNote {
            message_id,
            operation: self.name().to_string(),
            key: DECISION.to_string(),
            value: serde_json::to_value(decision)?,
        }
        .into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_choice_response() {
        let response = serde_json::from_value::<JevResponse>(json!({
            "model": "jev-1.13.0",
            "answers": {
                "effort": {
                    "type": "choice",
                    "choice": "medium",
                    "probabilities": {
                        "off": 0.01,
                        "low": 0.09,
                        "medium": 0.8,
                        "high": 0.09,
                        "max": 0.01
                    },
                    "confidence": 0.75
                }
            },
            "usage": { "input_tokens": 42, "output_tokens": 7 }
        }))
        .unwrap();

        assert_eq!(response.answers.effort.choice, ThinkingEffort::Medium);
        assert_eq!(response.answers.effort.probabilities["medium"], 0.8);
    }

    #[test]
    fn reads_selected_effort_from_turn_metadata() {
        let mut kickoff = Message::user().with_text("fix the bug");
        kickoff.metadata.set_operation_note(
            "auto_effort",
            DECISION,
            serde_json::to_value(EffortDecision {
                effort: Some(ThinkingEffort::High),
                model: Some(MODEL.to_string()),
                confidence: Some(0.91),
                probabilities: HashMap::from([("high".to_string(), 0.91)]),
            })
            .unwrap(),
        );

        assert_eq!(selected_effort(&[kickoff]), Some(ThinkingEffort::High));
    }

    #[test]
    fn fallback_decision_preserves_configured_effort() {
        let mut kickoff = Message::user().with_text("fix the bug");
        kickoff.metadata.set_operation_note(
            "auto_effort",
            DECISION,
            serde_json::to_value(EffortDecision::fallback()).unwrap(),
        );

        assert_eq!(selected_effort(&[kickoff]), None);
    }

    #[test]
    fn does_not_reuse_a_previous_turns_effort() {
        let mut previous = Message::user().with_text("solve a hard problem");
        previous.metadata.set_operation_note(
            "auto_effort",
            DECISION,
            serde_json::to_value(EffortDecision {
                effort: Some(ThinkingEffort::Max),
                model: Some(MODEL.to_string()),
                confidence: Some(0.99),
                probabilities: HashMap::from([("max".to_string(), 0.99)]),
            })
            .unwrap(),
        );
        let current = Message::user().with_text("hello");

        assert_eq!(selected_effort(&[previous, current]), None);
    }
}
