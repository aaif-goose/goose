use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures::{stream::BoxStream, StreamExt};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agents::state_machine::{
    run_goose, submitted_report, Emitter, GooseEffect, ImplementPlanOperation, PlanOperation,
    StateMachine, SupervisorOperation, SUBMIT_FEEDBACK_TOOL_NAME, SUBMIT_IMPLEMENTATION_TOOL_NAME,
    SUBMIT_PLAN_TOOL_NAME,
};
use crate::agents::{Agent, AgentEvent, SessionConfig, StateMachineResources};
use crate::config::{Config, GooseMode};
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

#[derive(Clone, Deserialize)]
struct PlanReport {
    findings: String,
    plan: String,
}

impl PlanReport {
    fn fallback(problem: &str) -> Self {
        Self {
            findings: "The planning deadline expired before a complete report was submitted. Inspect the repository and tests before changing it.".to_string(),
            plan: format!(
                "Complete the original task end to end, using the repository and tests to resolve implementation details. Run the most relevant verification and fix failures.\n\nOriginal task:\n{problem}"
            ),
        }
    }
}

#[derive(Deserialize)]
struct SupervisorReport {
    requires_action: bool,
    feedback: String,
}

#[derive(Deserialize)]
struct ImplementationReport {
    summary: String,
    verification: String,
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
    deadline: Option<Instant>,
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
        let emit = Emitter::new(tx, cancel.clone());
        let session = {
            let run = run_goose(machine, runtime, session_id, &emit);
            tokio::pin!(run);
            let deadline = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                    None => std::future::pending().await,
                }
            };
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Some(AgentEvent::HistoryReplaced(_)) => {}
                            Some(event) => yield Ok(event),
                            None => break Err(anyhow!("hidden state-machine event stream closed")),
                        }
                    }
                    result = &mut run => break result,
                    _ = &mut deadline => {
                        cancel.cancel();
                        break Err(anyhow!("state-machine stage deadline reached"));
                    }
                }
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

fn plan_from(session: &Session) -> Result<PlanReport> {
    serde_json::from_value(report_value(session, SUBMIT_PLAN_TOOL_NAME)?)
        .context("invalid planner report")
}

fn feedback_from(session: &Session) -> Result<SupervisorReport> {
    serde_json::from_value(report_value(session, SUBMIT_FEEDBACK_TOOL_NAME)?)
        .context("invalid supervisor feedback")
}

fn implementation_from(session: &Session) -> Result<ImplementationReport> {
    serde_json::from_value(report_value(session, SUBMIT_IMPLEMENTATION_TOOL_NAME)?)
        .context("invalid implementation report")
}

fn remaining_seconds(started: Instant, time_limit: Duration) -> u64 {
    time_limit.saturating_sub(started.elapsed()).as_secs()
}

