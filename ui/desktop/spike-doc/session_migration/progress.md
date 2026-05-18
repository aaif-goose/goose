# ACP Session Migration Progress

## Status

Current status: step 1 complete; next slice is text-only conversation load.

Owner: Codex

Last updated: 2026-05-18

Plan: `ui/desktop/spike-doc/session_migration/acp-session-migration-plan.md`

Working agreement: `ui/desktop/spike-doc/session_migration/working-agreement.md`

## Progress Checklist

### 1. Harden ACP Client

Detailed plan: `01-harden-acp-client.md`

- [x] Add `AcpNotificationHandler` interface.
- [x] Add `setAcpNotificationHandler(...)`.
- [x] Route ACP `sessionUpdate` notifications to the registered handler.
- [x] Add ACP permission handler interface.
- [x] Add `setAcpPermissionHandler(...)`.
- [x] Replace silent always-cancel permission behavior before live chat is enabled.
- [x] Confirm ACP reconnect still clears cached client state.

Notes:

- Updated `ui/desktop/src/acp/acpConnection.ts`.
- No behavior migration yet; Step 1 only adds integration points.
- If no permission handler is registered, ACP permission requests now warn before
  returning `cancelled`.
- Reconnect cleanup remains unchanged.
- Ran `pnpm exec prettier --write src/acp/acpConnection.ts`.
- Ran `pnpm run typecheck`; passed with the existing Node engine warning
  (`^24.10.0` wanted, shell has `v25.8.1`).

### 2. Text-Only Conversation Load Slice

Detailed plans: `02-acp-session-wrapper.md`, `03-notification-adapter.md`,
`05-conversation-load.md`

#### 2A. Minimal Session API Wrapper

- [x] Create `ui/desktop/src/acp/sessions.ts`.
- [x] Add `loadAcpSession`.
- [x] Keep load response narrow; do not infer messages from the direct response.
- [x] Keep wrapper free of React state and UI side effects.

#### 2B. Session Notification Router

- [ ] Create `ui/desktop/src/acp/sessionNotificationRouter.ts`, or equivalent.
- [ ] Register one global ACP notification router with
  `setAcpNotificationHandler`.
- [ ] Add session-scoped subscription/unsubscription routed by ACP `sessionId`.
- [ ] Add router tests for session routing, multiple subscribers, unsubscribe,
  double-unsubscribe, and no-subscriber dispatch.

#### 2C. Minimal Text Adapter

- [ ] Create `ui/desktop/src/acp/sessionNotificationAdapter.ts`.
- [ ] Convert `user_message_chunk`.
- [ ] Convert `agent_message_chunk`.
- [ ] Convert minimal `session_info_update`, if needed for load.
- [ ] Add adapter tests for text-only replay.

#### 2D. Load Hook Integration

- [ ] Register session-scoped notification subscription before `session/load`.
- [ ] Replace REST `resumeAgent` loading for the text-only load path.
- [ ] Preserve loading conversation state.
- [ ] Preserve session load error state.
- [ ] Call `onSessionLoaded` at the correct point.
- [ ] Manually verify an existing text-only session loads through ACP.

Notes:

- This is the first true vertical slice. It should prove wrapper, router,
  adapter, and React load state together for existing text-only sessions.
- 2A added only `loadAcpSession(sessionId, workingDir)`. It calls ACP
  `session/load` with `sessionId`, `cwd`, and `mcpServers: []`, then returns the
  ACP `LoadSessionResponse` directly. Conversation content still must come from
  `session/update` notifications in later 2B-2D work.
- Ran `pnpm exec prettier --write src/acp/sessions.ts`.
- Ran `pnpm run typecheck`; passed with the existing Node engine warning
  (`^24.10.0` wanted, shell has `v25.8.1`).

### 3. Live Text Prompt Slice

Detailed plans: `02-acp-session-wrapper.md`, `03-notification-adapter.md`,
`06-live-prompt-streaming.md`

- [ ] Add `promptAcpSession`.
- [ ] Reuse the session-scoped router for live prompt notifications.
- [ ] Extend adapter behavior as needed for live text accumulation.
- [ ] Replace REST `sessionReply` for text prompt path.
- [ ] Set streaming state on submit.
- [ ] Handle successful ACP prompt completion.
- [ ] Handle ACP prompt errors.
- [ ] Preserve task completion desktop notification behavior.
- [ ] Preserve `AppEvents.MESSAGE_STREAM_FINISHED`, if still needed.
- [ ] Replace REST session-name refresh with ACP session info handling.
- [ ] Manually verify a text prompt streams through ACP.

Notes:

- TBD

### 4. Cancellation Slice

Detailed plan: `07-cancellation.md`

- [ ] Add `cancelAcpSession`.
- [ ] Replace REST `sessionCancel` with ACP `session/cancel`.
- [ ] Track ACP active prompt state.
- [ ] Make stop no-op when there is no active prompt.
- [ ] Clear active prompt refs on cancel.
- [ ] Return UI to idle or cancelled state.
- [ ] Verify cancellation does not leave stale loading/thinking state.

Notes:

- TBD

### 5. Tool Call Display Slice

Detailed plan: `03-notification-adapter.md`

- [ ] Convert `agent_thought_chunk`.
- [ ] Convert `tool_call`.
- [ ] Convert `tool_call_update`.
- [ ] Convert `usage_update`.
- [ ] Preserve token state after load and live prompt.
- [ ] Preserve tool call history after load.
- [ ] Preserve or support reduced-motion batching.
- [ ] Add adapter tests for thinking, tool calls, tool updates, and usage.

Notes:

- TBD

### 6. Tool Permission Slice

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

- Updated the Step 8 plan with context for permission handler readiness and
  permission request lifetime risks.

### 7. Session Creation Slice

Detailed plan: `04-session-creation.md`

- [ ] Add `createAcpSession`.
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

### 8. Optional Session List Slice

Detailed plan: `02-acp-session-wrapper.md`

- [ ] Add `listAcpSessions`, if included in this migration.
- [ ] Map ACP `SessionInfo` into the shape consumed by desktop session list UI.
- [ ] Audit whether session list should remain REST for the first migration PR.

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

- [ ] Load an existing session with plain text.
- [ ] Send a prompt and receive streamed assistant text.
- [ ] Cancel a running prompt.
- [ ] Load an existing session with thinking content.
- [ ] Load an existing session with tool calls.
- [ ] Send a prompt that triggers tool approval.
- [ ] Approve a tool request.
- [ ] Reject a tool request.
- [ ] Create a new session.
- [ ] Navigate away from and back to an active session.
- [ ] Navigate away from and back to a completed session.
- [ ] Confirm session name updates still appear.

Automated:

- [ ] Router test for session-scoped dispatch.
- [ ] Router test for multiple subscribers.
- [ ] Router test for unsubscribe and double-unsubscribe.
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
