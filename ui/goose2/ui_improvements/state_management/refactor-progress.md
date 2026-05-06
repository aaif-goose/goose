**Goose2 Zustand Refactor Progress**

**Current Status**
- Phase 1 implementation complete.
- Phase 2 not started.

**Documents**
- Added master review: `goose2-zustand-state-management-review.md`
- Added master plan: `goose2-zustand-state-management-improvement-plan.md`
- Added per-phase execution plans:
  - `phase-1-selector-cleanup.md`
  - `phase-2-selector-read-layer.md`
  - `phase-3-session-side-effects.md`
  - `phase-4-store-boundaries.md`
  - `phase-5-project-store.md`
  - `phase-6-persistence.md`
  - `phase-7-test-reset-coverage.md`
  - `phase-8-optional-immer.md`

**Decisions**
- Keep the original master plan order.
- Do not make test reset cleanup Phase 1.
- Clean up test reset patterns opportunistically when a refactor PR already touches those tests.
- Keep the dedicated test reset and coverage pass as Phase 7.
- Create per-phase execution files so each phase has scope, guardrails, validation, and success criteria.
- In Phase 1, keep existing hook/component public contracts unchanged. For example, `usePersonas()` should keep returning `personas` and `isLoading` even though its current production consumer only uses command functions.

**Phase Status**

| Phase | Name | Status | Notes |
| --- | --- | --- | --- |
| 1 | Remove whole-store subscriptions | Complete | Replaced broad subscriptions in `usePersonas.ts`, `useChat.ts`, `Sidebar.tsx`, and `AppShell.tsx`. |
| 2 | Introduce selector-first read layer | Pending | Starts after Phase 1 exposes repeated selectors. |
| 3 | Separate session side effects | Pending | Keep compatibility while migrating call sites. |
| 4 | Split clearest store boundaries | Pending | Split `agentStore` UI state, then `chatSessionStore` UI state. |
| 5 | Refactor project store | Pending | Make mutation policy explicit. |
| 6 | Standardize persistence | Pending | Do after boundaries are clearer. |
| 7 | Test reset and coverage | Pending | Dedicated cleanup after boundary changes stabilize. |
| 8 | Optional Immer | Pending | Skip unless update readability still justifies it. |

**Known Current-Code Findings To Recheck Before Phase 1**
- Broad subscription scan is clean after Phase 1:
  - `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`
- `useShallow` is currently unused in `ui/goose2/src`.
- `chatSessionStore.updateSession` currently mixes local state patching with backend side effects.
- `projectStore` currently owns local cache, API orchestration, state, active selection, and reorder behavior.
- `agentStore` currently mixes catalog/provider state with persona editor UI state.

**Next Step**
- Start Phase 2 using `phase-2-selector-read-layer.md`.

**Validation Log**
- Phase 1 broad subscription scan: clean.
- Phase 1 focused tests: `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar` passed.

**Follow-Ups To Revisit Later**
- Revisit whether `usePersonas()` should remain both a read hook and command hook. Today `AgentsView` already reads `personas` and `personasLoading` through direct store selectors and only uses `usePersonas()` for `createPersona`, `updatePersona`, `deletePersona`, and `refreshFromDisk`. Do not change that contract during Phase 1.
- Revisit the sidebar runtime selector in Phase 2. Phase 1 selects the whole `sessionStateById` record to avoid a complex derived selector, which is still narrower than the whole `chatStore`; Phase 2 should evaluate whether visible sidebar session items need a dedicated selector/helper.
