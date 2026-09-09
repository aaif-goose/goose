use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures::{stream::BoxStream, StreamExt};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agents::state_machine::{
    run_goose, submitted_report, Emitter, GooseEffect, PlanOperation, StateMachine,
    SupervisorOperation, SUBMIT_FEEDBACK_TOOL_NAME, SUBMIT_PLAN_TOOL_NAME,
};
use crate::agents::{Agent, AgentEvent, SessionConfig};
use crate::config::Config;
use crate::conversation::message::{Message, SystemNotificationType};
use crate::model_config::model_config_from_user_config;
use crate::providers::base::Provider;
use crate::providers::create_with_working_dir;
use crate::session::{Session, SessionManager, SessionType};

const PLANNER_MODEL: &str = "GOOSE_PLANNER_MODEL";
const SUPERVISOR_MODEL: &str = "GOOSE_SUPERVISOR_MODEL";
const IMPLEMENTER_MODEL: &str = "GOOSE_IMPLEMENTER_MODEL";
const TIME_LIMIT_SECONDS: &str = "GOOSE_SUPERVISED_TIME_LIMIT_SECONDS";
const DEFAULT_TIME_LIMIT_SECONDS: u64 = 900;
const TRACE_TYPE: &str = "goose_supervised_trace";

pub(crate) struct SupervisedModels {
    planner: String,
    supervisor: String,
    implementer: String,
}

fn complete_models(
    planner: Option<String>,
    supervisor: Option<String>,
    implementer: Option<String>,
) -> Option<SupervisedModels> {
    let present = |model: Option<String>| model.filter(|model| !model.trim().is_empty());
    Some(SupervisedModels {
        planner: present(planner)?,
        supervisor: present(supervisor)?,
        implementer: present(implementer)?,
    })
}

pub(in crate::agents) fn configured_models() -> Option<SupervisedModels> {
    let config = Config::global();
    complete_models(
        config.get_param::<String>(PLANNER_MODEL).ok(),
        config.get_param::<String>(SUPERVISOR_MODEL).ok(),
        config.get_param::<String>(IMPLEMENTER_MODEL).ok(),
    )
}

#[derive(Deserialize)]
struct SupervisorReport {
    requires_action: bool,
    feedback: String,
}

fn trace_event(
    stage: &str,
    role: &str,
    provider: &str,
    model: Option<&str>,
    duration: Duration,
    details: serde_json::Value,
) -> AgentEvent {
    AgentEvent::Message(
        Message::assistant()
            .with_text(
                serde_json::json!({
                    "type": TRACE_TYPE,
                    "schema_version": 1,
                    "stage": stage,
                    "role": role,
                    "provider": provider,
                    "model": model,
                    "duration_ms": duration.as_millis(),
                    "details": details,
                })
                .to_string(),
            )
            .with_visibility(false, false),
    )
}

fn role_event(role: &str, model: &str, activity: &str) -> AgentEvent {
    AgentEvent::Message(
        Message::assistant()
            .with_system_notification(
                SystemNotificationType::InlineMessage,
                format!("{role} ({model}): {activity}"),
            )
            .with_visibility(true, false),
    )
}

fn run_hidden<'a>(
    machine: &'a StateMachine<'_, Session, GooseEffect>,
    runtime: &'a SessionManager,
    session_id: &'a str,
    prompt: String,
    cancel: CancellationToken,
) -> (
    BoxStream<'a, Result<AgentEvent>>,
    oneshot::Receiver<Result<Session>>,
) {
    let (result_tx, result_rx) = oneshot::channel();
    let events = async_stream::stream! {
        if let Err(error) = runtime
            .add_message(session_id, &Message::user().with_text(prompt))
            .await
        {
            let _ = result_tx.send(Err(error));
            return;
        }
        let (tx, mut rx) = mpsc::channel(32);
        let emit = Emitter::new(tx, cancel);
        let run = run_goose(machine, runtime, session_id, &emit);
        tokio::pin!(run);
        let session = loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(AgentEvent::HistoryReplaced(_)) => {}
                        Some(event) => yield Ok(event),
                        None => break Err(anyhow!("hidden state-machine event stream closed")),
                    }
                }
                result = &mut run => break result,
            }
        };
        drop(emit);
        while let Some(event) = rx.recv().await {
            if !matches!(event, AgentEvent::HistoryReplaced(_)) {
                yield Ok(event);
            }
        }
        let _ = result_tx.send(session);
    };
    (Box::pin(events), result_rx)
}

