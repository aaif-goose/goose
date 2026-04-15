use super::{
    Gateway, GatewayConfig, GatewayHandler, IncomingMessage, OutgoingMessage, PlatformUser,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
const POLL_TIMEOUT_SECS: u64 = 30;
const MAX_MESSAGE_LENGTH: usize = 4096;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
/// Maximum voice file size we'll attempt to download (20 MB, Telegram's bot API limit).
const MAX_VOICE_FILE_SIZE: i64 = 20 * 1024 * 1024;

pub struct TelegramGateway {
    bot_token: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    from: Option<TelegramUser>,
    chat: TelegramChat,
    text: Option<String>,
    voice: Option<TelegramVoice>,
    audio: Option<TelegramAudio>,
}

#[derive(Debug, Deserialize)]
struct TelegramVoice {
    file_id: String,
    #[allow(dead_code)]
    duration: Option<i32>,
    #[allow(dead_code)]
    mime_type: Option<String>,
    file_size: Option<i64>,
}

/// Audio files sent as documents (not inline voice notes).
#[derive(Debug, Deserialize)]
struct TelegramAudio {
    file_id: String,
    #[allow(dead_code)]
    duration: Option<i32>,
    #[allow(dead_code)]
    mime_type: Option<String>,
    file_size: Option<i64>,
}

/// Metadata extracted from a Telegram voice note or audio attachment.
struct VoiceInfo<'a> {
    file_id: &'a str,
    file_size: Option<i64>,
    duration: Option<i32>,
    mime_type: Option<&'a str>,
}

/// Response from the Telegram `getFile` API.
#[derive(Debug, Deserialize)]
struct TelegramFile {
    #[allow(dead_code)]
    file_id: String,
    file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    first_name: String,
    last_name: Option<String>,
    #[allow(dead_code)]
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    chat_type: String,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

impl TelegramGateway {
    pub fn new(config: &GatewayConfig) -> anyhow::Result<Self> {
        let bot_token = config.platform_config["bot_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing bot_token in platform_config"))?
            .to_string();

        Ok(Self {
            bot_token,
            client: Client::new(),
        })
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", TELEGRAM_API_BASE, self.bot_token, method)
    }

    async fn get_updates(&self, offset: Option<i64>) -> anyhow::Result<Vec<TelegramUpdate>> {
        let mut params = serde_json::json!({
            "timeout": POLL_TIMEOUT_SECS,
            "allowed_updates": ["message"],
        });
        if let Some(offset) = offset {
            params["offset"] = serde_json::json!(offset);
        }

        let resp: TelegramResponse<Vec<TelegramUpdate>> = self
            .client
            .post(self.api_url("getUpdates"))
            .json(&params)
            .timeout(std::time::Duration::from_secs(POLL_TIMEOUT_SECS + 10))
            .send()
            .await?
            .json()
            .await?;

        resp.result.ok_or_else(|| {
            anyhow::anyhow!(
                "Telegram API error: {}",
                resp.description.unwrap_or_default()
            )
        })
    }

