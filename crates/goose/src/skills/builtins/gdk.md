---
name: gdk
description: Build programs on the goose Development Kit (GDK). Use when writing Rust, Python, or Kotlin code that calls models through goose providers (streaming, completion, compaction, tool calls), or when embedding the whole goose agent in another program over the Agent Client Protocol (ACP). Covers install, the provider API surface, the ACP server, and the failure modes that matter.
---

The GDK is goose's library surface. One Rust crate, `goose-sdk`, is the source of
every binding; Python and Kotlin are generated from it with UniFFI, so all three
languages share the same types, behavior, and version number.

**The GDK is alpha (0.x).** The surface may change between releases. Pin an exact
version and re-check the API reference when upgrading.

## Choose the integration first

This is the decision that determines everything else. Get it right before writing code.

| You want | Use | What you get |
| --- | --- | --- |
| To call models and compose *parts* of goose into your own program | **GDK provider API** (in-process) | Providers, streaming, completion, compaction. You own the loop, tools, prompts, and state. |
| The *whole* goose agent as a feature of your program | **ACP server** (`goose acp` over stdio) | Sessions, tool execution, extensions/MCP, permissions, context management, persistence. goose owns the loop. |

Ask: **who runs the agent loop?**

- *The program being written* runs the loop → provider API. You decide what a turn is, when to call
  tools, what goes in context. goose is embedded in the containing program.
- *goose* runs the loop → ACP. The program sends prompts and renders updates. The program
  wants to be a client of everything goose has to offer

Do not use the provider API to rebuild an agent that goose already is. If you find
yourself writing a tool-dispatch loop, permission prompts, session persistence, or
extension loading on top of the provider API, you want ACP instead.

Do not use ACP when you only need one module. Spawning a subprocess or network connection and
get a single completion for example is the wrong tool.

The two can be combined: an ACP client for agent work, plus the provider API for
side tasks like classification or summarization that should not touch the session.

## Install

### Rust

```bash
cargo add goose-sdk --features uniffi
```

The feature flag is key. With **default features** the crate only
re-exports the ACP wire types (`goose_sdk::custom_requests`,
`goose_sdk::custom_notifications`) for building an ACP client. The in-process
provider API lives behind `--features uniffi` as `goose_sdk::bindings`. Omit the
feature and `goose_sdk::bindings` will not exist.

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

## Provider API

### Build a provider

| Function | Arguments |
| --- | --- |
| `openai_provider` | `api_key` |
| `anthropic_provider` | `api_key`, `base_url` (optional), `beta_headers` |
| `groq_provider` | `api_key` |
| `databricks_provider` | `host`, `token` |
| `databricks_v2_provider` | `host`, `token` |
| `declarative_provider_from_json` | `json` |

Each has a matching `*_default_model()` returning a model name string, so you
never hard-code one.

Any provider speaking an OpenAI- or Anthropic-compatible API can be defined in
JSON instead of new Rust code. `${ENV_VAR}` placeholders in that JSON are resolved
**when the provider is constructed**, not at call time — so the variable must be
set before construction.

```python
provider = goose.declarative_provider_from_json(open("deepseek.json").read())
```

### Provider methods

| Method | Purpose |
| --- | --- |
| `name()` | Provider name |
| `supported_features()` | `Vec<Feature>`: `Tools`, `Streaming`, `Images`, `JsonSchema`, `Reasoning` |
| `context_limit(model)` | Context window size for a model (async) |
| `stream(model, system, messages, tools)` | Returns a `ProviderStream` (async) |
| `complete(model, system, messages, tools)` | One-shot `ProviderCompletion` (async) |
| `compact(model_name, messages, templates)` | Summarize a conversation past the context window (async) |

Check `supported_features()` rather than assuming. Sending tools to a provider
without `Tools`, or images without `Images`, is a runtime failure you can predict.

### Streaming

`stream()` returns a `ProviderStream`; call `next_chunk()` until it returns
`None`/null.

| Chunk | Meaning |
| --- | --- |
| `TextChunk` | Assistant text |
| `ToolChunk` | Tool call request — `id`, `name`, `arguments_json` |
| `ThinkingChunk` / `RedactedThinkingChunk` | Reasoning output |
| `EndChunk` | Stream finished, carries final token `Usage` |
| `ErrorChunk` | **Mid-stream failure**, carries a `GooseStreamError` |

