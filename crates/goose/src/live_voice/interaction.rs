use super::service::{
    LiveVoiceInteractionCompletion, LiveVoiceInteractionGuard, LiveVoiceTranscriptPublisher,
};
use super::transcript::{DelegationContext, LiveTranscript};
use crate::{conversation::message::Message, session::SessionManager, token_counter::TokenCounter};
use futures::future::BoxFuture;
use goose_providers::live_voice_provider::{
    DelegationUpdate, DelegationUpdateDelivery, ProviderConnection, ProviderConnectionEvent,
};
use rmcp::model::Role;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) const PROVIDER_CLEANUP_TIMEOUT: Duration = Duration::from_secs(20);
pub(super) const DELEGATION_INSTRUCTION: &str =
    "Based on this conversation, identify and complete the user's request.";
const DELEGATION_UPDATE_TOKEN_LIMIT: usize = 500;
const UNDELIVERED_DELEGATION_UPDATE_NOTICE: &str =
    "I couldn't confirm that the latest delegated update reached this voice conversation. Please ask me to share it again.";
const SAVED_RESULT_NOTICE: &str = "\n\nThe full result is saved in Goose.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveVoiceInteractionId(pub(crate) String);

impl LiveVoiceInteractionId {
    pub(super) fn new() -> Self {
        Self(format!("live_{}", Uuid::now_v7()))
    }
}

pub(super) struct LiveVoiceInteraction {
    provider_connection: Box<dyn ProviderConnection>,
    provider_event_ids: HashSet<String>,
    provider_delegation_ids: HashSet<String>,
    transcript: LiveTranscript,
    delegated_main_agent_run: Option<DelegatedMainAgentRun>,
    session_manager: Arc<SessionManager>,
    transcript_publisher: LiveVoiceTranscriptPublisher,
    main_agent: LiveMainAgent,
    interaction_guard: LiveVoiceInteractionGuard,
}

struct DelegatedMainAgentRun {
    // GPT-Live expects the final result on the delegation that started this run.
    provider_delegation_id: String,
    result_future: BoxFuture<'static, String>,
}

enum LiveVoiceInteractionEvent {
    StopRequested,
    MainAgentFinished(String),
    Provider(ProviderConnectionEvent),
}

enum DelegationDecision {
    Ignore,
    Reject(String),
    Accept(String),
}

impl LiveVoiceInteraction {
    pub(super) fn new(
        provider_connection: Box<dyn ProviderConnection>,
        session_manager: Arc<SessionManager>,
        transcript_publisher: LiveVoiceTranscriptPublisher,
        main_agent: LiveMainAgent,
        interaction_guard: LiveVoiceInteractionGuard,
    ) -> Self {
        Self {
            provider_connection,
            provider_event_ids: HashSet::new(),
            provider_delegation_ids: HashSet::new(),
            transcript: LiveTranscript::default(),
            delegated_main_agent_run: None,
            session_manager,
            transcript_publisher,
            main_agent,
            interaction_guard,
        }
    }

    pub(super) async fn run(mut self) {
        let stop_requested = self.interaction_guard.stop_requested().clone();
        let mut stopping = false;
        let completion = loop {
            match self.next_event(&stop_requested, stopping).await {
                LiveVoiceInteractionEvent::StopRequested => {
                    match timeout(PROVIDER_CLEANUP_TIMEOUT, self.cleanup_provider()).await {
                        Ok(Ok(())) => stopping = true,
                        _ => break LiveVoiceInteractionCompletion::Failed,
                    }
                }
                LiveVoiceInteractionEvent::MainAgentFinished(result) => {
                    if self.handle_main_agent_finished(result).await.is_err() {
                        self.stop_provider_after_error(stopping).await;
                        break LiveVoiceInteractionCompletion::Failed;
                    }
                }
                LiveVoiceInteractionEvent::Provider(ProviderConnectionEvent::TranscriptDelta {
                    event_id,
                    role,
                    text,
                    end_ms,
                    ..
                }) => {
                    if self
                        .receive_transcript(event_id, role, text, end_ms)
                        .await
                        .is_err()
                    {
                        self.stop_provider_after_error(stopping).await;
                        break LiveVoiceInteractionCompletion::Failed;
                    }
                }
                LiveVoiceInteractionEvent::Provider(
                    ProviderConnectionEvent::DelegationRequested {
                        event_id,
                        delegation_id,
                        offset_ms,
                    },
                ) => {
                    if stopping {
                        continue;
                    }
                    if self
                        .receive_delegation(event_id, delegation_id, offset_ms)
                        .await
                        .is_err()
                    {
                        self.stop_provider_after_error(stopping).await;
                        break LiveVoiceInteractionCompletion::Failed;
                    }
                }
                LiveVoiceInteractionEvent::Provider(ProviderConnectionEvent::Closed) => {
                    break if stopping {
                        LiveVoiceInteractionCompletion::Stopped
                    } else {
                        LiveVoiceInteractionCompletion::Failed
                    };
                }
                LiveVoiceInteractionEvent::Provider(
                    ProviderConnectionEvent::ReceiverLagged | ProviderConnectionEvent::Failed,
                ) => {
                    self.stop_provider_after_error(stopping).await;
                    break LiveVoiceInteractionCompletion::Failed;
                }
            }
        };

        self.finish_interaction(completion).await;
    }

