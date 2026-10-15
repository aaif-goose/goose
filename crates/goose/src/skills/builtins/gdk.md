---
name: gdk
description: Build programs on the goose Development Kit (GDK). Use when writing Rust, Python, or Kotlin code that calls models through goose providers (streaming, completion, compaction, tool calls), or when embedding the whole goose agent in another program over the Agent Client Protocol (ACP). Covers what the GDK is, when to use it, how to install it, and where to find the current API docs.
---

The GDK is goose's library surface: the parts of goose (model providers,
compaction, the agent loop, local inference) published as Rust crates, plus
Python and Kotlin bindings generated from the `goose-sdk` crate with UniFFI.
Every language shares one version number.

**The GDK is alpha (0.x).** The API may change between releases. Pin an exact
version, and look up names and signatures in the
[current API docs](#find-the-current-api-contract) instead of relying on memory.

## Choose the integration first

This is the decision that determines everything else. Get it right before writing code.

| You want | Use | What you get |
| --- | --- | --- |
| To call models and compose *parts* of goose into your own program | **GDK SDK API** (in-process) | Providers, streaming, completion, compaction. You own the loop, tools, prompts, and state. |
| The *whole* goose agent as a feature of your program | **ACP server** (`goose acp` over stdio) | Sessions, tool execution, extensions/MCP, permissions, context management, persistence. goose owns the loop. |

Ask: **who runs the agent loop?**

- *The program being written* runs the loop → provider API. You decide what a turn is, when to call
  tools, what goes in context. goose is embedded in the containing program.
- *goose* runs the loop → ACP. The program sends prompts and renders updates. The program
  wants to be a client of everything goose has to offer

The two can be combined: an ACP client for agent work, plus the provider API for
side tasks like classification or summarization that should not touch the session.

## Install

### Rust

Rust programs depend on the individual GDK crates directly. Add only the ones you need:

| Install | Use it to |
| --- | --- |
| `cargo add goose-providers --features rustls-tls` | Call models: ready-made providers for Anthropic, OpenAI, Google, Databricks, Ollama, and others, plus dozens of OpenAI-compatible services defined in JSON. Each supports streaming and one-shot completion. It also re-exports everything in `goose-provider-types` |
| `cargo add goose-provider-types` | Use the provider contract and conversation types (messages, tool calls, model config, usage, errors) without any HTTP stack, to implement your own provider or write code that works with any provider. |
| `cargo add goose-context-management` | Keep long conversations within a model's context window by summarizing the history into a single message. It works with any provider and can write the summary back into your own conversation type. |
| `cargo add goose-agent` | Build your own agent loop as an ordered set of steps (your logic, model inference, and your MCP tools) that a state machine runs. All state is rebuilt from the stored conversation, and you supply persistence. Skip it if goose's own loop over ACP fits. |
| `cargo add goose-local-inference --features hf-hub` | Run models on-device with no API key: GGUF through llama.cpp (with `cuda`/`vulkan` features), or MLX on Apple silicon (`mlx`), with tool-call emulation for models that lack native tools. Usually used as an ordinary provider via `goose-providers --features local-inference`; `hf-hub` adds Hugging Face model downloads. |
| `cargo add goose-sdk-types` | Call goose's custom `_goose/*` ACP methods (tools, extensions, config, session management) from a client of `goose acp` with typed requests, responses, and notifications, instead of hand-writing JSON-RPC. Use it alongside the `agent-client-protocol` crate. |
| `cargo add goose-download-manager` | Download large model files with resume, multi-file shards, auth tokens, cancellation, and progress tracking. `goose-local-inference` with `hf-hub` already includes it; add it directly only for standalone downloads. |

`goose-providers` needs **exactly one** TLS feature (`rustls-tls` or
`native-tls`); its HTTP client is built without TLS by default. Enabling `local-inference`
(or `cuda`, `vulkan`, `mlx`) on it adds `goose-local-inference` as an ordinary
`Provider`. All GDK crates share one version number. Keep them pinned to the
same version.

`goose-sdk` is the crate the Python and Kotlin bindings are built from. Rust
programs should use the crates above instead.

### Python

```bash
pip install goose-sdk    # installs as goose-sdk, imports as goose
```

Requires Python 3.9+. Wheels bundle the native library — nothing to build.

```python
import goose
```

### Kotlin / JVM

```kotlin
dependencies {
    implementation("io.github.aaif-goose:gdk:<version>")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
}
```

Artifact version matches the Rust crate version. Classes live in
`io.github.aaif_goose`. The jar bundles native libraries for macOS (arm64,
x86-64), Linux (arm64, x86-64), and Windows (x86-64).

On **JDK 24+ add `--enable-native-access=ALL-UNNAMED`** — the GDK loads its native
library through JNA and will warn or fail without it.

## Find the current API contract

The GDK is alpha, so function names, signatures, and types change between
releases. This skill doesn't list them. Look them up for the version you pinned
before writing code. These sources work whether or not the goose source is
checked out:

| What | Where |
| --- | --- |
| Python / Kotlin / Rust SDK (`goose-sdk`), versioned | <https://goose-docs.ai/docs/gdk/sdk/api-reference/> |
| Source of truth for the cross-language SDK surface | <https://github.com/aaif-goose/goose/blob/main/crates/goose-sdk/src/bindings.rs> (raw: <https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose-sdk/src/bindings.rs>) |
| Individual Rust crates | `https://docs.rs/<crate>` (for example <https://docs.rs/goose-providers>), source under <https://github.com/aaif-goose/goose/tree/main/crates> |
| Typed `_goose/*` ACP methods | <https://docs.rs/goose-sdk-types> |
| Agent Client Protocol | <https://agentclientprotocol.com> |
| Runnable examples (Rust ACP client, Python, Kotlin, declarative JSON) | <https://github.com/aaif-goose/goose/tree/main/crates/goose-sdk/examples> |

`main` can be ahead of the release you installed. To match it, replace `main`
with the release tag in GitHub URLs, or open the matching version on docs.rs or
in the docs-site version selector. If the goose repo is checked out, the same
files are at the same paths. Read the examples before writing new code. They
show the current call shapes, error handling, and ACP handshake.

## Rules that don't depend on the API

**Pin an exact version.** Not a range. The Rust crates, PyPI package, and Maven
artifact share one version number, so keep them identical across a project that
uses more than one language.

**Don't hard-code model names, context windows, or secrets.** Ask the provider
for them, and read API keys from the environment.

**Handle mid-stream errors, not just errors from the call that starts the
stream.** Streams can fail after they start, and that path is the one that gets
skipped.

**Never auto-approve ACP permission requests outside examples and tests.** They
are the security boundary between the agent and the user's machine. Show them to
a person or apply an explicit policy.

**Prefer ACP for anything agent-shaped.** Tool execution, permissions, MCP
extensions, session persistence, and context management are already solved
there. Building them on the provider API means reimplementing goose, badly.

**If the user has no provider configured,** the fix is `goose configure`, not a
code change.
