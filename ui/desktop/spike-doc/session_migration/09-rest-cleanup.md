# 09 REST Cleanup

## Goal

After the ACP flow works, remove unused desktop REST session imports and code
paths.

## Files

- `ui/desktop/src/hooks/useChatStream.ts`
- `ui/desktop/src/hooks/useSessionEvents.ts`
- `ui/desktop/src/sessions.ts`
- Tests that mock REST session APIs

## Implementation Steps

1. Search desktop source for REST session imports:

   ```bash
   rg "sessionReply|sessionCancel|sessionEvents|resumeAgent|getSession|startAgent" ui/desktop/src
   ```

2. Remove only imports and code paths that are replaced by ACP in this PR.

3. Keep unrelated REST APIs, including extension mutations and other non-session
   surfaces.

4. Do not manually edit generated OpenAPI files.

5. Leave server REST endpoints in place unless the PR also proves no desktop
   runtime usage remains and removing them is low risk.

6. Update tests that mocked REST session APIs so they mock the ACP wrapper or
   adapter instead.

## Completion Criteria

- Migrated desktop session/chat behavior no longer imports REST session APIs.
- Unrelated REST APIs are untouched.
- Generated OpenAPI files are untouched.
- Tests no longer mock removed REST calls.

## Risks

- Removing a REST import used by an unmigrated surface.
- Accidentally editing generated OpenAPI files.
- Server endpoint removal expanding the PR beyond the desktop migration.
