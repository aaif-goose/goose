**Goose2 Zustand Refactor Progress**

**Current Status**
- Phase 1 implementation complete.
- Phase 2 implementation complete.
- Phase 3 implementation complete.

**Documents**
- Added master review: `goose2-zustand-state-management-review.md`
- Added master plan: `goose2-zustand-state-management-improvement-plan.md`
- Added per-phase execution plans:
  - `phase-1-selector-cleanup.md`
  - `phase-2-selector-read-layer.md`
  - `phase-3-session-side-effects.md`
  - `phase-4-store-boundaries.md`
  - `phase-5-session-workflow-failure-policy.md`
  - `phase-6-project-store.md`
  - `phase-7-persistence.md`
  - `phase-8-test-reset-coverage.md`
  - `phase-9-optional-immer.md`

**Decisions**
- Keep simple numbered phases without `A`/`B` sub-phases.
- Do not make test reset cleanup Phase 1.
- Clean up test reset patterns opportunistically when a refactor PR already touches those tests.
- Keep the dedicated test reset and coverage pass as Phase 8.
- Create per-phase execution files so each phase has scope, guardrails, validation, and success criteria.
- In Phase 1, keep existing hook/component public contracts unchanged. For example, `usePersonas()` should keep returning `personas` and `isLoading` even though its current production consumer only uses command functions.
- For Phase 3, use explicit `chatSessionOperations.ts` functions for backend-aware chat-session workflows. Use `patchSession` for local-only store updates. For title and project updates, call backend first and patch local state only after success.

**Phase Status**

| Phase | Name | Status | Notes |
| --- | --- | --- | --- |
| 1 | Remove whole-store subscriptions | Complete | Replaced broad subscriptions in `usePersonas.ts`, `useChat.ts`, `Sidebar.tsx`, and `AppShell.tsx`. |
| 2 | Introduce selector-first read layer | Complete | Added initial `chatSessionStore`, `chatStore`, `agentStore`, and `projectStore` selector helpers. Deferred derived values, store-boundary issues, and component-boundary issues to later phases. |
| 3 | Separate session side effects | Complete | Removed generic `updateSession`; local patches use `patchSession`, and title/project persistence uses explicit operations. |
| 4 | Split clearest store boundaries | Pending | Split `agentStore` UI state, then `chatSessionStore` UI state. |
| 5 | Decide session workflow failure policy | Pending | Behavior-policy follow-up after Phase 4 clarifies session-store boundaries. |
| 6 | Refactor project store | Pending | Make mutation policy explicit. |
| 7 | Standardize persistence | Pending | Do after boundaries are clearer. |
| 8 | Test reset and coverage | Pending | Dedicated cleanup after boundary changes stabilize. |
| 9 | Optional Immer | Pending | Skip unless update readability still justifies it. |

**Known Current-Code Findings To Recheck Before Phase 1**
- Broad subscription scan is clean after Phase 1:
  - `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`
- `useShallow` is currently unused in `ui/goose2/src`.
- Phase 3 removed `chatSessionStore.updateSession`; local-only session changes now use `patchSession`.
- `projectStore` currently owns local cache, API orchestration, state, active selection, and reorder behavior.
- `agentStore` currently mixes catalog/provider state with persona editor UI state.

**Next Step**
- Start Phase 4 using `phase-4-store-boundaries.md`.

**Validation Log**
- Phase 1 broad subscription scan: clean.
- Phase 1 TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 1 focused tests: `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar` passed.
- Phase 2 Step 1 TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 Step 2 TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 Step 3A TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 Step 3B TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 Step 4A TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 Step 4B TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 2 focused tests: `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar useProviderInventory` passed.
- Phase 2 `useProviderInventory` / `useShallow` assessment complete: no code change recommended.
- Phase 2 final broad subscription scan: `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src` found no matches.
- Phase 2 final `useShallow` scan: `rg "useShallow|zustand/shallow" ui/goose2/src` found no matches, as expected.
- Phase 3 TypeScript check: `cd ui/goose2 && pnpm exec tsc --noEmit` passed.
- Phase 3 focused tests: `cd ui/goose2 && pnpm test -- chatSessionOperations chatSessionStore` passed; Vitest ran 109 files / 632 tests.
- Phase 3 final generic `updateSession` scan: no `chatSessionStore.updateSession` action or call sites remain; only explicit `updateSessionTitle` / `updateSessionProject` operation/API names remain.

