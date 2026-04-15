//! ACP Client Example
//!
//! Spawns `goose acp` as a child process and sends it a completion request
//! using the Agent Client Protocol over stdio.
//!
//! # Prerequisites
//!
//! You must have goose built and a provider configured (`goose configure`).
//!
//! # Usage
//!
//! ```bash
//! cargo run -p goose-sdk --example acp_client -- "What is 2 + 2?"
//! ```
//!
//! Or with a custom goose binary path:
//!
//! ```bash
//! cargo run -p goose-sdk --example acp_client -- --goose-bin ./target/debug/goose "Explain Rust's ownership model in one sentence"
//! ```

use goose_sdk::custom_requests::GetExtensionsRequest;
use sacp::schema::{
    ContentBlock, InitializeRequest, ProtocolVersion, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate,
};
use sacp::{Client, ConnectionTo};
use std::path::PathBuf;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse args: [--goose-bin PATH] PROMPT
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (goose_bin, prompt) = parse_args(&args)?;

    eprintln!("🚀 Spawning: {} acp", goose_bin.display());

    let mut child = tokio::process::Command::new(&goose_bin)
        .arg("acp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to spawn '{}': {e}", goose_bin.display()))?;

    let child_stdin = child.stdin.take().expect("stdin should be piped");
    let child_stdout = child.stdout.take().expect("stdout should be piped");

    let transport = sacp::ByteStreams::new(child_stdin.compat_write(), child_stdout.compat());

    let prompt_clone = prompt.clone();

    Client
        .builder()
        .name("acp-client-example")
        // Print session notifications (agent text, tool calls, etc.)
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                match &notification.update {
                    SessionUpdate::AgentMessageChunk(chunk) => {
                        if let ContentBlock::Text(text) = &chunk.content {
                            print!("{}", text.text);
                        }
                    }
                    SessionUpdate::ToolCall(tool_call) => {
                        eprintln!("🔧 Tool call: {}", tool_call.title);
                    }
                    SessionUpdate::ToolCallUpdate(update) => {
                        if let Some(status) = &update.fields.status {
                            eprintln!("   Tool status: {:?}", status);
                        }
                    }
                    _ => {}
                }
                Ok(())
            },
            sacp::on_receive_notification!(),
        )
        // Auto-approve all permission requests
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                eprintln!("✅ Auto-approving permission request");
                let option_id = request.options.first().map(|opt| opt.option_id.clone());
                match option_id {
                    Some(id) => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                    )),
                    None => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    )),
                }
            },
            sacp::on_receive_request!(),
        )
        .connect_with(transport, async move |cx: ConnectionTo<sacp::Agent>| {
            // Step 1: Initialize
            eprintln!("🤝 Initializing...");
            let init_response = cx
                .send_request(InitializeRequest::new(ProtocolVersion::LATEST))
                .block_task()
                .await?;
            eprintln!("✓ Agent initialized: {:?}", init_response.agent_info);

            let response = cx
                .send_request(GetExtensionsRequest {})
                .block_task()
                .await?;
            eprintln!("Extensions: {:?}", response.extensions);

            // Step 2: Create a session and send the prompt
            eprintln!("💬 Sending prompt: \"{}\"", prompt_clone);
            cx.build_session_cwd()?
                .block_task()
                .run_until(async |mut session| {
                    session.send_prompt(&prompt_clone)?;
                    let response = session.read_to_string().await?;

                    // read_to_string collects text; we already printed chunks above,
                    // so just print a newline to finish.
                    println!();
                    eprintln!("✅ Done ({} chars)", response.len());
                    Ok(())
                })
                .await
        })
        .await?;

    let _ = child.kill().await;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, String), String> {
    let mut goose_bin = PathBuf::from("goose");
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--goose-bin" => {
                i += 1;
                goose_bin = PathBuf::from(args.get(i).ok_or("--goose-bin requires a value")?);
            }
            _ => break,
        }
        i += 1;
    }

    let prompt = args[i..].join(" ");

    if prompt.is_empty() {
        return Err("Usage: acp_client [--goose-bin PATH] PROMPT".into());
    }

    Ok((goose_bin, prompt))
}
