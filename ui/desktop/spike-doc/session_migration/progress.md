# ACP Session Migration Progress

## Status

Current status: not started.

Owner: TBD

Last updated: TBD

Plan: `ui/desktop/spike-doc/session_migration/acp-session-migration-plan.md`

## Progress Checklist

### 1. Harden ACP Client

Detailed plan: `01-harden-acp-client.md`

- [ ] Add `AcpNotificationHandler` interface.
- [ ] Add `setAcpNotificationHandler(...)`.
- [ ] Route ACP `sessionUpdate` notifications to the registered handler.
- [ ] Add ACP permission handler interface.
- [ ] Add `setAcpPermissionHandler(...)`.
- [ ] Replace silent always-cancel permission behavior before live chat is enabled.
- [ ] Confirm ACP reconnect still clears cached client state.

Notes:

- TBD

### 2. Add ACP Session API Wrapper

Detailed plan: `02-acp-session-wrapper.md`

- [ ] Create `ui/desktop/src/acp/sessions.ts`.
- [ ] Add `createAcpSession`.
- [ ] Add `loadAcpSession`.
- [ ] Add `promptAcpSession`.
- [ ] Add `cancelAcpSession`.
- [ ] Add `listAcpSessions`, if included in this migration.
- [ ] Normalize ACP responses into desktop-facing types.
- [ ] Keep wrapper free of React state and UI side effects.

Notes:

- TBD

### 3. Add ACP Notification Adapter

Detailed plan: `03-notification-adapter.md`

- [ ] Create `ui/desktop/src/acp/sessionNotificationAdapter.ts`.
- [ ] Define adapter state for one session.
- [ ] Define desktop update event shape.
- [ ] Convert `user_message_chunk`.
- [ ] Convert `agent_message_chunk`.
- [ ] Convert `agent_thought_chunk`.
- [ ] Convert `tool_call`.
- [ ] Convert `tool_call_update`.
- [ ] Convert `usage_update`.
- [ ] Convert `session_info_update`.
- [ ] Preserve or support reduced-motion batching.
- [ ] Add adapter unit tests for representative update sequences.

Notes:

- TBD

### 4. Migrate Session Creation

Detailed plan: `04-session-creation.md`

- [ ] Replace REST `startAgent` usage in `createSession`.
- [ ] Preserve `AppEvents.SESSION_CREATED`.
- [ ] Preserve `AppEvents.ADD_ACTIVE_SESSION`.
- [ ] Preserve `setView('pair', ...)` behavior.
- [ ] Verify launcher/new chat flow.
- [ ] Verify recipe-related creation paths.
- [ ] Verify extension override creation paths.
- [ ] Add backend ACP parity if required for recipes or extension overrides.

Notes:

- TBD

### 5. Migrate Conversation Load

Detailed plan: `05-conversation-load.md`

- [ ] Register ACP notification handler before `session/load`.
- [ ] Replace REST `getSession`/`resumeAgent` loading path.
- [ ] Scope notifications by ACP `sessionId`.
- [ ] Reset adapter state when session changes.
- [ ] Use `session/load` notifications for conversation replay.
- [ ] Preserve loading conversation state.
- [ ] Preserve session load error state.
- [ ] Preserve token state after load.
- [ ] Preserve tool call history after load.
- [ ] Preserve session metadata/name after load.
- [ ] Call `onSessionLoaded` at the correct point.

Notes:

- TBD

### 6. Migrate Live Prompt Streaming

Detailed plan: `06-live-prompt-streaming.md`

- [ ] Remove REST request ID routing from migrated chat flow.
- [ ] Replace REST `sessionReply` with ACP `session/prompt`.
- [ ] Set streaming state on submit.
- [ ] Feed live ACP notifications through the adapter.
- [ ] Handle successful ACP prompt completion.
- [ ] Handle ACP prompt errors.
- [ ] Preserve task completion desktop notification behavior.
- [ ] Preserve `AppEvents.MESSAGE_STREAM_FINISHED`, if still needed.
- [ ] Replace REST session-name refresh with ACP session info handling.
- [ ] Remove `useSessionEvents` from the migrated chat path.