**Follow-Ups To Revisit Later**
- Revisit whether `usePersonas()` should remain both a read hook and command hook. Today `AgentsView` already reads `personas` and `personasLoading` through direct store selectors and only uses `usePersonas()` for `createPersona`, `updatePersona`, `deletePersona`, and `refreshFromDisk`. Do not change that contract during Phase 1.
- Revisit the sidebar runtime selector in Phase 2. Phase 1 selects the whole `sessionStateById` record to avoid a complex derived selector, which is still narrower than the whole `chatStore`; Phase 2 should evaluate whether visible sidebar session items need a dedicated selector/helper.
- Keep selector fallbacks stable. Phase 1 found that returning a fresh `[]` from a selector fallback in `useChat` can trigger React snapshot churn; use module-level constants or derive fallback values outside the selector.
- In Phase 8, keep component-test store mocks selector-aware. Phase 1 production code now calls bound hooks with selectors, so mocks need to support `useStore((state) => ...)` as well as any remaining direct mock patterns.
- Phase 1 removed dead code exposed by the selector cleanup: `retryLastMessage`, `buildSkillRetryOptions`, and the unused `findLastIndex` helper.
- After Phase 3 creates explicit title/project workflow actions, revisit backend failure policy for rename and project assignment. Options include backend-first, optimistic with rollback, refresh-on-failure, or optimistic with user-visible error.
- After Phase 4 clarifies `chatSessionStore` boundaries, revisit archive/unarchive. They currently remain store actions with API calls and optimistic-without-rollback behavior: local visibility changes immediately, backend failures are logged, and local state is not restored.

**Phase 2 Audit Notes**
- Completed Step 1:
  - Added `chatSessionSelectors.ts` with `selectSessions`, `selectActiveSessionId`, `selectHasHydratedSessions`, and `selectSessionsLoading`.
  - Updated `AppShell.tsx` and `Sidebar.tsx` to use those simple field selectors.
- Completed Step 2:
  - Added `chatSelectors.ts` with `selectMessagesBySession` and `selectSessionStateById`.
  - Updated `AppShell.tsx` and `Sidebar.tsx` to use those simple field selectors.
- Completed Step 3A:
  - Added `agentSelectors.ts` with `selectPersonas` and `selectPersonasLoading`.
  - Updated `usePersonas.ts`, `AgentsView.tsx`, and `useChatSessionController.ts` to use persona domain selectors.
  - Left persona editor UI state selectors inline so Phase 4 extraction remains visible.
- Completed Step 3B:
  - Added `selectSelectedProvider` to `agentSelectors.ts`.
  - Updated `AppShell.tsx` and `useProviderSelection.ts` to use the selected-provider selector.
  - Left provider lists/loading/actions inline pending later `agentStore` boundary work.
- Completed Step 4A:
  - Renamed the internal `ProjectState` type to exported `ProjectStore` because the type includes state and actions.
  - Added `projectSelectors.ts` with `selectProjects`.
  - Updated direct `projects` field reads in `AppShell.tsx`, `Sidebar.tsx`, `SkillsView.tsx`, `SessionHistoryView.tsx`, and `useChatSessionController.ts`.
  - Left project actions, loading, and derived project lookup reads inline pending Phase 6 project-store ownership work.
- Completed Step 4B:
  - Updated `SessionHistoryView.tsx` to use existing `selectSessions` and `selectMessagesBySession`.
  - A scan found no remaining exact inline reads for selector helpers already created.
  - Left derived/id-dependent reads, UI-state reads, one-off loading reads, and action selectors inline.
- Completed `useProviderInventory` / `useShallow` assessment:
  - `useProviderInventory` reads `entries` and `loading` with separate narrow selectors.
  - Grouping these into an object selector with `useShallow` would not meaningfully reduce rerenders because consumers should still update when either value changes.
  - Kept the hook unchanged.
