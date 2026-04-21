//! Fixture tests for the shell integration templates emitted by `goose term init`.
//!
//! These tests run the sibling CLI binary Cargo builds for integration tests,
//! normalize the output for host-portable comparison, and diff against
//! checked-in fixtures. The fixtures freeze the default shell-brand output of
//! `main@HEAD`; branded builds run smoke assertions instead.
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
use std::sync::OnceLock;

use goose_cli::Brand;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shell_templates")
}

fn cli_bin() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(resolve_cli_bin).as_path()
}

fn resolve_cli_bin() -> PathBuf {
    let target_dir = std::env::current_exe()
        .expect("failed to resolve test binary path")
        .parent()
        .and_then(Path::parent)
        .expect("failed to resolve target dir from test binary")
        .to_path_buf();
    let branded_name = format!(
        "{}{}",
        Brand::get().binary_name,
        std::env::consts::EXE_SUFFIX
    );
    let branded_path = target_dir.join(&branded_name);
    if branded_path.is_file() {
        return branded_path;
    }

    let bins: Vec<PathBuf> = fs::read_dir(&target_dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", target_dir.display()))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.file_type().ok()?.is_file().then_some(entry.path())
        })
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            !name.starts_with('.')
                && name != format!("generate_manpages{}", std::env::consts::EXE_SUFFIX)
                && !name.ends_with(".d")
        })
        .collect();

    match bins.as_slice() {
        [path] => path.clone(),
        [] => panic!(
            "no CLI test binary found next to test artifacts in {}",
            target_dir.display()
        ),
        _ => panic!(
            "expected exactly one CLI test binary in {}, found: {}",
            target_dir.display(),
            bins.iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn shell_brand_matches_default() -> bool {
    let brand = Brand::get();
    brand.product_name == "goose"
        && brand.binary_name == "goose"
        && brand.shell_alias_primary == "goose"
        && brand.shell_alias_short == "g"
        && brand.shell_fn_prefix == "goose"
        && brand.interactive_style == "goose"
}

/// Run `goose term init <shell>` (optionally with `--default`) in an isolated
/// HOME/XDG env so it doesn't create sessions in the real user's config.
fn render_shell(shell: &str, with_default: bool, scratch: &Path) -> String {
    let mut cmd = Command::new(cli_bin());
    cmd.arg("term").arg("init").arg(shell);
    if with_default {
        cmd.arg("--default");
    }
    cmd.env("HOME", scratch)
        .env("XDG_DATA_HOME", scratch.join("data"))
        .env("XDG_CONFIG_HOME", scratch.join("config"))
        .env("XDG_STATE_HOME", scratch.join("state"));

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
    let bin = cli_bin().to_string_lossy();
    s.lines()
        .filter(|line| !line.contains("AGENT_SESSION_ID"))
        .map(|line| line.replace(bin.as_ref(), "{GOOSE_BIN}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_branded_output(name: &str, shell: &str, with_default: bool, actual_normalized: &str) {
    let brand = Brand::get();

    assert!(
        !actual_normalized.is_empty(),
        "{name} produced empty output for shell {shell}"
    );

    for placeholder in [
        "{session_id}",
        "{goose_bin}",
        "{binary_name}",
        "{product_name}",
        "{interactive_prefix}",
        "{alias_primary}",
        "{alias_short}",
        "{fn_prefix}",
        "{command_not_found_handler}",
    ] {
        assert!(
            !actual_normalized.contains(placeholder),
            "{name} left placeholder {placeholder} in rendered output:\n{actual_normalized}"
        );
    }

    for expected in [
        "{GOOSE_BIN}",
        brand.binary_name,
        brand.shell_alias_primary,
        brand.shell_alias_short,
        brand.shell_fn_prefix,
    ] {
        assert!(
            actual_normalized.contains(expected),
            "{name} missing branded token `{expected}` in rendered output:\n{actual_normalized}"
        );
    }

    if with_default {
        let expected_handler = format!(
            "{}Command '$1' not found. Asking {}...",
            brand.interactive_prefix(),
            brand.product_name
        );
        assert!(
            actual_normalized.contains(&expected_handler),
            "{name} missing branded command-not-found message:\n{actual_normalized}"
        );
    } else {
        assert!(
            !actual_normalized.contains("Asking "),
            "{name} unexpectedly rendered command-not-found handler:\n{actual_normalized}"
        );
    }
}

fn check_or_update(name: &str, shell: &str, with_default: bool, actual: String) {
    let path = fixtures_dir().join(name);
    let actual_normalized = normalize(&actual);

    if !shell_brand_matches_default() {
        assert!(
            std::env::var_os("UPDATE_SHELL_FIXTURES").is_none(),
            "shell fixtures snapshot only the default shell brand; rebuild without GOOSE_BRAND_* shell overrides to update {}",
            path.display()
        );
        assert_branded_output(name, shell, with_default, &actual_normalized);
        return;
    }

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
    check_or_update(
        "bash.txt",
        "bash",
        false,
        render_shell("bash", false, scratch.path()),
    );
}

#[test]
fn bash_with_command_not_found() {
    let scratch = scratch_home();
    check_or_update(
        "bash_default.txt",
        "bash",
        true,
        render_shell("bash", true, scratch.path()),
    );
}

#[test]
fn zsh_default_brand() {
    let scratch = scratch_home();
    check_or_update(
        "zsh.txt",
        "zsh",
        false,
        render_shell("zsh", false, scratch.path()),
    );
}

#[test]
fn zsh_with_command_not_found() {
    let scratch = scratch_home();
    check_or_update(
        "zsh_default.txt",
        "zsh",
        true,
        render_shell("zsh", true, scratch.path()),
    );
}

#[test]
fn fish_default_brand() {
    let scratch = scratch_home();
    check_or_update(
        "fish.txt",
        "fish",
        false,
        render_shell("fish", false, scratch.path()),
    );
}

#[test]
fn powershell_default_brand() {
    let scratch = scratch_home();
    check_or_update(
        "powershell.txt",
        "powershell",
        false,
        render_shell("powershell", false, scratch.path()),
    );
}
