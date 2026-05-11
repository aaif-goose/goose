**Phase 3: Separate Chat Session Side Effects From Generic Store Actions**

**Status**
- Complete.

**Goal**
- Make session mutation semantics explicit.
- Ensure generic local patch actions do not hide backend writes.
- Use backend-first operations for persisted session fields so local state is patched only after backend success.
- Keep this phase focused on side-effect semantics even though Phase 2 identified broader `chatSessionStore` responsibility overload.
- Create clearly named operation functions so backend failure policy is centralized and visible.

**Scope**
- `ui/goose2/src/features/chat/stores/chatSessionStore.ts`
- `ui/goose2/src/features/chat/hooks/useChat.ts`
- `ui/goose2/src/features/chat/hooks/useResolvedAgentModelPicker.ts`
- `ui/goose2/src/features/chat/hooks/useChatSessionController.ts`
- New `ui/goose2/src/features/chat/stores/chatSessionOperations.ts`

**Out Of Scope**
- Do not split `chatSessionStore` UI state yet.
- Do not refactor `projectStore`.
- Do not change ACP API behavior.
- Do not change title generation, archive, unarchive, or project assignment UX.
- Do not solve `chatSessionStore` context-panel or workspace UI ownership here; that belongs in Phase 4.
- Do not introduce rollback, refresh-on-failure, or broad user-visible error policy in this phase.
- Do not use `commands` naming for the new module, because Tauri backend commands already use that term.

**Execution Steps**

1. Identify all `updateSession` call sites.
   - Classify each call as local-only, title rename, project update, timestamp update, provider/model update, archive-related, or other.
   - Keep a note of calls that also reveal broad `chatSessionStore` ownership, but do not expand this phase beyond mutation semantics.
   - Current classification:
     - manual title update: `AppShell.tsx` should move to `updateSessionTitle`.
     - project assignment: `AppShell.tsx` and `useChatSessionController.ts` should move to `updateSessionProject`.
     - backend/session-info title metadata: `acpSessionInfoUpdate.ts` should be local-only `patchSession`.
     - generated local title, timestamps, provider/model/persona metadata, ACP notification model updates, and test setup should be local-only `patchSession`.
     - mixed pending patch in `useChatSessionController.ts` should split `projectId` into `updateSessionProject` and keep provider/persona/model fields as `patchSession`.

2. Add a local-only action.
   - Introduce `patchSession` in `chatSessionStore`.
   - It should only update Zustand state.
   - It should not inspect patch fields for backend meaning.
   - Keep the existing shallow-merge local update behavior exactly the same for now.

3. Keep compatibility while migrating.
   - Temporarily keep `updateSession`.
   - Have `updateSession` delegate to `patchSession` only after persisted call sites have moved to operations.
   - Remove `updateSession` when no production or test call sites remain.

4. Add explicit operation functions.
   - Add `chatSessionOperations.ts`.
   - Add `updateSessionTitle(sessionId, title)`.
     - Call backend rename first.
     - After success, call `patchSession(sessionId, { title, userSetName: true })`.
     - Let errors throw so UI call sites can decide how to surface failure.
   - Add `updateSessionProject(sessionId, projectId)`.
     - Call backend project update first.
     - After success, call `patchSession(sessionId, { projectId })`.
     - Let errors throw so UI call sites can decide how to surface failure.
   - Import backend API functions with aliases if needed to avoid name conflicts, for example `renameSession as updateSessionTitleApi`.
   - Leave archive/unarchive in the store initially unless migration remains simple after title/project cleanup.

5. Migrate call sites one category at a time.
   - Timestamp-only and UI/local patches should use `patchSession`.
   - Manual title rename should call `updateSessionTitle`.
   - Project assignment should call `updateSessionProject`.
   - Backend/session-info updates should use `patchSession` only, even when the patch includes `title` or `userSetName`.
   - `userSetName` alone should never imply a backend call.
   - Archive flows can stay on existing explicit `archiveSession` / `unarchiveSession` until title/project ambiguity is gone.

6. Remove hidden side effects from `updateSession`.
   - Once call sites are migrated, remove `updateSession`.
   - Avoid leaving two names with different implied semantics.

7. Add focused tests.
   - `patchSession` does not call backend APIs.
   - `updateSessionTitle` calls backend rename before patching local state.
   - `updateSessionTitle` does not patch local state if backend rename rejects.
   - `updateSessionProject` calls backend project update before patching local state.
   - `updateSessionProject` does not patch local state if backend project update rejects.
   - Backend/session-info updates with `title` and/or `userSetName` are local-only.

8. Record failure-policy follow-up.
   - After this phase, evaluate whether archive and unarchive should also move to operation functions and whether persisted operations need user-visible error handling.
   - Track that decision in `phase-5-session-workflow-failure-policy.md`.

**Validation**
- `rg "updateSession\\(" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- chatSessionStore useChat useChatSessionController`

**Success Criteria**
- Generic local session patching is local-only.
- Backend writes happen through explicitly named operations.
- Existing chat rename, project update, archive, and unarchive flows still work.
