# Eredu migration validation

Updated baseline: `jbg/eredu` main resolved on 2026-09-09 to
`dc8c8d8c501f6f8e99cc602c8a9d994927b4cf51`, replacing
`b4ef9f323155c90c278b394fff62edfe1761d3be`. All direct Eredu dependencies use the new
revision with default features disabled. Validation ran on an Apple Silicon Mac
with 256 GiB unified memory.

## Automated checks

| Check | Result |
| --- | --- |
| Local inference, no default features | 84 tests passed; model-dependent regression ignored |
| Local inference with `hf-hub`, without MLX | 142 tests passed; model-dependent regression ignored |
| Local inference with `mlx,hf-hub` | 142 tests passed; two model-dependent tests ignored |
| Direct LFM activation regression with checkpoint tokenizer/template | Passed separately; scripted tokens through the Eredu public API |
| Native LFM tools, LFM plain template, Muse external draft, Muse plain-template external draft | All four harness runs passed, including disconnect/reuse and eviction |
| Local inference clippy, `mlx,hf-hub`, all targets, warnings denied | Passed |
| Goose and CLI clippy, libraries/binaries, `mlx,code-mode,rustls-tls` | Passed |
| Native CLI build, `mlx,code-mode,rustls-tls` | Passed |
| Rust formatting and whitespace checks | Passed |

ACP schema/type generation, client build, desktop typecheck, five settings tests,
and translation validation passed again for the format/backend separation. The
settings tests cover the format default, selecting Eredu for GGUF, clearing an
override, and preserving a selected backend when resetting generation settings.

The 20 adapter tests exercise the actual generic Eredu facade with a backend whose
model state contains `Rc<ThreadId>`. The backend is constructed, used and dropped
on the worker thread. Provider-level tests use that same worker through Goose's
model slots and real finalization/disconnect monitor. Coverage includes defaults,
request precedence, persisted legacy values, templates, all residency outcomes,
planner constraints, explicit devices, exact context headroom, cancellation,
settlement failure, worker panic, reuse, eviction, load deduplication, semantic
streaming, schema validation, emulation history, and speculative scheduling. The
updated regressions require both Qwen XML tool calls and their results, plus exact
committed counts, TTFT, EOS/caller stops, and speculation for unrecognized templates.

No agent-loop behavior was changed. CUDA and Vulkan feature forwarding remains
unchanged and applies to llama.cpp. No CUDA/Vulkan hardware or Windows/Linux native
toolchain validation was performed on this Mac.

## Verified fixes

1. **LFM checkpoint native tool activation.** The direct public-API regression now
   requires a completed `lookup(code=7)` call instead of the old activation error.
   With real LFM2.5-1.2B-Instruct weights and the embedded checkpoint template,
   ForceNative called `lookup(code=7)` and answered “BLUE” after receiving the
   result. The native harness also passed chat, disconnect/reuse, and eviction.
2. **Repeated Qwen XML calls.** The adapter regression now requires both calls,
   distinct IDs, arguments `x=1` and `x=2`, and a subsequent prompt containing both
   calls and their tool results. Exact output accounting includes the complete
   scripted sequence and EOS. This tests the Qwen XML protocol through Eredu;
   it is not a real Qwen checkpoint test.
3. **Muse external draft dtype.** Native Muse-Glimmer-30B generation with its
   external assistant completed successfully. The first embedded-template response
   committed 101 tokens, proposed 285 draft tokens and accepted 5, with 468 ms
   TTFT. The old attention-mask/BF16 error did not recur. Disconnect/reuse and
   eviction also passed. These observations verify execution and telemetry, not
   a speculative speedup.
4. **Unrecognized templates.** Goose now calls Eredu's `generate_prepared_text`
   or `generate_prepared_text_speculative`, retaining the common terminal usage
   and draft-statistics mapping. Portable tests require TTFT, Unicode streaming,
   exact committed counts, caller stops/EOS, and speculative output under inherited
   sampling and Mirostat. Real LFM generation with `tests/support/plain.jinja`
   passed and reported TTFT (45 ms for the first response). Real Muse external
   drafting with that same unrecognized template also passed: its first response
   committed 60 tokens, proposed 177 draft tokens, and reported 482 ms TTFT.
   All proposals in that response were rejected by target verification; portable
   coverage separately verifies accepted speculative tokens on this path.

Every successful native run records `eredu_execution_plan` in terminal usage.
The observed backend/device was `mlx:metal:0` with fully resident weights.
Host/disk residency is covered by controlled portable observations, not by forcing
these native models into those modes.