`StreamChunk` is a non-exhaustive-in-practice enum that will grow. Match the
variants you handle and ignore the rest — do not write a `match`/`when` that
assumes today's variant list is final.

### Messages and tools

`ProviderMessage { role, content }` where `role` is `User`, `Assistant`, or
`Tool`, and `content` is a list of `MessageContent`:

`Text { text }`, `Image { mime_type, data }`,
`ToolRequest { id, name, arguments_json }`,
`ToolResult { id, success, content_json }`,
`Thinking { thinking, signature }`, `RedactedThinking { data }`.

`ProviderTool { name, description, input_schema_json, annotations_json? }`.

Note the `_json` suffixes: tool schemas, tool arguments, and tool results cross
the FFI boundary as **JSON strings**, not structured objects. Serialize and parse
them yourself; malformed JSON surfaces as `GooseError::Generic`.

To carry out a tool call: read a `ToolChunk`, parse `arguments_json`, run the
tool, then append an `Assistant` message containing the `ToolRequest` followed by
a `Tool` message containing a `ToolResult` with **the same `id`**. Mismatched or
missing ids are the most common cause of a provider rejecting the next turn.

### Model config

`ProviderModelConfig` requires only `model_name`. Useful optional fields:
`context_limit`, `temperature`, `max_tokens`, `reasoning`, `timeout_ms`,
`toolshim` / `toolshim_model`, `request_params_json`, `provider_params_json`, and
`request_headers` (per-request HTTP headers that override static provider headers).

### Compaction

`compact(model_name, messages, templates)` summarizes a conversation into a single
message so it can continue past the model's context window. Input is
`CompactionMessage { role, text }` — text only, because compaction reads
conversations as text. Pass `None` for `templates` to use
`default_compaction_templates()`, or override the `compaction` and `summary`
prompts.

Trigger it off `context_limit()` and the `Usage` on `EndChunk` rather than waiting
for `ContextLengthExceeded`.

### Request logging

`install_request_logger(logger)` captures provider requests as JSONL. It is
**process-wide and can only be installed once for the lifetime of the process** —
call it during startup, never per-request or per-test. The `RequestLogger`
interface is `start() -> u64` (an id passed to every `write` for that request, so
concurrent requests stay separate) and `write(request_id, record)`.

### Errors

Errors raised **before** the stream starts are thrown as `GooseError`
(`GooseException` in Kotlin). Errors **mid-stream** arrive as an `ErrorChunk`
carrying a `GooseStreamError { kind, message, retry_after_ms }`. You must handle
both paths — a `try`/`catch` around `stream()` alone will miss mid-stream failures.

Both share the same kinds: `RateLimited`, `OutputTokenLimitExceeded`,
`ContextLengthExceeded`, `Authentication`, `Timeout`, `ProviderUnavailable`,
`Generic`.

Respond by kind, not by string matching:

- `RateLimited` → back off, honoring `retry_after_ms` when present
- `ContextLengthExceeded` → `compact()` and retry
- `OutputTokenLimitExceeded` → raise `max_tokens` or ask for less
- `Authentication` → fail fast, do not retry
- `Timeout` / `ProviderUnavailable` → retry with backoff
- `Generic` → surface the message

## ACP server

`goose acp` speaks the Agent Client Protocol over stdio. Your program spawns it as
a child process and becomes the ACP *client*.

The client owns the environment the agent acts in: it answers permission requests,
provides filesystem access, and renders session updates. goose owns the agent loop.

### Flow

1. Spawn `goose acp` with piped stdin/stdout (leave stderr inherited for logs).
2. Wrap the pipes in a byte-stream transport.
3. Send `InitializeRequest` with the protocol version and read back
   `agent_info` and `agent_capabilities`.
4. Create a session, send a prompt.
5. Handle `SessionNotification` updates as they stream in, and respond to
   `RequestPermissionRequest`.

The agent advertises: `load_session`, session `list`/`delete`/`close`, prompt
capabilities (`image: true`, `audio: false`, `embedded_context: true`), and MCP
over HTTP. Read these from `InitializeResponse` rather than assuming — they are
version-dependent.

Auth method is `goose-provider` ("Configure Provider"): if the user has no
provider set up, the fix is `goose configure`, not a code change.

