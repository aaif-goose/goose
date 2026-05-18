# 05 Conversation Load

## Goal

Replace REST `getSession`/`resumeAgent` loading with ACP `session/load`.

## Files

- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/acp/sessions.ts`
- `ui/desktop/src/acp/sessionNotificationAdapter.ts`

## Implementation Steps

1. In `useChatStream`, replace the initial REST load path with
   `loadAcpSession(sessionId, workingDir)`.

2. Register the ACP notification handler before calling `loadAcpSession` so
   replay notifications are not missed.

3. Scope notifications by `notification.sessionId`. Ignore notifications for
   other active sessions unless the hook is intentionally managing multiple
   sessions.

4. Reset adapter state before loading a different session.

5. Treat the `loadAcpSession` promise as "agent/session setup accepted", not as
   "conversation messages returned". The replayed messages arrive through
   notifications.

6. Preserve the current user-facing states:

   - loading conversation
   - load error
   - idle after replay completes

7. If ACP does not emit an explicit "replay complete" marker, use the
   `loadAcpSession` promise resolution as the point where replay/setup has
   completed enough for the hook to call `onSessionLoaded`.

8. Replace any post-load `getSession` dependency with metadata from ACP
   notifications or a dedicated ACP-backed metadata wrapper.

## Behavior To Preserve

- loading state
- session load errors
- initial conversation display
- token state
- tool call history
- session name/info

## Completion Criteria

- Loading an existing conversation no longer uses REST session load APIs.
- ACP replayed updates produce the same visible conversation state.
- `onSessionLoaded` still runs at the expected time.

## Risks

- Replay notifications can be missed if handler registration happens too late.
- ACP may not have an explicit replay-complete signal.
- Session metadata refresh may still depend on REST until replaced.
