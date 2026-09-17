use super::interaction::{LiveMainAgent, LiveVoiceInteraction, LiveVoiceInteractionId};
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::execution::ActiveRunRegistry;
use crate::session::SessionManager;
use crate::token_counter::TokenCounter;
use goose_providers::live_voice_provider::{LiveVoiceInputMessage, LiveVoiceProvider};
pub(crate) use goose_providers::live_voice_provider::{WebRtcAnswer, WebRtcOffer};
#[cfg(feature = "live-voice")]
use goose_providers::openai_live_voice_provider::OpenAiLiveVoiceProvider;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const LIVE_VOICE_INPUT_MESSAGE_LIMIT: usize = 128;
const LIVE_VOICE_INPUT_TOKEN_LIMIT: usize = 8_192;
#[cfg(feature = "live-voice")]
const LIVE_VOICE_ENABLED_CONFIG_KEY: &str = "GOOSE_LIVE_VOICE_ENABLED";
#[cfg(feature = "live-voice")]
const LIVE_VOICE_CONFIG_KEY: &str = "GOOSE_LIVE_VOICE";
#[cfg(feature = "live-voice")]
const DEFAULT_OPENAI_LIVE_VOICE: &str = "marin";
#[cfg(feature = "live-voice")]
const LIVE_SESSION_INSTRUCTIONS: &str = concat!(
    "You are Goose's live voice interface. Keep the conversation natural and concise.\n",
    "Interruption policy: Stop speaking when the user interrupts and listen to what they say.\n",
    "Delegation policy:\n",
    "Backend tools:\n",
    "- Goose can use backend reasoning and tools for longer tasks.\n",
    "Delegate to Goose when:\n",
    "- The user has finished stating a complete request that needs backend tools or reasoning.\n",
    "- The user corrects or changes backend work already in progress.\n",
    "Do not delegate to Goose when:\n",
    "- The request is unfinished or is missing a required detail such as a location, object, ",
    "command, or desired outcome. Ask one brief clarification and wait for the answer.\n",
    "- The user is greeting you or making conversation that you can answer directly.\n",
    "After delegating, briefly say the work is underway. Keep listening and accept corrections ",
    "while Goose works. Do not guess the result. Present delegated results directly. Only say ",
    "the task stopped or finished after Goose confirms it."
);
#[cfg(feature = "live-voice")]
const DELEGATION_DELIVERY_FAILURE_INSTRUCTIONS: &str = concat!(
    "The latest update for the delegated request could not be delivered. Tell the user you ",
    "couldn't bring the update into this voice conversation and ask them to try again. Do not ",
    "say whether the delegated work succeeded or failed."
);

type LiveVoiceInteractionControls = Arc<Mutex<HashMap<String, Arc<LiveVoiceInteractionControl>>>>;
pub(crate) type LiveVoiceTranscriptPublisher = Arc<dyn Fn(Message) + Send + Sync>;
type LiveVoiceResolver =
    Arc<dyn Fn() -> Result<Arc<dyn LiveVoiceProvider>, &'static str> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveVoiceInteractionCompletion {
    Stopped,
    Failed,
}

pub(crate) struct StartLiveVoiceInteractionResult {
    pub(crate) interaction_id: LiveVoiceInteractionId,
    pub(crate) answer: WebRtcAnswer,
    pub(crate) completion_rx: watch::Receiver<Option<LiveVoiceInteractionCompletion>>,
}

#[derive(Debug)]
pub(crate) enum LiveVoiceError {
    Unavailable,
    StartFailed,
    StopFailed,
}

struct LiveVoiceInteractionControl {
    interaction_id: LiveVoiceInteractionId,
    stop_requested: CancellationToken,
    cleanup_finished: CancellationToken,
    completion_tx: watch::Sender<Option<LiveVoiceInteractionCompletion>>,
}

impl LiveVoiceInteractionControl {
    fn request_stop(&self) -> watch::Receiver<Option<LiveVoiceInteractionCompletion>> {
        self.stop_requested.cancel();
        self.completion_tx.subscribe()
    }

