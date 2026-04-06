use crate::session::message_to_markdown;
use anyhow::{Context, Result};

use cliclack::{confirm, multiselect, select};
use etcetera::home_dir;
use goose::session::{generate_diagnostics, Session, SessionManager};
use goose::utils::safe_truncate;
use regex::Regex;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

const TRUNCATED_DESC_LENGTH: usize = 60;

fn display_path_with_tilde(path: &Path) -> String {
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

async fn remove_sessions(session_manager: &SessionManager, sessions: Vec<Session>) -> Result<()> {
    println!("The following sessions will be removed:");
    for session in &sessions {
        println!("- {} {}", session.id, session.name);
    }

    let should_delete = confirm("Are you sure you want to delete these sessions?")
        .initial_value(false)
        .interact()?;

    if should_delete {
        for session in sessions {
            session_manager.delete_session(&session.id).await?;
            println!("Session `{}` removed.", session.id);
        }
    } else {
        println!("Skipping deletion of the sessions.");
    }

    Ok(())
}

fn prompt_interactive_session_removal(sessions: &[Session]) -> Result<Vec<Session>> {
    if sessions.is_empty() {
        println!("No sessions to delete.");
        return Ok(vec![]);
    }

    let mut selector = multiselect(
        "Select sessions to delete (use spacebar, Enter to confirm, Ctrl+C to cancel):",
    );

    let display_map: std::collections::HashMap<String, Session> = sessions
        .iter()
        .map(|s| {
            let desc = if s.name.is_empty() {
                "(no name)"
            } else {
                &s.name
            };
            let truncated_desc = safe_truncate(desc, TRUNCATED_DESC_LENGTH);
            let display_text = format!("{} - {} ({})", s.updated_at, truncated_desc, s.id);
            (display_text, s.clone())
        })
        .collect();

    for display_text in display_map.keys() {
        selector = selector.item(display_text.clone(), display_text.clone(), "");
    }

    let selected_display_texts: Vec<String> = selector.interact()?;

    let selected_sessions: Vec<Session> = selected_display_texts
        .into_iter()
        .filter_map(|text| display_map.get(&text).cloned())
        .collect();

    Ok(selected_sessions)
}

pub async fn handle_session_remove(
    session_id: Option<String>,
    name: Option<String>,
    regex_string: Option<String>,
) -> Result<()> {
    let session_manager = SessionManager::instance();

    let matched_sessions: Vec<Session>;

    if let Some(id_val) = session_id {
        match session_manager.get_session(&id_val, false).await {
            Ok(session) => matched_sessions = vec![session],
            Err(_) => return Err(anyhow::anyhow!("Session ID '{}' not found.", id_val)),
        }
    } else if let Some(name_val) = name {
        let all_sessions = session_manager.list_all_sessions().await?;
        if let Some(session) = all_sessions.into_iter().find(|s| s.name == name_val) {
            matched_sessions = vec![session];
        } else {
            return Err(anyhow::anyhow!(
                "Session with name '{}' not found.",
                name_val
            ));
        }
    } else if let Some(regex_val) = regex_string {
        let session_regex = Regex::new(&regex_val)
            .with_context(|| format!("Invalid regex pattern '{}'", regex_val))?;

        let visible_sessions = session_manager.list_sessions().await?;
        matched_sessions = visible_sessions
            .into_iter()
            .filter(|session| session_regex.is_match(&session.id))
            .collect();

        if matched_sessions.is_empty() {
            println!("Regex string '{}' does not match any sessions", regex_val);
            return Ok(());
        }
    } else {
        let visible_sessions = session_manager.list_sessions().await?;
        if visible_sessions.is_empty() {
            return Err(anyhow::anyhow!("No sessions found."));
        }
        matched_sessions = prompt_interactive_session_removal(&visible_sessions)?;
    }

    if matched_sessions.is_empty() {
        return Ok(());
    }

    remove_sessions(&session_manager, matched_sessions).await
}

fn write_line_or_broken_pipe_ok<W: Write>(out: &mut W, line: &str) -> Result<bool> {
    match writeln!(out, "{line}") {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(e) => Err(e.into()),
    }
}

pub async fn handle_session_list(
    format: String,
    ascending: bool,
    working_dir: Option<PathBuf>,
    limit: Option<usize>,
) -> Result<()> {
    let session_manager = SessionManager::instance();
    let mut sessions = session_manager.list_sessions().await?;

    if let Some(ref pat) = working_dir {
        let pat_lower = pat.to_string_lossy().to_lowercase();
        sessions.retain(|s| {
            s.working_dir
                .to_string_lossy()
                .to_lowercase()
                .contains(&pat_lower)
        });
    }

    if ascending {
        sessions.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    } else {
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }

    if let Some(n) = limit {
        sessions.truncate(n);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();

    match format.as_str() {
        "json" => {
            let payload = serde_json::to_string(&sessions)?;
            if !write_line_or_broken_pipe_ok(&mut out, &payload)? {
                return Ok(());
            }
        }
        _ => {
            if sessions.is_empty() {
                if !write_line_or_broken_pipe_ok(&mut out, "No sessions found")? {
                    return Ok(());
                }
                return Ok(());
            }

            if !write_line_or_broken_pipe_ok(&mut out, "Available sessions:")? {
                return Ok(());
            }

            for session in sessions {
                let output = format!(
                    "{} - {} - {} - {}",
                    session.id,
                    session.name,
                    session.updated_at,
                    display_path_with_tilde(&session.working_dir)
                );
                if !write_line_or_broken_pipe_ok(&mut out, &output)? {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

pub async fn handle_session_export(
    session_id: String,
    output_path: Option<PathBuf>,
    format: String,
) -> Result<()> {
    let session_manager = SessionManager::instance();
    let session = match session_manager.get_session(&session_id, true).await {
        Ok(session) => session,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Session '{}' not found or failed to read: {}",
                session_id,
                e
            ));
        }
    };

    let output = match format.as_str() {
        "json" => serde_json::to_string_pretty(&session)?,
        "yaml" => serde_yaml::to_string(&session)?,
        "markdown" => {
            let conversation = session
                .conversation
                .ok_or_else(|| anyhow::anyhow!("Session has no messages"))?;
            export_session_to_markdown(conversation.messages().to_vec(), &session.name)
        }
        _ => return Err(anyhow::anyhow!("Unsupported format: {}", format)),
    };

    if let Some(output_path) = output_path {
        fs::write(&output_path, output).with_context(|| {
            format!("Failed to write to output file: {}", output_path.display())
        })?;
        println!("Session exported to {}", output_path.display());
    } else {
        println!("{}", output);
    }

    Ok(())
}

pub async fn handle_diagnostics(session_id: &str, output_path: Option<PathBuf>) -> Result<()> {
    println!(
        "Generating diagnostics bundle for session '{}'...",
        session_id
    );

    let session_manager = SessionManager::instance();
    let diagnostics_data = generate_diagnostics(&session_manager, session_id)
        .await
        .with_context(|| {
            format!(
                "Failed to write to generate diagnostics bundle for session '{}'",
                session_id
            )
        })?;

    let output_file = if let Some(path) = output_path {
        path.clone()
    } else {
        PathBuf::from(format!("diagnostics_{}.zip", session_id))
    };

    let mut file = fs::File::create(&output_file).context(format!(
        "Failed to create output file: {}",
        output_file.display()
    ))?;

    file.write_all(&diagnostics_data)
        .context("Failed to write diagnostics data")?;

    println!("Diagnostics bundle saved to: {}", output_file.display());

    Ok(())
}

fn export_session_to_markdown(
    messages: Vec<goose::conversation::message::Message>,
    session_name: &String,
) -> String {
    let mut markdown_output = String::new();

    markdown_output.push_str(&format!("# Session Export: {}\n\n", session_name));

    if messages.is_empty() {
        markdown_output.push_str("*(This session has no messages)*\n");
        return markdown_output;
    }

    markdown_output.push_str(&format!("*Total messages: {}*\n\n---\n\n", messages.len()));

    // Track if the last message had tool requests to properly handle tool responses
    let mut skip_next_if_tool_response = false;

    for message in &messages {
        // Check if this is a User message containing only ToolResponses
        let is_only_tool_response = message.role == rmcp::model::Role::User
            && message.content.iter().all(|content| {
                matches!(
                    content,
                    goose::conversation::message::MessageContent::ToolResponse(_)
                )
            });

        // If the previous message had tool requests and this one is just tool responses,
        // don't create a new User section - we'll attach the responses to the tool calls
        if skip_next_if_tool_response && is_only_tool_response {
            // Export the tool responses without a User heading
            markdown_output.push_str(&message_to_markdown(message, false));
            markdown_output.push_str("\n\n---\n\n");
            skip_next_if_tool_response = false;
            continue;
        }

        // Reset the skip flag - we'll update it below if needed
        skip_next_if_tool_response = false;

        // Output the role prefix except for tool response-only messages
        if !is_only_tool_response {
            let role_prefix = match message.role {
                rmcp::model::Role::User => "### User:\n",
                rmcp::model::Role::Assistant => "### Assistant:\n",
            };
            markdown_output.push_str(role_prefix);
        }

        // Add the message content
        markdown_output.push_str(&message_to_markdown(message, false));
        markdown_output.push_str("\n\n---\n\n");

        // Check if this message has any tool requests, to handle the next message differently
        if message.content.iter().any(|content| {
            matches!(
                content,
                goose::conversation::message::MessageContent::ToolRequest(_)
            )
        }) {
            skip_next_if_tool_response = true;
        }
    }

    markdown_output
}

/// Prompt the user to interactively select a session
///
/// Shows a list of available sessions and lets the user select one
pub async fn prompt_interactive_session_selection(
    session_manager: &SessionManager,
) -> Result<String> {
    let sessions = session_manager.list_sessions().await?;

    if sessions.is_empty() {
        return Err(anyhow::anyhow!("No sessions found"));
    }

    // Build the selection prompt
    let mut selector = select("Select a session to export:");

    // Map to display text
    let display_map: std::collections::HashMap<String, Session> = sessions
        .iter()
        .map(|s| {
            let desc = if s.name.is_empty() {
                "(no name)"
            } else {
                &s.name
            };
            let truncated_desc = safe_truncate(desc, TRUNCATED_DESC_LENGTH);

            let display_text = format!("{} - {} ({})", s.updated_at, truncated_desc, s.id);
            (display_text, s.clone())
        })
        .collect();

    // Add each session as an option
    for display_text in display_map.keys() {
        selector = selector.item(display_text.clone(), display_text.clone(), "");
    }

    // Add a cancel option
    let cancel_value = String::from("cancel");
    selector = selector.item(cancel_value, "Cancel", "Cancel export");

    // Get user selection
    let selected_display_text: String = selector.interact()?;

    if selected_display_text == "cancel" {
        return Err(anyhow::anyhow!("Export canceled"));
    }

    // Retrieve the selected session
    if let Some(session) = display_map.get(&selected_display_text) {
        Ok(session.id.clone())
    } else {
        Err(anyhow::anyhow!("Invalid selection"))
    }
}