fn report_value(session: &Session, tool_name: &str) -> Result<serde_json::Value> {
    let conversation = session
        .conversation
        .as_ref()
        .ok_or_else(|| anyhow!("state-machine session has no conversation"))?;
    submitted_report(conversation, tool_name)?.ok_or_else(|| anyhow!("{tool_name} was not called"))
}

fn plan_from(session: &Session) -> Result<String> {
    report_value(session, SUBMIT_PLAN_TOOL_NAME)?
        .get("plan")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("submit_plan returned no plan"))
}

fn feedback_from(session: &Session) -> Result<SupervisorReport> {
    serde_json::from_value(report_value(session, SUBMIT_FEEDBACK_TOOL_NAME)?)
        .context("invalid supervisor feedback")
}

async fn create_role_session(
    runtime: &SessionManager,
    parent: &Session,
    name: &str,
    provider_name: &str,
    model_config: goose_providers::model::ModelConfig,
) -> Result<Session> {
    let session = runtime
        .create_session(
            parent.working_dir.clone(),
            name.to_string(),
            SessionType::Hidden,
            parent.goose_mode,
        )
        .await?;
    runtime
        .update(&session.id)
        .parent_session_id(Some(parent.id.clone()))
        .provider_name(provider_name)
        .model_config(model_config)
        .extension_data(parent.extension_data.clone())
        .apply()
        .await?;
    runtime.get_session(&session.id, false).await
}

async fn context_limit(
    provider: &Arc<dyn Provider>,
    model_config: &goose_providers::model::ModelConfig,
) -> usize {
    provider
        .get_context_limit(model_config)
        .await
        .unwrap_or_else(|_| model_config.context_limit())
}

