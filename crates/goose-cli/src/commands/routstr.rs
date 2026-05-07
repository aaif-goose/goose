//! Routstr profile + Cashu helpers used by `goose configure`.
//!
//! Profile management — switching the active Routstr URL, refunding the
//! previously active profile into the local Cashu wallet, and auto-funding
//! the newly active profile from the local wallet — is driven entirely
//! through the `goose configure → Configure Providers → Routstr` URL
//! prompt. No top-level `goose routstr` subcommand is exposed; the surface
//! area is intentionally limited to the wallet (`goose wallet`) and the
//! configure flow.

use anyhow::{anyhow, bail, Result};
use cdk::Amount;
use console::style;
use goose::config::Config;
use goose::providers::routstr_api::{
    active_profile_name, balance_info, create_balance, load_profile, load_profiles, refund_balance,
    set_active_profile, topup_balance, upsert_profile, BalanceInfoResponse, ProviderApiError,
    RoutstrProfile, ROUTSTR_DEFAULT_HOST,
};

use crate::commands::routstr_pending;
use crate::commands::wallet::{open_wallet, receive_into_wallet, withdraw_to_token};

/// Default top-up amount in sats when the configure flow auto-funds the
/// active Routstr profile after a URL switch.
pub const DEFAULT_TOPUP_SATS: u64 = 2000;

/// Switch the active profile to `name`. Refunds whatever the previously
/// active profile holds back into the local wallet (best-effort), then
/// auto-tops the new profile from the local wallet up to
/// [`DEFAULT_TOPUP_SATS`] (capped at the local wallet's actual balance).
///
/// Used by [`prompt_and_set_routstr_url`] when the URL the user enters in
/// `goose configure` matches a profile other than the active one.
pub async fn handle_profile_use(name: String) -> Result<()> {
    let config = Config::global();
    let profiles = load_profiles(config)?;
    if !profiles.contains_key(&name) {
        bail!(
            "Routstr profile {name:?} not found. Available: {:?}",
            profiles.keys().collect::<Vec<_>>()
        );
    }

    let current = active_profile_name(config);
    if current == name {
        println!(
            "{}",
            style(format!("Already on routstr profile {name:?}.")).dim()
        );
        return Ok(());
    }

    if let Some(active_profile) = profiles.get(&current) {
        if !active_profile.api_key.is_empty() {
            match refund_active_into_wallet(&current, active_profile).await {
                Ok(sats) => {
                    println!(
                        "{}",
                        style(format!(
                            "✓ refunded {sats} sats from {current:?} into local wallet"
                        ))
                        .green()
                    );
                    let mut updated = active_profile.clone();
                    updated.api_key.clear();
                    upsert_profile(config, &current, updated)?;
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        style(format!(
                            "⚠ refund of {current:?} failed: {e}. \
                             Queued for retry — the next `goose wallet \
                             topup/balance/withdraw` will try again."
                        ))
                        .yellow()
                    );
                    if let Err(qe) = routstr_pending::enqueue(
                        &active_profile.url,
                        &active_profile.api_key,
                        e.to_string(),
                    ) {
                        eprintln!(
                            "{}",
                            style(format!(
                                "  ⚠ couldn't queue pending refund ({qe}); the api_key is still \
                                 in ROUTSTR_PROFILES.{current:?} for manual retry."
                            ))
                            .yellow()
                        );
                    }
                    // Clear the api_key from the profile slot — the queue
                    // is now the source of truth for it. If we left it in
                    // the profile, the next switch back here would try to
                    // refund a key we already enqueued.
                    let mut updated = active_profile.clone();
                    updated.api_key.clear();
                    upsert_profile(config, &current, updated)?;
                }
            }
        }
    }

    set_active_profile(config, &name)?;
    println!(
        "{}",
        style(format!("✓ active routstr profile is now {name:?}")).green()
    );

    let new_profile = profiles
        .get(&name)
        .cloned()
        .ok_or_else(|| anyhow!("internal: profile vanished during switch"))?;
    if let Err(e) = autotopup_after_switch(&name, &new_profile).await {
        eprintln!(
            "{}",
            style(format!(
                "⚠ auto-topup skipped: {e}. Top up the local wallet with `goose wallet topup <cashu-token>` and re-run `goose configure → Routstr` against the same URL to fund this profile."
            ))
            .yellow()
        );
    }

    Ok(())
}

