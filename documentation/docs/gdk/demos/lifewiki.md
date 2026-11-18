---
title: lifewiki
description: A personal wiki that reconciles changing facts while preserving the original notes.
displayed_sidebar: gdkSidebar
---

# lifewiki

Tell lifewiki something about your life and an agent turns it into a personal
wiki. Add a later note that contradicts an earlier one, and the agent updates
the relevant pages while keeping the original notes and their history.

Built with **GDK**: `goose-agent`, `goose-providers`, and optional local inference.

[Source](https://github.com/jamadeo/lifewiki) ·
[Watch the recording](https://github.com/jamadeo/lifewiki/blob/main/demo/demo.mov)

## The clever bit

The agent can rewrite the wiki because the wiki is not the source of truth.
The append-only log is. Every generated claim can cite the note behind it,
and git records the changes. If the prompts or model improve, the wiki can be
rebuilt from the original log.

Building and asking use the same agent loop with different capabilities.
A build can search, read, and write pages. A question opens the workspace
read-only and does not offer write tools. The distinction is enforced by the
application, not just an instruction asking the model to behave.

## How it works

1. Capture a note as plain text in the log.
2. Run a build. The agent searches and reads the wiki, then reconciles the new
   information with existing pages.
3. Commit the wiki changes and processing cursor together.
4. Answer questions from the resulting wiki with a separate, read-only role.

The application supplies its own operations to GDK's state machine. In
[`src/agent.rs`](https://github.com/jamadeo/lifewiki/blob/main/src/agent.rs),
the steps put limits, completion, and tool execution ahead of inference:

```rust
let steps: Vec<Step<'_, Session, ConversationEffect>> = vec![
    Step::Operation(Arc::new(MaxTurnsOperation)),
    Step::Operation(Arc::new(TerminalOperation)),
    Step::Operation(Arc::new(EndTurnOperation)),
    Step::Operation(Arc::new(ToolExecutionOperation)),
    Step::Inference(Arc::new(InferenceRunner {
        backend: self.backend.clone(),
    })),
];
```

GDK supplies the execution machinery; lifewiki owns the tools, roles, storage,
and rules for finishing a run. See also the
[tool implementation](https://github.com/jamadeo/lifewiki/blob/main/src/tools.rs)
and [design notes](https://github.com/jamadeo/lifewiki/blob/main/docs/design.md).

## Run it yourself

You need Rust and either an Anthropic API key or a configured local model.
Local inference has extra build and hardware requirements; follow the
[repository's instructions](https://github.com/jamadeo/lifewiki#local-inference)
for that path.

For a hosted-model build without the optional local backend:

```bash
git clone https://github.com/jamadeo/lifewiki.git
cd lifewiki
cargo install --path . --no-default-features
lw init
```

With `ANTHROPIC_API_KEY` set in your environment:

```bash
lw add "Ada lives in Lisbon."
lw add "Ada moved to Porto last month."
lw build
lw ask "Where does Ada live?"
```

The [recorded walkthrough](https://github.com/jamadeo/lifewiki/blob/main/demo/README.md)
shows reconciliation, citations, questions, and git history. Its local model is
pre-warmed and playback pauses are shortened; it does not show first-run loading time.