    async fn send_text(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        let html = super::telegram_format::markdown_to_telegram_html(text);
        for chunk in split_message(&html, MAX_MESSAGE_LENGTH) {
            let resp = self
                .client
                .post(self.api_url("sendMessage"))
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": chunk,
                    "parse_mode": "HTML",
                }))
                .send()
                .await?;

            if let Ok(body) = resp.json::<TelegramResponse<serde_json::Value>>().await {
                if !body.ok {
                    tracing::warn!(
                        error = body.description.as_deref().unwrap_or("unknown"),
                        "Telegram rejected HTML, falling back to plain text"
                    );
                    for plain_chunk in split_message(text, MAX_MESSAGE_LENGTH) {
                        self.client
                            .post(self.api_url("sendMessage"))
                            .json(&serde_json::json!({
                                "chat_id": chat_id,
                                "text": plain_chunk,
                            }))
                            .send()
                            .await?;
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    async fn send_chat_action(&self, chat_id: i64, action: &str) -> anyhow::Result<()> {
        self.client
            .post(self.api_url("sendChatAction"))
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "action": action,
            }))
            .send()
            .await?;
        Ok(())
    }

    /// Download a file from Telegram by its `file_id`.
    ///
    /// This is a two-step process:
    /// 1. Call `getFile` to obtain the server-side `file_path`.
    /// 2. Fetch the raw bytes from `https://api.telegram.org/file/bot<TOKEN>/<file_path>`.
    async fn download_file(&self, file_id: &str) -> anyhow::Result<Vec<u8>> {
        // Step 1 – resolve file_id → file_path
        let resp: TelegramResponse<TelegramFile> = self
            .client
            .post(self.api_url("getFile"))
            .json(&serde_json::json!({ "file_id": file_id }))
            .send()
            .await?
            .json()
            .await?;

        let tg_file = resp.result.ok_or_else(|| {
            anyhow::anyhow!(
                "Telegram getFile error: {}",
                resp.description.unwrap_or_default()
            )
        })?;

        let file_path = tg_file
            .file_path
            .ok_or_else(|| anyhow::anyhow!("Telegram getFile returned no file_path"))?;

        // Step 2 – download raw bytes
        let download_url = format!(
            "{}/file/bot{}/{}",
            TELEGRAM_API_BASE, self.bot_token, file_path
        );
        let bytes = self.client.get(&download_url).send().await?.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Save voice bytes to a temporary file and return the path.
    ///
    /// Files are stored under `<tmp>/goose_voice/voice_<uuid>.<ext>` so Goose
    /// can access them via its shell tools.  The extension is derived from the
    /// MIME type when available, falling back to `.ogg` for voice notes.
    ///
    /// On Unix the directory is created with mode `0700` and files with `0600`
    /// so other local users cannot read private voice content.
    fn save_voice_file(
        bytes: &[u8],
        mime_type: Option<&str>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let dir = std::env::temp_dir().join("goose_voice");
        std::fs::create_dir_all(&dir)?;

        // Restrict directory permissions to owner-only on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }

        let ext = mime_type
            .and_then(|m| m.rsplit('/').next())
            .map(|sub| {
                // Normalise common MIME sub-types to file extensions.
                match sub {
                    "mpeg" => "mp3",
                    "mp4" | "x-m4a" => "m4a",
                    "ogg" => "ogg",
                    "wav" | "x-wav" => "wav",
                    other => other,
                }
            })
            .unwrap_or("ogg");

        let filename = format!("voice_{}.{ext}", uuid::Uuid::new_v4());
        let path = dir.join(filename);
        std::fs::write(&path, bytes)?;

        // Restrict file permissions to owner-only on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(path)
    }

    /// Build the text prompt that tells Goose about a voice message file.
    fn voice_prompt(
        path: &std::path::Path,
        duration: Option<i32>,
        mime_type: Option<&str>,
    ) -> String {
        let duration_hint = duration
            .map(|d| format!(" (duration: {d}s)"))
            .unwrap_or_default();
        let format_hint = mime_type
            .map(|m| format!(" The file format is {m}."))
            .unwrap_or_default();
        format!(
            "The user sent a voice message{duration_hint}. \
             The audio file is saved at: {}{format_hint}\n\n\
             Please transcribe this audio file using available command-line tools \
             (e.g. whisper, ffmpeg, sox, or any STT utility you can find on this system) \
             and then respond to what the user said. \
             If no transcription tool is available, let the user know and ask them to type their message instead.",
            path.display()
        )
    }

    /// Extract metadata from either a voice note or an audio attachment.
    /// Returns `None` when neither is present.
    fn voice_info(msg: &TelegramMessage) -> Option<VoiceInfo<'_>> {
        if let Some(ref v) = msg.voice {
            return Some(VoiceInfo {
                file_id: &v.file_id,
                file_size: v.file_size,
                duration: v.duration,
                mime_type: v.mime_type.as_deref(),
            });
        }
        if let Some(ref a) = msg.audio {
            return Some(VoiceInfo {
                file_id: &a.file_id,
                file_size: a.file_size,
                duration: a.duration,
                mime_type: a.mime_type.as_deref(),
            });
        }
        None
    }

    fn to_platform_user(tg_msg: &TelegramMessage) -> PlatformUser {
        PlatformUser {
            platform: "telegram".to_string(),
            user_id: tg_msg.chat.id.to_string(),
            display_name: tg_msg.from.as_ref().map(|u| {
                let mut name = u.first_name.clone();
                if let Some(ref last) = u.last_name {
                    name.push(' ');
                    name.push_str(last);
                }
                name
            }),
        }
    }
}

