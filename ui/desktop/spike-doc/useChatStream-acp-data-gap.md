# useChatStream → ACP Data-Gap Analysis

Scope: what data `ui/desktop/src/hooks/useChatStream.ts` consumes today (via REST `resumeAgent` + SSE), and what's missing if we replace that path with ACP `session/load` + `SessionUpdate` notifications.

## TL;DR

- ACP `session/load` returns a small typed `LoadSessionResponse` and streams the rest via `SessionUpdate` notifications, instead of REST's one-shot full-session response.
- Most chat-critical data is covered by ACP today (conversation replay, usage updates, tool calls, modes, provider/model).
- Six pieces are missing: `working_dir`, accumulated input/output tokens, accumulated cost (schema present, server returns `None`), `recipe`, `user_recipe_values`, extension-load results, and a load-complete signal.
- Most gaps can be patched via `_meta` bags on existing payloads. Two need either new `SessionUpdate` variants or a side-channel notification.

## What `useChatStream` consumes today

### Inputs (hook props)
- `sessionId: string` — drives everything else.

### One-shot load — from `resumeAgent({ session_id, load_model_and_extensions: true })`

Read by the hook / `BaseChat`:

- `session.id, name, working_dir` — identity / layout
- `session.conversation: Message[]` — initial chat history
- `session.recipe`, `session.user_recipe_values` — recipe rendering + param substitution
- `session.total_tokens` — initial context-fill bar
- `session.accumulated_input_tokens` — CostTracker spend chip
- `session.accumulated_output_tokens` — CostTracker spend chip
- `session.accumulated_cost` — CostTracker $ chip
- `session.provider_name`, `session.model_config.model_name` — model/provider chip + pricing lookup
- `session.model_config.context_limit` — context-fill bar `tokenLimit`
- `extension_results` — extension-load error toasts

Carried through but never read:
- `session.input_tokens`, `session.output_tokens`, `session.accumulated_total_tokens`

### Streaming events (SSE) — handled in `createEventProcessor`

| Event | Payload read |
|---|---|
| `Message` | `message: Message`, `token_state.{totalTokens, accumulatedInputTokens, accumulatedOutputTokens, accumulatedCost}` |
| `UpdateConversation` | `conversation: Message[]` (full replace, e.g. after compaction) |
| `Notification` | the event itself (tool-call notifications) |
| `Error` | `error: string` (+ reload trigger on buffer overflow) |
| `Finish` | terminal signal |
| `Ping` | keepalive only |

### Periodic poll — `getSession`

- Only `name` — picked up after the first ~3 user turns to catch the auto-generated session title.

### Outbound writes

| Call | Payload | Trigger |
|---|---|---|
| `sessionReply` | `request_id`, `user_message`, optional `override_conversation` | user submits |
| `sessionCancel` | `request_id` | user stops |
| `forkSession` | `timestamp`, `truncate`, `copy` | edit/fork message |
| `updateSessionUserRecipeValues` | recipe param values | recipe params submit |
| `updateFromSession` | `session_id` | session change → push config |

### Side-channels (not chat data flow)
- `listApps({ session_id })` — fire-and-forget cache warm-up for the apps picker.

### Returns to `BaseChat`
- `session, messages, chatState, setChatState`
- `tokenState` (only 4 of 7 fields read: `totalTokens`, `accumulatedInputTokens`, `accumulatedOutputTokens`, `accumulatedCost`)
- `handleSubmit, submitElicitationResponse, stopStreaming, onMessageUpdate, setRecipeUserParams`
- `sessionLoadError, notifications`

## REST `resumeAgent` vs ACP `session/load`

| Aspect | `resumeAgent` (REST) | ACP `session/load` |
|---|---|---|
| Shape | One synchronous Promise resolves with the whole session row | Small typed `LoadSessionResponse` + streaming `SessionUpdate` notifications |
| Conversation | `session.conversation: Message[]` returned inline | Replayed as `UserMessageChunk` / `AgentMessageChunk` / `ToolCall` / `ToolCallUpdate` notifications |
| Usage | Inline `total_tokens`, `accumulated_*`, `accumulated_cost` | One initial `UsageUpdate` notification (used + size; cost optional) |
| Modes / models | Implicit in `model_config` / `provider_name` | First-class `modes`, `models` on the response |
| Extensions | `extension_results` returned inline | Agent setup happens in background after response; no current surface |
| Load complete | Promise resolution | No explicit signal |

## Gap table — useChatStream needs vs ACP coverage

