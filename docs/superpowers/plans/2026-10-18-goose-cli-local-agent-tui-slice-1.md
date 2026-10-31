# Goose CLI Local Agent TUI Slice 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give interactive Goose CLI users a focused terminal experience for discovering configured and installed local models, choosing them by name, and reading assistant answers clearly.

**Architecture:** Add a Ratatui application behind the existing interactive CLI boundary, using Crossterm for terminal events and a typed UI event/action boundary around the existing session. Extract a model inventory/picker view model that combines configured providers, configured local runtimes, and the existing Goose local-model catalog when the user explicitly browses; preserve existing agent, provider, download, and session logic. Keep `goose run` and an inline fallback on the current output path.

**Tech Stack:** Rust, Ratatui with Crossterm backend, Tokio, existing Goose provider/local-inference APIs, Cargo tests.

**Spec:** `docs/superpowers/specs/2026-10-18-goose-cli-local-agent-tui-design.md` (approved).

## Global Constraints

- Do not replace or rewrite the Goose agent runtime or provider implementations.
- Model discovery checks only configured or conventional local endpoints; it does not scan the network.
- Opening model discovery must not start a model pull or catalog download; catalog access and downloads happen only after explicit user actions.
- The configured default remains available and Goose must not silently replace it.
- Metadata distinguishes reported facts, catalog metadata, estimates, and unknown values.
- Assistant answers use normal foreground contrast; only secondary status information may be dimmed.
- The renderer must not expose private chain-of-thought.
- Offer a plain accessible renderer and a non-alternate-screen fallback.
- Preserve the existing `goose run` scriptable output behavior.

## Review Focus

- Non-TTY, redirected output, and terminals without alternate-screen support: stay on the inline renderer without corrupting terminal state.
- Narrow terminals and no-color terminals: keep answer text, model name, and selection state readable.
- Ollama/local inference unavailable or returning an error: keep configured model usable and show an actionable, non-fatal status.
- Model lists with duplicates, empty results, or an unknown configured default: show one stable entry per provider/model and retain the configured default.
- Assistant responses containing long Markdown/code blocks: preserve normal contrast and render content without exposing hidden reasoning.

---

## File Structure

- Create `crates/goose-cli/src/session/tui/mod.rs`: terminal lifecycle, event loop, rendering, and UI action dispatch; keep it separate from session/provider logic.
- Create `crates/goose-cli/src/session/tui/model_picker.rs`: typed model inventory entries, stable merge/deduplication, filtering, and picker state.
- Create `crates/goose-cli/src/session/tui/model_inventory.rs`: configured and locally installed model inventory backed by existing Goose provider/runtime APIs.
- Create `crates/goose-cli/src/session/tui/events.rs`: typed session-to-UI events and UI-to-session actions used by the adapter.
- Modify `crates/goose-cli/src/session/mod.rs`: select the full-screen UI only for interactive TTY sessions, adapt session events/actions, and keep the inline path as fallback.
- Modify `crates/goose-cli/src/session/input.rs`: route bare `/model` to the picker in full-screen mode while retaining `/model <name>` and `--provider` syntax.
- Modify `crates/goose-cli/src/session/output.rs`: provide a structured assistant-answer rendering entry point for the TUI and retain existing inline output functions.
- Modify `crates/goose-cli/Cargo.toml`: add Ratatui and Crossterm using `cargo add` and enable the Crossterm backend.
- Add focused unit tests alongside `model_picker.rs`, `events.rs`, and TUI rendering code; add session-level tests in the existing CLI test locations where session behavior is testable.

## Interfaces

- `ModelSource`: `Configured`, `Ollama`, `GooseLocal`, `Catalog`.
- `ModelAvailability`: `Configured`, `Installed`, `Downloadable`.
- `ModelInventoryEntry`: provider name, model name, source, availability, `is_active`, `is_default`, and optional metadata with provenance. Do not include RAM/VRAM fit in this slice; that belongs to Slice 2.
- `ModelInventory`: entries plus non-fatal per-source diagnostics.
- `ModelPickerState::new(entries: Vec<ModelInventoryEntry>, active: (String, String)) -> Self`, `set_query(&mut self, query: &str)`, `visible_entries(&self) -> &[ModelInventoryEntry]`, and `select(&self, index: usize) -> Option<ModelSelection>`.
- `SessionEvent`: `AssistantDelta(String)`, `AssistantAnswer(String)`, `ToolActivity(ToolActivityEvent)`, `Status(String)`, `ModelChanged { provider: String, model: String, is_local: bool }`, `Error(String)`, and `TurnFinished`.
- `UiAction`: `Submit(String)`, `OpenModelPicker`, `BrowseCatalog(String)`, `DownloadModel(ModelSelection)`, `SelectModel(ModelSelection)`, `Cancel`, `Resize`, and `Quit`.

## Tasks

### Task 1: Add a pure model inventory and picker state

**Files:**
- Create: `crates/goose-cli/src/session/tui/model_picker.rs`
- Modify: `crates/goose-cli/src/session/mod.rs` (register private `tui` module)