/// `goose configure → Configure Providers → Routstr` URL prompt.
///
/// Reconciles whatever URL the user types with the profile system:
/// - Same as the active profile's URL → no-op.
/// - Matches a different existing profile → switch to it (refund + auto-topup).
/// - New URL → refund the active profile, create a `default` profile with
///   the new URL, make it active.
pub async fn prompt_and_set_routstr_url() -> Result<()> {
    let config = Config::global();

    let active = active_profile_name(config);
    let profiles = load_profiles(config)?;
    let current_url = profiles
        .get(&active)
        .map(|p| p.url.clone())
        .unwrap_or_else(|| ROUTSTR_DEFAULT_HOST.to_string());

    let entered: String = cliclack::input("Routstr URL")
        .default_input(&current_url)
        .interact()?;
    let entered = entered.trim().to_string();
    if entered.is_empty() {
        return Ok(());
    }

    if entered == current_url && profiles.contains_key(&active) && !profiles.is_empty() {
        // Same URL — this is the user's escape hatch for "fund this
        // profile". If there's no api_key yet, *try* to drain the local
        // wallet via `topup_active_from_local`; if the local wallet is
        // empty, log a warning and let the configure flow proceed to the
        // model picker anyway (model fetch doesn't require an api_key, so
        // the user can still browse the catalogue before topping up).
        let active_profile = profiles.get(&active).cloned().unwrap_or_default();
        if active_profile.api_key.is_empty() {
            let _ = cliclack::log::info(format!(
                "routstr profile {active:?} already points at {entered}; \
                 funding from local wallet (if any)."
            ));
            if let Err(e) = autotopup_after_switch(&active, &active_profile).await {
                let _ = cliclack::log::warning(format!(
                    "auto-topup skipped: {e}. Browsing models anyway; \
                     re-run after `goose wallet topup <cashu-token>` to fund."
                ));
            }
            return Ok(());
        }
        let _ = cliclack::log::info(format!(
            "routstr profile {active:?} already points at {entered}; nothing to do."
        ));
        return Ok(());
    }

    // Clear any legacy top-level `ROUTSTR_HOST` so the profile's URL is
    // the only source of truth. Older builds wrote a flat `ROUTSTR_HOST`
    // and our `from_env` honours it as an override — leaving it in place
    // would silently mask the profile change the user just made.
    let _ = config.delete("ROUTSTR_HOST");

    // 1. Existing profile with a matching URL → switch to it.
    if let Some((existing_name, _)) = profiles
        .iter()
        .find(|(n, p)| **n != active && p.url == entered)
    {
        let existing_name = existing_name.clone();
        let _ = cliclack::log::info(format!(
            "URL {entered} matches existing routstr profile {existing_name:?}; switching."
        ));
        return handle_profile_use(existing_name).await;
    }

    // 2. Otherwise create / update a `default` profile and switch.
    //    The currently active profile's `default`-named slot is about to
    //    be overwritten with the new URL below, so we MUST refund (or
    //    enqueue for retry) its api_key before we lose track of it.
    if let Some(active_profile) = profiles.get(&active) {
        if !active_profile.api_key.is_empty() {
            match refund_active_into_wallet(&active, active_profile).await {
                Ok(sats) => {
                    let _ = cliclack::log::info(format!(
                        "refunded {sats} sats from {active:?} into local wallet before changing URL"
                    ));
                }
                Err(e) => {
                    let _ = cliclack::log::warning(format!(
                        "refund of {active:?} failed: {e}. Queued for retry — the \
                         next `goose wallet topup/balance/withdraw` will try again."
                    ));
                    if let Err(qe) = routstr_pending::enqueue(
                        &active_profile.url,
                        &active_profile.api_key,
                        e.to_string(),
                    ) {
                        let _ = cliclack::log::warning(format!(
                            "couldn't queue pending refund ({qe}); writing the api_key back into \
                             ROUTSTR_PROFILES.{active:?} so it isn't lost when we overwrite the \
                             default slot."
                        ));
                        // Best-effort fallback: keep the api_key on the
                        // OLD profile slot under a different name so it's
                        // still recoverable even if the queue file write
                        // failed.
                        let mut backup = active_profile.clone();
                        backup.url = active_profile.url.clone();
                        let backup_name = format!("{active}-pending-refund");
                        let _ = upsert_profile(config, &backup_name, backup);
                    }
                }
            }
            // Either way, clear the api_key from the active slot before
            // we overwrite it below — the queue (or the backup profile)
            // is now the source of truth for it.
            let mut updated = active_profile.clone();
            updated.api_key.clear();
            upsert_profile(config, &active, updated)?;
        }
    }

    let new_name = "default".to_string();
    let mut profiles = load_profiles(config)?;
    let new_profile = RoutstrProfile::new(entered.clone());
    profiles.insert(new_name.clone(), new_profile.clone());
    goose::providers::routstr_api::save_profiles(config, &profiles)?;
    set_active_profile(config, &new_name)?;
    let _ = cliclack::log::info(format!(
        "routstr profile {new_name:?} now points at {entered} and is active."
    ));

    // Auto-fund from the local wallet, just like a `goose routstr profile use`
    // switch would. Without this, the configure flow's downstream
    // `test_provider_configuration` step fires a chat against a brand-new
    // empty profile and 401s with the "no api_key yet" error before the
    // user has a chance to do anything. If the local wallet is empty the
    // auto-topup is a soft-fail (warn + proceed), matching the same-URL
    // branch above.
    if let Err(e) = autotopup_after_switch(&new_name, &new_profile).await {
        let _ = cliclack::log::warning(format!(
            "auto-topup skipped: {e}. Top up the local wallet with \
             `goose wallet topup <cashu-token>` and re-run \
             `goose configure → Routstr` against the same URL to fund this profile."
        ));
    }
    Ok(())
}

