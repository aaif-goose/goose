# Merge fixes for `unroll-agent-loop`

Working notes for repairing what the merges with `origin/main` lost. Delete this file
before the branch merges.

**Status: sections 1–3 are done.** `cargo test -p goose` is down to the 4 `jsonwebtoken`
lib failures and the 6 network-dependent `tests/providers.rs` failures, both environmental.
`cargo test -p goose-cli` is fully green. Outside `state_machine/**` the diff against
`origin/main` went from 50 files / +1688 / -1237 to 50 files / +1657 / **-620**; every
remaining deletion is on the intentional list below. Sections 4 and 5 are still open.

Two problems turned up during the work that were not merge damage:

- `cargo build -p goose-cli` did not compile on this branch at all. `ActionRequiredData`
  gained a `ToolConfirmationResponse` variant but `session/export.rs` was never given the
  arm, and the crate is not covered by `cargo test -p goose`. Fixed here, along with the
  missing `MessageContent::Error` arm.
- `tests/schedule_tool_security.rs::parse_errors_do_not_reflect_recipe_contents` was
  failing. Extracting `ScheduleTool` replaced the parse-by-extension check with
  `validate_recipe_template_from_content`, whose error is the raw serde message — which
  quotes the recipe file back at the caller. Parse failures now get the generic message
  again; the semantic checks (missing prompt, bad retry config) still report their own
  wording, which `recipe_scheduling_lifecycle` depends on.
- The platform-extension prompt snapshot had been regenerated without the `code-mode`
  feature, so it lost the `code_execution` section and only matched under
  `cargo test -p goose`. Regenerated with the workspace feature set: it now differs from
  main by the `## scheduler` section alone. Run the workspace form before touching that
  snapshot again.
- The scheduler extension contributed a bare `## scheduler` heading to every system prompt.
  It never called `.with_instructions(...)` (every other platform extension does), and with
  `default_enabled: true` it registered even on hosts with no scheduler service, where it
  also advertises no tools. `client_factory` now returns `Option<Box<dyn McpClientTrait>>`
  so an extension the host cannot provide declines instead of registering empty, and
  `SchedulerClient::new` returns `None` without a scheduler. `schedule_tool` stopped being
  an `Option` as a result, which removed the "Scheduler not available" dead end in
  `call_tool`.

## What happened

`57f1b3f20` (and earlier merges) resolved conflicts by keeping the branch's version of
`agent.rs` and `reply_parts.rs` wholesale, then hand-porting pieces of upstream back.
Work was lost in both directions:

- upstream's #10716 (stable agent event message identity) never made it into the branch
- the branch's own `MessageContent::Error` rendering in ACP and the markdown export was
  overwritten by a later merge taking main's side

Nine tests fail because of this: 7 in `crates/goose/tests/agent.rs`, 2 in
`crates/goose/tests/compaction.rs`. (The 4 `jsonwebtoken` failures in the lib and the 6 in
`tests/providers.rs` are environmental — no outbound network — and are not ours.)

Patching the visible symptoms would leave us guessing about the rest, so the two big files
get rebuilt from `origin/main` and the state-machine integration is reapplied on top.

## 1. Rebuild from `origin/main` — done

### `crates/goose/src/agents/agent.rs`

Restore main's version, then reapply only:

- `create_state_machine` and `reply_with_state_machine`
- the `state_machine::enabled()` dispatch — put it inside `reply_impl`, **not** `reply`, so
  the state machine path inherits main's `ensure_message_event_id` boundary. This is also
  the fix for ids missing on state-machine-emitted events; the ops do not assign them
  consistently and `Emitter::emit` does not either.
- `pub(crate)` on `stop_hook_denial_context_message`, `stop_hook_denial_notification`,
  `stop_hook_block_cap_warning`, `stop_hook_block_cap`, `emit_stop_hook`,
  `emit_stop_hook_blocking`, `has_pending_steers`, `drain_pending_steers`, `goal`, `grind`,
  `stop_hook_block_cap_override`
- `steer_queues: Mutex<HashMap<String, SteerQueue>>` in place of `pending_steers`, plus the
  `steer_queue()` accessor — `SteerOperation` shares the `Arc<Mutex<VecDeque<Message>>>`
- the `scheduler` argument to `ExtensionManager::new`
- removing the `PLATFORM_MANAGE_SCHEDULE_TOOL_NAME` dispatch and tool registration (the
  scheduler platform extension replaces it — see section 5)
- `tool_stream` / `ToolStreamItem` / `ToolStream` now live in `tool_execution.rs`
- clearing `final_output_tool.final_output` after `RetryResult::Retried` moved out of
  `RetryManager` into the caller
- `dispatch_tool_call` returning `ErrorData` rather than `anyhow` + downcast
- `MAX_TURNS_MESSAGE` imported from `ops_maxturns` instead of a second copy of the string

Everything else in the current diff is regression or churn. Specifically **do not** carry
over:

- the removal of `ensure_message_event_id`, `push_message_with_id`,
  `persist_message_with_id`, `persist_and_push_message_with_id`