**Interfaces:**
- Produces the `ModelSource`, `ModelAvailability`, `ModelInventoryEntry`, `ModelSelection`, and `ModelPickerState` interfaces defined above.
- Search is case-insensitive over provider, model name, source label, and availability label. Selection is by visible index and returns `None` for an invalid index.

- [ ] **Step 1: Write failing unit tests** for case-insensitive filtering, source/provider matching, stable duplicate removal, active/default markers, empty results, and out-of-range selection.
- [ ] **Step 2: Run the focused tests and confirm they fail** with missing picker types/behavior.
- [ ] **Step 3: Implement the pure inventory and picker state** without terminal or provider calls.
- [ ] **Step 4: Run the focused tests and confirm they pass.**
- [ ] **Step 5: Commit** as `feat: add model picker inventory state`.

### Task 2: Build configured/local model inventory from existing provider APIs

**Files:**
- Create: `crates/goose-cli/src/session/tui/model_inventory.rs`
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Reference: `crates/goose-providers/src/ollama.rs::fetch_ollama_model_names`
- Reference: `crates/goose-local-inference/src/management.rs::list_models`
- Reference: `crates/goose-cli/src/session/builder.rs` model resolution and `crates/goose-cli/src/session/mod.rs::handle_model`

**Interfaces:**
- `load_model_inventory(config: &Config, active_provider: &str, active_model: &str) -> Result<ModelInventory>` returns configured providers/models, models reported by their configured local runtimes, and non-fatal per-source diagnostics; local-inference results are included only when that feature is enabled.
- A local endpoint error is returned as an inventory status alongside usable configured entries; it must not fail the session or remove the configured model.
- No catalog request is performed by `load_model_inventory`.

- [ ] **Step 1: Add failing tests** for configured model retention on endpoint failure, Ollama model inclusion, local inference inclusion when enabled, duplicate merging, and no catalog fetch on initial inventory load.
- [ ] **Step 2: Run the focused tests and confirm they fail.**
- [ ] **Step 3: Implement inventory loading** using existing configuration/runtime functions only; query configured local endpoints without network discovery and never invoke download/pull operations.
- [ ] **Step 4: Run focused tests and confirm they pass.**
- [ ] **Step 5: Commit** as `feat: list configured local models`.

### Task 3: Add explicit local catalog browse and download

**Files:**
- Modify: `crates/goose-cli/src/session/tui/model_inventory.rs`
- Modify: `crates/goose-cli/src/session/tui/events.rs`
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Reference: `crates/goose-local-inference/src/management.rs::search_huggingface_models`, `huggingface_repo_variants`, and existing download/progress/cancellation APIs

**Interfaces:**
- `browse_local_catalog(query: &str) -> Result<Vec<ModelInventoryEntry>>` calls the existing Goose local-inference catalog only after `UiAction::BrowseCatalog`; rows use `Catalog`/`Downloadable` and mark unknown metadata as unknown.
- `download_model(selection: &ModelSelection, progress: impl Fn(DownloadProgress), cancel: CancellationToken) -> Result<InstalledModel>` delegates to Goose local-inference download facilities and returns only after install completes.
- Browsing may be unavailable without the `local-inference` feature and must provide an explanatory empty state. A download requires a second explicit action after the user views its details.

- [ ] **Step 1: Add failing tests** showing initial discovery does not call catalog APIs, Browse invokes them, unknown size is not fabricated, download starts only after the explicit confirm action, and cancellation/error states are rendered.
- [ ] **Step 2: Run the focused tests and confirm they fail.**
- [ ] **Step 3: Implement explicit catalog browse and download adapters** over existing management APIs; do not add direct Hugging Face/network implementation.
- [ ] **Step 4: Run focused tests and confirm they pass.**
- [ ] **Step 5: Commit** as `feat: browse and download local models`.

### Task 4: Define the session event/action boundary

**Files:**
- Create: `crates/goose-cli/src/session/tui/events.rs`
- Modify: `crates/goose-cli/src/session/tui/mod.rs`

**Interfaces:**
- Add the `SessionEvent` and `UiAction` enums specified above plus a `ToolActivityEvent` carrying tool name, short status text, and an optional expandable detail string.
- Event conversion must use user-visible assistant content and existing tool/session events. It must never serialize hidden reasoning or internal prompt content into the UI transcript.

- [ ] **Step 1: Add failing conversion tests** proving normal assistant output becomes visible answer/delta events, hidden assistant content produces no answer event, and tool/status events remain secondary event variants.
- [ ] **Step 2: Run the focused tests and confirm they fail.**
- [ ] **Step 3: Implement typed event/action definitions and conversions** at the CLI boundary without changing provider or agent APIs.
- [ ] **Step 4: Run focused tests and confirm they pass.**
- [ ] **Step 5: Commit** as `refactor: define interactive tui session events`.

### Task 5: Add the Ratatui/Crossterm terminal shell and accessible fallback routing

