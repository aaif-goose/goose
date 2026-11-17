---
title: GDK
displayed_sidebar: gdkSidebar
hide_table_of_contents: true
description: Build agents with GDK.
---

# GDK

Build agents that run across native applications, servers, and the browser,
resume from persisted state, and give you control over how they work.

GDK is the set of reusable agent-runtime components used to build goose. It
provides model integrations, a composable state machine, tools, context
management, and the interfaces needed to run an agent inside your application.

## Explore GDK

### [Getting Started](/docs/gdk/hello-world)

Build a [small Rust agent](https://github.com/jamadeo/ngrc/tree/dc25c579cd40d85f8d90b61e80cbf02b8838564f)
that can run shell commands, stream responses, and save its conversation between
runs. The walkthrough explains how to connect a provider, tools, and a state
machine, and which parts your application owns. Start here to see the pieces
working together before choosing how to adapt them.

### [Demos](/docs/gdk/demos)

Explore working projects that use GDK for [personal wikis](/docs/gdk/demos/lifewiki),
[phone calls](/docs/gdk/demos/goosebusters),
[screen understanding](/docs/gdk/demos/perception),
[voice conversations](/docs/gdk/demos/huddle), and
[custom interfaces to goose](/docs/gdk/demos/berd). Each demo links to its source
and explains the architectural idea behind it, along with what you need to run
it. Use them to see how the same components fit very different applications.

### [Concepts](/docs/gdk/concepts)

Understand the [state machine](/docs/gdk/concepts/state-machine),
[operations](/docs/gdk/concepts/operations),
[providers](/docs/gdk/concepts/providers), and
[authentication](/docs/gdk/concepts/authentication), and how they fit together.
These guides explain the problems each component solves, the choices behind its
design, and the implementations available today. Worked examples show how to
compose agent behavior and when to write your own operation or provider.

### [Reference](/docs/gdk/reference)

Look up the generated Rust APIs for the
[agent runtime](https://docs.rs/goose-agent/latest/goose_agent/),
[providers](https://docs.rs/goose-providers/latest/goose_providers/), and
[context management](https://docs.rs/goose-context-management/latest/goose_context_management/).
Explore the shared provider bindings through the
[Python API](/docs/gdk/sdk/api-reference?language=python) and
[Kotlin API](/docs/gdk/sdk/api-reference?language=kotlin), with runnable examples in
[Python](https://github.com/aaif-goose/goose/blob/main/crates/goose-sdk/examples/uniffi/provider.py)
and [Kotlin](https://github.com/aaif-goose/goose/blob/main/crates/goose-sdk/examples/uniffi/kotlin/src/main/kotlin/Main.kt).
For apps using goose as a backend, consult the [ACP reference](/docs/gdk/acp/reference).
Use the reference when you need exact types, methods, and configuration details
while building.

## GDK and goose

[goose](/docs/quickstart) is a complete agent application and the main reference
implementation for GDK. Use goose when you want a ready-to-run agent. Use GDK
when you want to build agent behavior into your own product.

The pre-built goose binary can also serve as the agent behind any
[ACP application](/docs/gdk/acp) you build. goose Desktop works this way,
connecting to a local goose server over ACP.