#[async_trait]
impl Gateway for TelegramGateway {
    fn gateway_type(&self) -> &str {
        "telegram"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut offset: Option<i64> = None;

        tracing::info!("Telegram gateway starting long-poll loop");

        // Spawn a background task that periodically removes stale voice files
        // (older than 1 hour) so they don't accumulate on disk.
        let cleanup_cancel = cancel.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
            loop {
                tokio::select! {
                    _ = cleanup_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        cleanup_voice_files(std::time::Duration::from_secs(3600));
                    }
                }
            }
        });

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Telegram gateway shutting down");
                    break;
                }
                result = self.get_updates(offset) => {
                    match result {
                        Ok(updates) => {
                            for update in updates {
                                offset = Some(update.update_id + 1);

                                let Some(tg_msg) = update.message else {
                                    continue;
                                };

                                // Determine the text to send to the handler.
                                // Voice/audio messages are downloaded, saved to
                                // disk, and converted into a prompt that asks
                                // Goose to transcribe the file using CLI tools.
                                let text = if let Some(voice) = Self::voice_info(&tg_msg) {
                                    // Reject files that exceed the Telegram bot
                                    // download limit.
                                    if voice.file_size.unwrap_or(0) > MAX_VOICE_FILE_SIZE {
                                        tracing::warn!(
                                            file_size = voice.file_size,
                                            "voice file exceeds size limit, skipping"
                                        );
                                        continue;
                                    }

                                    match self.download_file(voice.file_id).await {
                                        Ok(bytes) => match Self::save_voice_file(&bytes, voice.mime_type) {
                                            Ok(path) => Self::voice_prompt(&path, voice.duration, voice.mime_type),
                                            Err(e) => {
                                                tracing::error!(
                                                    error = %e,
                                                    "failed to save voice file"
                                                );
                                                continue;
                                            }
                                        },
                                        Err(e) => {
                                            tracing::error!(
                                                error = %e,
                                                "failed to download voice file from Telegram"
                                            );
                                            continue;
                                        }
                                    }
                                } else if let Some(ref t) = tg_msg.text {
                                    t.clone()
                                } else {
                                    // Neither text nor voice — skip.
                                    continue;
                                };

                                let user = Self::to_platform_user(&tg_msg);
                                let incoming = IncomingMessage {
                                    user,
                                    text,
                                    platform_message_id: Some(tg_msg.message_id.to_string()),
                                    attachments: vec![],
                                };

                                let handler = handler.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handler.handle_message(incoming).await {
                                        tracing::error!(error = %e, "error handling Telegram message");
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Telegram poll error");
                            tokio::time::sleep(RETRY_DELAY).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        let chat_id: i64 = user
            .user_id
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid chat_id: {}", user.user_id))?;

        match message {
            OutgoingMessage::Text { body } => {
                self.send_text(chat_id, &body).await?;
            }
            OutgoingMessage::Typing => {
                self.send_chat_action(chat_id, "typing").await?;
            }
        }

        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        let resp: TelegramResponse<serde_json::Value> = self
            .client
            .get(self.api_url("getMe"))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "invalid Telegram bot token: {}",
                resp.description.unwrap_or_default()
            );
        }

        if let Some(result) = &resp.result {
            if let Some(username) = result.get("username").and_then(|v| v.as_str()) {
                tracing::info!(bot = %username, "Telegram bot verified");
            }
        }

        Ok(())
    }
}

/// Remove voice files from the temp directory that are older than `max_age`.
fn cleanup_voice_files(max_age: std::time::Duration) {
    let dir = std::env::temp_dir().join("goose_voice");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now() - max_age;
    let mut removed = 0u32;
    for entry in entries.flatten() {
        let dominated = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .is_some_and(|t| t < cutoff);
        if dominated && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::debug!(removed, "cleaned up stale voice files");
    }
}

#[allow(clippy::string_slice)]
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        let mut cut = max_len;
        while cut > 0 && !remaining.is_char_boundary(cut) {
            cut -= 1;
        }
        if cut == 0 {
            cut = remaining
                .char_indices()
                .nth(1)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());
        }

        let split_at = remaining[..cut]
            .rfind('\n')
            .or_else(|| remaining[..cut].rfind(' '))
            .map(|pos| pos + 1)
            .unwrap_or(cut);

        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_short_message() {
        let chunks = split_message("hello world", 4096);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn split_at_newline() {
        let text = format!("{}\n{}", "a".repeat(4000), "b".repeat(200));
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4001);
        assert_eq!(chunks[1].len(), 200);
    }

    #[test]
    fn split_at_space() {
        let text = format!("{} {}", "a".repeat(4000), "b".repeat(200));
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4001);
        assert_eq!(chunks[1].len(), 200);
    }

    #[test]
    fn split_no_boundary() {
        let text = "a".repeat(5000);
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4096);
        assert_eq!(chunks[1].len(), 904);
    }

    #[test]
    fn split_exact_boundary() {
        let text = "a".repeat(4096);
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn split_empty() {
        let chunks = split_message("", 4096);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn split_multiple_chunks() {
        let text = format!(
            "{}\n{}\n{}",
            "a".repeat(4000),
            "b".repeat(4000),
            "c".repeat(4000)
        );
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn split_multibyte_chars() {
        let text = "🦆".repeat(1025); // 4100 bytes
        let chunks = split_message(&text, 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chars().count(), 1024);
        assert_eq!(chunks[1].chars().count(), 1);
    }

    #[test]
    fn voice_info_from_voice_message() {
        let msg = TelegramMessage {
            message_id: 1,
            from: None,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".into(),
            },
            text: None,
            voice: Some(TelegramVoice {
                file_id: "voice_file_123".into(),
                duration: Some(5),
                mime_type: Some("audio/ogg".into()),
                file_size: Some(10000),
            }),
            audio: None,
        };
        let info = TelegramGateway::voice_info(&msg);
        assert!(info.is_some());
        let v = info.unwrap();
        assert_eq!(v.file_id, "voice_file_123");
        assert_eq!(v.file_size, Some(10000));
        assert_eq!(v.duration, Some(5));
        assert_eq!(v.mime_type, Some("audio/ogg"));
    }

    #[test]
    fn voice_info_from_audio_message() {
        let msg = TelegramMessage {
            message_id: 1,
            from: None,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".into(),
            },
            text: None,
            voice: None,
            audio: Some(TelegramAudio {
                file_id: "audio_file_456".into(),
                duration: Some(120),
                mime_type: Some("audio/mpeg".into()),
                file_size: Some(500_000),
            }),
        };
        let info = TelegramGateway::voice_info(&msg);
        assert!(info.is_some());
        let v = info.unwrap();
        assert_eq!(v.file_id, "audio_file_456");
        assert_eq!(v.duration, Some(120));
        assert_eq!(v.mime_type, Some("audio/mpeg"));
    }

    #[test]
    fn voice_info_none_for_text() {
        let msg = TelegramMessage {
            message_id: 1,
            from: None,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".into(),
            },
            text: Some("hello".into()),
            voice: None,
            audio: None,
        };
        assert!(TelegramGateway::voice_info(&msg).is_none());
    }

    #[test]
    fn voice_prefers_voice_over_audio() {
        let msg = TelegramMessage {
            message_id: 1,
            from: None,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".into(),
            },
            text: None,
            voice: Some(TelegramVoice {
                file_id: "voice_wins".into(),
                duration: Some(3),
                mime_type: None,
                file_size: None,
            }),
            audio: Some(TelegramAudio {
                file_id: "audio_loses".into(),
                duration: Some(60),
                mime_type: None,
                file_size: None,
            }),
        };
        let v = TelegramGateway::voice_info(&msg).unwrap();
        assert_eq!(v.file_id, "voice_wins");
    }

    #[test]
    fn voice_prompt_includes_path_and_duration() {
        let path = std::path::PathBuf::from("/tmp/goose_voice/voice_test.ogg");
        let prompt = TelegramGateway::voice_prompt(&path, Some(10), Some("audio/ogg"));
        assert!(prompt.contains("/tmp/goose_voice/voice_test.ogg"));
        assert!(prompt.contains("(duration: 10s)"));
        assert!(prompt.contains("audio/ogg"));
        assert!(prompt.contains("transcribe"));
    }

    #[test]
    fn voice_prompt_without_duration() {
        let path = std::path::PathBuf::from("/tmp/goose_voice/voice_test.ogg");
        let prompt = TelegramGateway::voice_prompt(&path, None, None);
        assert!(!prompt.contains("duration"));
        assert!(prompt.contains("/tmp/goose_voice/voice_test.ogg"));
    }

    #[test]
    fn voice_prompt_with_mp3_mime() {
        let path = std::path::PathBuf::from("/tmp/goose_voice/voice_test.mp3");
        let prompt = TelegramGateway::voice_prompt(&path, Some(60), Some("audio/mpeg"));
        assert!(prompt.contains("audio/mpeg"));
        assert!(!prompt.contains("OGG"));
    }

    #[test]
    fn save_voice_file_creates_file_ogg() {
        let bytes = b"fake ogg data";
        let path = TelegramGateway::save_voice_file(bytes, Some("audio/ogg")).unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().ends_with(".ogg"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_voice_file_creates_file_mp3() {
        let bytes = b"fake mp3 data";
        let path = TelegramGateway::save_voice_file(bytes, Some("audio/mpeg")).unwrap();
        assert!(path.exists());
        assert!(path.to_str().unwrap().ends_with(".mp3"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_voice_file_defaults_to_ogg() {
        let bytes = b"unknown format";
        let path = TelegramGateway::save_voice_file(bytes, None).unwrap();
        assert!(path.to_str().unwrap().ends_with(".ogg"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cleanup_preserves_recent_files() {
        let dir = std::env::temp_dir().join("goose_voice");
        std::fs::create_dir_all(&dir).unwrap();
        let recent_file = dir.join("voice_cleanup_recent_test.ogg");
        std::fs::write(&recent_file, b"recent").unwrap();
        // With a 1-hour max_age, a just-created file should survive.
        cleanup_voice_files(std::time::Duration::from_secs(3600));
        assert!(recent_file.exists());
        let _ = std::fs::remove_file(&recent_file);
    }

    #[test]
    fn cleanup_handles_missing_dir() {
        // Should not panic even when the directory doesn't exist.
        cleanup_voice_files(std::time::Duration::from_secs(1));
    }

    #[test]
    fn split_preserves_content() {
        let text = format!(
            "{} {} {}",
            "a".repeat(3000),
            "b".repeat(3000),
            "c".repeat(3000)
        );
        let chunks = split_message(&text, 4096);
        let reassembled: String = chunks.join("");
        assert_eq!(reassembled, text);
    }
}