**Files:**
- Create: `crates/goose-cli/src/session/tui/mod.rs`
- Modify: `crates/goose-cli/src/session/mod.rs`
- Modify: `crates/goose-cli/Cargo.toml`

**Interfaces:**
- `run_tui(session: &mut CliSession, initial_prompt: Option<String>) -> Result<()>` owns terminal setup/restore and dispatches typed actions to the existing session handler.
- Enter the full-screen UI only when stdin/stdout are terminals and alternate-screen support can be initialized. On setup failure, restore the terminal if needed and use the existing inline loop.
- Run Ratatui through Crossterm; add dependencies with `cargo add ratatui --features crossterm` and `cargo add crossterm` (adjust features to the selected compatible Ratatui release and workspace MSRV).

- [ ] **Step 1: Add failing UI state/render tests** for narrow widths, no-color style selection, empty conversation, and normal-contrast answer paragraphs.
- [ ] **Step 2: Run the focused tests and confirm they fail.**
- [ ] **Step 3: Add dependencies and implement terminal lifecycle, resize/input loop, and basic conversation layout** with a clear header, transcript, and composer. The answer text uses the normal foreground palette; secondary labels alone use muted styling.
- [ ] **Step 4: Add startup routing** so scripts and redirected output keep the old path and TTY initialization failure falls back inline.
- [ ] **Step 5: Run focused tests and verify terminal restoration on normal quit and error paths.**
- [ ] **Step 6: Commit** as `feat: add interactive goose terminal ui`.

### Task 6: Integrate model picker and keep model switching on existing session logic

**Files:**
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Modify: `crates/goose-cli/src/session/tui/model_picker.rs`
- Modify: `crates/goose-cli/src/session/mod.rs`
- Modify: `crates/goose-cli/src/session/input.rs`

**Interfaces:**
- The header model control and bare `/model` open the same picker state.
- Enter on a configured or installed model dispatches `UiAction::SelectModel`; the adapter calls the existing session model switch path and updates the model header on success.
- Downloadable rows appear only after Browse; selecting one opens its detail/source/known-size view, and only a separate confirmation starts a download.
- Initial interactive startup opens the model discovery landing state with **Configured** and **Installed locally** sections. The configured default stays selectable even when no local models are reported; a missing configured provider is described and directs the user to local model setup/discovery. Runtime checks continue asynchronously and errors are visible without blocking the screen.

- [ ] **Step 1: Add failing interaction tests** for initial discovery sections, configured-default persistence with empty local inventory, header and `/model` opening the same picker, search narrowing, successful model switch, and failed switch retaining the previous active model.
- [ ] **Step 2: Run focused tests and confirm they fail.**
- [ ] **Step 3: Wire picker actions to the existing `handle_model` switching semantics** while preserving `/model <name>` and `/model --provider <name> [model]` behavior.
- [ ] **Step 4: Run focused tests and confirm they pass.**
- [ ] **Step 5: Commit** as `feat: select models from interactive picker`.

### Task 7: Render assistant answers with clear contrast and compact activity

**Files:**
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Modify: `crates/goose-cli/src/session/tui/events.rs`
- Modify: `crates/goose-cli/src/session/output.rs`

**Interfaces:**
- `render_assistant_answer(text: &str, width: u16) -> Vec<Line<'static>>` converts user-visible assistant Markdown/plain text into Ratatui lines with normal foreground contrast and readable code/list hierarchy.
- Tool and status events render as compact, visually secondary rows with an expansion action; they do not dim or inherit the answer style.
- Provide a plain inline rendering path when full-screen is unavailable, without changing `goose run` formats.

- [ ] **Step 1: Add failing renderer tests** for answer style contrast, Markdown hierarchy/code blocks, tool activity separation, hidden-content exclusion, and plain inline fallback.
- [ ] **Step 2: Run focused tests and confirm they fail.**
- [ ] **Step 3: Implement answer and activity rendering** with answer as the primary visual hierarchy, no private reasoning, and no color-dependent meaning.
- [ ] **Step 4: Run focused tests and inspect snapshots** at 80x24, 40x16, and no-color settings.
- [ ] **Step 5: Commit** as `fix: render tui answers with readable contrast`.

## Spec Coverage and Deferred Work

This plan covers the approved Slice 1: interactive TUI foundation, model inventory/picker, explicit on-demand catalog browsing/download, and answer-focused rendering. Memory-aware fit guidance and blocking, context/permission supervision, the global slash-action registry, subagent supervision, and advisor are outside Slice 1 and require their own implementation plans. The approved spec’s three-slice breakdown and technical boundaries remain authoritative.

## Verification

- Run the focused unit/integration tests specified by each task.
- Run `cargo fmt --check` for the touched Rust files.
- Build the CLI with the default feature set to verify Ratatui/Crossterm compatibility and feature-gated local inference.
- Manually verify interactive TTY startup, model picker, answer/tool contrast, terminal restoration, redirected output fallback, narrow terminal, and no-color rendering.
- Do not alter or test `goose run` output beyond confirming the interactive routing does not intercept non-interactive invocations.
