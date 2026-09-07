//! Provider-neutral commands and ordered events for a long-lived voice connection.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LiveVoiceMediaKind {
    #[default]
    WebRtc,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveVoiceCapabilities {
    pub available: bool,
    pub audio_input: bool,
    pub audio_output: bool,
    pub interruption: bool,
    pub transcription: bool,
    pub client_delegation: bool,
    pub typed_input: bool,
    pub media: LiveVoiceMediaKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveVoiceConfig {
    pub instructions: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveVoiceMediaRequest {
    WebRtc { offer_sdp: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveVoiceMediaAnswer {
    WebRtc { answer_sdp: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderCommandId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderDelegationId(pub String);

pub type ProviderTranscriptItemId = String;
pub type ProviderTurnId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveVoiceRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Opaque identity interpreted and validated only by the provider adapter.
pub struct ProviderStartupObservation(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderDelegationTarget {
    Client,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderDelegation {
    pub id: ProviderDelegationId,
    pub target: ProviderDelegationTarget,
    pub task: String,
    pub source_turn_id: Option<ProviderTurnId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectedTurnUpdateKind {
    Created,
    Delta,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptFragment {
    pub item_id: ProviderTranscriptItemId,
    pub role: LiveVoiceRole,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedTurn {
    pub turn_id: ProviderTurnId,
    pub role: Option<LiveVoiceRole>,
    /// A delta when `kind` is `Delta`; otherwise the provider's supplied text.
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: ProjectedTurnUpdateKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderCommand {
    ValidateStartupObservation {
        command_id: ProviderCommandId,
        observation: ProviderStartupObservation,
    },
    PauseInput {
        command_id: ProviderCommandId,
    },
    ResumeInput {
        command_id: ProviderCommandId,
    },
    DeliverDelegationResult {
        command_id: ProviderCommandId,
        delegation_id: ProviderDelegationId,
        result: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderDispatchResult {
    Dispatched,
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEvent {
    Ready,
    TranscriptFragment(TranscriptFragment),
    ProjectedTurn(ProjectedTurn),
    DelegationRequested(ProviderDelegation),
    InputPaused {
        command_id: ProviderCommandId,
    },
    InputResumed {
        command_id: ProviderCommandId,
    },
    DelegationResultAccepted {
        command_id: ProviderCommandId,
        delegation_id: ProviderDelegationId,
    },
    CommandRejected(Option<ProviderCommandId>, String),
    OutputActivityChanged(bool),
    UsageUpdated(ProviderUsage),
    RemoteClosed {
        reason: Option<String>,
        usage: Option<ProviderUsage>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEventStreamError {
    Lagged(u64),
    Failed(String),
}

pub type ProviderEventResult = std::result::Result<ProviderEvent, ProviderEventStreamError>;

/// The single authoritative receiver installed before provider start returns.
/// It is not cloneable or subscribable, and may already contain queued events.
pub struct ProviderEventReceiver {
    receiver: mpsc::UnboundedReceiver<ProviderEventResult>,
}

impl ProviderEventReceiver {
    pub fn new(receiver: mpsc::UnboundedReceiver<ProviderEventResult>) -> Self {
        Self { receiver }
    }

    pub async fn recv(&mut self) -> Option<ProviderEventResult> {
        self.receiver.recv().await
    }
}

pub struct ProviderConnection {
    pub media_answer: LiveVoiceMediaAnswer,
    pub control: Arc<dyn ProviderControl>,
    pub events: ProviderEventReceiver,
}

#[async_trait]
pub trait ProviderControl: Send + Sync {
    /// Reports only whether the command was dispatched or rejected locally.
    /// Provider acceptance or rejection arrives later through `ProviderEvent`.
    async fn dispatch(&self, command: ProviderCommand) -> ProviderDispatchResult;

    /// Initiates bounded, idempotent cleanup. Success is not remote acknowledgement;
    /// only `ProviderEvent::RemoteClosed` confirms the remote close.
    async fn close(&self) -> Result<()>;
}

#[async_trait]
pub trait LiveVoiceProvider: Send + Sync {
    fn capabilities(&self) -> LiveVoiceCapabilities;

    async fn start(
        &self,
        config: LiveVoiceConfig,
        media: LiveVoiceMediaRequest,
    ) -> Result<ProviderConnection>;
}

#[cfg(any(test, feature = "test-utils"))]
pub mod fake {
    use super::*;
    use futures::future::{BoxFuture, FutureExt, Shared};
    use std::sync::{Arc, Mutex};
    use tokio::sync::{mpsc, oneshot};

    pub fn channel(
        capabilities: LiveVoiceCapabilities,
    ) -> (
        Arc<FakeLiveVoiceProvider>,
        mpsc::UnboundedReceiver<FakeStartRequest>,
    ) {
        let (start_sender, start_receiver) = mpsc::unbounded_channel();
        (
            Arc::new(FakeLiveVoiceProvider {
                capabilities,
                start_sender,
            }),
            start_receiver,
        )
    }

    pub struct FakeLiveVoiceProvider {
        capabilities: LiveVoiceCapabilities,
        start_sender: mpsc::UnboundedSender<FakeStartRequest>,
    }

    #[async_trait]
    impl LiveVoiceProvider for FakeLiveVoiceProvider {
        fn capabilities(&self) -> LiveVoiceCapabilities {
            self.capabilities.clone()
        }

        async fn start(
            &self,
            config: LiveVoiceConfig,
            media: LiveVoiceMediaRequest,
        ) -> Result<ProviderConnection> {
            let (response_sender, response_receiver) = oneshot::channel();
            self.start_sender
                .send(FakeStartRequest {
                    config,
                    media,
                    response_sender,
                })
                .map_err(|_| anyhow::anyhow!("fake provider driver dropped"))?;
            response_receiver
                .await
                .map_err(|_| anyhow::anyhow!("fake provider start response dropped"))?
        }
    }

    pub struct FakeStartRequest {
        pub config: LiveVoiceConfig,
        pub media: LiveVoiceMediaRequest,
        response_sender: oneshot::Sender<Result<ProviderConnection>>,
    }

    impl FakeStartRequest {
        pub fn accept(self, media_answer: LiveVoiceMediaAnswer) -> Result<FakeConnectionDriver> {
            let (event_sender, event_receiver) = mpsc::unbounded_channel();
            let (command_sender, command_receiver) = mpsc::unbounded_channel();
            let (close_sender, close_receiver) = mpsc::unbounded_channel();
            let dispatch_result = Arc::new(Mutex::new(ProviderDispatchResult::Dispatched));
            let close = async move {
                let (response_sender, response_receiver) = oneshot::channel();
                close_sender
                    .send(response_sender)
                    .map_err(|_| "fake provider driver dropped".to_string())?;
                response_receiver
                    .await
                    .map_err(|_| "fake provider close response dropped".to_string())?
            }
            .boxed()
            .shared();
            let control = Arc::new(FakeProviderControl {
                command_sender,
                dispatch_result: dispatch_result.clone(),
                close,
            });
            self.response_sender
                .send(Ok(ProviderConnection {
                    media_answer,
                    control,
                    events: ProviderEventReceiver::new(event_receiver),
                }))
                .map_err(|_| anyhow::anyhow!("fake provider start caller dropped"))?;
            Ok(FakeConnectionDriver {
                event_sender,
                command_receiver,
                close_receiver,
                dispatch_result,
            })
        }

        pub fn reject(self, message: impl Into<String>) -> Result<()> {
            self.response_sender
                .send(Err(anyhow::anyhow!(message.into())))
                .map_err(|_| anyhow::anyhow!("fake provider start caller dropped"))
        }
    }

    pub struct FakeConnectionDriver {
        event_sender: mpsc::UnboundedSender<ProviderEventResult>,
        command_receiver: mpsc::UnboundedReceiver<ProviderCommand>,
        close_receiver: mpsc::UnboundedReceiver<oneshot::Sender<Result<(), String>>>,
        dispatch_result: Arc<Mutex<ProviderDispatchResult>>,
    }

    impl FakeConnectionDriver {
        pub fn emit(&self, event: ProviderEvent) -> Result<()> {
            self.send_event(Ok(event))
        }

        pub fn fail(&self, error: ProviderEventStreamError) -> Result<()> {
            self.send_event(Err(error))
        }

        fn send_event(&self, event: ProviderEventResult) -> Result<()> {
            self.event_sender
                .send(event)
                .map_err(|_| anyhow::anyhow!("fake provider event receiver dropped"))
        }

        pub fn set_dispatch_result(&self, result: ProviderDispatchResult) {
            *self
                .dispatch_result
                .lock()
                .expect("fake provider lock poisoned") = result;
        }

        pub async fn next_command(&mut self) -> Option<ProviderCommand> {
            self.command_receiver.recv().await
        }

        pub async fn next_close(&mut self) -> Option<oneshot::Sender<Result<(), String>>> {
            self.close_receiver.recv().await
        }
    }

    struct FakeProviderControl {
        command_sender: mpsc::UnboundedSender<ProviderCommand>,
        dispatch_result: Arc<Mutex<ProviderDispatchResult>>,
        close: Shared<BoxFuture<'static, Result<(), String>>>,
    }

    #[async_trait]
    impl ProviderControl for FakeProviderControl {
        async fn dispatch(&self, command: ProviderCommand) -> ProviderDispatchResult {
            let dispatch_result = self
                .dispatch_result
                .lock()
                .expect("fake provider lock poisoned")
                .clone();
            if let ProviderDispatchResult::Rejected(_) = dispatch_result {
                return dispatch_result;
            }
            if self.command_sender.send(command).is_err() {
                return ProviderDispatchResult::Rejected("fake provider driver dropped".into());
            }
            ProviderDispatchResult::Dispatched
        }

        async fn close(&self) -> Result<()> {
            self.close.clone().await.map_err(anyhow::Error::msg)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        async fn connect() -> (ProviderConnection, FakeConnectionDriver) {
            let (provider, mut starts) = channel(LiveVoiceCapabilities::default());
            let start = tokio::spawn(async move {
                provider
                    .start(
                        LiveVoiceConfig::default(),
                        LiveVoiceMediaRequest::WebRtc {
                            offer_sdp: "offer".into(),
                        },
                    )
                    .await
                    .unwrap()
            });
            let driver = starts
                .recv()
                .await
                .unwrap()
                .accept(LiveVoiceMediaAnswer::WebRtc {
                    answer_sdp: "answer".into(),
                })
                .unwrap();
            (start.await.unwrap(), driver)
        }

        #[tokio::test]
        async fn receiver_and_control_are_channel_driven() {
            let (mut connection, driver) = connect().await;
            driver.emit(ProviderEvent::Ready).unwrap();
            driver
                .emit(ProviderEvent::OutputActivityChanged(true))
                .unwrap();
            assert_eq!(
                connection.events.recv().await.unwrap().unwrap(),
                ProviderEvent::Ready
            );
            assert_eq!(
                connection.events.recv().await.unwrap().unwrap(),
                ProviderEvent::OutputActivityChanged(true)
            );

            let mut driver = driver;

            let dispatch = connection
                .control
                .dispatch(ProviderCommand::PauseInput {
                    command_id: ProviderCommandId("pause-1".to_string()),
                })
                .await;
            let command = driver.next_command().await.unwrap();
            assert!(matches!(
                &command,
                ProviderCommand::PauseInput { command_id } if command_id.0 == "pause-1"
            ));
            assert_eq!(dispatch, ProviderDispatchResult::Dispatched);

            let first_control = connection.control.clone();
            let first_close = tokio::spawn(async move { first_control.close().await });
            let _ = driver.next_close().await.unwrap().send(Ok(()));
            first_close.await.unwrap().unwrap();
            connection.control.close().await.unwrap();
            assert!(tokio::time::timeout(
                std::time::Duration::from_millis(10),
                driver.next_close()
            )
            .await
            .is_err());
        }

        #[tokio::test]
        async fn rejected_command_is_not_dispatched() {
            let (connection, mut driver) = connect().await;
            driver.set_dispatch_result(ProviderDispatchResult::Rejected("rejected".into()));

            let result = connection
                .control
                .dispatch(ProviderCommand::PauseInput {
                    command_id: ProviderCommandId("pause-1".into()),
                })
                .await;

            assert_eq!(result, ProviderDispatchResult::Rejected("rejected".into()));
            assert!(tokio::time::timeout(
                std::time::Duration::from_millis(10),
                driver.next_command()
            )
            .await
            .is_err());
        }

        #[tokio::test]
        async fn close_continues_after_the_caller_is_cancelled() {
            let (connection, mut driver) = connect().await;
            let control = connection.control.clone();
            let first_close = tokio::spawn(async move { control.close().await });
            let close_response = driver.next_close().await.unwrap();

            first_close.abort();
            assert!(first_close.await.unwrap_err().is_cancelled());
            close_response.send(Ok(())).unwrap();

            connection.control.close().await.unwrap();
            assert!(tokio::time::timeout(
                std::time::Duration::from_millis(10),
                driver.next_close()
            )
            .await
            .is_err());
        }
    }
}