Notes:

- TBD

### 7. Migrate Cancellation

Detailed plan: `07-cancellation.md`

- [ ] Replace REST `sessionCancel` with ACP `session/cancel`.
- [ ] Track ACP active prompt state.
- [ ] Make stop no-op when there is no active prompt.
- [ ] Clear active prompt refs on cancel.
- [ ] Return UI to idle or cancelled state.
- [ ] Verify cancellation does not leave stale loading/thinking state.

Notes:

- TBD

### 8. Wire Tool Permission Requests

Detailed plan: `08-tool-permissions.md`

- [ ] Define permission request UI state.
- [ ] Bridge ACP `requestPermission` into chat UI.
- [ ] Render tool approval request to user.
- [ ] Map approve-once decision to ACP selected outcome.
- [ ] Map approve-always decision to ACP selected outcome, if option exists.
- [ ] Map reject-once decision to ACP selected outcome.
- [ ] Map reject-always decision to ACP selected outcome, if option exists.
- [ ] Map dismiss/cancel to ACP cancelled outcome.
- [ ] Avoid assuming fixed ACP option IDs unless backend guarantees them.
- [ ] Verify approved tool call continues.
- [ ] Verify rejected tool call is handled cleanly.

Notes:

- TBD

### 9. Remove Desktop REST Session Usage

Detailed plan: `09-rest-cleanup.md`

- [ ] Search for session REST imports in `ui/desktop/src`.
- [ ] Remove replaced `sessionReply` usage.
- [ ] Remove replaced `sessionCancel` usage.
- [ ] Remove replaced `sessionEvents` usage.
- [ ] Remove replaced `resumeAgent` usage.
- [ ] Remove replaced `getSession` usage for migrated chat loading.
- [ ] Remove replaced `startAgent` usage.
- [ ] Update tests that mocked REST session APIs.
- [ ] Keep unrelated REST APIs untouched.
- [ ] Do not manually edit `ui/desktop/openapi.json`.

Notes:

- TBD

## Verification Checklist

Manual:

- [ ] Create a new session.
- [ ] Load an existing session with plain text.
- [ ] Load an existing session with thinking content.
- [ ] Load an existing session with tool calls.
- [ ] Send a prompt and receive streamed assistant text.
- [ ] Send a prompt that triggers tool approval.
- [ ] Approve a tool request.
- [ ] Reject a tool request.
- [ ] Cancel a running prompt.
- [ ] Navigate away from and back to an active session.
- [ ] Navigate away from and back to a completed session.
- [ ] Confirm session name updates still appear.

Automated:

- [ ] Adapter test for text chunk accumulation.
- [ ] Adapter test for thinking chunk conversion.
- [ ] Adapter test for tool call conversion.
- [ ] Adapter test for tool call update conversion.
- [ ] Adapter test for usage update conversion.
- [ ] Adapter test for session info update conversion.
- [ ] Permission mapping test for approve.
- [ ] Permission mapping test for reject.
- [ ] Permission mapping test for cancel.

## Blockers

- None recorded.

## Decisions

- No feature flag for migrated session behavior.
- No automatic REST fallback for ACP chat errors.
- REST remains only for session behavior not migrated yet.
- Extension migration is out of scope for this plan.

## Follow-Up

Track work that should not block the first ACP session migration PR.

- [ ] Remove server REST session endpoints after desktop no longer depends on them.
- [ ] Decide whether ACP should support full recipe deeplink/session creation parity.
- [ ] Decide whether ACP should support extension override inputs during session creation.
- [ ] Audit session list migration if it is not included in the first PR.
- [ ] Replace any remaining REST session metadata refresh with ACP metadata APIs.
- [ ] Review whether ACP needs an explicit replay-complete notification for `session/load`.
- [ ] Review reattach semantics for prompts that continue while the view is remounted.
- [ ] Document final `goosed` bridge removal requirements once desktop session/chat is ACP-backed.
- [ ] Add broader integration tests after the adapter and hook migration settle.
