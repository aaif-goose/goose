---
title: Huddle
description: A voice interface that delegates longer tasks to an ACP agent through GDK live provider events.
displayed_sidebar: gdkSidebar
---

# Huddle

Huddle is a small desktop application for talking to a local agent. A live voice
model handles the conversation. When a request needs work, Huddle passes it to
an ACP agent and sends the result back to the voice model to speak aloud.

Built with **GDK + ACP**: the live provider interface from `goose-providers`,
with goose or another ACP agent doing the delegated work.

[Source](https://github.com/jamadeo/goose-huddle)

## The clever bit

The voice conversation and the working agent have different jobs. The voice
model handles speech; a separate agent has the tools and context to carry out
the task. They do not need to be the same model or run in the same process.

Delegation events identify a position in the spoken conversation. Huddle turns
the transcript up to that point into an agent prompt, then returns the answer
to the corresponding delegation. Requests queue while the working agent is busy.

This is a small example of composing provider capabilities with an existing
agent instead of rebuilding the entire stack for a new interface.

## How it works

1. The webview captures microphone audio and plays the response over WebRTC.
2. Rust starts the live session through GDK and listens on its sideband control
   connection for transcript and delegation events.
3. A delegation becomes a prompt to the persistent ACP session.
4. The agent's result is returned through the live provider connection.

In [`voice.rs`](https://github.com/jamadeo/goose-huddle/blob/main/src-tauri/src/voice.rs),
the return path uses the provider's delegation interface:

```rust
let delivery = connection
    .send_delegation_update(DelegationUpdate {
        provider_delegation_id: delegation_id.clone(),
        text: text.clone(),
    })
    .await?;
```

The [transcript code](https://github.com/jamadeo/goose-huddle/blob/main/src-tauri/src/transcript.rs)
chooses the context for each request. The
[ACP client](https://github.com/jamadeo/goose-huddle/blob/main/src-tauri/src/acp.rs)
owns the agent subprocess and serializes prompts.

## Run it yourself

You need Rust 1.94 or later, an OpenAI API key with GPT-Live access, microphone
permission, and an ACP agent on your `PATH`. The default is `goose acp`.
Configure goose's model provider separately if you use that default.

With `OPENAI_API_KEY` set in your environment:

```bash
git clone https://github.com/jamadeo/goose-huddle.git
cd goose-huddle/src-tauri
cargo run
```

Open Settings to choose the agent command and working directory, then start a
huddle. See the [README](https://github.com/jamadeo/goose-huddle#running) for
packaging and configuration details.

:::warning Prototype permissions
Huddle automatically approves agent tool requests and can save the API key in
browser local storage. Try it in a disposable working directory with credentials
and resources appropriate for a prototype.
:::
