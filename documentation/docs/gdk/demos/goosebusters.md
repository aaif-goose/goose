---
title: GooseBusters
description: A phone-call assistant with custom operations for approval, supervision, and structured outcomes.
displayed_sidebar: gdkSidebar
---

# GooseBusters

Give GooseBusters a phone number and a task. It proposes a plan, asks for your
approval, conducts the call, and reports what happened with supporting quotes
from the conversation. A CLI and a Tauri desktop app expose the same workflow.

Built with **GDK**: `goose-agent` and the provider components, without depending
on the complete goose application.

[Source](https://github.com/DOsinga/goose-busters) ·
[Design](https://github.com/DOsinga/goose-busters/blob/main/GooseBusters.md)

## The clever bit

A phone call is not a sequence of model responses and tool calls. Speech and
webhooks arrive while other work is running. An earlier suggestion can become
stale as soon as the person on the phone says something new.

GooseBusters expresses this as custom state-machine operations: planning,
approval, dialing, call events, supervision, and final extraction. Incoming
speech can preempt a supervisor turn. The application checks whether an action
is still current before applying it.

The model is one part of the workflow, rather than the owner of its lifecycle.
Application code controls approval, the call record, and the effects applied to it.

## How it works

1. A planning turn produces voice-agent instructions and fields to collect.
2. The user approves, aborts, or gives feedback for a revised plan.
3. A Telnyx call connects to an OpenAI voice backend. The supervisor processes
   events and tracks progress while the conversation happens.
4. After hangup, extraction checks the transcript for the requested fields and
   the callee's supporting quotes. The call record is saved as YAML.

The [supervisor](https://github.com/DOsinga/goose-busters/blob/main/src/supervisor.rs)
rebuilds the machine for each turn from a snapshot of the application's record:

```rust
let snapshot = record.snapshot();
let turn = call_cancel.child_token();
pipeline.handle.set_turn_token(turn.clone());
let machine = StateMachine::new(build_steps(pipeline), turn.clone());
let emit = Emitter::new(tx.clone(), turn.clone());
```

It calls `machine.step`, applies the returned effects to its own record, and
emits UI events. The CLI and desktop app render those events differently without
implementing separate agent workflows. Read the
[operations](https://github.com/DOsinga/goose-busters/blob/main/src/ops.rs)
alongside the supervisor to see where the application controls execution.

## Run it yourself

This is an end-to-end prototype. Real calls need Rust, Telnyx and OpenAI
accounts, and a publicly reachable webhook endpoint. The repository documents
a Cloudflare tunnel setup and the credentials for each voice backend.

```bash
git clone https://github.com/DOsinga/goose-busters.git
cd goose-busters
```

Complete the [account and webhook setup](https://github.com/DOsinga/goose-busters#setup)
before starting the desktop app:

```bash
cargo run -p goosebusters
```

Use a number and recipient you have permission to call. Calls send audio and
transcripts to external services and may incur charges. See the repository's
[responsible-use notes](https://github.com/DOsinga/goose-busters#responsible-use)
before making a real call.
