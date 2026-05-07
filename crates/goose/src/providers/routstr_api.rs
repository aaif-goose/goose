//! Routstr profile config + balance-API client.
//!
//! The Routstr provider stores its credentials as a set of *profiles*. Each
//! profile is `{url, api_key}` where `api_key` is an `sk-...` bearer issued
//! by the proxy (via `GET /v1/balance/create`), **not** a Cashu token. Sats
//! belong to the local CDK wallet (managed by `goose wallet`); they only
//! move to a Routstr instance via `/v1/balance/topup`, and they come back
//! via `/v1/balance/refund`.

use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const ROUTSTR_DEFAULT_HOST: &str = "https://api.routstr.com";
pub const ROUTSTR_DEFAULT_PROFILE: &str = "default";

/// Active-profile pointer. Stored at the top level of `~/.config/goose/config.yaml`.
pub const ROUTSTR_ACTIVE_KEY: &str = "ROUTSTR_ACTIVE";
/// Per-profile config map. Stored at the top level of `~/.config/goose/config.yaml`.
pub const ROUTSTR_PROFILES_KEY: &str = "ROUTSTR_PROFILES";

/// One Routstr profile's persistent state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutstrProfile {
    /// Base URL of the Routstr proxy, e.g. `https://routstr.otrta.me`.
    pub url: String,
    /// `sk-...` bearer token issued by the proxy. Empty until the first
    /// successful `/v1/balance/create` or `/v1/balance/topup`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
}

impl RoutstrProfile {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            api_key: String::new(),
        }
    }
}

/// Load the named profile; if `name` is `None`, load whichever profile is
/// currently active. Falls back to `default` (and creates it on save) when
/// `ROUTSTR_ACTIVE` is unset.
pub fn load_profile(
    config: &crate::config::Config,
    name: Option<&str>,
) -> Result<(String, RoutstrProfile)> {
    let profiles = load_profiles(config)?;
    let name = match name {
        Some(n) => n.to_string(),
        None => active_profile_name(config),
    };
    let profile = profiles
        .get(&name)
        .cloned()
        .ok_or_else(|| anyhow!("Routstr profile {name:?} not found"))?;
    Ok((name, profile))
}

/// Read the `ROUTSTR_PROFILES` map from config. Returns an empty map if
/// nothing is set. Profiles are kept in insertion order.
pub fn load_profiles(config: &crate::config::Config) -> Result<IndexMap<String, RoutstrProfile>> {
    match config.get_param::<IndexMap<String, RoutstrProfile>>(ROUTSTR_PROFILES_KEY) {
        Ok(p) => Ok(p),
        Err(_) => Ok(IndexMap::new()),
    }
}

/// Persist the full `ROUTSTR_PROFILES` map.
pub fn save_profiles(
    config: &crate::config::Config,
    profiles: &IndexMap<String, RoutstrProfile>,
) -> Result<()> {
    let value = serde_json::to_value(profiles)
        .context("failed to serialise Routstr profiles to JSON value")?;
    config
        .set_param(ROUTSTR_PROFILES_KEY, &value)
        .context("failed to write ROUTSTR_PROFILES into config")?;
    Ok(())
}

/// Active-profile name, defaulting to `default` if unset.
pub fn active_profile_name(config: &crate::config::Config) -> String {
    config
        .get_param::<String>(ROUTSTR_ACTIVE_KEY)
        .unwrap_or_else(|_| ROUTSTR_DEFAULT_PROFILE.to_string())
}

/// Set the active-profile name. The profile must already exist in the
/// `ROUTSTR_PROFILES` map; we don't enforce that here because the caller
/// usually saves both in one go.
pub fn set_active_profile(config: &crate::config::Config, name: &str) -> Result<()> {
    config
        .set_param(ROUTSTR_ACTIVE_KEY, &serde_json::Value::String(name.to_string()))
        .context("failed to write ROUTSTR_ACTIVE into config")?;
    Ok(())
}

/// Insert (or update) a profile and write the map back. Does not change the
/// active-profile pointer.
pub fn upsert_profile(
    config: &crate::config::Config,
    name: &str,
    profile: RoutstrProfile,
) -> Result<()> {
    let mut profiles = load_profiles(config)?;
    profiles.insert(name.to_string(), profile);
    save_profiles(config, &profiles)
}

/// Drop a profile from the map. If it was the active one, fall back to the
/// first remaining profile (or unset `ROUTSTR_ACTIVE` if none are left).
pub fn remove_profile(config: &crate::config::Config, name: &str) -> Result<bool> {
    let mut profiles = load_profiles(config)?;
    let removed = profiles.shift_remove(name).is_some();
    if !removed {
        return Ok(false);
    }
    save_profiles(config, &profiles)?;

    if active_profile_name(config) == name {
        if let Some((next, _)) = profiles.iter().next() {
            set_active_profile(config, next)?;
        } else {
            // No profiles left; clear the active pointer so future operations
            // don't reference a missing profile.
            let _ = config.delete(ROUTSTR_ACTIVE_KEY);
        }
    }

    Ok(true)
}

