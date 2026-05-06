**Phase 3: Separate Chat Session Side Effects From Generic Store Actions**

**Status**
- Not started.

**Goal**
- Make session mutation semantics explicit.
- Ensure generic local patch actions do not hide backend writes.
- Preserve existing user-visible behavior while changing ownership boundaries.

**Scope**
- `ui/goose2/src/features/chat/stores/chatSessionStore.ts`
- `ui/goose2/src/features/chat/hooks/useChat.ts`
- `ui/goose2/src/features/chat/hooks/useResolvedAgentModelPicker.ts`
- `ui/goose2/src/features/chat/hooks/useChatSessionController.ts`
- New command/hook/API wrapper files if needed.

**Out Of Scope**
- Do not split `chatSessionStore` UI state yet.
- Do not refactor `projectStore`.
- Do not change ACP API behavior.
- Do not change title generation, archive, unarchive, or project assignment UX.

**Execution Steps**

1. Identify all `updateSession` call sites.
   - Classify each call as local-only, title rename, project update, timestamp update, provider/model update, archive-related, or other.

2. Add a local-only action.
   - Introduce a clearly named store action such as `patchSessionLocal`.
   - It should only update Zustand state.
   - Keep the existing local update behavior exactly the same.

3. Keep compatibility while migrating.
   - Temporarily keep `updateSession`.
   - Have `updateSession` delegate to `patchSessionLocal` and keep existing side effects until call sites are moved.
   - This keeps the first commit low risk.

4. Add explicit orchestration functions.
   - Add clearly named operations such as:
     - `renameSessionAndPersist`
     - `updateSessionProjectAndPersist`
     - `archiveSessionAndPersist`
     - `unarchiveSessionAndPersist`
   - Keep backend-facing calls in API or command-style modules, not hidden in generic store actions.

5. Migrate call sites one category at a time.
   - Timestamp-only and UI/local patches should use `patchSessionLocal`.
   - Rename flows should call the explicit rename operation.
   - Project assignment flows should call the explicit project update operation.
   - Archive flows should call explicit archive/unarchive operations.

6. Remove hidden side effects from `updateSession`.
   - Once call sites are migrated, either remove `updateSession` or make it an alias for local-only patching with a clear name.
   - Avoid leaving two names with different implied semantics.

7. Add focused tests.
   - Local patch does not call backend APIs.
   - Rename operation patches local state and calls backend rename.
   - Project update operation patches local state and calls backend project update.
   - Archive and unarchive preserve existing behavior.

**Validation**
- `rg "updateSession\\(" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- chatSessionStore useChat useChatSessionController`

**Success Criteria**
- Generic local session patching is local-only.
- Backend writes happen through explicitly named operations.
- Existing chat rename, project update, archive, and unarchive flows still work.
