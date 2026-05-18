# 02 ACP Session API Wrapper and Notification Router

## Goal

Add `ui/desktop/src/acp/sessions.ts` incrementally with the desktop-facing ACP
session wrappers needed by each vertical slice. Add the session-scoped
notification router needed to consume ACP `session/update` notifications safely
from multiple mounted chat sessions.

This step is an integration foundation, not an end-to-end chat migration. It can
prove TypeScript compatibility, ACP method shape, and notification routing
behavior in isolation. It cannot prove conversation load or live prompt behavior
until the adapter and hooks consume notifications in later steps.

## Files

- `ui/desktop/src/acp/sessions.ts`
- `ui/desktop/src/acp/sessionNotificationRouter.ts`
- `ui/desktop/src/acp/acpConnection.ts`
- Router tests under the existing desktop test pattern

## Implementation Steps

### 2A. Session API Wrapper

1. Import `getAcpClient` from `ui/desktop/src/acp/acpConnection.ts`.

2. Define local desktop-facing types where raw ACP types do not match existing
   desktop expectations. Keep these narrow; do not recreate the whole REST API
   type surface.

3. First vertical slice: add `loadAcpSession(sessionId, workingDir)`:

   - Calls `client.loadSession`.
   - Sends `sessionId`, `cwd: workingDir`, and `mcpServers: []`.
   - Does not expect conversation messages in the direct response. Conversation
     replay arrives through ACP `session/update`.

4. Later live prompt slice: add `promptAcpSession(sessionId, userInput, meta?)`:

   - Converts desktop user input into ACP `ContentBlock[]`.
   - Calls `client.prompt`.
   - Lets notifications drive the live UI state while the promise is pending.
   - Uses the final `PromptResponse` only to determine terminal state and usage
     details that are not already covered by notifications.

5. Later cancellation slice: add `cancelAcpSession(sessionId)`:

   - Calls `client.cancel({ sessionId })`.

6. Later session creation slice: add `createAcpSession(workingDir, options)`:

   - Calls `client.newSession`.
   - Sends `cwd: workingDir`.
   - Sends `mcpServers: []` initially.
   - Sends `_meta.client = 'goose'`.
   - Adds future metadata only when needed and supported by ACP.

7. Optional session list slice: add `listAcpSessions()` only if session-list
   migration is included in the same PR:

   - Calls `client.listSessions({})`.
   - Maps ACP `SessionInfo` into the shape consumed by desktop session list UI.

8. Keep the wrapper free of React state and UI side effects.

9. Treat wrapper verification as limited:

   - TypeScript confirms method names and request/response shapes.
   - Unit tests can mock `getAcpClient` and verify the wrapper calls ACP methods
     with the expected params.
   - This does not prove that `session/load` replay or `session/prompt`
     streaming works, because their useful data arrives through
     `session/update` notifications handled in later steps.

### 2B. Session Notification Router

10. Add one global ACP notification router and register it once with
    `setAcpNotificationHandler`.

11. Expose a session-scoped subscription API, for example:

    ```ts
    subscribeToAcpSession(
      sessionId: string,
      handler: (notification: SessionNotification) => Promise<void> | void
    ): () => void
    ```

    The exact function name can change, but the behavior should be:

    - store handlers by `notification.sessionId`
    - dispatch each ACP `session/update` only to subscribers for that session
    - return an unsubscribe function
    - remove empty session entries during cleanup
    - make unsubscribe idempotent

12. Do not let individual React chat hooks call `setAcpNotificationHandler`
    directly. Desktop can keep multiple chat sessions mounted at the same time,
    so a single global handler would cause the last mounted hook to win.

13. Add focused router tests:

    - dispatches a notification only to matching `sessionId` subscribers
    - supports multiple subscribers for one session
    - does not dispatch after unsubscribe
    - double-unsubscribe is harmless
    - notifications with no subscribers are ignored

## Session Creation Metadata

Session creation should set desktop metadata:

```ts
{
  cwd: workingDir,
  mcpServers: [],
  _meta: {
    client: 'goose'
  }
}
```

The `_meta.client` value matters because the ACP backend uses it to create a
desktop/user session rather than a programmatic ACP session.

## Completion Criteria

- Desktop code can call the ACP session methods needed by the current vertical
  slice through one wrapper module.
- ACP has one global notification handler that routes by `sessionId`.
- Multiple mounted chat sessions can subscribe without overwriting each other.
- UI hooks do not need raw ACP response details.
- The wrapper exposes no REST fallback.
- Verification expectations are clear: Step 2 proves wrapper and router
  mechanics, not end-to-end chat behavior.

## Risks

- Recipe and extension override inputs may not have ACP parity yet.
- `session/load` replay notifications may be missed if handlers are registered
  too late.
- Wrapper methods can typecheck while still being behaviorally unproven until
  adapter and hook migration consumes notifications.
- If React hooks use `setAcpNotificationHandler` directly instead of the router,
  mounted sessions can overwrite each other's notification handling.