// ============================================================================
// Routstr balance-API client
// ============================================================================

/// Response from `GET /v1/balance/create?initial_balance_token=cashuB...`.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceCreateResponse {
    pub api_key: String,
    /// Balance, in **mSats** (millisats).
    pub balance: i64,
}

/// Response from `GET /v1/balance/info` (`Authorization: Bearer sk-...`).
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceInfoResponse {
    pub api_key: String,
    /// Balance, in **mSats** (millisats).
    pub balance: i64,
    #[serde(default)]
    pub reserved: i64,
    #[serde(default)]
    pub total_requests: i64,
    #[serde(default)]
    pub total_spent: i64,
}

/// Response from `POST /v1/balance/refund` (`Authorization: Bearer sk-...`).
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceRefundResponse {
    /// Cashu token encoding the refunded sats.
    pub token: String,
    /// Refunded amount. Routstr instances disagree on the unit and the
    /// JSON type:
    ///   - `routstr.otrta.me` returns `"sats": "976"` (string)
    ///   - upstream `api.routstr.com` returns `"msats": 450000` (integer)
    ///
    /// `RefundAmount` accepts both shapes and normalises to integer sats
    /// via [`RefundAmount::as_sats`].
    #[serde(flatten)]
    pub amount: RefundAmount,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RefundAmount {
    Sats {
        #[serde(deserialize_with = "deserialize_int_or_string")]
        sats: i64,
    },
    MSats {
        #[serde(deserialize_with = "deserialize_int_or_string")]
        msats: i64,
    },
}

impl RefundAmount {
    pub fn as_sats(&self) -> i64 {
        match self {
            RefundAmount::Sats { sats } => *sats,
            RefundAmount::MSats { msats } => msats / 1000,
        }
    }
}

/// Accept an integer in either JSON-number or quoted-string form. Routstr's
/// `/v1/balance/refund` returns the amount as a stringified number on some
/// instances (`{"sats": "976"}`).
fn deserialize_int_or_string<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an integer or numeric string")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            Ok(v as i64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.parse::<i64>().map_err(de::Error::custom)
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<i64, E> {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_any(V)
}

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("failed to build HTTP client for Routstr balance API")
}

fn balance_url(host: &str, path: &str) -> Result<url::Url> {
    let mut u = url::Url::parse(host).with_context(|| format!("invalid Routstr URL: {host}"))?;
    if !u.path().ends_with('/') {
        let p = format!("{}/", u.path());
        u.set_path(&p);
    }
    Ok(u.join(path)?)
}

/// `GET /v1/balance/create?initial_balance_token=<cashu>`
///
/// Exchanges a Cashu token for a fresh `sk-...` bearer with a tracked
/// balance on the proxy. Used both for first-time setup and for
/// re-funding a profile whose previous key was refunded.
pub async fn create_balance(
    host: &str,
    cashu_token: &str,
) -> Result<BalanceCreateResponse, ProviderApiError> {
    let mut url = balance_url(host, "v1/balance/create").map_err(ProviderApiError::Request)?;
    url.query_pairs_mut()
        .append_pair("initial_balance_token", cashu_token);
    let client = http_client().map_err(ProviderApiError::Request)?;
    let response: reqwest::Response = client
        .get(url)
        .send()
        .await
        .map_err(|e| ProviderApiError::Request(anyhow!(e)))?;
    parse_response(response).await
}

/// `POST /v1/balance/topup?cashu_token=<cashu>` with `Authorization: Bearer sk-...`.
///
/// Adds the value of `cashu_token` to the existing api_key's balance.
/// Returns the proxy's response (typically the new balance, but the schema
/// is undocumented at the time of writing — we surface the raw JSON).
pub async fn topup_balance(
    host: &str,
    api_key: &str,
    cashu_token: &str,
) -> Result<serde_json::Value, ProviderApiError> {
    let mut url = balance_url(host, "v1/balance/topup").map_err(ProviderApiError::Request)?;
    url.query_pairs_mut().append_pair("cashu_token", cashu_token);
    let client = http_client().map_err(ProviderApiError::Request)?;
    let response: reqwest::Response = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| ProviderApiError::Request(anyhow!(e)))?;
    parse_response(response).await
}

