use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, bail, Result};
use tokio::process::Command;

use super::review::{build_goose_review_args, extract_final_assistant_text, parse_findings};
use super::CopilotPrefs;

pub fn locate_goose_binary() -> Result<PathBuf> {
    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            let candidate = parent.join("goose");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Ok(PathBuf::from("goose"))
}

pub async fn run_goose_review(
    workdir: &Path,
    base: &str,
    head: &str,
    prefs: &CopilotPrefs,
) -> Result<Vec<super::Finding>> {
    let bin = locate_goose_binary()?;
    let args = build_goose_review_args(base, head, prefs);

    let output = Command::new(&bin)
        .current_dir(workdir)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        bail!(
            "goose review exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("goose review stdout was not UTF-8: {e}"))?;
    parse_findings(&stdout)
}

pub async fn run_goose_for_reply(workdir: &Path, prompt: &str) -> Result<String> {
    let bin = locate_goose_binary()?;
    let output = Command::new(&bin)
        .current_dir(workdir)
        .args([
            "run",
            "--no-session",
            "--quiet",
            "--output-format",
            "json",
            "-t",
            prompt,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "goose run exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("goose run stdout was not UTF-8: {e}"))?;
    extract_final_assistant_text(&stdout)
}
