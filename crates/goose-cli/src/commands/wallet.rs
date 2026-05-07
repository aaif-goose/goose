//! Local Cashu wallet behind the Routstr provider.
//!
//! This is a *plain* CDK wallet — it receives Cashu tokens, holds proofs in
//! a local redb store, and lets the user mint a fresh Cashu token to drain
//! some or all of the balance. It knows **nothing** about Routstr.
//!
//! Sats only move to a Routstr instance via `goose routstr topup` (which
//! drains some of the local balance into a Cashu token and POSTs that to
//! `<host>/v1/balance/topup`). Sats come back via `goose routstr refund`
//! (which calls `<host>/v1/balance/refund`, redeems the returned Cashu
//! token here, and zeros the api_key on the proxy).

use anyhow::{bail, Result};
use bip39::Mnemonic;
use cdk::amount::SplitTarget;
use cdk::nuts::CurrencyUnit;
use cdk::wallet::{ReceiveOptions, SendOptions, Wallet};
use cdk::Amount;
use cdk_redb::WalletRedbDatabase;
use home::home_dir;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

const DEFAULT_MINT_URL: &str = "https://mint.minibits.cash/Bitcoin";

/// Balance + provenance info for a single local CDK wallet, useful for
/// human-readable reporting.
pub struct LocalWalletStatus {
    pub balance_sats: u64,
    pub mint_url: String,
    pub seed_path: PathBuf,
}

pub async fn handle_wallet_balance() -> Result<()> {
    let wallet = open_wallet().await?;
    // Try to drain any queued failed-refunds first so the balance we
    // print reflects all reachable sats.
    let _ = crate::commands::routstr_pending::drain(&wallet, true).await;
    let balance: Amount = wallet.total_balance().await?;
    println!("local wallet: {} sats", u64::from(balance));
    println!("mint:         {}", DEFAULT_MINT_URL);

    // Also show the active Routstr profile's balance so the user sees
    // both layers (local sats + proxy-tracked sats) in one command.
    print_active_profile_balance().await;
    Ok(())
}

/// Look up the active Routstr profile and, if it's funded, ask the proxy
/// for its current balance via `GET /v1/balance/info`. Prints a one-line
/// summary or a friendly hint if there's nothing to show.
async fn print_active_profile_balance() {
    use goose::config::Config;
    use goose::providers::routstr_api::{active_profile_name, balance_info, load_profile};

    let config = Config::global();
    let active = active_profile_name(config);
    let (name, profile) = match load_profile(config, Some(&active)) {
        Ok(pair) => pair,
        Err(_) => {
            println!();
            println!("active profile: (none — run `goose configure → Routstr` to add one)");
            return;
        }
    };

    println!();
    println!("active profile: {name} ({})", profile.url);

    if profile.api_key.trim().is_empty() {
        println!(
            "proxy balance:  (not funded yet — run `goose configure → Routstr → {}` to fund it from the local wallet)",
            profile.url
        );
        return;
    }

    match balance_info(&profile.url, &profile.api_key).await {
        Ok(info) => {
            // Routstr reports balance in millisats. Divide by 1000 for sats.
            let bal_sats = info.balance / 1000;
            let bal_msats = info.balance;
            let spent_sats = info.total_spent / 1000;
            print!("proxy balance:  {} sats ({} mSats)", bal_sats, bal_msats);
            if info.reserved > 0 {
                print!(" — {} mSats reserved", info.reserved);
            }
            println!();
            if info.total_requests > 0 {
                println!(
                    "spent:          {} sats over {} request{}",
                    spent_sats,
                    info.total_requests,
                    if info.total_requests == 1 { "" } else { "s" }
                );
            }
        }
        Err(e) => {
            println!("proxy balance:  (couldn't reach {} — {e})", profile.url);
        }
    }
}

pub async fn handle_wallet_topup(token: String) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        println!("No token provided. Operation cancelled.");
        return Ok(());
    }

    let wallet = open_wallet().await?;
    // Drain any queued failed-refunds before receiving the new token —
    // a topup is a natural moment to retry network-dependent refunds
    // (the user is online if they're typing a token).
    let _ = crate::commands::routstr_pending::drain(&wallet, true).await;

    let amount = wallet
        .receive(token, ReceiveOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to receive token: {e}"))?;

    let balance: Amount = wallet.total_balance().await?;
    println!(
        "Received {} sats. Local wallet balance: {} sats.",
        u64::from(amount),
        u64::from(balance),
    );
    Ok(())
}

