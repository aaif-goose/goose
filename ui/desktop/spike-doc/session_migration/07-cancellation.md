# 07 Cancellation

## Goal

Replace REST `sessionCancel` with ACP `session/cancel`.

## Files

- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/acp/sessions.ts`

## Implementation Steps

1. Replace `sessionCancel` imports in chat flow with `cancelAcpSession`.

2. Track only ACP active prompt state:

   - active session ID
   - active prompt pending flag
   - optional abort/cancel UI state

3. On stop:

   - call `cancelAcpSession(sessionId)`
   - clear active prompt refs
   - set chat state back to idle or cancelled, matching existing UX

4. Handle idempotency: if there is no active prompt, stop should be a no-op.

5. Confirm a cancelled ACP prompt does not leave stale loading, thinking, or
   waiting-for-user-input UI.

## Completion Criteria

- Stop button uses ACP cancel.
- Cancelling an active prompt returns the UI to a stable state.
- Calling stop without an active prompt is harmless.

## Risks

- Prompt promise and cancel notification may race.
- The UI may briefly show stale streaming state after cancellation.
