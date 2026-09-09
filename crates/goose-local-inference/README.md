# Local inference

The `local` provider runs models in process. Artifact format and inference backend
are independent: GGUF defaults to `llamacpp`, and SafeTensors defaults to `eredu`.
Eredu can also load GGUF and any other format supported by its artifact loader.
Nanbeige 4.2 loads with its embedded template and thinking controls. Its native
XML tool calls currently require Eredu's exact newline format; alternate whitespace
can leave a call displayed as text instead of executed.

On macOS, build with `cargo build -p goose-cli --features eredu` to enable Eredu
through MLX. The `eredu` feature is opt-in; `cuda` and `vulkan` enable llama.cpp
acceleration.

## Backend selection

Selection precedence is per-model override → `GOOSE_LOCAL_BACKEND` → format default.
Downloads select a format without fixing the backend.

```sh
goose local-models backend '<model-id-or-path>' eredu
goose local-models backend '<model-id-or-path>' llamacpp
goose local-models backend '<model-id-or-path>' auto
GOOSE_LOCAL_BACKEND=eredu goose run --provider local --model '<model-id-or-path>' --text 'Hello'
```

In Desktop, open **Settings → Local Inference → model settings → Inference
backend**. Choose **Default** to clear the model override. Both interfaces share
the same saved settings.

## Settings

Eredu resolves generation settings in this order: request overrides → per-model
overrides → checkpoint configuration → Eredu defaults. Empty fields inherit;
displayed effective values are not saved as overrides. **Reset to defaults**
clears model settings while retaining the backend selection.

Migration from the unreleased SafeMLX backend replaces each legacy `mlx` model's
settings with only `backend_id: eredu`, discarding all old overrides once.

Eredu handles checkpoint templates, sampling, device residency, and speculation.
Goose selects an available accelerator unless a device is specified. llama.cpp
controls such as GPU layers and batch size are rejected when using Eredu.
Tool mode can be automatic, native, or emulated; native support depends on the
model and its template.

Goose's general thinking preference uses controls accepted by the selected template.
Unsupported controls retain checkpoint behavior; explicit model controls and template
arguments take precedence and are validated by Eredu.
The output token limit includes reasoning; increase it if a reply ends before the answer.

## Implementation and checks

[`eredu_factory.rs`](src/eredu_factory.rs) supplies the concrete backend to
[`eredu_adapter`](src/eredu_adapter/). A dedicated worker thread owns planning,
loading, generation, and cleanup. Goose manages downloads, configuration, model
reuse, cancellation, and tool execution. Request logs include Eredu's execution
plan, usage, timing, and speculative statistics.

From the repository root:

```sh
source bin/activate-hermit
cargo test -p goose-local-inference --no-default-features --features hf-hub
cargo test -p goose-local-inference --no-default-features --features eredu,hf-hub
```

The first command runs portable tests; the second enables the native backend on
macOS. Checkpoint-dependent tests are ignored by default; see
[`eredu_native.rs`](tests/eredu_native.rs) for the required environment variables.
