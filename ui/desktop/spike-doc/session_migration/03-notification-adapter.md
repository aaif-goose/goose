# 03 Notification Adapter

## Goal

Add a protocol adapter that translates ACP `session/update` notifications into
the current desktop chat state model. Use the session-scoped notification router
from Step 2; this step should focus on conversion logic, not global
registration.

## Files

- `ui/desktop/src/acp/sessionNotificationAdapter.ts`
- Adapter tests under the existing desktop test pattern

## Implementation Steps

1. Use the Step 2 session-scoped router rather than calling
   `setAcpNotificationHandler` directly from React hooks.

2. Register the session-scoped subscription before calling ACP methods that emit
   replay or live notifications, especially `session/load` and `session/prompt`.
   This avoids dropping notifications that arrive while the method promise is
   still pending.

3. Define an adapter state object that stores the current desktop state for one
   session:

   - `messages`
   - `tokenState`
   - `chatState`
   - pending tool call state, if needed
   - latest session metadata, if needed

4. Define a small output event shape for the React hook to consume:

   ```ts
   type AcpDesktopUpdate =
     | { type: 'messages'; messages: Message[] }
     | { type: 'tokenState'; tokenState: TokenState }
     | { type: 'chatState'; chatState: ChatState }
     | { type: 'sessionInfo'; name?: string }
     | { type: 'error'; error: string };
   ```

5. Add a function that accepts ACP `SessionNotification` and returns one or more
   desktop updates:

   ```ts
   applyAcpSessionNotification(notification: SessionNotification): AcpDesktopUpdate[]
   ```

6. Handle replay and live updates through the same conversion path. ACP
   `session/load` sends replayed messages as notifications, and
   `session/prompt` sends live messages the same way.

7. Map `user_message_chunk` into a desktop user `Message`.

8. Map `agent_message_chunk` into an assistant `Message`.

9. Map `agent_thought_chunk` into the desktop thinking content shape.

10. Map `tool_call` into the desktop tool request display shape.

11. Map `tool_call_update` into the matching desktop tool response/update shape.

12. Map `usage_update` into `TokenState`.

13. Map `session_info_update` into session metadata updates, especially session
    name changes.

14. Preserve existing batching behavior where needed. The existing hook batches
    updates for reduced-motion users; either keep that in the hook or expose
    adapter updates in a way that still lets the hook batch them.

15. Add unit tests before the hook is fully migrated. This keeps the riskiest
    conversion logic testable without driving the whole UI.

## Completion Criteria

- `session/load` and `session/prompt` callers can register before notifications
  start arriving by using the Step 2 router.
- ACP notification conversion is isolated from React components.
- Replay and live streaming use the same conversion path.
- Unit tests cover text, thinking, tool calls, tool updates, usage, and session
  info updates.

## Risks

- ACP chunk identity may not map directly to desktop message IDs.
- Tool call update ordering may differ from the REST event model.
- Reduced-motion batching can regress if updates are emitted too eagerly.
- Notifications can still be missed if callers subscribe after invoking
  `session/load` or `session/prompt`.

### Notification Registration Race

ACP `session/load` and `session/prompt` can emit `session/update` notifications
while their method promises are still pending. If the UI registers its
session-scoped notification handler after making the method call, early replay
or streaming chunks can be dropped.

Likely symptoms:

- loading an existing session shows an incomplete or empty conversation
- the first assistant/user chunks of a live reply are missing
- token usage or session name updates do not appear
- the prompt promise resolves, but the visible message state never caught up

Mitigation:

- register the session-scoped subscription before calling `session/load` or
  `session/prompt`
- keep the subscription active until the load/prompt terminal state is known
- add tests that simulate notifications arriving before the method promise
  resolves

### Global Handler Contention

`setAcpNotificationHandler` installs one process-level handler for the shared ACP
WebSocket. Desktop currently keeps multiple chat sessions mounted so they can
stream in the background. If each React hook sets the global ACP handler
directly, the last mounted or last rendered hook wins.

Likely symptoms:

- a hidden/background session stops receiving updates
- a visible session displays updates from another session
- switching sessions changes which chat receives future notifications
- unmounting one chat clears the handler for all active chats

Mitigation:

- call `setAcpNotificationHandler` once from a central router module
- route every notification by `notification.sessionId`
- let each chat hook subscribe/unsubscribe by session ID
- ensure cleanup removes only that hook's subscription, not the global handler

### Subscription Lifetime Leaks

Long-lived mounted sessions and background streams mean stale subscriptions can
accumulate if cleanup is incomplete.

Likely symptoms:

- duplicate message chunks after navigating away and back
- React state updates after a component has unmounted
- memory growth as sessions are opened and closed

Mitigation:

- return and call an unsubscribe function from every subscription
- remove empty handler sets from the router
- make unsubscription idempotent
- add tests for subscribe, dispatch, unsubscribe, and double-unsubscribe

### Load Replay vs Live Stream Ambiguity

The same `session/update` notification shape is used for conversation replay and
live prompt streaming. The adapter must avoid treating replayed historical
messages as a new active stream, while still using the same conversion logic.

Likely symptoms:

- loading history briefly shows streaming/typing state
- replayed tool calls appear as pending approvals
- old messages are appended twice when a session is reloaded

Mitigation:

- keep conversion logic shared, but let the caller provide load vs live context
  for state transitions
- make adapter state scoped to one session and reset it deliberately when
  starting a fresh load
- test replay sequences separately from live prompt sequences

### Out-of-Order or Late Notifications

ACP clients should continue accepting tool call updates after cancellation, and
the server may emit final updates near prompt completion. Notifications may
arrive after the UI has moved to idle, cancelled, or a different route.

Likely symptoms:

- cancelled prompts leave stale pending tool calls
- late tool updates reopen a completed stream
- final chunks are dropped during navigation or cancellation

Mitigation:

- keep session subscriptions alive through cancellation cleanup until terminal
  prompt handling completes
- make terminal-state handling idempotent
- accept late updates for the matching session while ignoring updates for
  unsubscribed sessions