- Simple selector candidates:
  - chat: `selectMessagesBySession`, `selectSessionStateById`
  - chat sessions: `selectSessions`, `selectActiveSessionId`, `selectHasHydratedSessions`, `selectSessionsLoading`
  - agents: `selectPersonas`, `selectPersonasLoading`, `selectSelectedProvider`
  - projects: `selectProjects`, `selectFetchProjects`, `selectReorderProjects`
- Derived values should stay derived, not become stored attributes:
  - active session from `sessions + activeSessionId`
  - Home session from `sessions + homeSessionId`
  - project lookup by id from `projects + projectId`
  - sidebar grouped session rows from `sessions + messagesBySession + sessionStateById + project ids`
  - Phase 4 should recheck active/Home session derivation and sidebar grouping after chat/session UI state moves.
  - Phase 6 should recheck project lookup helpers after project-store ownership is clearer.
- Duplicated or legacy state to revisit later:
  - `chatStore.activeSessionId` and `chatSessionStore.activeSessionId` duplicate active-session selection. Track for Phase 4 boundary review.
  - `agentStore.isLoading` may be a legacy/general loading flag now that personas, agents, and providers have specific loading fields. Track for Phase 4 and test-reset cleanup in Phase 8.
- Imperative derived helper methods to revisit later:
  - `chatStore.getActiveMessages`
  - `chatSessionStore.getActiveSession`
  - `chatSessionStore.getArchivedSessions`
  - `projectStore.getActiveProject`
  - `agentStore.getActiveAgent`
  - `agentStore.getAgentsByPersona`
  - `agentStore.getBuiltinPersonas`
  - `agentStore.getCustomPersonas`
  - Keep these only where useful for `getState()`/callback code; prefer selectors or pure helpers for React reads.
- Store-boundary signals to carry into later phases:
  - `agentStore` mixes personas, providers, active agent, selected provider, and persona editor UI. Track for Phase 4.
  - `chatStore` mixes messages, runtime, drafts, queue, loading, scroll targets, and cleanup. Track for Phase 4 / Phase 9 reassessment.
  - `chatSessionStore` mixes session records, hydration/loading, active selection, creation, archive, mutation, context panel state, and workspace UI. Track for Phase 3 and Phase 4.
- Phase 2 guardrail:
  - Selectors should not hide real feature complexity. For `agentStore`, use selectors only for stable repeated domain reads such as personas and selected provider.
  - Do not add selectors for persona editor UI state in Phase 2; keep that visible for Phase 4 UI-store extraction.
  - If a feature still needs many store dependencies after selector cleanup, track it as a component/store boundary issue instead of adding more selectors.
  - Selector files created in Phase 2 are part of the read layer, not proof that the current store boundaries are final.
  - When Phase 4 or Phase 6 moves state across stores, update, move, or delete the corresponding Phase 2 selectors in the same change.
- Component-boundary signals to carry into later phases:
  - `AppShell` is orchestration-heavy: projects, sessions, Home setup, provider/model choice, keyboard shortcuts, dialogs, sidebar wiring, and chat activation. Track as a later component decomposition concern.
  - `Sidebar` performs visible-session filtering, project grouping, runtime badge derivation, search resolver creation, and expanded-project persistence. Track derived helpers during Phase 2 and possible component decomposition later.
  - Rename behavior is owned high in `AppShell` and prop-drilled through `Sidebar`/sidebar sections to `SidebarChatRow`, and through `SessionHistoryView` to `SessionCard`. Track for later component/action-controller cleanup.
  - Rename row components close edit mode immediately and expose void callbacks, so backend-first rename failures cannot currently show inline pending/error state. Track UX policy in Phase 5.
- Phase 2 implementation plan:
  - Start with small chat-session selector helpers because they are simple field selectors.
  - Refactor only `AppShell.tsx` and `Sidebar.tsx` first.
  - Do not use selectors to hide the larger Phase 3-5 architecture problems.