The cached Muse assistant still omits tokenizer metadata required by the pinned
facade for vocabulary proof. Native validation uses the previously staged
`/private/tmp/goose-eredu-assistant`, containing copy-on-write assistant weights
and the target's matching tokenizer metadata. Cached checkpoints were unchanged.

The original migration also observed a Muse native ATEM tool-payload parsing
failure and rejection of the cached Muse GGUF by Goose's llama.cpp dependency
(`unknown architecture: muse-glimmer`). Those separate checks were not repeated
for this four-fix update; this report makes no new claim about them.

Native and portable logs are retained as `/private/tmp/goose-eredu-update-*.log`.
The four native-run results and usage summaries are in
`/private/tmp/goose-eredu-update-native-results.json`.

## Reproduction

Activate Hermit before these commands. `GOOSE_EREDU_TEST_MODEL` is an absolute
checkpoint directory; all native tests assert Eredu's selected plan in final usage.

```sh
source bin/activate-hermit
cargo test -p goose-local-inference --no-default-features --features hf-hub
cargo test -p goose-local-inference --no-default-features --features mlx,hf-hub

# Ordinary chat, emulation, custom template/Mirostat, disconnect and reuse:
GOOSE_EREDU_TEST_MODEL=/path/to/LFM2.5-1.2B-Instruct \
GOOSE_EREDU_TEST_PHASE=remaining \
  cargo test -p goose-local-inference --features mlx,hf-hub \
  --test eredu_native -- --ignored --nocapture

# Native tool round trip using the embedded LFM checkpoint template:
GOOSE_EREDU_TEST_MODEL=/path/to/LFM2.5-1.2B-Instruct \
GOOSE_EREDU_TEST_PHASE=tools \
  cargo test -p goose-local-inference --features mlx,hf-hub \
  --test eredu_native -- --ignored --nocapture

# Direct public-API regression for successful LFM checkpoint-native activation:
GOOSE_EREDU_TEST_MODEL=/path/to/LFM2.5-1.2B-Instruct \
  cargo test -p goose-local-inference --features hf-hub \
  --test eredu_upstream -- --ignored
```

Set `GOOSE_EREDU_TEST_DRAFT` to a compatible assistant directory to exercise the
selected speculative plan. For an unrecognized template, also set
`GOOSE_EREDU_TEST_TEMPLATE="$PWD/crates/goose-local-inference/tests/support/plain.jinja"`
and `GOOSE_EREDU_TEST_PHASE=chat`. Set `GOOSE_EREDU_TEST_PHASE` to `chat`, `tools`,
`reasoning`, `emulation`, `custom`, `remaining`, or `all` (default). Native tests are
strict assertions; model/protocol failures remain failures.

## Self-test recipe

The rebuilt CLI reran `goose run --recipe goose-self-test.yaml --params
test_phases=local-eredu` with LFM's embedded checkpoint template, ForceNative,
isolated settings, and `/private/tmp/goose-eredu-update-selftest-workspace` as the
requested workspace. The recipe now also asks for TTFT and available draft stats.

This recipe still **failed**: LFM emitted a fabricated success report without
calling a tool, and the requested workspace did not exist afterward. The CLI
returned zero, which is not treated as a recipe pass. Request diagnostics confirm
Eredu on `mlx:metal:0`, fully resident, with 228 committed output tokens and 381 ms
TTFT. The native focused tool round trip above did pass with the same checkpoint.
Log: `/private/tmp/goose-eredu-update-selftest.log`.

The original `b4ef9f32` baseline also failed recipe validation: Muse hit its ATEM
protocol error, while LFM either returned a plan or selected inappropriate tools.
Those earlier logs remain under `/private/tmp/goose-eredu-selftest*.log`.

## LFM comparison with llama.cpp

Downloaded LiquidAI's official `LFM2.5-1.2B-Instruct-BF16.gguf` using the installed
Hugging Face CLI:

```sh
hf download LiquidAI/LFM2.5-1.2B-Instruct-GGUF LFM2.5-1.2B-Instruct-BF16.gguf --quiet
```

The resolved snapshot is `6767265158422fb8a19c62ceb45f16f05363615b`. The file is
2,343,326,528 bytes and has architecture `lfm2`, 148 tensors, and EOS token 7.
It uses BF16 matrix weights and F32 auxiliary tensors; the cached SafeTensors
checkpoint uses BF16. The official GGUF has an older embedded chat template.
Goose's existing `llama-cpp-2` / `llama-cpp-sys-2` version `0.1.146` loads it
successfully. No llama.cpp dependency change was needed.

The same failure occurs through llama.cpp:

