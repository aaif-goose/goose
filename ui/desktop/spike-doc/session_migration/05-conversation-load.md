# 05 Conversation Load

## Goal

Replace REST `getSession`/`resumeAgent` loading with ACP `session/load`.

This step should not proceed by partially reading `resumeAgent.session` while
also replaying ACP messages. First make ACP load emit enough session metadata
through `session_info_update` for the hook to build its desktop session state.

## Files

- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/acp/sessions.ts`
- `ui/desktop/src/acp/sessionNotificationAdapter.ts`

## Implementation Steps

1. Server first: update Goose ACP `session/load` to emit an initial
   `session_info_update` before replaying conversation updates.

   Minimum metadata:

   - `title`
   - `updatedAt`
   - `_meta.goose.messageCount`
   - `_meta.goose.userSetName`

   Add more `_meta.goose.*` fields only when a desktop load call site needs
   them.

2. Client adapter: parse `session_info_update` into a desktop-facing session
   metadata update.

3. Client adapter: parse `usage_update` into a desktop-facing usage update.

4. In `useChatStream`, resolve the existing session metadata from the session
   list before calling ACP load.

   - Use the session list entry matching `sessionId`.
   - Use that entry's `working_dir` as the ACP `cwd`.
   - Do not use `getInitialWorkingDir()` for existing sessions.
   - Do not pass an arbitrary fallback if the list entry has no working
     directory. Surface a load error instead.

5. Replace the initial REST `resumeAgent` conversation load path with
   `loadAcpSession(sessionId, workingDir)`.

6. Register the ACP notification handler before calling `loadAcpSession` so
   replay notifications are not missed.

7. Scope notifications by `notification.sessionId`. Ignore notifications for
   other active sessions unless the hook is intentionally managing multiple
   sessions.

8. Reset adapter state before loading a different session.

9. Treat the `loadAcpSession` promise as "agent/session setup accepted", not as
   "conversation messages returned". The replayed messages arrive through
   notifications.

10. Preserve the current user-facing states:

   - loading conversation
   - load error
   - idle after replay completes

11. If ACP does not emit an explicit "replay complete" marker, use the
   `loadAcpSession` promise resolution as the point where replay/setup has
   completed enough for the hook to call `onSessionLoaded`.

12. Keep the session-list metadata dependency explicit and temporary until the
   session-list migration moves to ACP or ACP load can safely derive the saved
   working directory server-side.

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
- Passing the wrong `cwd` to ACP `session/load` can overwrite the saved working
  directory for the session in Goose, so missing metadata must not silently fall
  back to app startup working directory.
- If `useChatStream` reads `resumeAgent.session` and ACP replay at the same
  time, the load path has two competing sources of truth. Avoid this except as a
  short-lived experimental bridge.