| useChatStream needs | ACP path | Status | Suggested fix |
|---|---|---|---|
| `session.id` | from request | ✓ | — |
| `session.name` | `SessionInfoUpdate` notification | ✓ async | rely on notification; show placeholder until it arrives |
| `session.working_dir` | only on `session/list` `SessionInfo`, **not on `session/load`** | ✗ gap | add to `LoadSessionResponse._meta.workingDir`, or expose on `SessionInfoUpdate` |
| `session.conversation` (Message[]) | replayed as `*MessageChunk` + `ToolCall*` notifications | ✓ streamed | rebuild `Message[]` client-side from chunk notifications (no server change needed) |
| `session.total_tokens` | `UsageUpdate.used` | ✓ | — |
| `session.accumulated_input_tokens` | — | ✗ gap | emit via `UsageUpdate._meta.accumulatedInputTokens` |
| `session.accumulated_output_tokens` | — | ✗ gap | emit via `UsageUpdate._meta.accumulatedOutputTokens` |
| `session.accumulated_cost` | `UsageUpdate.cost` (schema has it, goose returns `None`) | ⚠ unfilled | populate `cost: Some(Cost { amount, currency })` in `build_usage_update` from `session.accumulated_cost` |
| `session.provider_name` | `LoadSessionResponse.models.current_model_id` (provider derivable from `ModelInfo`) | ✓ first-class | use it directly; no `_meta` hack needed |
| `session.model_config.model_name` | same — `SessionModelState.current_model_id` / `ModelInfo` | ✓ first-class | use it directly |
| `session.model_config.context_limit` | implicitly via `UsageUpdate.size` | ⚠ partial | for context-fill bar use `used / size` directly; if needed independently, populate `ModelInfo._meta.contextLimit` |
| `session.recipe` | — | ✗ gap | add to `LoadSessionResponse._meta.recipe` (full recipe JSON) |
| `session.user_recipe_values` | — | ✗ gap | add to `LoadSessionResponse._meta.userRecipeValues` |
| `extension_results` (load failures) | — | ✗ gap | new typed update `SessionUpdate::ExtensionLoadResult { name, status, error? }`, or surface via `SessionInfoUpdate._meta.extensionResults` once setup finishes |
| Load-complete signal | — | ✗ gap | emit one `SessionInfoUpdate` with `_meta.replayComplete: true` after replay + agent-setup finishes; client uses this to flip `chatState` from `LoadingConversation` to `Idle` |

## Server-side touchpoints

All `_meta`-based fixes live in two functions in `crates/goose/src/acp/server.rs`:

- `session_meta` (line 256) — expand for `session/list` AND add a sibling for `session/load` carrying `working_dir`, `recipe`, `user_recipe_values`, `accumulated_*`.
- `build_usage_update` (line 1042) — enrich to populate `cost` and add `_meta` accumulated fields.

Provider/model fields require no extra work — they're already populated into `SessionModelState` by `prepare_session_init_config` (line ~2869) and returned on `LoadSessionResponse`.

Two structural additions need more work:
- `ExtensionLoadResult` notification — either a new `SessionUpdate` variant (upstream schema PR) or a side-channel via `SessionInfoUpdate._meta`.
- Load-complete signal — same options.

## Client-side touchpoints

For the ACP-based hook:

- Replace `resumeAgent` Promise resolution with: subscribe to notifications → call `session/load` → accumulate chunks/tool-calls into `Message[]` → seed `tokenState` from first `UsageUpdate` → read provider/model from `LoadSessionResponse.models` → wait for `replayComplete` to clear loading state.
- For the context-fill bar, divide `UsageUpdate.used / UsageUpdate.size` directly — no separate model-context-limit lookup needed.
- Drop unused `tokenState.{inputTokens, outputTokens, accumulatedTotalTokens}` from the client's `TokenState` shape since they're no longer in the payload.

## Code dead-weight observations (orthogonal to ACP)

These are unrelated to ACP but worth noting:

- `tokenState.{inputTokens, outputTokens, accumulatedTotalTokens}` are never read by the UI. Pure pass-through. Removable if the server-side openapi schema is trimmed too.
- `updateFromSession` round-trip and `listApps` warm-up are actively used in the REST path; they don't have ACP analogs and don't need to be ported.

## Related findings

- [session-list-token-count-inconsistency.md](./session-list-token-count-inconsistency.md) — `SessionListView` vs `ScheduleDetailView` use different fields for the same "tokens used by this session" UX. Should be unified on `accumulated_total_tokens`.
