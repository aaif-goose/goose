# Goose CLI TUI Live Turn Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep interactive Goose CLI users in the full-screen conversation view while assistant responses and tool activity arrive.

**Architecture:** Add an optional typed session-event sender at the existing interactive response boundary. Run each TUI turn while the terminal screen remains active, consuming events alongside the existing session future and redrawing the conversation from the session transcript. Preserve the existing agent stream, permission submission, cancellation token, inline output path, and all `goose run` formats.

**Tech Stack:** Rust, Tokio, Ratatui, Crossterm, existing Goose CLI session/event types.

**Spec:** `docs/superpowers/specs/2026-10-18-goose-cli-local-agent-tui-design.md` (approved), Conversation view and Technical boundaries.

## Global Constraints

- The TUI belongs in the Goose CLI and should not replace the agent runtime or provider implementations.
- The implementation plan should introduce a narrow event/action boundary incrementally rather than rewrite provider, session persistence, or agent behavior.
- The renderer must not expose private chain-of-thought; it can show concise progress/status events and tool activity.
- Keep an inline interactive fallback while the full-screen TUI is rolled out.
- The existing CLI remains scriptable: `goose run` and its text/JSON/stream-json output are unchanged.
- Use existing Goose mode and `Permission` options for action prompts.

## Review Focus

- Assistant message chunks arriving during a turn: append visible user-facing content once and keep normal answer contrast.
- Tool call and result messages: show concise activity without showing arguments or hidden content as assistant prose.
- Permission requests during a live turn: retain the current choice set, submission path, and cancellation behavior.
- Cancellation and stream errors: finish the TUI turn cleanly, retain the current transcript recovery behavior, and restore terminal state.
- Non-TTY and `goose run` invocations: do not initialize or emit TUI events into existing output contracts.

---

## File Structure

- Modify `crates/goose-cli/src/session/tui/events.rs`: add event conversion for visible assistant chunks, tool activity, status, and terminal turn outcomes.
- Modify `crates/goose-cli/src/session/tui/mod.rs`: add live-turn transcript state/rendering and keep Crossterm/Ratatui active while the response future and event receiver are polled.
- Modify `crates/goose-cli/src/session/mod.rs`: optionally forward events from the existing interactive agent stream, preserving current permission, cancellation, and non-TUI paths.
- Modify `crates/goose-cli/src/session/output.rs` only if a structured helper is needed to identify visible tool activity; keep existing inline and machine-output renderers intact.

## Interfaces

- `SessionEvent` remains the typed event boundary. Add conversion helpers that return no event for hidden assistant content and represent tool activity separately from assistant answer text.
- `SessionEvent::ResumeUi` restores the live TUI after existing prompts. An optional suspend control channel carries oneshot acknowledgements so the session waits until the TUI has released terminal input before opening a permission or elicitation prompt.
- `CliSession::process_message_with_events(message, cancel_token, interactive, event_tx, suspend_tx)` forwards `event_tx: Option<tokio::sync::mpsc::UnboundedSender<SessionEvent>>` and the optional suspend control sender through `process_agent_response`. Existing `process_message` delegates with both senders set to `None`, preserving all current callers and output behavior.
- `LiveTurnState::apply_event(&mut self, event: SessionEvent)` updates the live transcript and status. It contains only display state, so the renderer does not borrow `CliSession` while the response future is running.
- `run_conversation_turn(session, content) -> Result<()>` owns a TUI turn's event receiver and suspend acknowledgement receiver, and draws while polling the existing `process_message_with_events` future, session events, and terminal input. At turn completion it syncs the transcript from `session.messages` and returns the processing result.

## Tasks

### Task 1: Define and test live-turn event conversion

**Files:**
- Modify: `crates/goose-cli/src/session/tui/events.rs`
- Test: inline unit tests in `events.rs`

**Interfaces:**
- Produces event conversion helpers for user-visible assistant text and tool activity; Task 2 and Task 3 consume these conversions.

