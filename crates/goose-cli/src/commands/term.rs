use anyhow::{anyhow, Result};
use chrono;
use goose::config::Config;
use goose::conversation::message::{Message, MessageContent, MessageMetadata};
use goose::session::{SessionManager, SessionType};
use rmcp::model::Role;

use crate::branding::Brand;
use crate::session::{build_session, SessionBuilderConfig};

use clap::ValueEnum;

#[derive(ValueEnum, Clone, Debug)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    #[value(alias = "pwsh")]
    Powershell,
}

struct ShellConfig {
    script_template: &'static str,
    command_not_found: Option<&'static str>,
}

impl Shell {
    fn config(&self) -> &'static ShellConfig {
        match self {
            Shell::Bash => &BASH_CONFIG,
            Shell::Zsh => &ZSH_CONFIG,
            Shell::Fish => &FISH_CONFIG,
            Shell::Powershell => &POWERSHELL_CONFIG,
        }
    }
}

static BASH_CONFIG: ShellConfig = ShellConfig {
    script_template: r#"export AGENT_SESSION_ID="{session_id}"
alias @{alias_primary}='{goose_bin} term run'
alias @{alias_short}='{goose_bin} term run'

{fn_prefix}_preexec() {
    [[ "$1" =~ ^{binary_name}\ term ]] && return
    [[ "$1" =~ ^(@{alias_primary}|@{alias_short})($|[[:space:]]) ]] && return
    ('{goose_bin}' term log "$1" &) 2>/dev/null
}

if [[ -z "${fn_prefix}_preexec_installed" ]]; then
    {fn_prefix}_preexec_installed=1
    trap '{fn_prefix}_preexec "$BASH_COMMAND"' DEBUG
fi{command_not_found_handler}"#,
    command_not_found: Some(
        r#"

command_not_found_handle() {
    echo "{interactive_prefix}Command '$1' not found. Asking {product_name}..."
    '{goose_bin}' term run "$@"
    return 0
}"#,
    ),
};

static ZSH_CONFIG: ShellConfig = ShellConfig {
    script_template: r#"export AGENT_SESSION_ID="{session_id}"
alias @{alias_primary}='{goose_bin} term run'
alias @{alias_short}='{goose_bin} term run'

{fn_prefix}_preexec() {
    [[ "$1" =~ ^{binary_name}\ term ]] && return
    [[ "$1" =~ ^(@{alias_primary}|@{alias_short})($|[[:space:]]) ]] && return
    ('{goose_bin}' term log "$1" &) 2>/dev/null
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec {fn_prefix}_preexec{command_not_found_handler}"#,
    command_not_found: Some(
        r#"

command_not_found_handler() {
    echo "{interactive_prefix}Command '$1' not found. Asking {product_name}..."
    '{goose_bin}' term run "$@"
    return 0
}"#,
    ),
};

static FISH_CONFIG: ShellConfig = ShellConfig {
    script_template: r#"set -gx AGENT_SESSION_ID "{session_id}"
function @{alias_primary}; {goose_bin} term run $argv; end
function @{alias_short}; {goose_bin} term run $argv; end

function {fn_prefix}_preexec --on-event fish_preexec
    string match -q -r '^{binary_name} term' -- $argv[1]; and return
    string match -q -r '^(@{alias_primary}|@{alias_short})($|\s)' -- $argv[1]; and return
    {goose_bin} term log "$argv[1]" 2>/dev/null &
end"#,
    command_not_found: None,
};

static POWERSHELL_CONFIG: ShellConfig = ShellConfig {
    script_template: r#"$env:AGENT_SESSION_ID = "{session_id}"
function @{alias_primary} {{ & '{goose_bin}' term run @args }}
function @{alias_short} {{ & '{goose_bin}' term run @args }}

Set-PSReadLineKeyHandler -Chord Enter -ScriptBlock {{
    $line = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$null)
    if ($line -notmatch '^{binary_name} term' -and $line -notmatch '^(@{alias_primary}|@{alias_short})($|\s)') {{
        Start-Job -ScriptBlock {{ & '{goose_bin}' term log $using:line }} | Out-Null
    }}
    [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
}}"#,
    command_not_found: None,
};

pub async fn handle_term_init(
    shell: Shell,
    name: Option<String>,
    with_command_not_found: bool,
) -> Result<()> {
    let session_manager = SessionManager::instance();

    let working_dir = std::env::current_dir()?;
    let named_session = if let Some(ref name) = name {
        let sessions = session_manager
            .list_sessions_by_types(&[SessionType::Terminal])
            .await?;
        sessions.into_iter().find(|s| s.name == *name)
    } else {
        None
    };

    let session = match named_session {
        Some(s) => s,
        None => {
            let session = session_manager
                .create_session(
                    working_dir,
                    format!("{} Term Session", Brand::get().product_name_cap()),
                    SessionType::Terminal,
                    Config::global().get_goose_mode().unwrap_or_default(),
                )
                .await?;

            if let Some(name) = name {
                session_manager
                    .update(&session.id)
                    .user_provided_name(name)
                    .apply()
                    .await?;
            }

            session
        }
    };

    let brand = Brand::get();
    let goose_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| brand.binary_name.to_string());

    let script = render_shell_script(
        shell.config(),
        &session.id,
        &goose_bin,
        with_command_not_found,
        brand,
    );

    println!("{}", script);
    Ok(())
}

