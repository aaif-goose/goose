# goose-sdk

The Goose SDK exposes Goose's agent functionality outside of the main `goose` binary

## 1. ACP client/server (default)

With default features, this crate is a thin Rust library re-exporting the shared types so you can build an Agent Client Protocol client that talks to `goose acp` (or any ACP-compatible Goose server) over stdio.

See `examples/acp_client.rs`:

```bash
cargo run -p goose-sdk --example acp_client -- "What is 2 + 2?"
```

This path has no dependency on the `goose` core crate — it speaks to Goose as an external process via ACP + Goose's custom `_goose/*` JSON-RPC methods.

## 2. uniffi bindings (Python / Kotlin)

With `--features uniffi`, the crate compiles as a `cdylib`/`staticlib` that embeds the `goose` core in-process and exposes an `Agent` object to Python and Kotlin via [uniffi-rs](https://github.com/mozilla/uniffi-rs).

Build the library, generate bindings, and run the example pings:

```bash
just python   # generates Python bindings + runs examples/uniffi/ping_aaif.py
just kotlin   # generates Kotlin bindings + runs examples/uniffi/PingAaif.kt
```

Generated bindings land in `generated/`. The shared types from `goose-sdk-types` appear as native records in both languages.

## Packaging

Build a distributable artifact for the current platform. Both artifacts bundle the native `libgoose_sdk` for the host platform/arch.

### Python wheel

```bash
just python-wheel
pip install crates/goose-sdk/packaging/python/dist/goose_sdk-*.whl
```

```python
from goose_sdk import Agent, EventSink
from goose_sdk.goose_sdk_types import ProviderSpec, ExtensionSpec, AgentEvent

class Printer(EventSink):
    def on_event(self, event):
        if isinstance(event, AgentEvent.ASSISTANT_TEXT):
            print(event.text, end="", flush=True)
    def on_error(self, error): print("error:", error)
    def on_done(self): print()

agent = Agent()
agent.configure(
    ProviderSpec(name="openai", model="gpt-4o"),
    [ExtensionSpec.BUILTIN(name="developer")],
)
agent.reply("ping aaif.io", Printer())
```

### Kotlin/JVM JAR

```bash
just kotlin-jar
# → crates/goose-sdk/packaging/kotlin/dist/goose-sdk-0.1.0-<os>-<arch>.jar
```

Consumers must also have `net.java.dev.jna:jna:5.14.0` and `org.jetbrains.kotlin:kotlin-stdlib` on the classpath. Call `NativeLoader.ensureLoaded()` once before touching any uniffi-generated type — it extracts the bundled native library and points JNA at it.

```kotlin
import io.aaif.goose.sdk.{Agent, EventSink, NativeLoader}
import io.aaif.goose.sdk_types.{AgentEvent, ExtensionSpec, ProviderSpec}

fun main() {
    NativeLoader.ensureLoaded()
    val agent = Agent()
    agent.configure(
        ProviderSpec(name = "openai", model = "gpt-4o"),
        listOf(ExtensionSpec.Builtin(name = "developer")),
    )
    agent.reply("ping aaif.io", object : EventSink {
        override fun onEvent(event: AgentEvent) {
            if (event is AgentEvent.AssistantText) print(event.text)
        }
        override fun onError(error: String) = System.err.println("error: $error")
        override fun onDone() = println()
    })
}
```

Provider credentials for both are read from the same global Goose config (env vars, OS keyring, `~/.config/goose/config.yaml`) used by the `goose` CLI.

## When to use which

- **ACP** — use this when you need the full Goose feature surface (sessions, sources, providers, dictation, onboarding, etc.) or process isolation, and you're happy to spawn `goose acp` as a subprocess and speak JSON-RPC over stdio from any language.
- **uniffi** — use this when you want the Goose agent embedded directly inside a Python or Kotlin host process with native types, lower latency, and no subprocess, and the current minimal `Agent` surface (`configure` + `reply`) is enough for your use case.

## Shared types: `goose-sdk-types`

The `goose-sdk-types` crate holds the wire types used by both consumers above — request/response structs for Goose's custom JSON-RPC ACP methods (`AddExtensionRequest`, `GooseToolCallRequest`, provider/session/sources/dictation requests, etc.) and the streaming `AgentEvent`, `ExtensionSpec`, and `ProviderSpec` records.

Keeping these types in a small, dependency-light crate lets the ACP path serialize/deserialize them as JSON-RPC and the uniffi path expose them as native records in Python/Kotlin — from one source of truth.