- `attach_turn_usage` losing its `preferred_message_id` argument
- the removal of the response-id carrier logic for split tool-request messages
- `stop_hook_context` losing `.with_working_dir(...)` (see section 2)
- `command_starts_turn` inlined at the `/goal` `/grind` call site (see section 2)

### `crates/goose/src/agents/reply_parts.rs`

Restore main's version, then reapply only the extraction that `ops_llm` calls:

- `prepare_inference_tools`
- `prepare_tools_for_provider`
- `stream_response_from_provider` as a free function

Two deviations were dropped rather than reapplied, because nothing outside the legacy path
needs them: `prompt_manager.load_subdirectory_hints(working_dir)` (the state machine calls it
through `build_system_prompt`) and `with_extension_and_tool_counts(extension_count, tools.len())`
in place of main's `tool_count`. `apply_tool_annotations` also stays where main had it —
`ops_llm` applies annotations itself, so moving it into `list_tools` was never needed.

`update_session_metrics` goes back to main's signature verbatim
(`post_compaction_context_tokens: Option<i32>`) and its three callers pass
`Some(compaction.retained_context_tokens)` again. The `bool` version derives the new
baseline from the summarization call's output tokens, which ignores everything retained —
that is what the two `tests/compaction.rs` failures are about.

Keep main's four tests: `prepare_toolshim_tools_applies_writable_annotations`,
`normal_provider_stream_groups_only_contiguous_mergeable_chunks`,
`toolshim_provider_stream_assigns_missing_message_id`,
`toolshim_provider_stream_preserves_provider_message_id`. The behaviour they cover is still
live; only the tests were deleted.

### `crates/goose-cli/src/session/output.rs`

Restore main's version (it has #10493's `is_user_visible` guard and `user_visible_content()`
projection in both render paths), then re-add just the `MessageContent::Error` arms and the
`ActionRequiredData::ToolConfirmationResponse` arms.

### `crates/goose/tests/agent.rs`, `crates/goose/tests/compaction.rs`, `crates/goose/src/agents/execute_commands.rs`

Restore the deleted upstream tests and the `command_starts_turn` helper (with its test).
`execute_commands.rs` keeps its branch changes otherwise: `is_known_slash_command`, the
recipe-persisting `resolve_command`, `Conversation::last`.

## 2. Straight reverts — done

- `stop_hook_context` gets `.with_working_dir(...)` back. It was the last caller, so
  `HookContext::working_dir` currently serialises as `null` for *every* hook event, not just
  Stop. Hook plugins read that field.
- `crates/goose/src/providers/oauth.rs` — the `test_token_cache` rewrite is unrelated to
  this branch. Revert it.
- Comments deleted from non-state-machine tests (e.g. the audience note in
  `tests/compaction.rs::assert_conversation_compacted`) come back.

## 3. Re-land branch work a later merge dropped — done

- `crates/goose/src/acp/server.rs` — `MessageContent::Error` as an agent message chunk, and
  `Error(CreditsExhausted)` routed through `prompt_error_from_message_content` so the
  desktop payment flow still fires. Today that function only matches `SystemNotification`,
  so a provider error under the state machine is invisible on desktop.
- `crates/goose-cli/src/session/export.rs` — `MessageContent::Error` arm. It currently falls
  through to `WARNING: Message content type could not be rendered to Markdown`.

Both were added in `1729c902b` and overwritten afterwards.

## 4. Simplifications — still open

- `OperationResult::NotApplicable(Emitter)` threads the emitter back through the result,
  which forces `Option<Emitter>` + `take()` in `machine.rs` and a runtime
  `anyhow!("step did not return the event emitter")` for a type-level invariant. `Emitter`
  is `Clone` and ops clone it internally anyway, so it guarantees nothing. Pass `&Emitter`
  and make the enum `NotApplicable | Applied(StepResult)`.
- `state_machine::usage::estimate_context` is a copy of
  `context_mgmt::count_retained_context_tokens`. Once `retained_context_tokens` is consumed
  again, both paths can share one function.
- `phase1_basic_tools.md` and `test_results.tsv` at the repo root are self-test artifacts.
  Remove them.

## 5. Scheduler tool rename — accepted

Converting the scheduler into a platform extension renames the model-facing tool from
`platform__manage_schedule` to `scheduler__manage_schedule`. `PermissionManager` keys stored
permissions by tool name, so every saved "always allow" for that tool is lost and recipes or
hook matchers naming the old tool stop matching. Accepted as-is — no migration.

## Also outstanding

`cargo clippy --workspace --all-targets -- -D warnings` fails with 8 errors, all inside
`state_machine/**` and all predating this repair: UTF-8 string indexing in `dummy_api.rs`,
`too_many_arguments` on `InferenceRunner::new` and four `dummy_api` helpers, and
`large_enum_variant` on `StateEffect::SetRecipe`. The branch cannot merge past the lint gate
until those are dealt with.

## Verification

After each file:

```bash
cargo test -p goose --test agent --test compaction
cargo test -p goose --lib agents::state_machine
```

Green on all three is the evidence that the rebuild restored what the merge dropped. Then:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test -p goose --no-fail-fast
```

Expect the 4 `jsonwebtoken` lib failures and the 6 network-dependent `tests/providers.rs`
failures to remain; nothing else should fail.