| Run | Observed behavior | File-operation result |
| --- | --- | --- |
| Original recipe, llama.cpp defaults, embedded GGUF template, ForceNative, 256-token limit | Claimed the tool returned `eredu-ok` and fabricated execution diagnostics; 186 output tokens | Failed; no tool call and no workspace |
| Matched greedy control, llama.cpp | Printed shell commands in a Markdown code block; 74 output tokens | Failed; no tool call and no workspace |
| Matched greedy control, Eredu | Fabricated a JSON success report; 159 output tokens | Failed; no tool call and no workspace |

The matched control used a temporary copy of the recipe accepting either backend
and requesting whatever diagnostics that backend actually records. Both runs used
the same SafeTensors checkpoint template, greedy sampling, a 256-token limit,
ForceNative, no penalties, and the same requested workspace path. The logged
`input` objects (system prompt, messages, tools, and settings) were identical.
All configuration and test artifacts were isolated under `/private/tmp`.

The llama.cpp request logs identify backend `llamacpp` and generation path
`native`; their raw generated text contains zero `<|tool_call_start|>` markers.
The expected workspace directories did not exist after any run. A zero CLI exit
code and the model's claimed success are not counted as self-test passes. This
reproduces the action-following failure on both backends; it does not establish
that either backend always fails with this model or prompt.

Logs: `/private/tmp/goose-lfm-llama-default.log`,
`/private/tmp/goose-lfm-matched-llamacpp.log`, and
`/private/tmp/goose-lfm-matched-mlx.log`. Matched-run configuration and request
logs are in the corresponding directories. The temporary comparison recipe is
`/private/tmp/goose-lfm-comparison-recipe.yaml`; checkpoint metadata is in
`/private/tmp/goose-lfm-gguf-metadata.json`.

## Format and backend separation

Artifact formats are `gguf` and `safetensors`; backend IDs are `llamacpp` and
`eredu`. Selection precedence is per-model override, `GOOSE_LOCAL_BACKEND`, then
format default (GGUF → llama.cpp; SafeTensors → Eredu). Downloads and cache records
carry format independently. Eredu artifact admission handles supported formats
without a Goose format allowlist. Existing persisted `mlx` backend IDs migrate to
`eredu`, discarding all other settings for each legacy model and avoiding a second
write. Read-only base layers apply the same reset in memory.

The native harness passed again with both official LFM2.5-1.2B-Instruct
SafeTensors and the comparable BF16 GGUF selected through `backend_id: eredu`.
Both returned streamed text and usage, reused loaded weights, recovered from
consumer disconnection, and evicted cleanly. The GGUF execution report identified
`artifact_format: gguf` with fully resident execution on `mlx:metal:0`; the latter
is Eredu's physical execution backend, distinct from Goose's `eredu` backend ID.
Logs: `/private/tmp/goose-format-gguf-eredu-native.log` and
`/private/tmp/goose-format-safetensors-native.log`.

The format/backend update's rebuilt CLI verified migration on an isolated config,
no second rewrite, `auto` reset, global override, per-model precedence, and rejection of llama.cpp
for SafeTensors. Real GGUF CLI chat returned text through both llama.cpp (default)
and Eredu (override); request logs recorded the matching backend IDs. Three config
migration tests passed, including JSON-encoded settings and read-only base layers.
The runtime matrix passed 84 tests without features and 142 tests with either
`hf-hub` or `mlx,hf-hub`. Local inference and Goose/CLI clippy passed with warnings
denied. CLI verification logs live under `/private/tmp/goose-format-cli/`.

LFM tool-use remains a separate limitation: the SafeTensors CLI run with Goose's
default tools reached Eredu but rejected a native argument named `async` as an
invalid Python identifier. The updated self-test recipe ran through GGUF/Eredu
but emitted an unrelated `load` call as text instead of completing the requested
file round trip. These are not recorded as self-test passes. See
`/private/tmp/goose-format-cli/safetensors-default-chat.log` and
`/private/tmp/goose-format-selftest/run.log`.

SafeTensors CLI inference subsequently returned `hello` with the same default
Eredu backend and `tool_calling: force_emulated`; its log is
`/private/tmp/goose-format-cli/safetensors-emulated-chat.log`. The initial CLI
process returned zero despite the native schema error, so process exit status
alone was not counted as evidence of successful generation.

The SafeMLX settings reset passed all three migration tests again: legacy models
retain only `backend_id: eredu`, JSON-encoded settings receive the same reset,
and read-only base layers reset in memory. The tests also verify that migration
does not run again or modify other models' settings. The adapter inheritance
regression passed with the resulting backend-only settings and no generation
overrides.
