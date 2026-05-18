# 02 ACP Session API Wrapper

## Goal

Add `ui/desktop/src/acp/sessions.ts` with desktop-facing wrappers for ACP
session methods.

## Files

- `ui/desktop/src/acp/sessions.ts`
- `ui/desktop/src/acp/acpConnection.ts`

## Implementation Steps

1. Import `getAcpClient` from `ui/desktop/src/acp/acpConnection.ts`.

2. Define local desktop-facing types where raw ACP types do not match existing
   desktop expectations. Keep these narrow; do not recreate the whole REST API
   type surface.

3. Add `createAcpSession(workingDir, options)`:

   - Calls `client.newSession`.
   - Sends `cwd: workingDir`.
   - Sends `mcpServers: []` initially.
   - Sends `_meta.client = 'goose'`.
   - Adds future metadata only when needed and supported by ACP.

4. Add `loadAcpSession(sessionId, workingDir)`:

   - Calls `client.loadSession`.
   - Sends `sessionId`, `cwd: workingDir`, and `mcpServers: []`.
   - Does not expect conversation messages in the direct response. Conversation
     replay arrives through ACP `session/update`.

5. Add `promptAcpSession(sessionId, userInput, meta?)`:

   - Converts desktop user input into ACP `ContentBlock[]`.
   - Calls `client.prompt`.
   - Lets notifications drive the live UI state while the promise is pending.
   - Uses the final `PromptResponse` only to determine terminal state and usage
     details that are not already covered by notifications.

6. Add `cancelAcpSession(sessionId)`:

   - Calls `client.cancel({ sessionId })`.

7. Add `listAcpSessions()` only if session-list migration is included in the
   same PR:

   - Calls `client.listSessions({})`.
   - Maps ACP `SessionInfo` into the shape consumed by desktop session list UI.

8. Keep the wrapper free of React state and UI side effects.

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

- Desktop code can call ACP session methods through one wrapper module.
- UI hooks do not need raw ACP response details.
- The wrapper exposes no REST fallback.

## Risks

- Recipe and extension override inputs may not have ACP parity yet.
- `session/load` replay notifications may be missed if handlers are registered
  too late.
