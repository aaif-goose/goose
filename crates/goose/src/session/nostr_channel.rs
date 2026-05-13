use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

type ChannelRow = (String, String, String, String, String, String, Option<i64>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelRole {
    Owner,
    Participant,
}

impl std::fmt::Display for ChannelRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelRole::Owner => write!(f, "owner"),
            ChannelRole::Participant => write!(f, "participant"),
        }
    }
}

impl std::str::FromStr for ChannelRole {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "owner" => Ok(ChannelRole::Owner),
            "participant" => Ok(ChannelRole::Participant),
            _ => Err(anyhow::anyhow!("Invalid channel role: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NostrChannel {
    pub session_id: String,
    pub event_id: String,
    pub nevent: String,
    pub encryption_key: String,
    pub relays: Vec<String>,
    pub role: ChannelRole,
    pub last_checked_at: Option<i64>,
}

pub async fn save_channel(pool: &SqlitePool, channel: &NostrChannel) -> Result<()> {
    let relays_json = serde_json::to_string(&channel.relays)?;
    let role = channel.role.to_string();
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO nostr_channels
            (session_id, event_id, nevent, encryption_key, relays_json, role, last_checked_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&channel.session_id)
    .bind(&channel.event_id)
    .bind(&channel.nevent)
    .bind(&channel.encryption_key)
    .bind(&relays_json)
    .bind(&role)
    .bind(channel.last_checked_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_channel(pool: &SqlitePool, session_id: &str) -> Result<Option<NostrChannel>> {
    let row: Option<ChannelRow> = sqlx::query_as(
        r#"
            SELECT session_id, event_id, nevent, encryption_key, relays_json, role, last_checked_at
            FROM nostr_channels
            WHERE session_id = ?
            "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((
            session_id,
            event_id,
            nevent,
            encryption_key,
            relays_json,
            role,
            last_checked_at,
        )) => {
            let relays: Vec<String> = serde_json::from_str(&relays_json)?;
            let role: ChannelRole = role.parse()?;
            Ok(Some(NostrChannel {
                session_id,
                event_id,
                nevent,
                encryption_key,
                relays,
                role,
                last_checked_at,
            }))
        }
        None => Ok(None),
    }
}

pub async fn list_channels_by_role(
    pool: &SqlitePool,
    role: &ChannelRole,
) -> Result<Vec<NostrChannel>> {
    let role_str = role.to_string();
    let rows: Vec<ChannelRow> = sqlx::query_as(
        r#"
            SELECT session_id, event_id, nevent, encryption_key, relays_json, role, last_checked_at
            FROM nostr_channels
            WHERE role = ?
            "#,
    )
    .bind(&role_str)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(
            |(session_id, event_id, nevent, encryption_key, relays_json, role, last_checked_at)| {
                let relays: Vec<String> = serde_json::from_str(&relays_json)?;
                let role: ChannelRole = role.parse()?;
                Ok(NostrChannel {
                    session_id,
                    event_id,
                    nevent,
                    encryption_key,
                    relays,
                    role,
                    last_checked_at,
                })
            },
        )
        .collect()
}

pub async fn update_last_checked(pool: &SqlitePool, session_id: &str, ts: i64) -> Result<()> {
    sqlx::query("UPDATE nostr_channels SET last_checked_at = ? WHERE session_id = ?")
        .bind(ts)
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_channel(pool: &SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM nostr_channels WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}
