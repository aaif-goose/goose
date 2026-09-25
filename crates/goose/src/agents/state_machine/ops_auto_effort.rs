use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_providers::api_client::{ApiClient, AuthMethod};
use goose_providers::conversation::{Conversation, EffectiveRole};
use goose_providers::decision::{
    DecisionAnswer, DecisionProvider, DecisionQuestion, DecisionRequest,
};
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;
use goose_providers::typesafe::{TypeSafeProvider, TYPESAFE_DEFAULT_HOST, TYPESAFE_DEFAULT_MODEL};
use serde::{Deserialize, Serialize};

use super::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult, CLIENT_LOG,
};
use crate::config::Config;
use crate::session::Session;

const DECISION: &str = "decision";

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
}

impl AutoEffortOperation {
    pub(super) fn new(provider: Arc<dyn DecisionProvider>) -> Self {
        Self { provider }
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

        let tls_config = crate::config::tls::provider_tls_config_from_config(config).ok()?;
        let api_client = ApiClient::with_timeout_and_tls(
            TYPESAFE_DEFAULT_HOST.to_string(),
            AuthMethod::BearerToken(api_key),
            Duration::from_secs(2),
            tls_config,
        )
        .ok()?;

        Some(Self::new(Arc::new(TypeSafeProvider::new(api_client))))
    }

    async fn classify(&self, request: &str) -> Result<EffortDecision> {
        let mut response = self
            .provider
            .create_decision(&DecisionRequest {
                model: TYPESAFE_DEFAULT_MODEL.to_string(),
                state: request.into(),
                questions: HashMap::from([(
                    "effort".to_string(),
                    DecisionQuestion::Choice {
                        instructions:
                            "Choose the least thinking effort that can reliably handle this request."
                                .to_string(),
                        criteria: HashMap::from([
                            (
                                "off".to_string(),
                                "No reasoning is needed, such as a greeting or a direct factual response."
                                    .to_string(),
                            ),
                            (
                                "low".to_string(),
                                "A small amount of reasoning is enough for a simple, well-scoped task."
                                    .to_string(),
                            ),
                            (
                                "medium".to_string(),
                                "The task needs several reasoning steps or ordinary coding work."
                                    .to_string(),
                            ),
                            (
                                "high".to_string(),
                                "The task is complex, ambiguous, or needs careful planning and verification."
                                    .to_string(),
                            ),
                            (
                                "max".to_string(),
                                "The task is exceptionally difficult or high stakes and benefits from the deepest available reasoning."
                                    .to_string(),
                            ),
                        ]),
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

        Ok(EffortDecision {
            effort: Some(choice.parse().map_err(anyhow::Error::msg)?),
            model: Some(response.model),
            confidence: Some(confidence),
            probabilities,
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