// =================== internal helpers ===================

async fn refund_active_into_wallet(name: &str, profile: &RoutstrProfile) -> Result<u64> {
    let resp = refund_balance(&profile.url, &profile.api_key)
        .await
        .map_err(|e| anyhow!("refund {name:?} failed: {e}"))?;
    let wallet = open_wallet().await?;
    let received = receive_into_wallet(&wallet, &resp.token).await?;
    Ok(received.max(resp.amount.as_sats() as u64))
}

async fn autotopup_after_switch(name: &str, profile: &RoutstrProfile) -> Result<()> {
    let current_sats: u64 = if profile.api_key.is_empty() {
        0
    } else {
        let info: BalanceInfoResponse = balance_info(&profile.url, &profile.api_key)
            .await
            .map_err(|e| anyhow!(e))?;
        (info.balance / 1000) as u64
    };
    if current_sats >= DEFAULT_TOPUP_SATS {
        println!("  {name:?} already has {current_sats} sats; skipping auto-topup.");
        return Ok(());
    }

    let needed = DEFAULT_TOPUP_SATS.saturating_sub(current_sats);
    // Open the wallet in its own scope so the redb lock is released before
    // `topup_active_from_local` re-opens the same database.
    let local_sats = {
        let wallet = open_wallet().await?;
        u64::from(wallet.total_balance().await?)
    };
    if local_sats == 0 {
        bail!(
            "local wallet empty — top up with `goose wallet topup <cashu-token>` then re-run `goose configure → Routstr` against the same URL"
        );
    }
    let to_send = local_sats.min(needed);
    topup_active_from_local(to_send).await
}

/// Drain `amount_sats` from the local wallet into the *currently active*
/// Routstr profile. Creates the profile's `sk-...` api_key on first use
/// (`/v1/balance/create`) or tops up an existing one (`/v1/balance/topup`).
async fn topup_active_from_local(amount_sats: u64) -> Result<()> {
    let config = Config::global();
    let active = active_profile_name(config);
    let (active_name, mut profile) = load_profile(config, Some(&active))?;

    let wallet = open_wallet().await?;
    let local_balance: Amount = wallet.total_balance().await?;
    if local_balance == Amount::ZERO {
        bail!("local wallet empty — top up with `goose wallet topup <cashu-token>` first");
    }

    let to_send: Amount = Amount::from(amount_sats).min(local_balance);
    let token = withdraw_to_token(&wallet, to_send).await?;

    if profile.api_key.is_empty() {
        let resp = create_balance(&profile.url, &token)
            .await
            .map_err(|e| anyhow!(e))?;
        profile.api_key = resp.api_key;
        upsert_profile(config, &active_name, profile.clone())?;
        println!(
            "{}",
            style(format!(
                "✓ created api_key for {active_name:?} with {} sats ({} mSats) initial balance",
                resp.balance / 1000,
                resp.balance,
            ))
            .green()
        );
    } else {
        let _ = topup_balance(&profile.url, &profile.api_key, &token)
            .await
            .map_err(|e| anyhow!(e))?;
        println!(
            "{}",
            style(format!(
                "✓ topped up {active_name:?} by {} sats",
                u64::from(to_send)
            ))
            .green()
        );
    }

    let local_after: Amount = wallet.total_balance().await?;
    println!(
        "  local wallet: {} sats ({} sats sent to proxy)",
        u64::from(local_after),
        u64::from(to_send),
    );
    Ok(())
}

#[allow(dead_code)]
fn short_err(e: &ProviderApiError) -> String {
    let s = e.to_string();
    if s.len() > 80 {
        format!("{}...", &s[..77])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_topup_is_2000_sats() {
        assert_eq!(DEFAULT_TOPUP_SATS, 2000);
    }
}