/// Render a shell integration script from a template and the active branding.
///
/// Extracted from [`handle_term_init`] so downstream tests can exercise the
/// placeholder substitution without creating a real session.
fn render_shell_script(
    config: &ShellConfig,
    session_id: &str,
    goose_bin: &str,
    with_command_not_found: bool,
    brand: &Brand,
) -> String {
    let apply_brand_placeholders = |s: &str| -> String {
        s.replace("{goose_bin}", goose_bin)
            .replace("{binary_name}", brand.binary_name)
            .replace("{product_name}", brand.product_name)
            .replace("{interactive_prefix}", brand.interactive_prefix())
            .replace("{alias_primary}", brand.shell_alias_primary)
            .replace("{alias_short}", brand.shell_alias_short)
            .replace("{fn_prefix}", brand.shell_fn_prefix)
    };

    let command_not_found_handler = if with_command_not_found {
        config
            .command_not_found
            .map(apply_brand_placeholders)
            .unwrap_or_default()
    } else {
        String::new()
    };

    apply_brand_placeholders(config.script_template)
        .replace("{session_id}", session_id)
        .replace("{command_not_found_handler}", &command_not_found_handler)
}

pub async fn handle_term_log(command: String) -> Result<()> {
    let session_id = std::env::var("AGENT_SESSION_ID").map_err(|_| {
        anyhow!(
            "AGENT_SESSION_ID not set. Run 'eval \"$({} term init <shell>)\"' first.",
            Brand::get().binary_name
        )
    })?;

    let message = Message::new(
        Role::User,
        chrono::Utc::now().timestamp_millis(),
        vec![MessageContent::text(command)],
    )
    .with_metadata(MessageMetadata::user_only())
    .with_generated_id();

    let session_manager = SessionManager::instance();
    session_manager.add_message(&session_id, &message).await?;

    Ok(())
}

pub async fn handle_term_run(prompt: Vec<String>) -> Result<()> {
    let prompt = prompt.join(" ");
    let session_id = std::env::var("AGENT_SESSION_ID").map_err(|_| {
        anyhow!(
            "AGENT_SESSION_ID not set.\n\n\
             Add to your shell config (~/.zshrc or ~/.bashrc):\n    \
             eval \"$({} term init zsh)\"\n\n\
             Then restart your terminal or run: source ~/.zshrc",
            Brand::get().binary_name
        )
    })?;

    let working_dir = std::env::current_dir()?;
    let session_manager = SessionManager::instance();

    session_manager
        .update(&session_id)
        .working_dir(working_dir)
        .apply()
        .await?;

    let session = session_manager.get_session(&session_id, true).await?;
    let user_messages_after_last_assistant: Vec<&Message> =
        if let Some(conv) = &session.conversation {
            conv.messages()
                .iter()
                .rev()
                .take_while(|m| m.role != Role::Assistant)
                .collect()
        } else {
            Vec::new()
        };

    if let Some(oldest_user) = user_messages_after_last_assistant.last() {
        session_manager
            .truncate_conversation(&session_id, oldest_user.created)
            .await?;
    }

    let prompt_with_context = if user_messages_after_last_assistant.is_empty() {
        prompt
    } else {
        let history = user_messages_after_last_assistant
            .iter()
            .rev() // back to chronological order
            .map(|m| m.as_concat_text())
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "<shell_history>\n{}\n</shell_history>\n\n{}",
            history, prompt
        )
    };

    let config = SessionBuilderConfig {
        session_id: Some(session_id),
        resume: true,
        interactive: false,
        quiet: true,
        ..Default::default()
    };

    let mut session = build_session(config).await;
    session.headless(prompt_with_context).await?;

    Ok(())
}

/// Handle `goose term info` - print compact session info for prompt integration
pub async fn handle_term_info() -> Result<()> {
    let session_id = match std::env::var("AGENT_SESSION_ID") {
        Ok(id) => id,
        Err(_) => return Ok(()),
    };

    let session_manager = SessionManager::instance();
    let session = session_manager.get_session(&session_id, false).await.ok();
    let total_tokens = session.as_ref().and_then(|s| s.total_tokens).unwrap_or(0) as usize;

    let config = goose::config::Config::global();
    let model_name = config
        .get_goose_model()
        .ok()
        .map(|name| {
            let short = name.rsplit('/').next().unwrap_or(&name);
            if let Some(stripped) = short.strip_prefix("goose-") {
                stripped.to_string()
            } else {
                short.to_string()
            }
        })
        .unwrap_or_else(|| "?".to_string());

    let context_limit = config
        .get_goose_model()
        .ok()
        .and_then(|model_name| {
            config.get_goose_provider().ok().and_then(|provider_name| {
                goose::model::ModelConfig::new(&model_name)
                    .ok()
                    .map(|c| c.with_canonical_limits(&provider_name))
            })
        })
        .map(|mc| mc.context_limit())
        .unwrap_or(128_000);

    let percentage = if context_limit > 0 {
        ((total_tokens as f64 / context_limit as f64) * 100.0).round() as usize
    } else {
        0
    };

    let filled = (percentage / 20).min(5);
    let empty = 5 - filled;
    let dots = format!("{}{}", "●".repeat(filled), "○".repeat(empty));

    println!("{} {}", dots, model_name);

    Ok(())
}
