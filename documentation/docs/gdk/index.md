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

- [SDK](/docs/gdk/sdk) — use goose's provider layer as an in-process
  library from Rust, Python, or Kotlin.
- [ACP](/docs/gdk/acp) — connect to goose as a separate agent process over
  stdio, HTTP, or WebSocket.

## Agent skill

goose ships a built-in `gdk` skill covering the SDK API, the ACP server, and the
failure modes that matter, so goose can write GDK programs without being told the
surface first. It is enabled by default — no installation needed.

To give another agent the same context, download
[`gdk.md`](pathname:///files/skills/gdk.md) and drop it in that agent's skills directory
(for goose that is `~/.agents/skills/`).
