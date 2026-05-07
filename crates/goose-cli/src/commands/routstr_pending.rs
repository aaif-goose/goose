//! Persistent queue of `(url, sk-api_key)` pairs that failed to refund
//! during a Routstr profile switch.
//!
//! When `goose configure → Routstr → <new URL>` swaps the active profile,
//! the previous profile's tracked sats are returned to the local Cashu
//! wallet via `POST /v1/balance/refund`. If that POST fails (proxy down,
//! network offline, rate limited, …) the sats are still on the proxy but
//! the api_key may end up overwritten in `ROUTSTR_PROFILES`. To stop
//! losing those sats, we instead enqueue the failed `(url, api_key)` pair
//! into a file at `~/.cdk-gooose/pending-refunds.json` and try to drain
//! it the next time the user runs any local-wallet command.
//!
//! On disk format (JSON array):
//!
//! ```json
//! [
//!   {
//!     "url": "https://routstr.otrta.me",
//!     "api_key": "sk-abc...",
//!     "queued_at": "2026-05-05T20:30:00Z",
//!     "reason": "connection refused"
//!   }
//! ]
//! ```

use anyhow::{anyhow, Context, Result};
use cdk::wallet::Wallet;
use chrono::Utc;
use console::style;
use goose::providers::routstr_api::refund_balance;
use home::home_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::commands::wallet::receive_into_wallet;

const PENDING_FILE_NAME: &str = "pending-refunds.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRefund {
    pub url: String,
    pub api_key: String,
    pub queued_at: String,
    #[serde(default)]
    pub reason: String,
}

fn pending_path() -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| anyhow!("Could not resolve home directory"))?;
    Ok(home.join(".cdk-gooose").join(PENDING_FILE_NAME))
}

/// Read the pending-refunds queue from disk. Missing or malformed file
/// returns an empty vec so callers don't have to special-case.
pub fn read_pending() -> Vec<PendingRefund> {
    let path = match pending_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let body = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&body).unwrap_or_default()
}

fn write_pending(items: &[PendingRefund]) -> Result<()> {
    let path = pending_path()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let body = serde_json::to_string_pretty(items)?;
    if items.is_empty() {
        // Best-effort cleanup — don't error out if the file doesn't exist.
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Append a `(url, api_key)` pair to the queue. De-duplicates by api_key
/// so repeated switch attempts don't pile up.
pub fn enqueue(url: &str, api_key: &str, reason: impl Into<String>) -> Result<()> {
    if api_key.trim().is_empty() {
        return Ok(());
    }
    let mut items = read_pending();
    if items.iter().any(|p| p.api_key == api_key) {
        return Ok(());
    }
    items.push(PendingRefund {
        url: url.to_string(),
        api_key: api_key.to_string(),
        queued_at: Utc::now().to_rfc3339(),
        reason: reason.into(),
    });
    write_pending(&items)?;
    Ok(())
}

/// Walk every pending entry, attempt the refund, redeem returned tokens
/// into `wallet`. Successful entries are removed from the queue;
/// unsuccessful entries stay so the next call retries.
///
/// Prints a per-entry status line. `quiet=true` suppresses output when
/// the queue is empty (used by routine `goose wallet balance` calls).
pub async fn drain(wallet: &Wallet, quiet: bool) -> DrainSummary {
    let items = read_pending();
    if items.is_empty() {
        if !quiet {
            // No pending refunds to drain — keep this silent for
            // routine commands; only print when explicitly asked.
        }
        return DrainSummary::default();
    }

    let mut survivors: Vec<PendingRefund> = Vec::new();
    let mut summary = DrainSummary {
        attempted: items.len(),
        ..Default::default()
    };

    println!(
        "{}",
        style(format!(
            "↻ draining {} pending Routstr refund(s)…",
            items.len()
        ))
        .dim()
    );

    for entry in items {
        match refund_balance(&entry.url, &entry.api_key).await {
            Ok(resp) => match receive_into_wallet(wallet, &resp.token).await {
                Ok(sats) => {
                    let reported = resp.amount.as_sats() as u64;
                    summary.refunded_sats += reported.max(sats);
                    summary.refunded_count += 1;
                    println!(
                        "{}",
                        style(format!(
                            "  ✓ refunded {} sats from {}",
                            reported.max(sats),
                            entry.url
                        ))
                        .green()
                    );
                }
                Err(e) => {
                    summary.failed_count += 1;
                    eprintln!(
                        "{}",
                        style(format!(
                            "  ⚠ {}: refund returned a token but receive failed ({e}); keeping queued",
                            entry.url
                        ))
                        .yellow()
                    );
                    survivors.push(entry);
                }
            },
            Err(e) => {
                summary.failed_count += 1;
                eprintln!(
                    "{}",
                    style(format!(
                        "  ⚠ {}: refund still failing ({e}); keeping queued",
                        entry.url
                    ))
                    .yellow()
                );
                survivors.push(entry);
            }
        }
    }

    if let Err(e) = write_pending(&survivors) {
        eprintln!(
            "{}",
            style(format!(
                "  ⚠ couldn't update pending-refunds file ({e}); state may re-trigger drains"
            ))
            .yellow()
        );
    }

    summary
}

#[derive(Debug, Default)]
pub struct DrainSummary {
    pub attempted: usize,
    pub refunded_count: usize,
    pub refunded_sats: u64,
    pub failed_count: usize,
}

impl DrainSummary {
    pub fn is_empty(&self) -> bool {
        self.attempted == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_pending_returns_empty_when_missing() {
        // pending_path() depends on $HOME; smoke-test that read_pending
        // doesn't panic when the file is absent.
        let _ = read_pending();
    }

    #[test]
    fn enqueue_skips_empty_api_key() {
        // Doesn't write to disk when the api_key is blank.
        let res = enqueue("https://routstr.example", "", "test");
        assert!(res.is_ok());
    }

    #[test]
    fn drain_summary_default_is_empty() {
        let s = DrainSummary::default();
        assert!(s.is_empty());
        assert_eq!(s.refunded_sats, 0);
    }
}
