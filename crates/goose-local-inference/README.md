# goose-local-inference

On-device model inference for goose. Runs GGUF models through `llama.cpp` (via
`llama-cpp-2`), with an optional eredu backend on Apple silicon for SafeTensors and GGUF.

Reach it through [`goose-providers`](../goose-providers) with the
`local-inference` feature, which exposes `LocalInferenceProvider` as an ordinary
`Provider`.

## Features

Default is `[]` — CPU inference.

- `hf-hub` — Hugging Face model discovery, downloads, cache inventory, and
  management APIs. Without it, models can still be loaded directly from paths.
- `cuda`, `vulkan` — GPU acceleration via the corresponding `llama-cpp-2` backend.
- `mlx` — eredu with its MLX/Metal execution backend on Apple silicon.
  The feature name is retained for compatibility.

## What it handles

- **Runtime and placement** — `InferenceRuntime` describes the machine and
  `available_inference_memory_bytes` helps choose a cached model that will fit.
- **Model lifecycle** — `is_model_loaded`, `loaded_model_ids`, and `evict_model`
  manage what's resident. With `hf-hub` enabled, `hf_models` uses the Hugging
  Face cache as the model inventory, `management` exposes it to clients, and
  `huggingface_auth` handles gated repos.
- **Prompt formatting** — `prompt_template` applies the model's chat template;
  `builtin_chat_template_names()` lists the bundled ones.
- **Tool calling** — eredu's semantic events and llama.cpp's `tool_parsing`
  extract tool calls from model output, and `tool_emulation` (toolshim) fills in
  for models with no native tool support.
- **Richer outputs** — `thinking_output` separates reasoning blocks from the
  answer; `multimodal` handles image input.
- **Config** — `config_resolver` and `provider_utils` resolve settings such as
  `LOCAL_LLM_MODEL`.

## Loading models from a path

Set the model name to a local path to bypass the Hugging Face cache. A `.gguf`
file defaults to llama.cpp. A SafeTensors model directory containing
`config.json`, a supported tokenizer (`tokenizer.json`, `tokenizer.model`, or
`spiece.model`), and weights defaults to eredu; a path to one of its
`.safetensors` files is accepted as well. Relative paths
are resolved from the process working directory.

Models loaded this way remain user-owned: Goose can load and evict them from
memory, but does not include them in the cached-model inventory or delete their
files.

## Selecting an inference backend

The desktop model settings expose **Inference backend**. Automatic selects
llama.cpp for GGUF and eredu for SafeTensors. Choose eredu explicitly to run a
GGUF with MLX. This reuses the cached checkpoint and model ID; switching engines
does not download another copy. Available choices reflect the platform and build;
eredu validates the particular checkpoint when loading it.

Programmatic callers can set `ModelSettings.backend_id` to `eredu` or `llamacpp`,
or leave it unset for automatic selection. Existing `mlx` settings are accepted
as an alias for eredu. Goose persists these settings under
`GOOSE_LOCAL_MODEL_SETTINGS`, keyed by model ID. llama.cpp cannot load SafeTensors.
An explicitly selected backend never silently falls back to another engine.

Download requests may supply `format` (`gguf` or `safetensors`) independently of
`backendId`. Old `mlx` requests and `mlx-safetensors` format values remain accepted.

Eredu uses its prepared-chat pipeline for native tools, reasoning, sampling,
cancellation, and external drafting (including supported Gemma assistants). Auto tool mode uses Goose's tool
emulation when the checkpoint/template cannot support native tools. Force-native
mode reports the incompatibility. Tool calls are emitted only after completion;
partial argument streams never become executable Goose tool requests.

### Eredu compatibility

- The dependency is pinned to git main revision
  `464d40c4b1d38ed6f449c53cca46a3d80cc4cf9f` across the eredu crates.
- Checkpoint architecture, tensor encoding, tokenizer, and processor must be
  supported by eredu; GGUF compatibility with llama.cpp does not imply eredu
  compatibility.
- Embedded and custom Jinja templates are supported. The named built-in templates
  exposed by llama.cpp are specific to that engine; use embedded metadata or the
  full Jinja source with eredu.
- GGUF projectors must be discoverable by eredu beside the checkpoint (or in its
  supported parent search locations). Ambiguous companions or a selection that
  differs from Goose's projector are rejected. Select llama.cpp for layouts
  requiring an explicit projector override.
- Image attachments are decoded locally. Prompt usage and context budgeting use
  eredu's count of actual model positions in the prepared input, including media;
  `image_token_estimate` is not used by this backend.