/// `POST /v1/balance/refund` with `Authorization: Bearer sk-...`.
///
/// Returns a Cashu token encoding all unspent sats associated with this
/// api_key. After a successful refund the api_key's balance is zero (and in
/// practice the key is consumed — call `create_balance` again with the
/// returned token to start a fresh tracked balance).
pub async fn refund_balance(
    host: &str,
    api_key: &str,
) -> Result<BalanceRefundResponse, ProviderApiError> {
    let url = balance_url(host, "v1/balance/refund").map_err(ProviderApiError::Request)?;
    let client = http_client().map_err(ProviderApiError::Request)?;
    let response: reqwest::Response = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| ProviderApiError::Request(anyhow!(e)))?;
    parse_response(response).await
}

/// `GET /v1/balance/info` with `Authorization: Bearer sk-...`.
pub async fn balance_info(
    host: &str,
    api_key: &str,
) -> Result<BalanceInfoResponse, ProviderApiError> {
    let url = balance_url(host, "v1/balance/info").map_err(ProviderApiError::Request)?;
    let client = http_client().map_err(ProviderApiError::Request)?;
    let response: reqwest::Response = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| ProviderApiError::Request(anyhow!(e)))?;
    parse_response(response).await
}

/// Errors surfaced by the Routstr balance-API client. Distinguishes
/// network/IO problems from proxy-reported errors so callers can decide
/// whether to retry.
#[derive(thiserror::Error, Debug)]
pub enum ProviderApiError {
    /// Underlying request setup or transport error (network down, bad URL,
    /// unparseable response body, …).
    #[error("routstr balance api request failed: {0}")]
    Request(#[from] anyhow::Error),
    /// The proxy returned a non-2xx status with an error body.
    #[error("routstr balance api returned HTTP {status}: {message}")]
    Proxy {
        status: reqwest::StatusCode,
        message: String,
    },
}

async fn parse_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, ProviderApiError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        // Try to extract an error message from the standard envelopes:
        //   {"detail": "..."} or {"detail": {"error": {"message": "..."}}}
        //   or {"error": {"message": "..."}}
        let payload = serde_json::from_str::<serde_json::Value>(&body).ok();
        let message = payload
            .as_ref()
            .and_then(|p| {
                p.get("detail")
                    .and_then(|d| {
                        d.as_str()
                            .map(String::from)
                            .or_else(|| d.get("error").and_then(extract_message))
                    })
                    .or_else(|| p.get("error").and_then(extract_message))
            })
            .unwrap_or_else(|| body.clone());
        return Err(ProviderApiError::Proxy { status, message });
    }

    serde_json::from_str::<T>(&body)
        .with_context(|| format!("decoding {body}"))
        .map_err(ProviderApiError::Request)
}

fn extract_message(v: &serde_json::Value) -> Option<String> {
    v.get("message")
        .and_then(|m| m.as_str().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refund_amount_normalises_msats_and_sats() {
        let sats: BalanceRefundResponse =
            serde_json::from_value(json!({"token": "x", "sats": 1000})).unwrap();
        assert_eq!(sats.amount.as_sats(), 1000);

        let msats: BalanceRefundResponse =
            serde_json::from_value(json!({"token": "x", "msats": 1234567})).unwrap();
        assert_eq!(msats.amount.as_sats(), 1234);
    }

    #[test]
    fn refund_amount_accepts_stringified_sats() {
        // routstr.otrta.me returns `"sats": "976"` (string), which broke
        // refund parsing in our first end-to-end smoke test and silently
        // dropped the Cashu token. Make sure both string and int forms
        // survive.
        let parsed: BalanceRefundResponse =
            serde_json::from_value(json!({"token": "cashuB...", "sats": "976"})).unwrap();
        assert_eq!(parsed.amount.as_sats(), 976);

        let parsed: BalanceRefundResponse =
            serde_json::from_value(json!({"token": "cashuB...", "msats": "1500"})).unwrap();
        assert_eq!(parsed.amount.as_sats(), 1);
    }

    #[test]
    fn balance_create_response_parses_real_routstr_payload() {
        let raw = json!({
            "api_key": "sk-deadbeef",
            "balance": 1000000
        });
        let parsed: BalanceCreateResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.api_key, "sk-deadbeef");
        assert_eq!(parsed.balance, 1000000);
    }

    #[test]
    fn balance_info_response_tolerates_extra_fields() {
        let raw = json!({
            "api_key": "sk-x",
            "balance": 500,
            "reserved": 0,
            "is_child": false,
            "parent_key": null,
            "total_requests": 3,
            "total_spent": 17,
            "balance_limit": null,
            "balance_limit_reset": null,
            "validity_date": null
        });
        let parsed: BalanceInfoResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.balance, 500);
        assert_eq!(parsed.total_requests, 3);
        assert_eq!(parsed.total_spent, 17);
    }
}
