# 04 Session Creation

## Goal

Update `ui/desktop/src/sessions.ts` so new sessions use ACP `session/new`.

## Files

- `ui/desktop/src/sessions.ts`
- `ui/desktop/src/acp/sessions.ts`
- Tests that cover session creation

## Implementation Steps

1. Replace REST `startAgent` usage in `createSession` with `createAcpSession`.

2. Keep `startNewSession` and `resumeSession` event dispatch behavior stable:

   - `AppEvents.SESSION_CREATED`
   - `AppEvents.ADD_ACTIVE_SESSION`
   - existing `setView('pair', ...)` behavior

3. Return a desktop-shaped session object compatible with existing callers.

4. Check all `createSession` call sites before switching:

   - launcher/new chat
   - recipe flows
   - session fork/edit flows, if any
   - tests

5. If current callers pass recipe or extension override options, decide whether
   the one PR includes ACP parity for those inputs. If yes, add backend support
   before switching the caller. If no, reduce the migration scope so those entry
   points are not silently broken.

## Parity Checks

Before switching, check whether the current REST creation behavior needs parity
for:

- working directory
- recipe deeplinks
- recipe IDs
- extension overrides

## Completion Criteria

- New desktop sessions are created through ACP.
- Existing session creation events still fire.
- Existing navigation after new session creation still works.
- Unsupported creation options are either implemented in ACP or kept out of the
  migration scope.

## Risks

- Recipes or extension overrides may be dropped if parity is not handled.
- Desktop session list may expect fields that ACP `session/new` does not return
  directly.
