# 06 Live Prompt Streaming

## Goal

Replace REST `sessionReply` plus `useSessionEvents` with ACP `session/prompt`.

## Files

- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/hooks/useSessionEvents.ts`
- `ui/desktop/src/acp/sessions.ts`
- `ui/desktop/src/acp/sessionNotificationAdapter.ts`

## Implementation Steps

1. Remove the request ID creation path for ACP live chat. Do not create
   `request_id` or `chat_request_id` values for ACP.

2. Register the same ACP notification adapter used by conversation load.

3. On submit:

   - append or prepare the user message state as the current UI expects
   - set `ChatState.Streaming`
   - call `promptAcpSession`
   - let ACP notifications update assistant text, thinking, tools, and usage

4. On `promptAcpSession` success:

   - inspect the ACP stop reason
   - clear active prompt state
   - call the existing finish behavior

5. On `promptAcpSession` error:

   - clear active prompt state
   - surface the error through the same UI error path used today

6. Keep notification behavior after task completion:

   - desktop notification setting
   - window focus check
   - `AppEvents.MESSAGE_STREAM_FINISHED`, if still needed

7. Replace REST session-name refresh after early replies with ACP session info
   updates when possible.

8. Remove `useSessionEvents` from the ACP chat path. That hook is REST/SSE
   specific and should not be adapted to ACP.

## Target Flow

```text
user submits message
  -> call ACP session/prompt
  -> receive session/update notifications on the WebSocket
  -> adapter updates Message[] / TokenState / ChatState
  -> prompt response resolves with final stop reason
  -> mark stream finished
```

## Completion Criteria

- Sending a prompt no longer uses REST `sessionReply`.
- Live assistant output streams through ACP notifications.
- REST `useSessionEvents` is not used by the migrated chat path.
- Prompt completion and error handling preserve current UX.

## Risks

- REST reattach semantics do not directly map to ACP.
- Prompt response may resolve after all visible content has already streamed.
- Session name refresh currently tied to REST `getSession` needs ACP replacement.