    async fn next_event(
        &mut self,
        stop_requested: &CancellationToken,
        stopping: bool,
    ) -> LiveVoiceInteractionEvent {
        if stopping {
            return LiveVoiceInteractionEvent::Provider(
                self.provider_connection.next_event().await,
            );
        }

        let delegated_run = &mut self.delegated_main_agent_run;
        let provider_connection = &mut self.provider_connection;
        tokio::select! {
            biased;
            _ = stop_requested.cancelled() => LiveVoiceInteractionEvent::StopRequested,
            result = async {
                delegated_run
                    .as_mut()
                    .expect("main agent is running")
                    .result_future
                    .as_mut()
                    .await
            }, if delegated_run.is_some() => LiveVoiceInteractionEvent::MainAgentFinished(result),
            event = provider_connection.next_event() => LiveVoiceInteractionEvent::Provider(event),
        }
    }

    async fn receive_transcript(
        &mut self,
        event_id: String,
        role: Role,
        text: String,
        end_ms: u64,
    ) -> anyhow::Result<()> {
        let Some(transcript_delta_to_display) =
            self.record_transcript(event_id, role, &text, end_ms)
        else {
            return Ok(());
        };

        (self.transcript_publisher)(transcript_delta_to_display);
        save_raw_transcript_entries(
            &self.session_manager,
            self.interaction_guard.session_id(),
            &mut self.transcript,
        )
        .await?;
        Ok(())
    }

    fn record_transcript(
        &mut self,
        event_id: String,
        role: Role,
        delta_text: &str,
        end_ms: u64,
    ) -> Option<Message> {
        if !self.provider_event_ids.insert(event_id) {
            return None;
        }
        self.transcript.append(role, delta_text, end_ms)
    }