- Draft models must be supported eredu target–assistant pairings. Changing the
  draft model reloads the eredu session. Mirostat works with speculation, but
  optimistic lookahead is disabled in this adapter. Goose currently selects
  external drafting only; it has no setting for eredu's embedded prediction heads.
- The Apple backend targets macOS 14+ and requires Xcode 26.2+ for its native
  build. Other platforms retain llama.cpp; eredu is not enabled there by this
  integration.

### Memory planning and remaining gaps

Goose includes `memory_forecast` in its existing eredu inference response log.
The forecast borrows the exact prepared request before generation, using eredu's
ordinary or speculative API as appropriate. It retains the full estimate,
assumptions, execution contract, and speculative plan when available. Forecast
errors are recorded as `error` and do not reject generation. An already-cancelled
request has no forecast. These are loaded-model estimates: they exclude loading
peaks and are recorded with the response, not exposed as a preflight UI check.

Eredu now automatically caps an untouched native MLX allocator-cache default at
256 MiB when realizing a model, preserving smaller defaults and explicit native
settings. Goose inherits that process-global policy. This limits retained reusable
allocator blocks, not active tensors, KV state, or total process memory; it can
trade some throughput for lower retention. Goose does not expose a cache-policy
override or change the limit on each request.

Forecasts are descriptive estimates, not allocation limits or reservations.
Goose does not yet use them for admission, model recommendations, or automatic
prefill tuning. Cached-model recommendations still compare checkpoint file size
with available memory, without request-specific KV state, workspace, or loading
peaks. Forecasts use eredu's default calibration and observed availability, with
no application budget or extra reserve configured by Goose.

The existing forecast calls pick up eredu's execution-derived workspace accounting,
including attention tile retention, resident parameter conversions, dense LFM2
hybrid workspaces, and supported routed mechanisms. Forecast fields are serialized
in full, preserving new coverage and missing-mechanism details. These improvements
do not imply complete coverage for every checkpoint in a model family. Reducing
a requested chunk size cannot reduce memory for a path requiring full-pass
prefill, such as the current LFM2 path.

Upstream bounds independent autoregressive drafting when both loaded models have
workspace coverage. It also supports embedded startup and settled continuation
forecasts for covered mechanisms, including an all-attention dense Qwen3.5 text
configuration. Released Qwen hybrid schedules still have gaps for gated-delta
recurrence and some expert reductions. Feature-conditioned external assistants
(including Gemma assistants), prepared media, transfers, and other uncovered
mechanisms can still have unknown upper bounds. Goose cannot currently select
the embedded mode, so that new upstream coverage is not exposed here.

Eredu supports forecasts at settled ordinary, independent speculative, and
covered embedded speculative boundaries. Goose's callback-based generation does
not expose controlled sessions or sample continuation forecasts. It resets
request state and replays the conversation each turn, so KV prefix reuse remains
unimplemented. Arbitrary in-flight outlooks and feature-conditioned external
assistant continuations remain upstream gaps.

Optional retained F32 weight conversions now have their own managed 256 MiB
budget, separate from the allocator cache. Goose leaves the execution-plan
override unset and inherits that finite default. The bound covers retained
payload plus outstanding reservations across the target's eligible owners;
embedded prediction shares it, while each separately loaded external drafter has
its own budget. Unsupported retention paths report an effective disabled policy.
Unlimited retention requires an explicit upstream override that Goose does not
request. This resolves the earlier unbounded-retention gap.

Eligible mixed-dtype Metal decode projections now read the original weights
without full-weight F32 casts. Other paths, including multi-row prefill, can still
need temporary casts. Neither retention nor allocator-cache policy limits those
temporaries, native backing capacity, KV state, or total process memory. Request
reset preserves admitted conversions; dropping their owner releases its claims.
Eredu also exposes settled trimming, but Goose does not currently expose trimming
or custom conversion-retention budgets.

Goose's existing loaded forecasts include the new retention policy, scope, usage,
and reservation accounting without adding retained payload to resident parameters
twice or clamping temporary workspace to the retention budget. These observations
precede generation, so use a subsequent request's forecast to inspect warmed
residency; Goose does not yet expose a separate post-generation retention report.

See the pinned [upstream memory documentation](https://github.com/jbg/eredu/blob/464d40c4b1d38ed6f449c53cca46a3d80cc4cf9f/doc/generation-memory.md)
for coverage, calibration assumptions, and validation scope.

## Building

The `llama.cpp` backends compile native code, so a C/C++ toolchain is required,
plus the CUDA or Vulkan SDK when selecting those features.

```bash
cargo build -p goose-local-inference
cargo build -p goose-local-inference --features hf-hub
cargo build -p goose-local-inference --features mlx
```