pub async fn handle_wallet_withdraw(amount: Option<u64>) -> Result<()> {
    let wallet = open_wallet().await?;
    // Drain queued refunds first so the user can withdraw whatever
    // came back from previously-stranded api_keys in the same call.
    let _ = crate::commands::routstr_pending::drain(&wallet, true).await;
    let balance: Amount = wallet.total_balance().await?;

    if balance == Amount::ZERO {
        println!("Local wallet is empty.");
        return Ok(());
    }

    let amount = amount.map(Amount::from).unwrap_or(balance).min(balance);

    let prep_send = wallet.prepare_send(amount, SendOptions::default()).await?;
    let token = prep_send.confirm(None).await?;
    println!("{}", token);

    let new_balance: Amount = wallet.total_balance().await?;
    eprintln!(
        "Withdrew {} sats. Local wallet balance: {} sats.",
        u64::from(amount),
        u64::from(new_balance),
    );
    Ok(())
}

/// Open the local CDK wallet, creating the seed/redb on first use. Public
/// so the new `goose routstr` subcommand can reuse it for topup/refund.
pub async fn open_wallet() -> Result<Wallet> {
    let work_dir = wallet_dir()?;
    fs::create_dir_all(&work_dir)?;

    let cdk_wallet_path = work_dir.join("cdk-goose.redb");
    let wallet_db = WalletRedbDatabase::new(&cdk_wallet_path)?;

    let seed_path = work_dir.join("seed");
    let mnemonic = match fs::metadata(&seed_path) {
        Ok(_) => Mnemonic::from_str(&fs::read_to_string(&seed_path)?)?,
        Err(_) => {
            let mnemonic = Mnemonic::generate(12)?;
            tracing::info!("Creating new Cashu wallet seed");
            fs::write(&seed_path, mnemonic.to_string())?;
            mnemonic
        }
    };

    let seed = mnemonic.to_seed_normalized("");
    let wallet = Wallet::new(
        DEFAULT_MINT_URL,
        CurrencyUnit::Sat,
        Arc::new(wallet_db),
        seed,
        None,
    )?;

    if let Err(e) = wallet.recover_incomplete_sagas().await {
        tracing::warn!("recover_incomplete_sagas failed: {e}");
    }

    Ok(wallet)
}

pub async fn wallet_status() -> Result<LocalWalletStatus> {
    let wallet = open_wallet().await?;
    let balance: Amount = wallet.total_balance().await?;
    Ok(LocalWalletStatus {
        balance_sats: u64::from(balance),
        mint_url: DEFAULT_MINT_URL.to_string(),
        seed_path: wallet_dir()?.join("seed"),
    })
}

/// Withdraw `amount` sats from the local wallet and return the encoded
/// Cashu token. Helper for the `goose routstr topup` flow.
pub async fn withdraw_to_token(wallet: &Wallet, amount: Amount) -> Result<String> {
    let balance: Amount = wallet.total_balance().await?;
    if balance < amount {
        bail!(
            "Local wallet has {} sats, need {}.",
            u64::from(balance),
            u64::from(amount)
        );
    }
    let prep_send = wallet.prepare_send(amount, SendOptions::default()).await?;
    let token = prep_send.confirm(None).await?;
    Ok(token.to_string())
}

/// Receive a Cashu token returned by `/v1/balance/refund` into the local
/// wallet. Returns the amount of sats added.
pub async fn receive_into_wallet(wallet: &Wallet, token: &str) -> Result<u64> {
    let amount = wallet
        .receive(token.trim(), ReceiveOptions::default())
        .await?;
    Ok(u64::from(amount))
}

/// Best-effort split for a desired top-up amount. Caps at the available
/// local balance.
pub fn capped_topup(desired: Amount, available: Amount) -> Amount {
    if available < desired {
        available
    } else {
        desired
    }
}

fn wallet_dir() -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| anyhow::anyhow!("Could not resolve home directory"))?;
    Ok(home.join(".cdk-gooose"))
}

/// Use SplitTarget::default() for prepare_send. Re-exported for the routstr
/// subcommand so it can keep parity if it ever needs to call swap directly.
pub fn default_split_target() -> SplitTarget {
    SplitTarget::default()
}