    fn request_cleanup(&self) -> CancellationToken {
        self.stop_requested.cancel();
        self.cleanup_finished.clone()
    }

    fn publish_completion(&self, completion: LiveVoiceInteractionCompletion) {
        if self.completion_tx.borrow().is_none() {
            self.completion_tx.send_replace(Some(completion));
        }
    }
}

/// Owns the interaction reservation from before agent preparation until cleanup finishes.
pub(crate) struct LiveVoiceInteractionGuard {
    active_runs: Arc<ActiveRunRegistry>,
    interactions_by_session: LiveVoiceInteractionControls,
    session_id: String,
    control: Arc<LiveVoiceInteractionControl>,
    completion_on_drop: Option<LiveVoiceInteractionCompletion>,
}

impl LiveVoiceInteractionGuard {
    fn new(
        active_runs: Arc<ActiveRunRegistry>,
        interactions_by_session: LiveVoiceInteractionControls,
        session_id: &str,
        control: Arc<LiveVoiceInteractionControl>,
    ) -> Self {
        Self {
            active_runs,
            interactions_by_session,
            session_id: session_id.to_string(),
            control,
            completion_on_drop: None,
        }
    }

    pub(super) fn publish_completion(&self, completion: LiveVoiceInteractionCompletion) {
        self.control.publish_completion(completion);
    }

    pub(super) fn publish_completion_after_cleanup(
        &mut self,
        completion: LiveVoiceInteractionCompletion,
    ) {
        self.completion_on_drop = Some(completion);
    }

    pub(super) fn stop_requested(&self) -> &CancellationToken {
        &self.control.stop_requested
    }

    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }

    #[cfg(test)]
    pub(super) fn for_test(session_id: &str) -> Self {
        let (completion_tx, _) = watch::channel(None);
        Self::new(
            Arc::new(ActiveRunRegistry::default()),
            Arc::new(Mutex::new(HashMap::new())),
            session_id,
            Arc::new(LiveVoiceInteractionControl {
                interaction_id: LiveVoiceInteractionId::new(),
                stop_requested: CancellationToken::new(),
                cleanup_finished: CancellationToken::new(),
                completion_tx,
            }),
        )
    }
}

impl Drop for LiveVoiceInteractionGuard {
    fn drop(&mut self) {
        remove_interaction_if_current(
            &self.interactions_by_session,
            &self.session_id,
            &self.control.interaction_id,
        );
        self.active_runs.finish_live(&self.session_id);
        self.publish_completion(
            self.completion_on_drop
                .unwrap_or(LiveVoiceInteractionCompletion::Failed),
        );
        self.control.cleanup_finished.cancel();
    }
}

pub struct LiveVoiceService {
    live_voice_resolver: LiveVoiceResolver,
    interactions_by_session: LiveVoiceInteractionControls,
    active_runs: Arc<ActiveRunRegistry>,
}

impl LiveVoiceService {
    pub fn from_config(active_runs: Arc<ActiveRunRegistry>) -> Self {
        Self::new(Arc::new(configured_live_voice), active_runs)
    }

    fn new(live_voice_resolver: LiveVoiceResolver, active_runs: Arc<ActiveRunRegistry>) -> Self {
        Self {
            live_voice_resolver,
            interactions_by_session: Arc::new(Mutex::new(HashMap::new())),
            active_runs,
        }
    }