fn recent_progress(session: &Session) -> String {
    session
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
        .join("\n\n")
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
            GooseMode::Auto,
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
        let workflow_started = Instant::now();
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
        let extension_configs = self.extension_manager.get_extension_configs().await;
        let planner_provider = create_with_working_dir(
            &provider_name,
            extension_configs.clone(),
            parent.working_dir.clone(),
        )
        .await?;
        let supervisor_provider =
            create_with_working_dir(&provider_name, Vec::new(), parent.working_dir.clone()).await?;
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
        self.update_goose_mode(GooseMode::Auto, &session_config.id)
            .await?;

        let (planner_extensions, supervisor_extensions) = tokio::try_join!(
            self.extension_manager_for_session(
                planner_provider.clone(),
                extension_configs.clone(),
                &planner_session,
            ),
            self.extension_manager_for_session(
                supervisor_provider.clone(),
                Vec::new(),
                &supervisor_session,
            ),
        )?;

        let planner_machine = self.create_state_machine(
            StateMachineResources {
                provider: planner_provider.clone(),
                model_config: planner_model.clone(),
                extension_manager: planner_extensions,
                context_limit: context_limit(&planner_provider, &planner_model).await,
            },
            session_config.max_turns,
            cancel.child_token(),
            self.steer_queue(&planner_session.id).await,
            Some(Arc::new(PlanOperation)),
        );
        let supervisor_machine = self.create_state_machine(
            StateMachineResources {
                provider: supervisor_provider.clone(),
                model_config: supervisor_model.clone(),
                extension_manager: supervisor_extensions,
                context_limit: context_limit(&supervisor_provider, &supervisor_model).await,
            },
            session_config.max_turns,
            cancel.child_token(),
            self.steer_queue(&supervisor_session.id).await,
            Some(Arc::new(SupervisorOperation)),
        );
        let implementer_context_limit =
            context_limit(&implementer_provider, &implementer_model).await;
        let problem = user_message.agent_visible_content().as_concat_text();
        let time_limit = Duration::from_secs(
            Config::global()
                .get_param::<u64>(TIME_LIMIT_SECONDS)
                .unwrap_or(DEFAULT_TIME_LIMIT_SECONDS),
        );
        let main_session_id = session_config.id.clone();

        Ok(Box::pin(async_stream::try_stream! {
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

            let planning_deadline = workflow_started + time_limit.mul_f32(0.25);
            let supervision_at = workflow_started + time_limit.mul_f32(0.3);
            let finalization_at = workflow_started + time_limit.mul_f32(0.8);
            let workflow_deadline = workflow_started + time_limit;

            let stage_started = Instant::now();
            yield role_event("Planner", &models.planner, "creating initial plan");
            let (mut events, planned) = run_hidden(
                &planner_machine,
                runtime.as_ref(),
                &planner_session.id,
                format!(
                    "Investigate the task thoroughly and submit an implementation-ready report. Do not change the working tree.\n\nTask:\n{problem}"
                ),
                cancel.child_token(),
                Some(planning_deadline),
            );
            while let Some(event) = events.next().await {
                yield event?;
            }
            let planned = planned.await.context("planner result stream closed")?;
            let mut report = match planned {
                Ok(session) => plan_from(&session)?,
                Err(error) if Instant::now() >= planning_deadline => {
                    yield trace_event(
                        "initial_plan",
                        "planner",
                        &provider_name,
                        Some(&models.planner),
                        stage_started.elapsed(),
                        serde_json::json!({ "timed_out": true, "error": error.to_string() }),
                    );
                    PlanReport::fallback(&problem)
                }
                Err(error) => Err(error)?,
            };
            yield trace_event(
                "initial_plan",
                "planner",
                &provider_name,
                Some(&models.planner),
                stage_started.elapsed(),
                serde_json::json!({ "findings": &report.findings, "plan": &report.plan }),
            );

            let mut accepted = false;
            let mut critique = None;
            if Instant::now() < planning_deadline {
                let stage_started = Instant::now();
                yield role_event("Supervisor", &models.supervisor, "critiquing plan");
                let (mut events, criticized) = run_hidden(
                    &supervisor_machine,
                    runtime.as_ref(),
                    &supervisor_session.id,
                    format!(
                        "Critique this planner report against the original task. Base the assessment only on the supplied report. Identify concrete corrections needed to make execution mechanical.\n\nTask:\n{problem}\n\nPlanner findings:\n{}\n\nProposed plan:\n{}",
                        report.findings, report.plan
                    ),
                    cancel.child_token(),
                    Some(planning_deadline),
                );
                while let Some(event) = events.next().await {
                    yield event?;
                }
                match criticized.await.context("supervisor result stream closed")? {
                    Ok(session) => {
                        let feedback = feedback_from(&session)?;
                        yield trace_event(
                            "plan_critique",
                            "supervisor",
                            &provider_name,
                            Some(&models.supervisor),
                            stage_started.elapsed(),
                            serde_json::json!({
                                "requires_action": feedback.requires_action,
                                "feedback": &feedback.feedback,
                            }),
                        );
                        critique = Some(feedback);
                    }
                    Err(error) if Instant::now() >= planning_deadline => {
                        yield trace_event(
                            "plan_critique",
                            "supervisor",
                            &provider_name,
                            Some(&models.supervisor),
                            stage_started.elapsed(),
                            serde_json::json!({ "timed_out": true, "error": error.to_string() }),
                        );
                    }
                    Err(error) => Err(error)?,
                }
            }

            if let Some(critique) = critique.filter(|_| Instant::now() < planning_deadline) {
                let stage_started = Instant::now();
                yield role_event("Planner", &models.planner, "revising plan");
                let (mut events, revised) = run_hidden(
                    &planner_machine,
                    runtime.as_ref(),
                    &planner_session.id,
                    format!(
                        "Address every criticism below. Inspect the repository again only when evidence is missing. Submit a complete replacement report with findings and a plan, and resolve the feedback in the report rather than deferring it.\n\nCritique:\n{}",
                        critique.feedback
                    ),
                    cancel.child_token(),
                    Some(planning_deadline),
                );
                while let Some(event) = events.next().await {
                    yield event?;
                }
                match revised.await.context("planner result stream closed")? {
                    Ok(session) => {
                        report = plan_from(&session)?;
                        yield trace_event(
                            "revised_plan",
                            "planner",
                            &provider_name,
                            Some(&models.planner),
                            stage_started.elapsed(),
                            serde_json::json!({
                                "findings": &report.findings,
                                "plan": &report.plan,
                            }),
                        );
                    }
                    Err(error) if Instant::now() >= planning_deadline => {
                        yield trace_event(
                            "revised_plan",
                            "planner",
                            &provider_name,
                            Some(&models.planner),
                            stage_started.elapsed(),
                            serde_json::json!({ "timed_out": true, "error": error.to_string() }),
                        );
                    }
                    Err(error) => Err(error)?,
                }
            }

            let mut rejection = None;
            if Instant::now() < planning_deadline {
                let stage_started = Instant::now();
                yield role_event("Supervisor", &models.supervisor, "checking revised plan");
                let (mut events, checked) = run_hidden(
                    &supervisor_machine,
                    runtime.as_ref(),
                    &supervisor_session.id,
                    format!(
                        "Accept or reject this revised report. Set `requires_action` to false only when the findings support the plan, every task requirement is covered, implementation choices are resolved, and the verification would demonstrate completion. Otherwise list only the remaining blockers.\n\nTask:\n{problem}\n\nPlanner findings:\n{}\n\nProposed plan:\n{}",
                        report.findings, report.plan
                    ),
                    cancel.child_token(),
                    Some(planning_deadline),
                );
                while let Some(event) = events.next().await {
                    yield event?;
                }
                match checked.await.context("supervisor result stream closed")? {
                    Ok(session) => {
                        let feedback = feedback_from(&session)?;
                        accepted = !feedback.requires_action;
                        if feedback.requires_action {
                            rejection = Some(feedback.feedback.clone());
                        }
                        yield trace_event(
                            "plan_acceptance",
                            "supervisor",
                            &provider_name,
                            Some(&models.supervisor),
                            stage_started.elapsed(),
                            serde_json::json!({
                                "accepted": accepted,
                                "feedback": &feedback.feedback,
                            }),
                        );
                    }
                    Err(error) if Instant::now() >= planning_deadline => {
                        yield trace_event(
                            "plan_acceptance",
                            "supervisor",
                            &provider_name,
                            Some(&models.supervisor),
                            stage_started.elapsed(),
                            serde_json::json!({ "timed_out": true, "error": error.to_string() }),
                        );
                    }
                    Err(error) => Err(error)?,
                }
            }

            if let Some(rejection) = rejection.filter(|_| Instant::now() < planning_deadline) {
                let stage_started = Instant::now();
                yield role_event("Planner", &models.planner, "resolving final blockers");
                let (mut events, revised) = run_hidden(
                    &planner_machine,
                    runtime.as_ref(),
                    &planner_session.id,
                    format!(
                        "Resolve every remaining blocker below and submit the final complete replacement findings and plan. Do not defer a blocker to the implementer.\n\nRemaining blockers:\n{rejection}"
                    ),
                    cancel.child_token(),
                    Some(planning_deadline),
                );
                while let Some(event) = events.next().await {
                    yield event?;
                }
                match revised.await.context("planner result stream closed")? {
                    Ok(session) => {
                        report = plan_from(&session)?;
                        yield trace_event(
                            "final_plan",
                            "planner",
                            &provider_name,
                            Some(&models.planner),
                            stage_started.elapsed(),
                            serde_json::json!({
                                "findings": &report.findings,
                                "plan": &report.plan,
                            }),
                        );
                    }
                    Err(error) if Instant::now() >= planning_deadline => {
                        yield trace_event(
                            "final_plan",
                            "planner",
                            &provider_name,
                            Some(&models.planner),
                            stage_started.elapsed(),
                            serde_json::json!({ "timed_out": true, "error": error.to_string() }),
                        );
                    }
                    Err(error) => Err(error)?,
                }
            }

            yield trace_event(
                "planning_complete",
                "orchestrator",
                &provider_name,
                None,
                workflow_started.elapsed(),
                serde_json::json!({
                    "accepted": accepted,
                    "deadline_reached": Instant::now() >= planning_deadline,
                }),
            );

            let implementer_cancel = cancel.child_token();
            let implementer_steer = self.steer_queue(&main_session_id).await;
            let implementer_machine = self.create_state_machine(
                StateMachineResources {
                    provider: implementer_provider.clone(),
                    model_config: implementer_model.clone(),
                    extension_manager: self.extension_manager.clone(),
                    context_limit: implementer_context_limit,
                },
                session_config.max_turns,
                implementer_cancel.clone(),
                implementer_steer.clone(),
                Some(Arc::new(ImplementPlanOperation::new(
                    report.findings.clone(),
                    report.plan.clone(),
                ))),
            );

            yield role_event("Implementer", &models.implementer, "implementing plan");
            let review_report = report.clone();
            let review_cancel = cancel.child_token();
            let progress_review = async {
                tokio::time::sleep_until(supervision_at.into()).await;
                let stage_started = Instant::now();
                let progress = runtime.get_session(&main_session_id, true).await?;
                let recent = recent_progress(&progress);
                let elapsed_seconds = workflow_started.elapsed().as_secs();
                let remaining_seconds = remaining_seconds(workflow_started, time_limit);
                let (mut events, checked) = run_hidden(
                    &supervisor_machine,
                    runtime.as_ref(),
                    &supervisor_session.id,
                    format!(
                        "Review the implementer's progress against the selected plan. {elapsed_seconds} seconds have elapsed and {remaining_seconds} seconds remain. Give only the highest-priority correction needed now; do not repeat the plan.\n\nPlanner findings:\n{}\n\nSelected plan:\n{}\n\nRecent progress:\n{recent}",
                        review_report.findings, review_report.plan
                    ),
                    review_cancel,
                    Some(workflow_deadline),
                );
                let mut collected = vec![role_event(
                    "Supervisor",
                    &models.supervisor,
                    "reviewing implementation progress",
                )];
                while let Some(event) = events.next().await {
                    collected.push(event?);
                }
                let checked = checked.await.context("supervisor result stream closed")??;
                Ok::<_, anyhow::Error>((
                    collected,
                    feedback_from(&checked)?,
                    stage_started.elapsed(),
                    elapsed_seconds,
                    remaining_seconds,
                ))
            };
            tokio::pin!(progress_review);
            let finalization_sleep = tokio::time::sleep_until(finalization_at.into());
            tokio::pin!(finalization_sleep);
            let deadline_sleep = tokio::time::sleep_until(workflow_deadline.into());
            tokio::pin!(deadline_sleep);
            let mut supervised = false;
            let mut finalization_sent = false;
            let mut timed_out = false;
            let (tx, mut rx) = mpsc::channel(32);
            let emit = Emitter::new(tx, implementer_cancel.clone());

            'implementation: loop {
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
                            review = &mut progress_review, if !supervised => {
                                let (events, feedback, duration, elapsed_seconds, remaining_seconds) =
                                    match review {
                                        Ok(review) => review,
                                        Err(error) => break Err(error),
                                    };
                                for event in events {
                                    yield event;
                                }
                                if feedback.requires_action {
                                    implementer_steer.lock().await.push_back(Message::user().with_text(format!(
                                        "Supervisor steering ({remaining_seconds} seconds remain):\n\n{}",
                                        feedback.feedback
                                    )));
                                }
                                yield trace_event(
                                    "progress_review",
                                    "supervisor",
                                    &provider_name,
                                    Some(&models.supervisor),
                                    duration,
                                    serde_json::json!({
                                        "workflow_elapsed_seconds": elapsed_seconds,
                                        "workflow_remaining_seconds": remaining_seconds,
                                        "requires_action": feedback.requires_action,
                                        "feedback": &feedback.feedback,
                                        "delivered_to_implementer": feedback.requires_action,
                                    }),
                                );
                                supervised = true;
                            }
                            _ = &mut finalization_sleep, if !finalization_sent => {
                                let remaining = remaining_seconds(workflow_started, time_limit);
                                implementer_steer.lock().await.push_back(Message::user().with_text(format!(
                                    "Only {remaining} seconds remain. Stop broad exploration. Complete the required deliverables, run the most relevant verification, fix failures, and call `submit_implementation` with evidence."
                                )));
                                yield trace_event(
                                    "finalization_steer",
                                    "orchestrator",
                                    &provider_name,
                                    None,
                                    Duration::ZERO,
                                    serde_json::json!({
                                        "workflow_elapsed_seconds": workflow_started.elapsed().as_secs(),
                                        "workflow_remaining_seconds": remaining,
                                        "delivered_to_implementer": true,
                                    }),
                                );
                                finalization_sent = true;
                            }
                            _ = &mut deadline_sleep => {
                                implementer_cancel.cancel();
                                timed_out = true;
                                break 'implementation;
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
                            review = &mut progress_review, if !supervised => {
                                let (events, feedback, duration, elapsed_seconds, remaining_seconds) =
                                    match review {
                                        Ok(review) => review,
                                        Err(error) => break Err(error),
                                    };
                                for event in events {
                                    yield event;
                                }
                                if feedback.requires_action {
                                    implementer_steer.lock().await.push_back(Message::user().with_text(format!(
                                        "Supervisor steering ({remaining_seconds} seconds remain):\n\n{}",
                                        feedback.feedback
                                    )));
                                }
                                yield trace_event(
                                    "progress_review",
                                    "supervisor",
                                    &provider_name,
                                    Some(&models.supervisor),
                                    duration,
                                    serde_json::json!({
                                        "workflow_elapsed_seconds": elapsed_seconds,
                                        "workflow_remaining_seconds": remaining_seconds,
                                        "requires_action": feedback.requires_action,
                                        "feedback": &feedback.feedback,
                                        "delivered_to_implementer": feedback.requires_action,
                                    }),
                                );
                                supervised = true;
                            }
                            _ = &mut finalization_sleep, if !finalization_sent => {
                                let remaining = remaining_seconds(workflow_started, time_limit);
                                implementer_steer.lock().await.push_back(Message::user().with_text(format!(
                                    "Only {remaining} seconds remain. Stop broad exploration. Complete the required deliverables, run the most relevant verification, fix failures, and call `submit_implementation` with evidence."
                                )));
                                yield trace_event(
                                    "finalization_steer",
                                    "orchestrator",
                                    &provider_name,
                                    None,
                                    Duration::ZERO,
                                    serde_json::json!({
                                        "workflow_elapsed_seconds": workflow_started.elapsed().as_secs(),
                                        "workflow_remaining_seconds": remaining,
                                        "delivered_to_implementer": true,
                                    }),
                                );
                                finalization_sent = true;
                            }
                            _ = &mut deadline_sleep => {
                                implementer_cancel.cancel();
                                timed_out = true;
                                break 'implementation;
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
            }
            drop(emit);
            while let Some(event) = rx.recv().await {
                yield event;
            }

            let final_session = runtime.get_session(&main_session_id, true).await?;
            let completion = implementation_from(&final_session).ok();
            if !supervised {
                yield trace_event(
                    "progress_review",
                    "supervisor",
                    &provider_name,
                    Some(&models.supervisor),
                    Duration::ZERO,
                    serde_json::json!({
                        "workflow_elapsed_seconds": workflow_started.elapsed().as_secs(),
                        "skipped": true,
                        "reason": "implementation ended before the review completed",
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
                        "workflow_elapsed_seconds": workflow_started.elapsed().as_secs(),
                        "skipped": true,
                        "reason": "implementation ended before the finalization checkpoint",
                    }),
                );
            }

            yield trace_event(
                "completed",
                "orchestrator",
                &provider_name,
                None,
                workflow_started.elapsed(),
                serde_json::json!({
                    "timed_out": timed_out,
                    "implementation_report": completion.map(|report| serde_json::json!({
                        "summary": report.summary,
                        "verification": report.verification,
                    })),
                    "final_review_run": false,
                }),
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

    #[tokio::test]
    async fn role_sessions_run_in_auto_mode() {
        let data_dir = tempfile::tempdir().expect("temporary session directory");
        let runtime = SessionManager::new(data_dir.path().to_path_buf());
        let parent = runtime
            .create_session(
                data_dir.path().to_path_buf(),
                "parent".to_string(),
                SessionType::User,
                GooseMode::Approve,
            )
            .await
            .expect("parent session");
        let model_config = goose_providers::model::ModelConfig::new("planner");

        let planner = create_role_session(&runtime, &parent, "Planner", "openrouter", model_config)
            .await
            .expect("planner session");

        assert_eq!(planner.goose_mode, GooseMode::Auto);
        assert_eq!(
            planner.parent_session_id.as_deref(),
            Some(parent.id.as_str())
        );
    }
}