- [x] **Step 1: Add failing tests** proving a visible assistant chunk becomes one answer/delta event, hidden assistant content produces none, and tool calls/results become `ToolActivity` rather than assistant prose.
- [x] **Step 2: Run `GOOSE_DISABLE_KEYRING=1 cargo test -p goose-cli --lib session::tui::events::tests`;** confirm the new conversion expectations fail because tool/stream conversion is absent.
- [x] **Step 3: Implement minimal typed conversions** using `Message::user_visible_content()` and existing tool request/result content; do not serialize hidden content or raw internal messages into the TUI.
- [x] **Step 4: Rerun the focused event tests;** expect all event conversion tests to pass.
- [x] **Step 5: Commit** as `feat: add live tui session event conversions` with commit signing enabled (`7d120a385`).

### Task 2: Forward live events from the existing interactive response stream

**Files:**
- Modify: `crates/goose-cli/src/session/mod.rs`
- Modify: `crates/goose-cli/src/session/tui/events.rs` only if conversion needs an adapter
- Test: unit tests in `session/mod.rs`

**Interfaces:**
- Consumes Task 1 event conversions.
- Produces optional event forwarding from `process_agent_response`; when no sender is supplied, existing inline rendering and `goose run` handling remain unchanged.

- [x] **Step 1: Add failing tests** for the response event adapter: visible assistant content is forwarded, tool activity is a distinct event, and a `None` sender does not produce TUI events.
- [x] **Step 2: Run focused session tests** and confirm event forwarding is not available.
- [x] **Step 3: Add `process_message_with_events` and thread its optional senders to the response loop.** When an event sender exists, forward typed events and skip inline assistant streaming; when absent, preserve current rendering. Request TUI suspension and await its acknowledgement before existing permission and elicitation prompts; send `ResumeUi` when each prompt returns. Keep the existing permission submission, cancellation token, stream draining, error recovery, and machine-output branches unchanged.
- [x] **Step 4: Run focused event/session tests;** expect forwarding and existing permission mapping tests to pass.
- [x] **Step 5: Commit** as `feat: forward live response events to TUI` with commit signing enabled (`35d2981ce`).

### Task 3: Keep the full-screen conversation active during generation

**Files:**
- Modify: `crates/goose-cli/src/session/tui/mod.rs`
- Modify: `crates/goose-cli/src/session/tui/events.rs`
- Test: inline unit tests in `tui/mod.rs`

**Interfaces:**
- Consumes Task 2's optional event sender.
- Produces `run_conversation_turn(session, content) -> Result<()>`, which keeps the conversation terminal active while polling the existing processing future and session event receiver.

- [x] **Step 1: Add failing tests** for transcript event application: assistant chunks accumulate in order, tool activity remains a distinct compact row, suspend/resume brackets pause and restore terminal input ownership, turn completion clears the working state, and errors remain visible without altering prior transcript messages.
- [x] **Step 2: Run focused TUI tests** and confirm the live-turn state behavior is absent.
- [x] **Step 3: Implement `LiveTurnState` and live-turn rendering** with a concurrent `tokio::select!` over the session future, TUI event receiver, suspend acknowledgement receiver, and terminal resize/input events. Release raw/alternate-screen ownership before acknowledging suspension, and reacquire it on `ResumeUi`. Keep answer text at normal contrast and tool/status rows visually secondary. Retain existing terminal restoration guards.
- [x] **Step 4: Route submitted TUI messages through `run_conversation_turn`;** leave command handling, inline fallback, and non-interactive `process_message` calls on their existing paths.
- [x] **Step 5: Run focused TUI/session tests and `cargo fmt --all`;** verify existing permission semantics and cancellation checks still pass.
- [x] **Step 6: Commit** as `feat: keep live turns in full-screen TUI` with commit signing enabled (`98b823d9a`).

## Verification

- Run the focused TUI event/session tests and the full CLI library suite: `GOOSE_DISABLE_KEYRING=1 cargo test -p goose-cli --lib`.
- Run `cargo check -p goose-cli`, `cargo fmt --all`, and `git diff --check`.
- Confirm permission requests still use the same `Permission` values and `submit_tool_confirmation`, cancellation still drains/reconciles the response as before, and no event sender is installed for `goose run`.
- Manually inspect the live response layout at 80x24, 40x16, and with no color; confirm terminal restoration on completion, error, cancellation, and quit.
