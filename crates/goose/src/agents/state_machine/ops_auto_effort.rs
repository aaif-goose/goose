use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_providers::conversation::{effective_role, Conversation, EffectiveRole};
use goose_providers::decision::{
    DecisionAnswer, DecisionProvider, DecisionQuestion, DecisionRequest,
};
use goose_providers::thinking::ThinkingEffort;
use serde::{Deserialize, Serialize};

use super::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult, CLIENT_LOG,
};
use crate::session::Session;

const DECISION: &str = "decision";
const OPERATION: &str = "auto_effort";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EffortDecision {
    effort: Option<ThinkingEffort>,
    model: Option<String>,
    confidence: Option<f64>,
    probabilities: HashMap<String, f64>,
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

pub struct AutoEffortOperation {
    provider: Arc<dyn DecisionProvider>,
    model: String,
    efforts: Vec<ThinkingEffort>,
}

impl AutoEffortOperation {
    pub(crate) fn new(
        provider: Arc<dyn DecisionProvider>,
        model: String,
        efforts: Vec<ThinkingEffort>,
    ) -> Self {
        Self {
            provider,
            model,
            efforts,
        }
    }

    async fn classify(&self, request: &str) -> Result<EffortDecision> {
        let criteria = self
            .efforts
            .iter()
            .map(|effort| {
                let description = match effort {
                    ThinkingEffort::Off => {
                        "No reasoning is needed, such as a greeting or a direct factual response."
                    }
                    ThinkingEffort::Low => {
                        "A small amount of reasoning is enough for a simple, well-scoped task."
                    }
                    ThinkingEffort::Medium => {
                        "The task needs several reasoning steps or ordinary coding work."
                    }
                    ThinkingEffort::High => {
                        "The task is complex, ambiguous, or needs careful planning and verification."
                    }
                    ThinkingEffort::Max => {
                        "The task is exceptionally difficult or high stakes and benefits from the deepest available reasoning."
                    }
                };
                (effort.to_string(), description.to_string())
            })
            .collect();
        let mut response = self
            .provider
            .create_decision(&DecisionRequest {
                model: self.model.clone(),
                state: request.into(),
                questions: HashMap::from([(
                    "effort".to_string(),
                    DecisionQuestion::Choice {
                        instructions:
                            "Choose the least thinking effort that can reliably handle this request."
                                .to_string(),
                        criteria,
                    },
                )]),
            })
            .await?;
        let answer = response
            .answers
            .remove("effort")
            .ok_or_else(|| anyhow!("decision provider returned no effort answer"))?;
        let DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
        } = answer
        else {
            return Err(anyhow!(
                "decision provider returned a non-choice effort answer"
            ));
        };
        let effort = choice.parse().map_err(anyhow::Error::msg)?;
        if !self.efforts.contains(&effort) {
            return Err(anyhow!(
                "decision provider returned unavailable effort '{effort}'"
            ));
        }

        Ok(EffortDecision {
            effort: Some(effort),
            model: Some(response.model),
            confidence: Some(confidence),
            probabilities,
        })
    }
}

pub(super) fn current_turn_effort(conversation: &Conversation) -> Option<ThinkingEffort> {
    let decision = messages_since_kickoff(conversation)
        .ok()?
        .first()?
        .metadata
        .operation_note(OPERATION, DECISION)?;
    serde_json::from_value::<EffortDecision>(decision.clone())
        .ok()?
        .effort
}

#[async_trait]
impl Operation<Session, GooseEffect> for AutoEffortOperation {
    fn name(&self) -> &'static str {
        OPERATION
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

        let request = messages
            .iter()
            .rev()
            .filter(|message| message.is_agent_visible())
            .find_map(|message| {
                let message = message.agent_visible_content();
                matches!(effective_role(&message), EffectiveRole::User)
                    .then(|| message.as_concat_text())
            })
            .unwrap_or_default();
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
        let client_log = decision.effort.map(|effort| format!("thinking {effort}"));
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