impl Agent {
    pub(crate) async fn reply_with_supervised_state_machines(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
        models: SupervisedModels,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let runtime = self.config.session_manager.clone();
        let cancel = cancel_token.unwrap_or_default();
        let parent = runtime.get_session(&session_config.id, false).await?;
        if let Some(schedule_id) = session_config.schedule_id.clone() {
            runtime
                .update(&session_config.id)
                .schedule_id(Some(schedule_id))
                .apply()
                .await?;
        }
        runtime
            .add_message(&session_config.id, &user_message)
            .await?;

        let provider_name = parent
            .provider_name
            .clone()
            .or_else(|| Config::global().get_goose_provider().ok())
            .ok_or_else(|| anyhow!("supervised state machines require a configured provider"))?;
        let planner_model = model_config_from_user_config(&provider_name, &models.planner)?;
        let supervisor_model = model_config_from_user_config(&provider_name, &models.supervisor)?;
        let implementer_model = model_config_from_user_config(&provider_name, &models.implementer)?;
        let extension_configs = self.get_extension_configs().await;
        let planner_provider = create_with_working_dir(
            &provider_name,
            extension_configs.clone(),
            parent.working_dir.clone(),
        )
        .await?;
        let supervisor_provider = create_with_working_dir(
            &provider_name,
            extension_configs,
            parent.working_dir.clone(),
        )
        .await?;
        let implementer_provider = self.provider().await?;

        let planner_session = create_role_session(
            runtime.as_ref(),
            &parent,
            "Planner",
            &provider_name,
            planner_model.clone(),
        )
        .await?;
        let supervisor_session = create_role_session(
            runtime.as_ref(),
            &parent,
            "Supervisor",
            &provider_name,
            supervisor_model.clone(),
        )
        .await?;
        runtime
            .update(&session_config.id)
            .provider_name(&provider_name)
            .model_config(implementer_model.clone())
            .apply()
            .await?;

        let planner_machine = self.create_state_machine(
            planner_provider.clone(),
            planner_model.clone(),
            context_limit(&planner_provider, &planner_model).await,
            session_config.max_turns,
            cancel.child_token(),
            self.steer_queue(&planner_session.id).await,
            Some(Arc::new(PlanOperation)),
        );
        let supervisor_machine = self.create_state_machine(
            supervisor_provider.clone(),
            supervisor_model.clone(),
            context_limit(&supervisor_provider, &supervisor_model).await,
            session_config.max_turns,
            cancel.child_token(),
            self.steer_queue(&supervisor_session.id).await,
            Some(Arc::new(SupervisorOperation)),
        );
        let implementer_cancel = cancel.child_token();
        let implementer_steer = self.steer_queue(&session_config.id).await;
        let implementer_machine = self.create_state_machine(
            implementer_provider.clone(),
            implementer_model.clone(),
            context_limit(&implementer_provider, &implementer_model).await,
            session_config.max_turns,
            implementer_cancel.clone(),
            implementer_steer.clone(),
            None,
        );
        let problem = user_message.agent_visible_content().as_concat_text();
        let time_limit = Duration::from_secs(
            Config::global()
                .get_param::<u64>(TIME_LIMIT_SECONDS)
                .unwrap_or(DEFAULT_TIME_LIMIT_SECONDS),
        );
        let main_session_id = session_config.id.clone();

        Ok(Box::pin(async_stream::try_stream! {
            let workflow_started = Instant::now();
            yield trace_event(
                "started",
                "orchestrator",
                &provider_name,
                None,
                Duration::ZERO,
                serde_json::json!({
                    "planner_model": &models.planner,
                    "supervisor_model": &models.supervisor,
                    "implementer_model": &models.implementer,
                    "time_limit_seconds": time_limit.as_secs(),
                }),
            );

            let stage_started = Instant::now();
            yield role_event("Planner", &models.planner, "creating initial plan");
            let (mut events, planned) = run_hidden(
                &planner_machine,
                runtime.as_ref(),
                &planner_session.id,
                format!(
                    "Investigate this software task and submit an implementation plan. Do not change the working tree.\n\n{problem}"
                ),
                cancel.child_token(),
            );
            while let Some(event) = events.next().await {
                yield event?;
            }
            let planned = planned.await.context("planner result stream closed")??;
            let plan = plan_from(&planned)?;
            yield trace_event(
                "initial_plan",
                "planner",
                &provider_name,
                Some(&models.planner),
                stage_started.elapsed(),
                serde_json::json!({ "plan": &plan }),
            );

            let stage_started = Instant::now();
            yield role_event("Supervisor", &models.supervisor, "critiquing plan");
            let (mut events, criticized) = run_hidden(
                &supervisor_machine,
                runtime.as_ref(),
                &supervisor_session.id,
                format!(
                    "Critique the proposed plan for this task. Inspect the repository independently, identify incorrect assumptions and missing work, and recommend concrete corrections.\n\nTask:\n{problem}\n\nProposed plan:\n{plan}"
                ),
                cancel.child_token(),
            );
            while let Some(event) = events.next().await {
                yield event?;
            }
            let criticized = criticized.await.context("supervisor result stream closed")??;
            let critique = feedback_from(&criticized)?;
            yield trace_event(
                "plan_critique",
                "supervisor",
                &provider_name,
                Some(&models.supervisor),
                stage_started.elapsed(),
                serde_json::json!({
                    "requires_action": critique.requires_action,
                    "feedback": &critique.feedback,
                }),
            );

            let stage_started = Instant::now();
            yield role_event("Planner", &models.planner, "revising plan");
            let (mut events, revised) = run_hidden(
                &planner_machine,
                runtime.as_ref(),
                &planner_session.id,
                format!(
                    "Revise your plan in response to this critique. Verify the criticism against the repository and submit a complete replacement plan.\n\nCritique:\n{}",
                    critique.feedback
                ),
                cancel.child_token(),
            );
            while let Some(event) = events.next().await {
                yield event?;
            }
            let revised = revised.await.context("planner result stream closed")??;
            let revised_plan = plan_from(&revised)?;
            yield trace_event(
                "revised_plan",
                "planner",
                &provider_name,
                Some(&models.planner),
                stage_started.elapsed(),
                serde_json::json!({ "plan": &revised_plan }),
            );
            runtime
                .add_message(
                    &main_session_id,
                    &Message::user()
                        .with_text(format!(
                            "Implement the task using this reviewed plan. Treat the repository and tests as authoritative and adjust the plan when necessary.\n\n{revised_plan}"
                        ))
                        .with_visibility(false, true),
                )
                .await?;

            yield role_event("Implementer", &models.implementer, "implementing revised plan");
            let implementation_started = Instant::now();
            let supervision_at = time_limit.mul_f32(0.3);
            let finalization_at = time_limit.mul_f32(0.8);
            let mut supervised = false;
            let mut finalization_sent = false;
            let (tx, mut rx) = mpsc::channel(32);
            let emit = Emitter::new(tx, implementer_cancel.clone());
            let timeout_cancel = implementer_cancel.clone();
            let timeout = tokio::spawn(async move {
                tokio::time::sleep(time_limit).await;
                timeout_cancel.cancel();
            });

            loop {
                let session = runtime.get_session(&main_session_id, true).await?;
                let result = {
                    let step = implementer_machine.step(&session, &emit);
                    tokio::pin!(step);
                    loop {
                        tokio::select! {
                            event = rx.recv() => {
                                if let Some(event) = event {
                                    yield event;
                                } else {
                                    break Err(anyhow!("implementer event stream closed"));
                                }
                            }
                            result = &mut step => break result,
                        }
                    }?
                };
                let Some(mut result) = result else {
                    break;
                };
                {
                    let apply = implementer_machine.apply(
                        runtime.as_ref(),
                        &session,
                        &mut result,
                        &emit,
                    );
                    tokio::pin!(apply);
                    loop {
                        tokio::select! {
                            event = rx.recv() => {
                                if let Some(event) = event {
                                    yield event;
                                } else {
                                    break Err(anyhow!("implementer event stream closed"));
                                }
                            }
                            result = &mut apply => break result,
                        }
                    }?;
                }
                while let Ok(event) = rx.try_recv() {
                    yield event;
                }
                if result.yield_to_client {
                    break;
                }

                let elapsed = implementation_started.elapsed();
                if !supervised && elapsed >= supervision_at {
                    let stage_started = Instant::now();
                    yield role_event(
                        "Supervisor",
                        &models.supervisor,
                        "reviewing implementation progress",
                    );
                    let progress = runtime.get_session(&main_session_id, true).await?;
                    let recent = progress
                        .conversation
                        .as_ref()
                        .into_iter()
                        .flat_map(|conversation| conversation.messages().iter().rev())
                        .map(Message::agent_visible_content)
                        .map(|message| message.as_concat_text())
                        .filter(|text| !text.trim().is_empty())
                        .map(|text| crate::utils::safe_truncate(&text, 2_000))
                        .take(8)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    let (mut events, checked) = run_hidden(
                        &supervisor_machine,
                        runtime.as_ref(),
                        &supervisor_session.id,
                        format!(
                            "Review the implementer's progress and provide a short steering message. Inspect the current working tree. About 70% of the implementation time remains.\n\nRevised plan:\n{revised_plan}\n\nRecent progress:\n{recent}"
                        ),
                        cancel.child_token(),
                    );
                    while let Some(event) = events.next().await {
                        yield event?;
                    }
                    let checked = checked.await.context("supervisor result stream closed")??;
                    let feedback = feedback_from(&checked)?;
                    yield trace_event(
                        "progress_review",
                        "supervisor",
                        &provider_name,
                        Some(&models.supervisor),
                        stage_started.elapsed(),
                        serde_json::json!({
                            "implementation_elapsed_ms": implementation_started.elapsed().as_millis(),
                            "requires_action": feedback.requires_action,
                            "feedback": &feedback.feedback,
                            "delivered_to_implementer": feedback.requires_action,
                        }),
                    );
                    if feedback.requires_action {
                        implementer_steer
                            .lock()
                            .await
                            .push_back(Message::user().with_text(format!(
                                "Supervisor steering:\n\n{}",
                                feedback.feedback
                            )));
                    }
                    supervised = true;
                }
                let elapsed = implementation_started.elapsed();
                if !finalization_sent && elapsed >= finalization_at {
                    implementer_steer.lock().await.push_back(Message::user().with_text(
                        "The time limit is approaching. Stop broad exploration, finish the smallest correct patch, run the most relevant tests available, and report the result.",
                    ));
                    yield trace_event(
                        "finalization_steer",
                        "orchestrator",
                        &provider_name,
                        None,
                        Duration::ZERO,
                        serde_json::json!({
                            "implementation_elapsed_ms": implementation_started.elapsed().as_millis(),
                            "delivered_to_implementer": true,
                        }),
                    );
                    finalization_sent = true;
                }
                if elapsed >= time_limit {
                    implementer_cancel.cancel();
                    break;
                }
            }
            drop(emit);
            while let Some(event) = rx.recv().await {
                yield event;
            }
            timeout.abort();

            if !supervised {
                yield trace_event(
                    "progress_review",
                    "supervisor",
                    &provider_name,
                    Some(&models.supervisor),
                    Duration::ZERO,
                    serde_json::json!({
                        "implementation_elapsed_ms": implementation_started.elapsed().as_millis(),
                        "skipped": true,
                        "reason": "implementation ended before the review ran",
                    }),
                );
            }
            if !finalization_sent {
                yield trace_event(
                    "finalization_steer",
                    "orchestrator",
                    &provider_name,
                    None,
                    Duration::ZERO,
                    serde_json::json!({
                        "implementation_elapsed_ms": implementation_started.elapsed().as_millis(),
                        "skipped": true,
                        "reason": "implementation ended before the finalization steer ran",
                    }),
                );
            }

            yield trace_event(
                "completed",
                "orchestrator",
                &provider_name,
                None,
                workflow_started.elapsed(),
                serde_json::json!({ "final_review_run": false }),
            );
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervised_models_require_all_three_settings() {
        assert!(complete_models(Some("planner".into()), Some("supervisor".into()), None).is_none());
        assert!(complete_models(
            Some("planner".into()),
            Some("supervisor".into()),
            Some("  ".into())
        )
        .is_none());

        let models = complete_models(
            Some("planner".into()),
            Some("supervisor".into()),
            Some("implementer".into()),
        )
        .expect("all models are configured");
        assert_eq!(models.planner, "planner");
        assert_eq!(models.supervisor, "supervisor");
        assert_eq!(models.implementer, "implementer");
    }

    #[test]
    fn trace_events_are_machine_readable_and_invisible() {
        let AgentEvent::Message(message) = trace_event(
            "initial_plan",
            "planner",
            "openrouter",
            Some("openai/planner"),
            Duration::from_millis(42),
            serde_json::json!({ "plan": "change the parser" }),
        ) else {
            panic!("trace event is not a message");
        };

        assert!(!message.is_user_visible());
        assert!(!message.is_agent_visible());
        let trace: serde_json::Value =
            serde_json::from_str(&message.as_concat_text()).expect("trace is valid JSON");
        assert_eq!(trace["type"], TRACE_TYPE);
        assert_eq!(trace["stage"], "initial_plan");
        assert_eq!(trace["model"], "openai/planner");
        assert_eq!(trace["duration_ms"], 42);
        assert_eq!(trace["details"]["plan"], "change the parser");
    }
}
