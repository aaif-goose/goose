//! The application-facing boundary for live voice providers.

use anyhow::Result;
use async_trait::async_trait;

const MAX_SDP_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcOffer(String);

impl WebRtcOffer {
    pub fn new(sdp: String) -> Option<Self> {
        valid_sdp(&sdp).then_some(Self(sdp))
    }

    pub fn into_sdp(self) -> String {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcAnswer(String);

impl WebRtcAnswer {
    pub fn new(sdp: String) -> Option<Self> {
        valid_sdp(&sdp).then_some(Self(sdp))
    }

    pub fn into_sdp(self) -> String {
        self.0
    }
}

fn valid_sdp(sdp: &str) -> bool {
    !sdp.is_empty() && sdp.len() <= MAX_SDP_BYTES
}

#[async_trait]
pub trait ProviderConnection: Send {
    async fn stop(&mut self) -> Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveVoiceProviderAvailability {
    Ready,
    Disabled,
    Unavailable,
}

#[async_trait]
pub trait LiveVoiceProvider: Send + Sync {
    fn availability(&self) -> LiveVoiceProviderAvailability;

    async fn start(
        &self,
        offer: WebRtcOffer,
    ) -> Result<(WebRtcAnswer, Box<dyn ProviderConnection>)>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdp_values_must_be_present_and_bounded() {
        assert!(WebRtcOffer::new(String::new()).is_none());
        assert!(WebRtcOffer::new("x".repeat(MAX_SDP_BYTES + 1)).is_none());
        assert!(WebRtcOffer::new("offer".into()).is_some());
        assert!(WebRtcAnswer::new("answer".into()).is_some());
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub mod fake {
    use super::*;
    use tokio::sync::{mpsc, oneshot};

    pub fn provider_channel() -> (
        std::sync::Arc<FakeLiveVoiceProvider>,
        mpsc::UnboundedReceiver<FakeStartRequest>,
    ) {
        provider_channel_with_availability(LiveVoiceProviderAvailability::Ready)
    }

    pub fn provider_channel_with_availability(
        availability: LiveVoiceProviderAvailability,
    ) -> (
        std::sync::Arc<FakeLiveVoiceProvider>,
        mpsc::UnboundedReceiver<FakeStartRequest>,
    ) {
        let (start_tx, start_rx) = mpsc::unbounded_channel();
        (
            std::sync::Arc::new(FakeLiveVoiceProvider {
                availability,
                start_tx,
            }),
            start_rx,
        )
    }

    pub struct FakeLiveVoiceProvider {
        availability: LiveVoiceProviderAvailability,
        start_tx: mpsc::UnboundedSender<FakeStartRequest>,
    }

    #[async_trait]
    impl LiveVoiceProvider for FakeLiveVoiceProvider {
        fn availability(&self) -> LiveVoiceProviderAvailability {
            self.availability
        }

        async fn start(
            &self,
            offer: WebRtcOffer,
        ) -> Result<(WebRtcAnswer, Box<dyn ProviderConnection>)> {
            let (response_tx, response_rx) = oneshot::channel();
            self.start_tx
                .send(FakeStartRequest { offer, response_tx })
                .map_err(|_| anyhow::anyhow!("fake provider driver dropped"))?;
            response_rx
                .await
                .map_err(|_| anyhow::anyhow!("fake provider start response dropped"))?
        }
    }

    pub struct FakeStartRequest {
        pub offer: WebRtcOffer,
        response_tx: oneshot::Sender<Result<(WebRtcAnswer, Box<dyn ProviderConnection>)>>,
    }

    impl FakeStartRequest {
        pub fn accept(self, answer: WebRtcAnswer) -> Result<FakeConnectionDriver> {
            let (stop_request_tx, stop_request_rx) = mpsc::unbounded_channel();
            self.response_tx
                .send(Ok((
                    answer,
                    Box::new(FakeProviderConnection { stop_request_tx }),
                )))
                .map_err(|_| anyhow::anyhow!("fake provider start caller dropped"))?;
            Ok(FakeConnectionDriver { stop_request_rx })
        }

        pub fn reject(self, message: impl Into<String>) -> Result<()> {
            self.response_tx
                .send(Err(anyhow::anyhow!(message.into())))
                .map_err(|_| anyhow::anyhow!("fake provider start caller dropped"))
        }
    }

    pub struct FakeConnectionDriver {
        stop_request_rx: mpsc::UnboundedReceiver<oneshot::Sender<Result<(), String>>>,
    }

    impl FakeConnectionDriver {
        pub async fn next_stop_request(&mut self) -> Option<oneshot::Sender<Result<(), String>>> {
            self.stop_request_rx.recv().await
        }
    }

    struct FakeProviderConnection {
        stop_request_tx: mpsc::UnboundedSender<oneshot::Sender<Result<(), String>>>,
    }

    #[async_trait]
    impl ProviderConnection for FakeProviderConnection {
        async fn stop(&mut self) -> Result<()> {
            let (response_tx, response_rx) = oneshot::channel();
            self.stop_request_tx
                .send(response_tx)
                .map_err(|_| anyhow::anyhow!("fake provider driver dropped"))?;
            response_rx
                .await
                .map_err(|_| anyhow::anyhow!("fake provider stop response dropped"))?
                .map_err(anyhow::Error::msg)
        }
    }
}
