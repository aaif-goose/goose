# Local inference

Goose's local provider separates artifact format (`gguf`, `safetensors`) from
inference backend (`llamacpp`, `eredu`). GGUF defaults to llama.cpp; SafeTensors
defaults to Eredu. Eredu can also load GGUF, and artifact admission is delegated
to Eredu so future supported formats do not require a Goose backend allowlist.
The Cargo feature `mlx` enables Eredu's current MLX factory on macOS. llama.cpp's `cuda` and `vulkan`
features continue to select llama.cpp acceleration; they do not enable Eredu CUDA.

Backend selection uses the per-model `backend_id` override first, then
`GOOSE_LOCAL_BACKEND` from environment/config, then the format default. Invalid or
unsupported explicit selections produce an error without silently falling back.
Downloads select an artifact format and do not persist a backend override.

```sh
goose local-models backend '<model-id-or-path>' eredu
goose local-models backend '<model-id-or-path>' llamacpp
goose local-models backend '<model-id-or-path>' auto
GOOSE_LOCAL_BACKEND=eredu goose run --provider local --model '<model-id-or-path>' --text 'Hello'
```

In Desktop, open Settings → Local Inference, expand the model's settings,
and choose **Inference backend**. **Default** removes the model override. The
format is displayed separately. Both interfaces share the same saved settings.
On first local-model access, persisted `backend_id: mlx` values are migrated to
`eredu` once, discarding every other setting for those models. Lower-priority
read-only config layers are normalized in memory. The old `mlx` format label is
now `safetensors`.

All Eredu workspace dependencies are pinned to
`dc8c8d8c501f6f8e99cc602c8a9d994927b4cf51` from
<https://github.com/jbg/eredu>. Eredu's default features are disabled. Goose no
longer depends directly on SafeMLX, safemlx-lm, or safemlx-lm-utils. SafeMLX is an
implementation dependency of Eredu's optional MLX backend.

## Ownership and loading

`eredu_factory.rs` is the concrete backend selection boundary. It supplies the
Eredu factory to the generic `eredu_adapter` worker. A future backend implements
Eredu's loading, planning, text-generation, capability, and speculative contracts
and supplies its factory at this boundary. Provider messages, settings, events,
errors, and model management carry no native tensors, devices, or streams.

The worker creates the factory, plans, loads, generates, settles, and destroys the
complete `PlannedModel` on one dedicated thread. Its handle transfers owned
requests and Goose events. Goose's existing model slots deduplicate loads,
serialize requests and eviction, and preserve loaded weights between requests.
The cache key includes target and draft paths, effective template settings,
explicit device, and execution constraints. Eredu owns per-request native caches;
the worker calls generic reset before each request and synchronize afterward.
Failed settlement ends the worker, and provider failures evict the cached handle.
Consumer disconnection cancels generation even before the first visible event.
Request-scoped native call IDs prevent collisions across conversation turns.

Eredu owns artifact interpretation, tokenizer and EOS metadata, templates,
sampling, incremental decoding, semantic reasoning and tool protocols, generation,
residency, and speculative scheduling. Goose owns downloads, credentials, paths,
configuration persistence, message conversion, application scheduling, request
logs, tool execution, and the shell/TypeScript emulation protocol.

## Device and execution policy

At this revision `AutomaticPlanRequest` requires a device; it does not choose one.
Goose uses the factory's portable hardware discovery. An explicit `device` must
match an available device ID. Otherwise Goose selects the available accelerator
with the lowest index (device ID breaks ties), then the lowest-index CPU if no
accelerator is available. It does not retry a failed explicit selection on another
device. MLX IDs include `metal:0` and `cpu:0`.

For that device, Eredu's default `AutomaticPlanner` chooses fully resident,
layerwise-host, or disk-streamed execution using observed resources, its normal
memory headroom, and capability admission. Goose does not choose residency.
Explicit `max_cached_shards` and draft-model selection are applied through
`plan_retained_with_overrides` before candidate admission and probing. The
selected report and retained artifact inspection are used unchanged for loading.
External drafts use target placement; otherwise Eredu may select embedded drafting.
The full planned model retains drafting resources, limits, and lookahead policy.
At this revision, external assistant directories must also contain their matching
tokenizer metadata: the facade proves vocabulary compatibility before loading.

Request logs include the selected execution report, explanations, text inspection,
usage, finish reason, and stats. Up to 32 recent compatible realizations retain
portable execution telemetry for subsequent planning in the same process.

## Inherited settings

Eredu generation precedence is:

1. Explicit request overrides.
2. Explicit per-model settings.
3. Checkpoint generation configuration.
4. Eredu fallbacks.

