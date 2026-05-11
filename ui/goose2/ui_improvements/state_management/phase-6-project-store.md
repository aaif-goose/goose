**Phase 6: Refactor Project Store Into Clearer Layers**

**Status**
- Not started.

**Goal**
- Reduce `projectStore` responsibility overload.
- Make API orchestration, local state transitions, cache behavior, and mutation policy easier to reason about.
- Address Phase 2 findings that project reads are common but the bigger issue is project workflow ownership, not selector syntax.
- Revisit project-heavy component responsibility after project-store ownership is clearer.

**Scope**
- `ui/goose2/src/features/projects/stores/projectStore.ts`
- New project command or hook module, such as `useProjectCommands.ts` or `projectCommands.ts`
- Project consumers such as `ProjectsView.tsx`, `Sidebar.tsx`, and `SettingsModal.tsx`

**Out Of Scope**
- Do not standardize all persistence yet.
- Do not change project API contracts.
- Do not redesign project UI.
- Do not change reorder UX unless the current behavior is clearly buggy.

**Execution Steps**

1. Document the current mutation policy.
   - Include the fact that `AppShell`, `Sidebar`, and project views depend on project list lookup by id, while `projectStore` also owns API orchestration and cache persistence.
   - `fetchProjects`: pessimistic load with cached bootstrap.
   - `addProject`, `editProject`, `removeProject`: backend first, local update after success.
   - `reorderProjects`: local optimistic reorder, persist cache, fire backend reorder without rollback.

2. Add local state actions.
   - Introduce local-only store actions with explicit names, for example:
     - `setProjectsLocal`
     - `upsertProjectLocal`
     - `removeProjectLocal`
     - `reorderProjectsLocal`
     - `setProjectsLoading`
     - `setActiveProject`
   - Keep old public actions temporarily if migration needs to be incremental.

3. Add project commands.
   - Move backend workflows into a command-style module or hook.
   - Commands can call project API modules and then local store actions.
   - Keep UI call sites simple.

4. Migrate consumers gradually.
   - Move `fetchProjects`, `addProject`, `editProject`, and `removeProject` consumers to commands.
   - Migrate reorder last because it has the least explicit failure behavior today.

5. Recheck project-related component responsibility.
   - `AppShell` currently orchestrates project fetch, project creation/edit/archive callbacks, project selection for new chats, sidebar project actions, and chat/session setup. After project-store policy is clarified, decide whether some project orchestration should move into a project controller hook or smaller app-shell child component.
   - `Sidebar` currently combines project display, session grouping by project, expanded-project persistence, runtime badges, and search resolver support. After project selectors/helpers are settled, decide whether project/session grouping belongs in a pure helper or a smaller sidebar controller.
   - Recheck project lookup derived values. Project lookup by id should remain derived from `projects + projectId`; if repeated lookup logic is noisy, use a pure helper over the project list instead of storing duplicate active/current project objects.
   - Recheck `projectStore.getActiveProject`. It is an imperative derived helper over `projects + activeProjectId`; keep it only if useful for `getState()`/callback code, and prefer selectors or pure helpers for React reads.
   - Record concrete follow-ups instead of doing a broad component refactor inside this phase unless the project-store change requires it.

6. Make reorder policy explicit.
   - Preserve current optimistic behavior unless product expectations require a change.
   - Add a clear comment or function name indicating whether reorder is optimistic without rollback, optimistic with refresh, or pessimistic.
   - Prefer adding failure handling in a separate small change if needed.

7. Reduce persistence coupling, but do not standardize persistence yet.
   - Keep cache behavior working.
   - Isolate cache reads/writes behind small helper functions if they remain in this phase.

8. Add focused tests.
   - Project cache bootstrap.
   - Add/edit/remove success behavior.
   - Reorder local ordering and order fields.
   - Reorder backend call shape.
   - Failure behavior documented by tests where feasible.

**Validation**
- `cd ui/goose2 && pnpm test -- project`
- `rg "useProjectStore\\(\\)" ui/goose2/src`
- Manual smoke: create, edit, reorder, and delete a project.

**Success Criteria**
- `projectStore` no longer owns every project workflow directly.
- Mutation policy is explicit.
- Cache behavior still works.
- Project UI behavior is unchanged.
