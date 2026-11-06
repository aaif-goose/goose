---
title: oi-kit
displayed_sidebar: gdkSidebar
hide_table_of_contents: true
description: Build agents with oi-kit.
---

# oi-kit

Build agents that run across native applications, servers, and the browser,
resume from persisted state, and give you control over how they work.

oi-kit is the set of reusable agent-runtime components used to build goose. It
provides model integrations, a composable state machine, tools, context
management, and the interfaces needed to run an agent inside your application.

## Explore oi-kit {#explore-gdk}

### [Getting Started](/docs/oi-kit/hello-world)

Build a [small Rust agent](https://github.com/jamadeo/ngrc/tree/dc25c579cd40d85f8d90b61e80cbf02b8838564f)
that can run shell commands, stream responses, and save its conversation between
runs. The walkthrough explains how to connect a provider, tools, and a state
machine, and which parts your application owns. Start here to see the pieces
working together before choosing how to adapt them.

### [Demos](/docs/oi-kit/demos)

Explore working projects that use oi-kit for [personal wikis](/docs/oi-kit/demos/lifewiki),
[phone calls](/docs/oi-kit/demos/goosebusters),
[screen understanding](/docs/oi-kit/demos/perception),
[voice conversations](/docs/oi-kit/demos/huddle), and
[custom interfaces to goose](/docs/oi-kit/demos/berd). Each demo links to its source
and explains the architectural idea behind it, along with what you need to run
it. Use them to see how the same components fit very different applications.

### [Concepts](/docs/oi-kit/concepts)

Understand the [state machine](/docs/oi-kit/concepts/state-machine),
[operations](/docs/oi-kit/concepts/operations),
[providers](/docs/oi-kit/concepts/providers), and
[authentication](/docs/oi-kit/concepts/authentication), and how they fit together.
These guides explain the problems each component solves, the choices behind its
design, and the implementations available today. Worked examples show how to
compose agent behavior and when to write your own operation or provider.

### [Reference](/docs/oi-kit/reference)

Look up the generated Rust APIs for the
[agent runtime](https://docs.rs/goose-agent/latest/goose_agent/),
[providers](https://docs.rs/goose-providers/latest/goose_providers/), and
[context management](https://docs.rs/goose-context-management/latest/goose_context_management/).
Explore the shared provider bindings through the
[Python API](/docs/oi-kit/sdk/api-reference?language=python) and
[Kotlin API](/docs/oi-kit/sdk/api-reference?language=kotlin), with runnable examples in
[Python](https://github.com/aaif-goose/goose/blob/main/crates/goose-sdk/examples/uniffi/provider.py)
and [Kotlin](https://github.com/aaif-goose/goose/blob/main/crates/goose-sdk/examples/uniffi/kotlin/src/main/kotlin/Main.kt).
For apps using goose as a backend, consult the [ACP reference](/docs/oi-kit/acp/reference).
Use the reference when you need exact types, methods, and configuration details
while building.

## oi-kit and goose {#gdk-and-goose}

[goose](/docs/quickstart) is a complete agent application and the main reference
implementation for oi-kit. Use goose when you want a ready-to-run agent. Use oi-kit
when you want to build agent behavior into your own product.

The pre-built goose binary can also serve as the agent behind any
[ACP application](/docs/oi-kit/acp) you build. goose Desktop works this way,
connecting to a local goose server over ACP.
