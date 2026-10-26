---
title: GDK Overview
sidebar_label: Overview
description: Build on goose with the SDK and ACP.
---

# GDK

The goose Development Kit (GDK) provides the core components developers need to
build agentic applications, including agent orchestration, model access, context
management, tools, memory, remote execution, automations, and routing. There are
two ways to use it:

- [SDK](/docs/gdk/sdk) — use goose's native Rust crates or the shared
  provider bindings for Python and Kotlin as in-process libraries.
- [ACP](/docs/gdk/acp) — connect to goose as a separate agent process over
  stdio, HTTP, or WebSocket.

For Rust, start with the crate documentation on docs.rs:
[`goose-agent`](https://docs.rs/goose-agent/latest/goose_agent/),
[`goose-providers`](https://docs.rs/goose-providers/latest/goose_providers/), and
[`goose-context-management`](https://docs.rs/goose-context-management/latest/goose_context_management/).
See the [Rust API reference](/docs/gdk/sdk/api-reference#rust-api-reference) for
the full list of GDK crates.
