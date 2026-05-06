**Phase 5: Decide Chat Session Workflow Failure Policy**

**Status**
- Not started.

**Goal**
- Decide whether chat-session workflow actions should keep optimistic-without-rollback behavior or move to a stronger consistency policy.
- Keep this as a separate behavior-change decision after Phase 3 makes title/project workflow actions explicit.
- Revisit archive/unarchive after Phase 4 clarifies store boundaries, because those workflows still live in `chatSessionStore` today.

**Why This Is Separate**
- Phase 3 removes hidden side effects without changing behavior.
- Failure-policy changes are user-visible and need intentional product/UX decisions.
- Once workflows such as `renameSessionAndPersist` and `updateSessionProjectAndPersist` exist, each policy can be changed in one place.

**Scope**
- Chat-session workflow actions created in Phase 3:
  - rename session
  - update session project
- Chat-session workflow actions to revisit after Phase 4:
  - archive session
  - unarchive session
- User-visible error handling for those workflows, if policy changes require it.
- Pending/error UI for direct user actions such as rename, if needed.
- Focused tests for success and failure behavior.

**Out Of Scope**
- Do not refactor `projectStore`; that remains Phase 6.
- Do not standardize all persistence; that remains Phase 7.
- Do not change unrelated chat send, replay, or model-selection behavior.

**Policy Options To Evaluate**

1. Keep optimistic without rollback.
   - Local state updates immediately.
   - Backend failure is logged.
   - Lowest UI friction, weakest consistency.

2. Backend first.
   - Backend succeeds before local state changes.
   - Strong consistency, slower perceived UI.

3. Optimistic with rollback.
   - Local state updates immediately.
   - Backend failure restores previous local state.
   - Better consistency, more rollback complexity.

4. Optimistic with refresh on failure.
   - Local state updates immediately.
   - Backend failure reloads sessions from backend.
   - Simpler than targeted rollback, but may be heavier.

5. Optimistic with user-visible error.
   - Local state updates immediately.
   - Backend failure leaves local state but tells the user.
   - Makes failure visible without solving consistency.

**Execution Steps**

1. Review Phase 3 workflow actions and call sites.
   - Confirm each workflow has one clear implementation point.
   - Confirm current optimistic-without-rollback behavior is covered by tests.

2. Choose policy per workflow.
   - Rename may be able to use backend-first or rollback.
   - Project assignment may need extra care because it affects sidebar grouping.
   - Archive/unarchive should be evaluated after Phase 4. Current behavior is optimistic without rollback: local visibility changes immediately, backend failures are logged, and local state is not restored. Moving these workflows out of `chatSessionStore` should happen after store boundaries are clearer.

3. Implement one workflow policy at a time.
   - Keep each behavior change small.
   - Add focused failure tests before moving to the next workflow.

4. Add user-visible error handling only if the chosen policy needs it.
   - Use existing toast/error patterns.
   - Avoid noisy errors for background refresh-style recovery.
   - Recheck rename UX. Today `SidebarChatRow` and `SessionCard` close inline edit immediately and invoke a void callback; backend-first rename failure can only be surfaced as a toast unless those components gain pending/error state.

**Validation**
- `cd ui/goose2 && pnpm test -- chatSessionStore useChatSessionController Sidebar`
- Manual smoke: rename chat, move chat to project, archive/unarchive chat, force or mock backend failure where feasible.

**Success Criteria**
- Each chat-session workflow has an explicit failure policy.
- Failure behavior is tested.
- Local/backend consistency behavior is intentional rather than inherited from the old generic `updateSession`.
