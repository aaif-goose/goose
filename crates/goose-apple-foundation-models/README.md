# Apple Foundation Models prototype

A goose provider backed by a small Rust binding crate and a statically
linked Swift C-ABI bridge. Inference stays in process and on device. There is no
Python dependency, helper process, `fm` invocation, server, or PCC support.
`swiftc` and `xcrun` are build tools only; users of the compiled binary do not
need Xcode.

## Build and use

The normal CLI and desktop backend builds include this provider by default.
Build for Apple silicon with Xcode 27 selected. This is a build SDK requirement,
not a new minimum runtime OS: the Swift bridge follows Rust's deployment target,
and Foundation Models and the newer Swift runtime are weak-linked. Intel and
non-macOS builds use a stub and do not require Swift or Xcode 27.

The shared CLI/desktop registry only registers the provider on macOS 27+ on
Apple silicon, so older systems cannot select or instantiate it, including from
a saved configuration. Registration checks OS support without loading the model.
Inference additionally requires Apple Intelligence enabled and the model ready.

From the repository root:

```sh
source bin/activate-hermit
export CARGO_BUILD_BUILD_DIR="$HOME/Library/Caches/cargo-build/goose"
cargo build -p goose-cli --bin goose
./target/debug/goose run --provider apple-foundation-models --model system \
  --no-profile --text "Write a short haiku about the sea."
```

For a minimal build, use `--no-default-features --features
apple-foundation-models,rustls-tls`. Other goose consumers can enable `goose/apple-foundation-models` or
`goose-providers/apple-foundation-models`. The binding crate's `native` feature
is off by default for users of the standalone binding crate.
Non-macOS builds have an unavailable stub and do not register the provider.

For the development desktop, point it at the debug binary:

```sh
GOOSE_BINARY="$PWD/target/debug/goose" just run-ui-only
```

`just run-ui` also includes the provider when rebuilding the release backend.
Installed desktop apps use their bundled backend; rebuilding the CLI does not
update those apps.

## Tool-loop contract

Each provider request reconstructs the transcript from goose's visible history.
It preserves user/assistant roles, tool-call IDs, parallel batches, and tool
results (including errors and structured MCP content). The native dynamic
profile replaces conversation history with those entries before generation,
preserving the framework's instructions entry (including tool definitions) and
avoiding an artificial user prompt after a tool result.

Apple tools contain only schemas and a callback that throws a private handoff
error. The profile also throws that error from `onToolCall`, with
`preserveTranscript` enabled. The bridge extracts the resulting tool-call batch
and returns it as goose `ToolRequest` messages. It cannot execute tools or
provide fabricated results. Goose performs approvals and execution, then calls
the provider again with results. Both agent loops share this provider; neither
loop is changed.

Tools whose schemas do not fit Apple's guided-generation subset use a fixed
`arguments_json` string envelope. The original JSON Schema is included in the
parameter description. Rust decodes the string and validates the original schema
before returning ordinary tool arguments to goose, so approvals and execution
see the original objects, including arbitrary dictionary keys. History replay
restores the same envelope for the model. External schema references are not
fetched; local `$defs`/`$ref` references are supported by validation.

A Swift task owns each native generation. Rust awaits a oneshot callback;
dropping the future cancels the Swift task. Request data is copied across the
ABI, and the terminal callback owns its Rust sender even after cancellation.
No native session persists between requests.

## Prototype limits

- Responses are buffered per model turn, including complete tool-call batches.
- Direct image and document inputs are rejected; this prototype is text-only.
- Schemas in Apple's supported subset use native guided generation with ordered
  properties and nullable unions. Open dictionaries and unsupported constraints
  (such as string length) use the validated JSON-text fallback. Invalid JSON or
  schema violations fail before a tool request reaches goose. Generating valid
  arguments through this fallback is less constrained than native generation.
  `format` annotations (such as `uint64`, `uri`, and `date-time`) become description
  hints; they do not add format validation to Apple's guided generation.
- Only the on-device `system` model is selectable. Context capacity comes from
  the native model, and native overflow/refusal errors preserve their categories.
- Large goose prompts/tool sets may exceed the small on-device context window.
  Start with a small tool set. No model-quality claim is made by build tests.

## Verification

```sh
cargo test -p goose-apple-foundation-models --features native
cargo test -p goose-providers --features apple-foundation-models --lib apple_foundation_models
cargo run -p goose-apple-foundation-models --features native --example availability
```

With `native` enabled on macOS 27+, schema tests also exercise Apple's actual
schema decoder without invoking the model. The compatibility test inspects its linked Mach-O
load commands to ensure the deployment target stays unchanged and both
Foundation Models and Swift concurrency remain weak-linked. The shared registry
regression covers CLI metadata, desktop setup catalog, and explicit lookup:

```sh
cargo test -p goose --features apple-foundation-models --lib \
  test_apple_foundation_models_selection_requires_supported_os
```

The availability example exits 1 with an explanatory message on macOS 26.
The Swift contract test uses a scripted model, no inference or Apple account:

```sh
xcrun swiftc -parse-as-library -swift-version 6 \
  -module-cache-path /tmp/goose-swift-module-cache \
  crates/goose-apple-foundation-models/swift/Bridge.swift \
  crates/goose-apple-foundation-models/tests/BridgeTests.swift \
  -o /tmp/goose-afm-bridge-tests
/tmp/goose-afm-bridge-tests
```

It verifies that system instructions and tool definitions reach the model on
every turn, a complete parallel tool batch, exactly one generation, exact prompt
history, and continuation after host-supplied tool results. On macOS 26 it
compiles but reports SKIPPED. It must pass on macOS 27 before treating the native
handoff as runtime-verified.

On macOS 27, run the `apple-foundation-models` phase of `goose-self-test.yaml`
with both values of `GOOSE_STATE_MACHINE`. Include real tool approval, denial,
cancellation, and multi-turn checks. These inference checks cannot run on macOS 26.

## Binding choice

Existing bindings examined: [fm-rs](https://github.com/blacktop/fm-rs) and
[foundation-models](https://docs.rs/foundation-models). Their session APIs own
tool execution; this prototype exposes a narrower, host-owned one-turn API
using macOS 27 profile hooks. The Swift concurrency SDK-stub linkage workaround
follows the approach in fm-rs, allowing downstream Rust binaries to link the
system runtime without custom rpath flags. The SDK itself is never modified.

Apple references: [tool calling](https://developer.apple.com/documentation/foundationmodels/expanding-generation-with-tool-calling),
[dynamic sessions](https://developer.apple.com/documentation/foundationmodels/composing-dynamic-sessions-with-instructions-and-profiles).