    pub(crate) fn availability(
        &self,
        session_id: Option<&str>,
        mode: GooseMode,
    ) -> Result<(), &'static str> {
        self.eligible_provider(session_id, mode).map(|_| ())
    }

    fn eligible_provider(
        &self,
        session_id: Option<&str>,
        mode: GooseMode,
    ) -> Result<Arc<dyn LiveVoiceProvider>, &'static str> {
        let provider = self.provider_for_mode(mode)?;
        if session_id.is_some_and(|session_id| self.active_runs.is_active(session_id)) {
            Err("Live voice is unavailable while this session is busy")
        } else {
            Ok(provider)
        }
    }

    fn provider_for_mode(
        &self,
        mode: GooseMode,
    ) -> Result<Arc<dyn LiveVoiceProvider>, &'static str> {
        let provider = (self.live_voice_resolver)()?;
        if mode != GooseMode::Auto {
            Err("Live voice requires Autonomous mode")
        } else {
            Ok(provider)
        }
    }

    pub(crate) fn reserve_interaction(
        &self,
        session_id: &str,
        mode: GooseMode,
    ) -> Result<LiveVoiceInteractionGuard, LiveVoiceError> {
        self.eligible_provider(Some(session_id), mode)
            .map_err(|_| LiveVoiceError::Unavailable)?;
        let (completion_tx, _) = watch::channel(None);
        let control = Arc::new(LiveVoiceInteractionControl {
            interaction_id: LiveVoiceInteractionId::new(),
            stop_requested: CancellationToken::new(),
            cleanup_finished: CancellationToken::new(),
            completion_tx,
        });
        {
            let mut interactions = self
                .interactions_by_session
                .lock()
                .expect("live voice lock poisoned");
            if !self.active_runs.start_live(session_id) {
                return Err(LiveVoiceError::Unavailable);
            }
            interactions.insert(session_id.to_string(), control.clone());
        }
        Ok(LiveVoiceInteractionGuard::new(
            self.active_runs.clone(),
            self.interactions_by_session.clone(),
            session_id,
            control,
        ))
    }

    pub(crate) async fn start_interaction(
        &self,
        interaction_guard: LiveVoiceInteractionGuard,
        offer: WebRtcOffer,
        session_manager: Arc<SessionManager>,
        transcript_publisher: LiveVoiceTranscriptPublisher,
        main_agent: LiveMainAgent,
    ) -> Result<StartLiveVoiceInteractionResult, LiveVoiceError> {
        let session = session_manager
            .get_session(&interaction_guard.session_id, true)
            .await
            .map_err(|_| LiveVoiceError::Unavailable)?;
        let provider = self
            .provider_for_mode(session.goose_mode)
            .map_err(|_| LiveVoiceError::Unavailable)?;
        let input_messages =
            live_voice_input_messages(&session.conversation.unwrap_or_default()).await?;
        if interaction_guard.stop_requested().is_cancelled() {
            return Err(LiveVoiceError::Unavailable);
        }
        let (answer, provider_connection) = provider
            .start(offer, input_messages)
            .await
            .map_err(|_| LiveVoiceError::StartFailed)?;

        let interaction_id = interaction_guard.control.interaction_id.clone();
        let completion_rx = interaction_guard.control.completion_tx.subscribe();
        let interaction = LiveVoiceInteraction::new(
            provider_connection,
            session_manager,
            transcript_publisher,
            main_agent,
            interaction_guard,
        );
        tokio::spawn(interaction.run());
        Ok(StartLiveVoiceInteractionResult {
            interaction_id,
            answer,
            completion_rx,
        })
    }

    pub(crate) async fn stop_interaction(
        &self,
        session_id: &str,
        interaction_id: &LiveVoiceInteractionId,
    ) -> Result<(), LiveVoiceError> {
        let completion_rx = {
            let interactions = self
                .interactions_by_session
                .lock()
                .expect("live voice lock poisoned");
            let Some(control) = interactions.get(session_id) else {
                return Ok(());
            };
            if &control.interaction_id != interaction_id {
                return Ok(());
            }
            control.request_stop()
        };
        match wait_for_completion(completion_rx).await? {
            LiveVoiceInteractionCompletion::Stopped => Ok(()),
            LiveVoiceInteractionCompletion::Failed => Err(LiveVoiceError::StopFailed),
        }
    }

    pub(crate) async fn stop_session_interaction(&self, session_id: &str) {
        let cleanup_finished = {
            let interactions = self
                .interactions_by_session
                .lock()
                .expect("live voice lock poisoned");
            interactions
                .get(session_id)
                .map(|control| control.request_cleanup())
        };

        if let Some(cleanup_finished) = cleanup_finished {
            cleanup_finished.cancelled().await;
        }
    }
}

