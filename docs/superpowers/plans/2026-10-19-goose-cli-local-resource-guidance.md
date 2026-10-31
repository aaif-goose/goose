# Goose CLI Local Resource Guidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Help local-model users choose variants that fit available inference memory and understand context use without presenting estimates as facts.

**Architecture:** Extend the existing TUI model detail flow with resource-fit data from Goose’s local inference management APIs. Keep assessment logic pure and source-aware; Ollama and other runtimes remain `unknown` unless they report enough trustworthy data. Add a `/context` view using session usage values and explicit unavailable labels, reusing existing session/context logic.

**Tech Stack:** Rust, Goose CLI, Goose local-inference management API, Ratatui, Tokio.

**Spec:** `docs/superpowers/specs/2026-10-18-goose-cli-local-agent-tui-design.md` (approved).

## Global Constraints

- “Do not use installed physical memory as if it were all free for inference.”
- “Clearly distinguish recommended, tight fit, over budget, and unknown rather than promising a model will run based on rough estimates.”
- “Prevent selection when known model requirements exceed the effective budget.”
- “If fit is based only on uncertain estimates, warn and explain the uncertainty instead of hard-blocking.”
- “Refresh resource availability before model launch.”
- Do not change Goose’s provider, agent, context compaction, or model-download policies.
- Do not infer local memory fit for Ollama from model names or file-size guesses.

## Review Focus

- Goose local inference reports zero/unavailable memory: show `unknown`, never `over budget`.
- Catalog variant size is absent or zero: show `unknown`, allow selection with an uncertainty note.
- Available memory changes after browsing: refresh the assessment before download/model launch.
- Ollama is reachable but does not report memory requirements: keep selection enabled and label fit `unknown`.
- Session usage or context limit is missing: report each missing value explicitly rather than substituting zero.

---

## File Structure

- Create `crates/goose-cli/src/session/tui/model_fit.rs`: pure fit states, provenance, and assessment from known available/required memory.
- Modify `crates/goose-cli/src/session/tui/model_inventory.rs`: retrieve variant recommendations and available memory through the existing local-inference management API; preserve unknowns.
- Modify `crates/goose-cli/src/session/tui/model_picker.rs`: carry fit assessment with model selection and preserve it during deduplication/filtering.
- Modify `crates/goose-cli/src/session/tui/mod.rs`: display fit in model details, prevent only reliably over-budget selections, refresh before launch, and implement `/context` detail UI.
- Modify `crates/goose-cli/src/session/input.rs`: register `/context` through the existing command parser.
- Modify `crates/goose-cli/src/session/mod.rs`: expose a small typed context snapshot from existing session metadata for the TUI.

## Interfaces

- `ModelFitStatus`: `Recommended`, `Fits`, `TightFit`, `OverBudget`, or `Unknown`; `Fits` distinguishes a usable variant from the backend's preferred variant.
- `ModelFitAssessment`: status, optional required bytes, optional available bytes, optional effective budget bytes, whether selection is blocked, and a short explanation.
- `assess_model_fit(required_bytes: Option<u64>, available_bytes: Option<u64>, recommended: Option<bool>, backend_no_fit: bool) -> ModelFitAssessment` treats absent/zero values as unknown. `backend_no_fit` is valid only for GGUF variants assessed by Goose's existing llamacpp recommender with a non-empty candidate list and non-zero available memory. Only that result blocks selection; size-only comparisons stay advisory.
- `ContextSnapshot`: provider-reported current token usage, accumulated token usage, optional context limit, and usage provenance; missing values stay absent.
- `UiAction::OpenContext` opens the context detail view; `/context` and the header use the same view state.

## Tasks

### Task 1: Add pure, provenance-aware model-fit assessment

**Files:**
- Create: `crates/goose-cli/src/session/tui/model_fit.rs`
- Modify: `crates/goose-cli/src/session/tui/mod.rs`