A new settings object uses `sampling: {"type":"Inherit"}` and absent penalties,
limits, and thinking overrides. Temperature sampling fields are independently
optional. Empty desktop inputs inherit; effective values are displayed separately
and never copied into saved overrides. Selecting model defaults resets sampling;
clearing a field resets that override. The reset button clears per-model overrides and preserves the saved backend selection. The settings preview parses checkpoint metadata into Eredu's public
configuration type and delegates resolution to Eredu without loading weights.

Migration from the unreleased SafeMLX backend replaces each legacy `mlx` model's
settings with just `backend_id: eredu`. All old overrides are discarded, including
sampling, penalties, limits, templates, and execution settings, so Eredu starts
with inherited defaults. Newly saved overrides remain explicit. llama.cpp applies
its previous defaults to absent fields.

Greedy explicitly selects `do_sample=false`. Mirostat V2 uses Eredu's prepared
ordinary/speculative APIs, including penalties and seed. It requires a positive
effective temperature; if checkpoint resolution yields zero, set an explicit
positive temperature. Goose returns an actionable error instead of running greedy.
Eredu resolves `do_sample` and temperature interactions. Generic ACP/CLI
`thinking_effort=off` maps to an explicit thinking-disable request; other effort
values and the `reasoning_effort` parameter go to Eredu for capability validation.
Explicit request context limits take precedence over per-model limits and are
bounded by the loaded model’s actual capacity.

Unspecified output limits remain unresolved until Eredu applies checkpoint or
prepared-chat defaults (256 tokens when the checkpoint supplies no limit).
Exact prepared-input counts and loaded-model capabilities bound that allowance to
actual context headroom. Admission errors use Goose's context-length error path.

llama.cpp batch size, GPU layers, CPU threads, mlock, and flash-attention controls
are scoped to llama.cpp. Explicit values on an Eredu model produce an error rather
than claiming to apply them. Migration clears these fields from legacy MLX settings.

Embedded and named checkpoint templates are interpreted by Eredu. Custom-inline
settings supply Jinja source through the same `TextModelOptions` used by inspection
and loading. Goose currently supplies `chatml` as an explicit Eredu builtin;
llama.cpp retains its own builtin list. Other builtin names require their Jinja
source. Missing checkpoint templates require an explicit template; there is no
model-name-based prompt fallback. Templates without a recognized semantic protocol
use Eredu's `generate_prepared_text` and `generate_prepared_text_speculative` APIs.
This preserves Eredu tokenization, sampling, stops, cancellation and exact counts. It does not
classify reasoning or parse native tools; Auto prepares Goose emulation first,
and explicit unsupported thinking is rejected. These APIs report committed-token
TTFT and support the selected speculative plan with the same terminal usage and
draft statistics as recognized templates.

## Tools, streaming, and usage

Auto chooses native tools only when the actual prepared conversation supports
them; otherwise it prepares Goose emulation before generation. ForceNative rejects
unsupported templates; ForceEmulated selects the existing application protocol.
Schema keywords are preserved, including numeric bounds, unions, nullability, and
nested objects. Invalid schema or runtime failures do not trigger a second run.

Native argument fragments remain provisional. Only Eredu's validated ToolCallEnd
produces an executable Goose tool request. Reasoning and visible text use Eredu's
semantic events; Goose's old native parser and extra thinking parser are absent
from this path. The thinking and emulation helpers needed by llama.cpp remain. Emulated history
uses the existing `$ command` / `Command output:` application transformation.

Ordinary and speculative output use the same terminal accounting: generated tokens
include all committed tokens, including special/terminal tokens. TTFT comes from
Eredu's first committed token even if it is hidden or structural. Model load time
and application elapsed time remain separate. Speculative statistics count target
work, proposed tokens, accepted tokens, rounds, and acceptance rate.

## Validation

From the repository root, activate Hermit first:

```sh
source bin/activate-hermit
cargo fmt --check
cargo test -p goose-local-inference --no-default-features --features hf-hub
cargo clippy -p goose-local-inference --all-targets --no-default-features --features hf-hub -- -D warnings
cargo test -p goose-local-inference --no-default-features --features mlx,hf-hub
cargo clippy -p goose-local-inference --all-targets --no-default-features --features mlx,hf-hub -- -D warnings
```

The portable mock constructs thread-bound backend state inside the actual adapter
worker and exercises planning, loading, semantic generation, admission, reset,
settlement, and destruction without MLX. These are behavioral tests, not evidence
of native model quality or performance.

Regenerate ACP contracts with `just generate-acp-types`, then run desktop type
checks and the settings tests. Native runs must explicitly select a SafeTensors
model directory or a GGUF model with `backend_id: eredu`. Inspect the `eredu` request log's execution
plan to verify the selected backend and residency. Keep hardware/model-dependent
validation separate from portable test results.

See [VALIDATION.md](VALIDATION.md) for the migration's native and portable results,
reproduction commands, and upstream limitations at the pinned revision.