    async fn receive_delegation(
        &mut self,
        event_id: String,
        delegation_id: String,
        offset_ms: u64,
    ) -> anyhow::Result<()> {
        match self.handle_delegation_request(event_id, delegation_id.clone(), offset_ms) {
            DelegationDecision::Ignore => {}
            DelegationDecision::Reject(text) => {
                self.send_delegation_update(delegation_id, bound_delegation_update(text).await)
                    .await?;
            }
            DelegationDecision::Accept(context) => {
                let session_id = self.interaction_guard.session_id().to_string();
                if self.delegated_main_agent_run.is_some() {
                    let input = format!("{context}\n{DELEGATION_INSTRUCTION}");
                    let response = match self.main_agent.steer(session_id, input).await {
                        Ok(response) => {
                            self.transcript
                                .mark_context_sent_to_main_agent_through(offset_ms);
                            response
                        }
                        Err(response) => response,
                    };
                    self.send_delegation_update(
                        delegation_id,
                        bound_delegation_update(response).await,
                    )
                    .await?;
                } else {
                    self.transcript.finish_transcript_entry_being_built();
                    save_raw_transcript_entries(
                        &self.session_manager,
                        &session_id,
                        &mut self.transcript,
                    )
                    .await?;
                    let input = format!("{context}\n{DELEGATION_INSTRUCTION}");
                    match self.main_agent.start(session_id, input) {
                        Ok(completion) => {
                            self.delegated_main_agent_run = Some(DelegatedMainAgentRun {
                                provider_delegation_id: delegation_id,
                                result_future: completion,
                            });
                            self.transcript
                                .mark_context_sent_to_main_agent_through(offset_ms);
                        }
                        Err(response) => {
                            self.send_delegation_update(
                                delegation_id,
                                bound_delegation_update(response).await,
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_delegation_request(
        &mut self,
        event_id: String,
        delegation_id: String,
        offset_ms: u64,
    ) -> DelegationDecision {
        if !self.provider_event_ids.insert(event_id)
            || !self.provider_delegation_ids.insert(delegation_id)
        {
            return DelegationDecision::Ignore;
        }
        match self.transcript.context_for_delegation(offset_ms) {
            DelegationContext::Stale => {
                DelegationDecision::Reject("The delegated conversation position is stale.".into())
            }
            DelegationContext::MissingUserInput => {
                DelegationDecision::Reject("I couldn't identify a request to complete.".into())
            }
            DelegationContext::Available(context) => DelegationDecision::Accept(context),
        }
    }

    async fn handle_main_agent_finished(&mut self, result: String) -> anyhow::Result<()> {
        let run = self
            .delegated_main_agent_run
            .take()
            .expect("main agent is running");
        self.transcript.finish_transcript_entry_being_built();
        save_raw_transcript_entries(
            &self.session_manager,
            self.interaction_guard.session_id(),
            &mut self.transcript,
        )
        .await?;
        self.send_delegation_update(
            run.provider_delegation_id,
            bound_delegation_update(result).await,
        )
        .await
    }

    async fn finish_interaction(mut self, mut completion: LiveVoiceInteractionCompletion) {
        match self.delegated_main_agent_run.take() {
            None => {
                if save_transcript_and_context_waiting_for_main_agent(
                    &self.session_manager,
                    self.interaction_guard.session_id(),
                    &mut self.transcript,
                )
                .await
                .is_err()
                {
                    completion = LiveVoiceInteractionCompletion::Failed;
                }
                self.interaction_guard
                    .publish_completion_after_cleanup(completion);
            }
            Some(run) => {
                self.interaction_guard.publish_completion(completion);
                let _ = run.result_future.await;
                let _ = save_transcript_and_context_waiting_for_main_agent(
                    &self.session_manager,
                    self.interaction_guard.session_id(),
                    &mut self.transcript,
                )
                .await;
            }
        }
    }

    async fn stop_provider_after_error(&mut self, stopping: bool) {
        if !stopping {
            let _ = timeout(PROVIDER_CLEANUP_TIMEOUT, self.cleanup_provider()).await;
        }
    }

    async fn send_delegation_update(
        &mut self,
        provider_delegation_id: String,
        text: String,
    ) -> anyhow::Result<()> {
        let delivery = self
            .provider_connection
            .send_delegation_update(DelegationUpdate {
                provider_delegation_id,
                text,
            })
            .await?;
        match delivery {
            DelegationUpdateDelivery::Delivered => {}
            DelegationUpdateDelivery::Undelivered => (self.transcript_publisher)(
                Message::assistant()
                    .with_id(format!("msg_live_{}", Uuid::now_v7()))
                    .with_text(UNDELIVERED_DELEGATION_UPDATE_NOTICE)
                    .user_only(),
            ),
        }
        Ok(())
    }

    async fn cleanup_provider(&mut self) -> anyhow::Result<()> {
        self.provider_connection.stop().await
    }
}

pub(crate) struct LiveMainAgent {
    start: Box<dyn Fn(String, String) -> Result<MainAgentRun, String> + Send + Sync>,
    steer: Box<dyn Fn(String, String) -> MainAgentSteerResponse + Send + Sync>,
}

type MainAgentRun = BoxFuture<'static, String>;
type MainAgentSteerResponse = BoxFuture<'static, Result<String, String>>;

impl LiveMainAgent {
    pub(crate) fn new(
        start: impl Fn(String, String) -> Result<MainAgentRun, String> + Send + Sync + 'static,
        steer: impl Fn(String, String) -> MainAgentSteerResponse + Send + Sync + 'static,
    ) -> Self {
        Self {
            start: Box::new(start),
            steer: Box::new(steer),
        }
    }

    fn start(&self, session_id: String, input: String) -> Result<MainAgentRun, String> {
        (self.start)(session_id, input)
    }

    fn steer(&self, session_id: String, input: String) -> MainAgentSteerResponse {
        (self.steer)(session_id, input)
    }
}

async fn save_raw_transcript_entries(
    session_manager: &SessionManager,
    session_id: &str,
    transcript: &mut LiveTranscript,
) -> anyhow::Result<()> {
    for message in transcript.raw_transcript_entries_waiting_to_save() {
        session_manager.add_message(session_id, message).await?;
    }
    transcript.mark_raw_transcript_entries_saved();
    Ok(())
}

async fn save_transcript_and_context_waiting_for_main_agent(
    session_manager: &SessionManager,
    session_id: &str,
    transcript: &mut LiveTranscript,
) -> anyhow::Result<()> {
    transcript.finish_transcript_entry_being_built();
    save_raw_transcript_entries(session_manager, session_id, transcript).await?;
    if let Some(context) = transcript.agent_only_context_message_waiting_to_save() {
        session_manager.add_message(session_id, &context).await?;
        transcript.mark_context_saved_for_main_agent();
    }
    Ok(())
}

async fn bound_delegation_update(text: String) -> String {
    let Ok(counter) = TokenCounter::new().await else {
        return text;
    };
    if counter.count_tokens(&text) <= DELEGATION_UPDATE_TOKEN_LIMIT {
        return text;
    }

    let allowed = DELEGATION_UPDATE_TOKEN_LIMIT - counter.count_tokens(SAVED_RESULT_NOTICE);
    let boundaries = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut low = 0;
    let mut high = boundaries.len();
    while low < high {
        let middle = (low + high).div_ceil(2);
        let end = boundaries.get(middle).copied().unwrap_or(text.len());
        let candidate = text
            .get(..end)
            .expect("delegation update boundary comes from char_indices");
        if counter.count_tokens(candidate) <= allowed {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let end = boundaries.get(low).copied().unwrap_or(text.len());
    let truncated = text
        .get(..end)
        .expect("delegation update boundary comes from char_indices");
    format!("{}{}", truncated.trim_end(), SAVED_RESULT_NOTICE)
}

#[cfg(test)]
mod tests;
