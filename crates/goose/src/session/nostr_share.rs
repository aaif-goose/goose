use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use nostr::nips::nip19::{FromBech32, Nip19Event, ToBech32};
use nostr::nips::nip44;
use nostr::prelude::*;
use nostr_sdk::Client;

use crate::config::{Config, ConfigError};

pub const EVENT_KIND: u16 = 30278;
pub const SUGGESTION_EVENT_KIND: u16 = 30279;
pub const CONFIG_RELAYS_KEY: &str = "GOOSE_NOSTR_RELAYS";

const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://nos.lol",
    "wss://relay.nostr.band",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NostrShare {
    pub deeplink: String,
    pub nevent: String,
    pub event_id: String,
    pub relays: Vec<String>,
    pub encryption_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedShareLink {
    pub nevent: String,
    pub decryption_key: String,
}

#[async_trait]
pub trait NostrPublisher {
    async fn publish(&self, event: Event, relays: &[String]) -> Result<()>;
}

#[async_trait]
pub trait NostrFetcher {
    async fn fetch(&self, event_id: EventId, relays: &[String]) -> Result<Event>;
}

pub struct LiveNostrClient;

#[async_trait]
impl NostrPublisher for LiveNostrClient {
    async fn publish(&self, event: Event, relays: &[String]) -> Result<()> {
        install_rustls_crypto_provider();
        let client = Client::default();
        for relay in relays {
            client
                .add_relay(relay)
                .await
                .with_context(|| format!("Failed to add relay {relay}"))?;
        }

        client.try_connect(Duration::from_secs(8)).await;
        let output = client
            .send_event_to(relays.iter().map(String::as_str), &event)
            .await
            .context("Failed to publish session to Nostr relays")?;
        client.shutdown().await;

        if output.success.is_empty() {
            return Err(anyhow!(
                "Failed to publish session to any Nostr relay: {:?}",
                output.failed
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl NostrFetcher for LiveNostrClient {
    async fn fetch(&self, event_id: EventId, relays: &[String]) -> Result<Event> {
        install_rustls_crypto_provider();
        let client = Client::default();
        for relay in relays {
            client
                .add_relay(relay)
                .await
                .with_context(|| format!("Failed to add relay {relay}"))?;
        }

        client.try_connect(Duration::from_secs(8)).await;
        let filter = Filter::new()
            .id(event_id)
            .kind(Kind::Custom(EVENT_KIND))
            .limit(1);
        let events = client
            .fetch_events_from(
                relays.iter().map(String::as_str),
                filter,
                Duration::from_secs(10),
            )
            .await
            .context("Failed to fetch shared session from Nostr relays")?;
        client.shutdown().await;

        events
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Shared session event not found"))
    }
}

#[cfg(feature = "rustls-tls")]
fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(not(feature = "rustls-tls"))]
fn install_rustls_crypto_provider() {}

pub fn default_relays() -> Vec<String> {
    DEFAULT_RELAYS
        .iter()
        .map(|relay| relay.to_string())
        .collect()
}

pub fn relays_from_config(config: &Config) -> Vec<String> {
    match config.get_param::<Vec<String>>(CONFIG_RELAYS_KEY) {
        Ok(relays) if !relays.is_empty() => normalize_relays(relays),
        Err(ConfigError::NotFound(_)) => default_relays(),
        _ => default_relays(),
    }
}

pub fn resolve_relays(cli_relays: Vec<String>, config: &Config) -> Vec<String> {
    if cli_relays.is_empty() {
        relays_from_config(config)
    } else {
        normalize_relays(cli_relays)
    }
}

pub async fn publish_session_json(session_json: &str, relays: Vec<String>) -> Result<NostrShare> {
    publish_session_json_with(session_json, relays, &LiveNostrClient).await
}

pub async fn publish_session_json_with<P>(
    session_json: &str,
    relays: Vec<String>,
    publisher: &P,
) -> Result<NostrShare>
where
    P: NostrPublisher + Sync,
{
    let relays = normalize_relays(relays);
    if relays.is_empty() {
        return Err(anyhow!("At least one Nostr relay is required"));
    }
    let relay_urls = relays
        .iter()
        .map(|relay| RelayUrl::parse(relay))
        .collect::<Result<Vec<_>, _>>()?;

    let publish_keys = Keys::generate();
    let encryption_key = SecretKey::generate();
    let encryption_keys = Keys::new(encryption_key.clone());
    let encrypted = nip44::encrypt(
        &encryption_key,
        &encryption_keys.public_key(),
        session_json,
        nip44::Version::V2,
    )?;

    let event = EventBuilder::new(Kind::Custom(EVENT_KIND), encrypted)
        .tag(Tag::identifier(format!(
            "goose-session-{}",
            uuid::Uuid::now_v7()
        )))
        .tag(Tag::parse(["client", "goose"])?)
        .sign_with_keys(&publish_keys)?;

    publisher.publish(event.clone(), &relays).await?;

    let nevent = Nip19Event::new(event.id)
        .author(event.pubkey)
        .kind(Kind::Custom(EVENT_KIND))
        .relays(relay_urls)
        .to_bech32()?;
    let decryption_key = encryption_key.to_secret_hex();
    let deeplink = build_deeplink(&nevent, &decryption_key);

    Ok(NostrShare {
        deeplink,
        nevent,
        event_id: event.id.to_hex(),
        relays,
        encryption_key: decryption_key,
    })
}

pub async fn import_session_json_from_deeplink(deeplink: &str) -> Result<String> {
    import_session_json_from_deeplink_with(deeplink, &LiveNostrClient).await
}

pub async fn import_session_json_from_deeplink_with<F>(
    deeplink: &str,
    fetcher: &F,
) -> Result<String>
where
    F: NostrFetcher + Sync,
{
    let ParsedShareLink {
        nevent,
        decryption_key,
    } = parse_deeplink(deeplink)?;
    let event_ref = Nip19Event::from_bech32(&nevent)?;
    let relays = event_ref
        .relays
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    if relays.is_empty() {
        return Err(anyhow!("Shared session link does not include any relays"));
    }

    let event = fetcher.fetch(event_ref.event_id, &relays).await?;
    if event.kind != Kind::Custom(EVENT_KIND) {
        return Err(anyhow!(
            "Unexpected Nostr event kind: {}",
            u16::from(event.kind)
        ));
    }

    let secret_key = SecretKey::parse(&decryption_key)?;
    let encryption_keys = Keys::new(secret_key.clone());
    nip44::decrypt(&secret_key, &encryption_keys.public_key(), event.content).map_err(Into::into)
}

pub fn build_deeplink(nevent: &str, decryption_key: &str) -> String {
    format!(
        "goose://sessions/nostr?nevent={}&key={}",
        urlencoding::encode(nevent),
        urlencoding::encode(decryption_key)
    )
}

/// Extract event ID (hex) and relay URLs from a bech32 nevent string.
pub fn parse_nevent(nevent: &str) -> Result<(String, Vec<String>)> {
    let event_ref = Nip19Event::from_bech32(nevent)?;
    let relays = event_ref.relays.iter().map(ToString::to_string).collect();
    Ok((event_ref.event_id.to_hex(), relays))
}

pub fn parse_deeplink(deeplink: &str) -> Result<ParsedShareLink> {
    let parsed = url::Url::parse(deeplink).context("Invalid Goose session share link")?;
    if parsed.scheme() != "goose"
        || parsed.host_str() != Some("sessions")
        || parsed.path() != "/nostr"
    {
        return Err(anyhow!("Invalid Goose Nostr session share link"));
    }

    let nevent = parsed
        .query_pairs()
        .find_map(|(key, value)| (key == "nevent").then(|| value.into_owned()))
        .ok_or_else(|| anyhow!("Missing nevent parameter"))?;
    let decryption_key = parsed
        .query_pairs()
        .find_map(|(key, value)| (key == "key").then(|| value.into_owned()))
        .ok_or_else(|| anyhow!("Missing decryption key parameter"))?;

    Ok(ParsedShareLink {
        nevent,
        decryption_key,
    })
}

fn normalize_relays(relays: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for relay in relays {
        let relay = relay.trim();
        if relay.is_empty() || normalized.iter().any(|existing| existing == relay) {
            continue;
        }
        normalized.push(relay.to_string());
    }
    normalized
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub text: String,
    pub sender_name: Option<String>,
    pub event_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SuggestionPayload {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sender_name: Option<String>,
}

pub async fn publish_suggestion(
    encryption_key_hex: &str,
    original_event_id: &str,
    text: &str,
    sender_name: Option<&str>,
    relays: Vec<String>,
) -> Result<()> {
    publish_suggestion_with(
        encryption_key_hex,
        original_event_id,
        text,
        sender_name,
        relays,
        &LiveNostrClient,
    )
    .await
}

pub async fn publish_suggestion_with<P>(
    encryption_key_hex: &str,
    original_event_id: &str,
    text: &str,
    sender_name: Option<&str>,
    relays: Vec<String>,
    publisher: &P,
) -> Result<()>
where
    P: NostrPublisher + Sync,
{
    let relays = normalize_relays(relays);
    if relays.is_empty() {
        return Err(anyhow!("At least one Nostr relay is required"));
    }

    let payload = SuggestionPayload {
        text: text.to_string(),
        sender_name: sender_name.map(|s| s.to_string()),
    };
    let payload_json = serde_json::to_string(&payload)?;

    let secret_key = SecretKey::parse(encryption_key_hex)?;
    let encryption_keys = Keys::new(secret_key.clone());
    let encrypted = nip44::encrypt(
        &secret_key,
        &encryption_keys.public_key(),
        &payload_json,
        nip44::Version::V2,
    )?;

    let signing_keys = Keys::generate();
    let parent_id = EventId::parse(original_event_id)?;
    let event = EventBuilder::new(Kind::Custom(SUGGESTION_EVENT_KIND), encrypted)
        .tag(Tag::event(parent_id))
        .tag(Tag::parse(["client", "goose"])?)
        .tag(Tag::parse(["t", "suggestion"])?)
        .sign_with_keys(&signing_keys)?;

    publisher.publish(event, &relays).await
}

pub async fn fetch_suggestions(
    encryption_key_hex: &str,
    original_event_id: &str,
    relays: Vec<String>,
    since: Option<u64>,
) -> Result<Vec<Suggestion>> {
    fetch_suggestions_with(
        encryption_key_hex,
        original_event_id,
        relays,
        since,
        &LiveNostrClient,
    )
    .await
}

pub async fn fetch_suggestions_with<F>(
    encryption_key_hex: &str,
    original_event_id: &str,
    relays: Vec<String>,
    since: Option<u64>,
    fetcher: &F,
) -> Result<Vec<Suggestion>>
where
    F: NostrSuggestionFetcher + Sync,
{
    let relays = normalize_relays(relays);
    if relays.is_empty() {
        return Err(anyhow!("At least one Nostr relay is required"));
    }

    let parent_id = EventId::parse(original_event_id)?;
    let events = fetcher.fetch_suggestions(parent_id, &relays, since).await?;

    let secret_key = SecretKey::parse(encryption_key_hex)?;
    let encryption_keys = Keys::new(secret_key.clone());

    let mut suggestions = Vec::new();
    for event in events {
        match nip44::decrypt(&secret_key, &encryption_keys.public_key(), &event.content) {
            Ok(decrypted) => {
                // Try JSON payload first, fall back to plain text for backwards compat
                let (text, sender_name) =
                    if let Ok(payload) = serde_json::from_str::<SuggestionPayload>(&decrypted) {
                        (payload.text, payload.sender_name)
                    } else {
                        (decrypted, None)
                    };
                suggestions.push(Suggestion {
                    text,
                    sender_name,
                    event_id: event.id.to_hex(),
                    timestamp: event.created_at.as_secs(),
                });
            }
            Err(_) => continue, // skip events we can't decrypt
        }
    }

    suggestions.sort_by_key(|s| s.timestamp);
    Ok(suggestions)
}

#[async_trait]
pub trait NostrSuggestionFetcher {
    async fn fetch_suggestions(
        &self,
        parent_event_id: EventId,
        relays: &[String],
        since: Option<u64>,
    ) -> Result<Vec<Event>>;
}

#[async_trait]
impl NostrSuggestionFetcher for LiveNostrClient {
    async fn fetch_suggestions(
        &self,
        parent_event_id: EventId,
        relays: &[String],
        since: Option<u64>,
    ) -> Result<Vec<Event>> {
        install_rustls_crypto_provider();
        let client = Client::default();
        for relay in relays {
            let _ = client.add_relay(relay).await;
        }

        client.try_connect(Duration::from_secs(3)).await;
        let mut filter = Filter::new()
            .kind(Kind::Custom(SUGGESTION_EVENT_KIND))
            .event(parent_event_id);
        if let Some(ts) = since {
            filter = filter.since(Timestamp::from(ts));
        }
        let events = client
            .fetch_events_from(
                relays.iter().map(String::as_str),
                filter,
                Duration::from_secs(5),
            )
            .await
            .context("Failed to fetch suggestions from Nostr relays")?;
        let _ = client.shutdown().await;

        Ok(events.into_iter().collect())
    }
}

/// Subscribe to real-time suggestions via Nostr relay subscription.
/// Returns a tokio mpsc receiver that yields suggestions as they arrive.
/// The subscription runs until the receiver is dropped.
pub async fn subscribe_suggestions(
    encryption_key_hex: &str,
    original_event_id: &str,
    relays: Vec<String>,
) -> Result<tokio::sync::mpsc::Receiver<Suggestion>> {
    let relays = normalize_relays(relays);
    if relays.is_empty() {
        return Err(anyhow!("At least one Nostr relay is required"));
    }

    install_rustls_crypto_provider();
    let client = Client::default();
    for relay in &relays {
        let _ = client.add_relay(relay).await;
    }
    client.try_connect(Duration::from_secs(5)).await;

    let parent_id = EventId::parse(original_event_id)?;
    let filter = Filter::new()
        .kind(Kind::Custom(SUGGESTION_EVENT_KIND))
        .event(parent_id)
        .since(Timestamp::now());

    client
        .subscribe(filter, None)
        .await
        .map_err(|e| anyhow!("Failed to subscribe: {e}"))?;

    let secret_key = SecretKey::parse(encryption_key_hex)?;
    let encryption_keys = Keys::new(secret_key.clone());

    let (tx, rx) = tokio::sync::mpsc::channel::<Suggestion>(32);

    tokio::spawn(async move {
        let _ = client
            .handle_notifications(|notification| {
                let tx = tx.clone();
                let secret_key = secret_key.clone();
                let encryption_keys = encryption_keys.clone();
                async move {
                    if tx.is_closed() {
                        return Ok(true);
                    }
                    if let nostr_sdk::RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind == Kind::Custom(SUGGESTION_EVENT_KIND) {
                            if let Ok(decrypted) = nip44::decrypt(
                                &secret_key,
                                &encryption_keys.public_key(),
                                &event.content,
                            ) {
                                let (text, sender_name) = if let Ok(payload) =
                                    serde_json::from_str::<SuggestionPayload>(&decrypted)
                                {
                                    (payload.text, payload.sender_name)
                                } else {
                                    (decrypted, None)
                                };

                                let suggestion = Suggestion {
                                    text,
                                    sender_name,
                                    event_id: event.id.to_hex(),
                                    timestamp: event.created_at.as_secs(),
                                };
                                if tx.send(suggestion).await.is_err() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    Ok(false)
                }
            })
            .await;
        let _ = client.shutdown().await;
    });

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct RecordingPublisher {
        event: Arc<Mutex<Option<Event>>>,
        relays: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl NostrPublisher for RecordingPublisher {
        async fn publish(&self, event: Event, relays: &[String]) -> Result<()> {
            *self.event.lock().unwrap() = Some(event);
            *self.relays.lock().unwrap() = relays.to_vec();
            Ok(())
        }
    }

    struct StaticFetcher(Event);

    #[async_trait]
    impl NostrFetcher for StaticFetcher {
        async fn fetch(&self, _event_id: EventId, _relays: &[String]) -> Result<Event> {
            Ok(self.0.clone())
        }
    }

    #[tokio::test]
    async fn publish_builds_deeplink_and_encrypted_kind_30278_event() {
        let event = Arc::new(Mutex::new(None));
        let relays = Arc::new(Mutex::new(Vec::new()));
        let publisher = RecordingPublisher {
            event: event.clone(),
            relays: relays.clone(),
        };

        let share = publish_session_json_with(
            r#"{"id":"session-id","conversation":{"messages":[]}}"#,
            vec!["wss://relay.example".to_string()],
            &publisher,
        )
        .await
        .unwrap();

        assert!(share.deeplink.starts_with("goose://sessions/nostr?"));
        assert!(share.nevent.starts_with("nevent1"));
        assert_eq!(share.relays, vec!["wss://relay.example"]);
        assert_eq!(*relays.lock().unwrap(), vec!["wss://relay.example"]);

        let event = event.lock().unwrap().clone().unwrap();
        assert_eq!(event.kind, Kind::Custom(EVENT_KIND));
        assert_ne!(
            event.content,
            r#"{"id":"session-id","conversation":{"messages":[]}}"#
        );
    }

    #[tokio::test]
    async fn publish_and_import_round_trips_session_json() {
        let event = Arc::new(Mutex::new(None));
        let publisher = RecordingPublisher {
            event: event.clone(),
            relays: Arc::new(Mutex::new(Vec::new())),
        };
        let json = r#"{"id":"session-id","name":"shared"}"#;

        let share =
            publish_session_json_with(json, vec!["wss://relay.example".to_string()], &publisher)
                .await
                .unwrap();

        let fetched_event = event.lock().unwrap().clone().unwrap();
        let imported =
            import_session_json_from_deeplink_with(&share.deeplink, &StaticFetcher(fetched_event))
                .await
                .unwrap();

        assert_eq!(imported, json);
    }

    #[test]
    fn parses_deeplink() {
        let parsed = parse_deeplink("goose://sessions/nostr?nevent=abc&key=def").unwrap();
        assert_eq!(parsed.nevent, "abc");
        assert_eq!(parsed.decryption_key, "def");
    }
}