### Session updates

`SessionNotification.update` is a `SessionUpdate` variant. The ones most clients
need: `AgentMessageChunk` (assistant text), `ToolCall` (a tool started, has a
human-readable `title`), and `ToolCallUpdate` (status changes). Ignore unhandled
variants — this list grows.

### Permissions

goose asks the client to approve tool calls via `RequestPermissionRequest`. Respond
with `RequestPermissionOutcome::Selected(option_id)` from `request.options`, or
`Cancelled`.

**Auto-approving everything is only acceptable in examples and tests.** In a real
program, surface the request to a human or apply an explicit policy. This is the
security boundary between the agent and the user's machine.

### goose-specific methods

Beyond standard ACP, goose exposes custom `_goose/*` JSON-RPC methods whose typed
wire structs live in `goose-sdk-types` and are re-exported from `goose_sdk`
(available with **default** features — no `uniffi` needed). They cover steering a
running session, listing tools, managing extensions, config and preferences,
prompt templates, session lifecycle (rename/archive/export/import/fork),
diagnostics, and apps.

Use the typed structs from `goose_sdk::custom_requests` instead of hand-rolling
JSON-RPC payloads; they are the single source of truth for these methods.

A complete, runnable Rust client is in the repo at
`crates/goose-sdk/examples/acp_client.rs`:

```bash
cargo run -p goose-sdk --example acp_client -- "What is 2 + 2?"
```

## Getting the best results

**Verify the surface before you write against it.** The GDK is alpha and this
skill can go stale. The generated reference at
<https://goose-docs.ai/docs/gdk/sdk/api-reference/> has a version selector; the
authoritative source is `crates/goose-sdk/src/bindings.rs`. If a name here does
not exist, trust the crate.

**Pin an exact version.** Not a range. `0.x` releases may break the surface, and
the Rust crate, PyPI package, and Maven artifact share one version number — keep
them identical across a polyglot project.

**Never hard-code model names.** Use `*_default_model()`. Never hard-code context
windows either — ask `context_limit()`.

**Keep secrets out of source.** Read API keys from the environment, or use a
declarative provider JSON with `${ENV_VAR}` placeholders.

**Handle both error paths from the first version you write.** Pre-stream
`GooseError` and mid-stream `ErrorChunk` are different code paths, and the
mid-stream one is the one that gets skipped and then bites under load.

**Consume the stream to completion.** Loop until `next_chunk()` returns
`None`/null. `EndChunk` carries the final `Usage` — that is where token counts and
cache-hit numbers come from. Dropping a stream early loses accounting.

**Track tokens and compact deliberately.** Compare `EndChunk` usage against
`context_limit()` and call `compact()` before you hit the wall.

**Match the language's idioms.**

- *Python*: everything on `Provider` is `async` — use `asyncio`, and
  `while chunk := await stream.next_chunk():`.
- *Kotlin*: prefer `provider.streamFlow(model, system, messages, tools)` over a
  manual `nextChunk()` loop; `tools` defaults to empty. Suspending functions map
  to coroutines and errors surface as `GooseException` subclasses. The
  `providers.openai.provider(...)` / `providers.openai.defaultModel()` helpers are
  thin wrappers over the generated `openaiProvider(...)` / `openaiDefaultModel()`.
- *Rust*: `ProviderModelConfig` implements `Default`, so
  `ProviderModelConfig { model_name: ..., ..Default::default() }` is the clean
  construction. Provider constructors return `Arc<Provider>`.

**Install the request logger once, at startup.** It is process-wide and a second
install fails.

**Prefer ACP for anything agent-shaped.** Tool execution, permissions, MCP
extensions, session persistence, and context management are solved there. Building
them on the provider API means reimplementing goose, badly.

## Reference

- GDK overview and quickstarts: <https://goose-docs.ai/docs/gdk>
- Generated API reference (Rust/Python/Kotlin, versioned):
  <https://goose-docs.ai/docs/gdk/sdk/api-reference/>
- Download this skill: <https://goose-docs.ai/files/skills/gdk.md>
- In-repo source of truth: `crates/goose-sdk/src/bindings.rs`
- Runnable examples: `crates/goose-sdk/examples/` (`acp_client.rs`,
  `uniffi/provider.py`, `uniffi/kotlin/`, `deepseek.json`)
