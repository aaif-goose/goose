# Handoff: mid-turn auto-compaction

## Status

- Repository: `/home/itlk/projekty/trwajace/goose_ai`
- Branch: `work/mid-turn-auto-compact`
- PR: https://github.com/aaif-goose/goose/pull/11903
- Ostatnia kodowa zmiana: `b857833 Bound compaction suffix to tool inference`;
  późniejsze commity dokumentują handoff i są wypchnięte.
- Read `AGENTS.md` and `PROJECT_STATUS.md` before changing code.
- This is maintainer-directed work; do not block on Ready status of issue #11072.

The documentation push started a fresh CI matrix. Check live status rather
than relying on an earlier run. One review thread remains unresolved.

## Open review item

- Thread id: `PRRT_kwDOMneZ986gvZqm`
- URL: https://github.com/aaif-goose/goose/pull/11903#discussion_r3970649254
- Finding: an agent-only continuation appended after a later assistant
  inference is not recognized as an accounting boundary. State machine can
  then count an old tool result twice before its prepared-request hook runs.

Relevant function: `context_tokens_since_last_inference` in
`crates/goose/src/context_mgmt/mod.rs`.

## Current model and required correction

The helper adds only context missing from provider usage after an inference
that emitted tool calls. It currently:

1. finds the latest assistant message with a tool request;
2. returns `None` if a later assistant response is followed by a new
   user-visible, non-tool-response, non-steer message;
3. counts only non-assistant agent-visible messages after the tool request.

This prevents two prior bugs: missing a tool response placed before a late
assistant chunk from the same stream, and double-counting that late assistant
output. It still misses normal agent-only continuations, such as retry and
stop-hook nudges. Treat the appropriate agent-only continuation after a later
inference as a new request boundary too, without treating tool responses,
steers, or same-inference events as a boundary. Inspect `ops_retry.rs` and
`ops_stop_hook.rs`.

## Parity and tests

Changes to agent-loop behavior require parity:

- Legacy: `crates/goose/src/agents/agent.rs`
- State machine: `crates/goose/src/agents/state_machine/`

Relevant tests:

- `context_mgmt/mod.rs`: `suffix_accounting_anchors_to_the_tool_call_when_a_later_chunk_is_persisted`, `suffix_accounting_stops_after_a_later_completed_inference`, `tool_compaction_without_text_prompt_adds_a_user_continuation`
- `agents/agent.rs`: `legacy_compacts_a_tool_result_before_the_next_inference`
- `state_machine/tests/compaction_lifecycle.rs`: `auto_compacts_after_a_tool_result_before_the_next_inference`

Add a deterministic state-machine regression for retry or stop-hook, and run
the legacy regression after the shared-helper change.

## Required workflow

Use `apply_patch`. Preserve/update `PROJECT_STATUS.md`; then commit and push.

Run `source bin/activate-hermit`, `cargo fmt`, relevant `cargo test -p goose
--lib ... -- --nocapture`, `cargo clippy -p goose --all-targets -- -D warnings`,
and `git diff --check`. Reply concisely in the review thread, resolve it, and
verify CI plus all unresolved threads. Continue through failures or new review
notes rather than stopping at analysis.