#[cfg(feature = "live-voice")]
fn configured_live_voice_enabled() -> bool {
    crate::config::Config::global()
        .get_param::<serde_json::Value>(LIVE_VOICE_ENABLED_CONFIG_KEY)
        .is_ok_and(|value| match value {
            serde_json::Value::Bool(enabled) => enabled,
            serde_json::Value::Number(enabled) => enabled.as_u64() == Some(1),
            serde_json::Value::String(enabled) => {
                enabled == "1" || enabled.eq_ignore_ascii_case("true")
            }
            _ => false,
        })
}

#[cfg(feature = "live-voice")]
fn configured_live_voice() -> Result<Arc<dyn LiveVoiceProvider>, &'static str> {
    if !configured_live_voice_enabled() {
        return Err("Live voice is disabled");
    }

    let config = crate::config::Config::global();
    let api_key = config
        .get_secret::<String>("OPENAI_API_KEY")
        .map_err(|_| "Live voice provider is not configured")?;
    let voice = config
        .get_param::<String>(LIVE_VOICE_CONFIG_KEY)
        .unwrap_or_else(|_| DEFAULT_OPENAI_LIVE_VOICE.into());
    OpenAiLiveVoiceProvider::new(
        api_key,
        voice,
        LIVE_SESSION_INSTRUCTIONS.into(),
        DELEGATION_DELIVERY_FAILURE_INSTRUCTIONS.into(),
    )
    .map(|provider| Arc::new(provider) as Arc<dyn LiveVoiceProvider>)
    .map_err(|_| "Live voice provider is not configured")
}

#[cfg(not(feature = "live-voice"))]
fn configured_live_voice() -> Result<Arc<dyn LiveVoiceProvider>, &'static str> {
    Err("Live voice is disabled")
}

async fn live_voice_input_messages(
    conversation: &Conversation,
) -> Result<Vec<LiveVoiceInputMessage>, LiveVoiceError> {
    let messages = conversation
        .messages()
        .iter()
        .filter(|message| message.is_user_visible())
        .filter_map(|message| {
            let text = message
                .user_visible_content()
                .content
                .iter()
                .filter_map(MessageContent::as_text)
                .collect::<String>();
            if text.trim().is_empty() {
                return None;
            }
            Some(LiveVoiceInputMessage {
                role: message.role.clone(),
                text,
            })
        })
        .collect::<Vec<_>>();
    if messages.is_empty() {
        return Ok(messages);
    }
    let token_counter = TokenCounter::new()
        .await
        .map_err(|_| LiveVoiceError::StartFailed)?;
    let mut start = messages.len();
    let mut tokens = 0;
    for (index, message) in messages.iter().enumerate().rev() {
        if messages.len() - start == LIVE_VOICE_INPUT_MESSAGE_LIMIT {
            break;
        }
        let message_tokens = token_counter.count_tokens(&message.text);
        if message_tokens > LIVE_VOICE_INPUT_TOKEN_LIMIT - tokens {
            break;
        }
        tokens += message_tokens;
        start = index;
    }
    Ok(messages.into_iter().skip(start).collect())
}

pub(crate) async fn wait_for_completion(
    mut completion_rx: watch::Receiver<Option<LiveVoiceInteractionCompletion>>,
) -> Result<LiveVoiceInteractionCompletion, LiveVoiceError> {
    loop {
        if let Some(completion) = *completion_rx.borrow() {
            return Ok(completion);
        }
        completion_rx
            .changed()
            .await
            .map_err(|_| LiveVoiceError::StopFailed)?;
    }
}

fn remove_interaction_if_current(
    interactions_by_session: &LiveVoiceInteractionControls,
    session_id: &str,
    interaction_id: &LiveVoiceInteractionId,
) {
    let mut interactions = interactions_by_session
        .lock()
        .expect("live voice lock poisoned");
    if matches!(
        interactions.get(session_id),
        Some(control) if &control.interaction_id == interaction_id
    ) {
        interactions.remove(session_id);
    }
}

#[cfg(test)]
mod tests;
