//! Fixture tests for the shell integration templates emitted by `goose term init`.
//!
//! These tests run the `goose term init <shell>` binary (built via the
//! `CARGO_BIN_EXE_goose` env var cargo sets for integration tests), normalize
//! the output for host-portable comparison, and diff against checked-in
//! fixtures. The fixtures freeze the default-brand output of `main@HEAD`.
//!
//! Two sources of per-machine variability are normalized before compare:
//!
//! - `AGENT_SESSION_ID=<timestamp>` — generated per invocation; stripped.
//! - The absolute `current_exe()` path embedded by `term init` — differs by
//!   checkout location; replaced with the placeholder `{GOOSE_BIN}`.
//!
//! To regenerate fixtures after an intentional template change, run
//! `UPDATE_SHELL_FIXTURES=1 cargo test --test shell_templates` once and commit
//! the regenerated files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shell_templates")
}

/// Run `goose term init <shell>` (optionally with `--default`) in an isolated
/// HOME/XDG env so it doesn't create sessions in the real user's config.
fn render_shell(shell: &str, with_default: bool, scratch: &Path) -> String {
    let bin = env!("CARGO_BIN_EXE_goose");
    let mut cmd = Command::new(bin);
    cmd.arg("term").arg("init").arg(shell);
    if with_default {
        cmd.arg("--default");
    }
    cmd.env("HOME", scratch)
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("XDG_CONFIG_HOME", scratch.join("config"))
        .env("XDG_STATE_HOME", scratch.join("state"))
        .env_remove("GOOSE_BRAND_PRODUCT_NAME")
        .env_remove("GOOSE_BRAND_BINARY_NAME")
        .env_remove("GOOSE_BRAND_SHELL_ALIAS_PRIMARY")
        .env_remove("GOOSE_BRAND_SHELL_ALIAS_SHORT")
        .env_remove("GOOSE_BRAND_SHELL_FN_PREFIX")
        .env_remove("GOOSE_BRAND_DEEPLINK_SCHEME")
        .env_remove("GOOSE_BRAND_GITHUB_OWNER")
        .env_remove("GOOSE_BRAND_GITHUB_REPO")
        .env_remove("GOOSE_BRAND_AGENT_IDENTITY");

    let output = cmd.output().expect("failed to run goose term init");
    assert!(
        output.status.success(),
        "goose term init {shell} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("non-utf8 shell template output")
}

/// Normalize host-specific output so fixtures are stable across machines:
///
/// - Strip any line that references `AGENT_SESSION_ID` — each of the four
///   shells uses a slightly different assignment syntax (`export X="…"`,
///   `set -gx X "…"`, `$env:X = "…"`), and the value itself is a per-
///   invocation timestamp.
/// - Replace the absolute `current_exe()` path embedded by `term init` with
///   the stable placeholder `{GOOSE_BIN}` (differs by checkout location).
fn normalize(s: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_goose");
    s.lines()
        .filter(|line| !line.contains("AGENT_SESSION_ID"))
        .map(|line| line.replace(bin, "{GOOSE_BIN}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn check_or_update(name: &str, actual: String) {
    let path = fixtures_dir().join(name);
    let actual_normalized = normalize(&actual);

    if std::env::var_os("UPDATE_SHELL_FIXTURES").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &actual_normalized).unwrap();
        eprintln!("wrote fixture {}", path.display());
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "fixture missing: {}. Run `UPDATE_SHELL_FIXTURES=1 cargo test \
             -p goose-cli --test shell_templates` to create it.",
            path.display()
        )
    });
    assert_eq!(
        expected, actual_normalized,
        "fixture {} drift (session_id line stripped, binary path normalized to {{GOOSE_BIN}})",
        name
    );
}

fn scratch_home() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[test]
fn bash_default_brand() {
    let scratch = scratch_home();
    check_or_update("bash.txt", render_shell("bash", false, scratch.path()));
}

#[test]
fn bash_with_command_not_found() {
    let scratch = scratch_home();
    check_or_update(
        "bash_default.txt",
        render_shell("bash", true, scratch.path()),
    );
}

#[test]
fn zsh_default_brand() {
    let scratch = scratch_home();
    check_or_update("zsh.txt", render_shell("zsh", false, scratch.path()));
}

#[test]
fn zsh_with_command_not_found() {
    let scratch = scratch_home();
    check_or_update("zsh_default.txt", render_shell("zsh", true, scratch.path()));
}

#[test]
fn fish_default_brand() {
    let scratch = scratch_home();
    check_or_update("fish.txt", render_shell("fish", false, scratch.path()));
}

#[test]
fn powershell_default_brand() {
    let scratch = scratch_home();
    check_or_update(
        "powershell.txt",
        render_shell("powershell", false, scratch.path()),
    );
}
