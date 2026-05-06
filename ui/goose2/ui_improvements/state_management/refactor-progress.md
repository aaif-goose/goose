**Goose2 Zustand Refactor Progress**

**Current Status**
- Planning in progress.
- Implementation not started.

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

**Phase Status**

| Phase | Name | Status | Notes |
| --- | --- | --- | --- |
| 1 | Remove whole-store subscriptions | Pending | First implementation phase. |
| 2 | Introduce selector-first read layer | Pending | Starts after Phase 1 exposes repeated selectors. |
| 3 | Separate session side effects | Pending | Keep compatibility while migrating call sites. |
| 4 | Split clearest store boundaries | Pending | Split `agentStore` UI state, then `chatSessionStore` UI state. |
| 5 | Refactor project store | Pending | Make mutation policy explicit. |
| 6 | Standardize persistence | Pending | Do after boundaries are clearer. |
| 7 | Test reset and coverage | Pending | Dedicated cleanup after boundary changes stabilize. |
| 8 | Optional Immer | Pending | Skip unless update readability still justifies it. |

**Known Current-Code Findings To Recheck Before Phase 1**
- Broad subscriptions currently exist in:
  - `AppShell.tsx`
  - `Sidebar.tsx`
  - `usePersonas.ts`
  - `useChat.ts`
- `useShallow` is currently unused in `ui/goose2/src`.
- `chatSessionStore.updateSession` currently mixes local state patching with backend side effects.
- `projectStore` currently owns local cache, API orchestration, state, active selection, and reorder behavior.
- `agentStore` currently mixes catalog/provider state with persona editor UI state.

**Next Step**
- Start Phase 1 using `phase-1-selector-cleanup.md`.