- [ ] Add tests for recommended, fits, tight-fit, backend over-budget, missing-size, zero-memory, and advisory-only size comparisons.
- [ ] Run `GOOSE_DISABLE_KEYRING=1 cargo test -p goose-cli --lib session::tui::model_fit::tests` and confirm the new types are missing.
- [ ] Implement `ModelFitStatus`, `ModelFitAssessment`, and `assess_model_fit` with explicit inputs and no host probing.
- [ ] Run the focused tests and confirm they pass.
- [ ] Commit as `feat: add local model fit assessments`.

### Task 2: Retrieve Goose local inference fit recommendations

**Files:**
- Modify: `crates/goose-cli/src/session/tui/model_inventory.rs`
- Modify: `crates/goose-cli/src/session/tui/model_picker.rs`
- Reference: `crates/goose-local-inference/src/management.rs::huggingface_repo_variants`
- Reference: `crates/goose-local-inference/src/management.rs::list_models`

- [ ] Add tests that preserve `Unknown` for Ollama and unavailable local-inference memory, and that associate the backend’s recommended variant with its reported available memory.
- [ ] Run focused inventory tests and confirm the fit metadata is not populated.
- [ ] Add an on-demand variant-fit adapter using Goose’s existing response (`recommended_index`, `available_memory_bytes`, and variant sizes); pass `backend_no_fit` only for a non-empty llama.cpp GGUF variant set whose recommender returned no index. Do not add a separate hardware probe or network request.
- [ ] Merge fit metadata without losing source/provenance and without turning missing values into zero-byte requirements.
- [ ] Run inventory and picker tests; commit as `feat: load local inference fit guidance`.

### Task 3: Show fit guidance and block only reliable over-budget choices

**Files:**
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Modify: `crates/goose-cli/src/session/tui/model_picker.rs`

- [ ] Add state tests for an over-budget variant being unselectable when the backend assessment is authoritative, and unknown/advisory variants remaining selectable with a warning.
- [ ] Run the focused UI tests and confirm the new selection gate fails.
- [ ] Display `Recommended`, `Tight fit`, `Over budget`, or `Unknown` in details; refresh fit data before model launch and require a new explicit choice if the status changed to over budget.
- [ ] Keep Ollama/model-name-only assessments unknown and selectable.
- [ ] Run focused UI tests plus `cargo check -p goose-cli`; commit as `feat: guide local model selection by memory`.

### Task 4: Add a context detail view and `/context`

**Files:**
- Modify: `crates/goose-cli/src/session/mod.rs`
- Modify: `crates/goose-cli/src/session/input.rs`
- Modify: `crates/goose-cli/src/session/tui/events.rs`
- Modify: `crates/goose-cli/src/session/tui/mod.rs`

- [ ] Add tests for reported current/accumulated usage, absent context limit, unavailable provider usage, and `/context` parsing.
- [ ] Run focused context/parser tests and confirm the new context action is not recognized.
- [ ] Build `ContextSnapshot` from existing session usage and model context-limit sources; never fabricate an estimate or label a provider count as Goose’s exact prompt composition.
- [ ] Add a context detail view reachable from the header and `/context`, retaining the conversation after close.
- [ ] Run focused tests, `cargo check -p goose-cli`, and the full CLI library suite; commit as `feat: add interactive context details`.

## Deferred From the Approved Slice 2

Permission cards require a typed permission-request event and a terminal suspend/resume path so the existing `Permission` policy remains authoritative. Plan and implement that as a separate follow-up after this resource/context work; do not replace existing approval semantics here. Compaction previews remain optional and must not change current compaction thresholds.

## Verification

- Run `cargo fmt --all`.
- Run the focused tests for each task and `GOOSE_DISABLE_KEYRING=1 cargo test -p goose-cli --lib` at the end.
- Run `cargo check -p goose-cli`.
- Manually verify known-fit, tight-fit, over-budget, unknown/Ollama, changed-memory refresh, `/context`, narrow terminal, and no-color behavior.
- `goose run` and its text/JSON/stream-json output remain unchanged.
