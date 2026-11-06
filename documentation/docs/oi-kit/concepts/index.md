---
title: Concepts
description: How oi-kit's state machine, operations, providers, and authentication fit together.
displayed_sidebar: gdkSidebar
---

# Concepts

oi-kit gives you components for building an agent, rather than one fixed agent
with a set of configuration options. Your application decides what work is
allowed, how state is stored, which models to use, and how the agent interacts
with the outside world.

The execution model is a state machine composed from operations. An operation
can run a tool, add context, enforce a limit, or pause for input. Inference is a
specialized step that reaches a model through a provider. The machine applies
the resulting effects through your runtime, then loads the updated session
before deciding what to do next.

## Start here

- [State machine](/docs/oi-kit/concepts/state-machine): how the execution cycle
  works, what yielding means, and how persistence makes resuming possible.
- [Operations](/docs/oi-kit/concepts/operations): the units you compose into that
  machine, a worked example, the operations available today, and when to write
  your own.
- [Providers](/docs/oi-kit/concepts/providers): the model interface, built-in and
  declarative implementations, and the difference between local inference and
  talking to a local model server.
- [Authentication](/docs/oi-kit/concepts/authentication): how a provider gets its
  credentials, which mechanisms ship with oi-kit, and how to supply your own.

Read [Hello World](/docs/oi-kit/hello-world) for a concrete assembly of these
pieces. These pages explain the design; the [reference](/docs/oi-kit/reference)
links to the full APIs. Examples here follow the oi-kit APIs in this checkout
(`0.1.0-alpha.13`); Hello World follows the example's pinned alpha.12 release.

If you want an existing agent behind your own interface instead, you can
[build an app with goose over ACP](/docs/oi-kit/acp) without assembling the runtime.
