**Goose2 Zustand State Management Improvement Plan**

This plan translates the findings from [goose2-zustand-state-management-review.md](/Users/lifei/Development/goose/ui/goose2/ui_improvements/state_management/goose2-zustand-state-management-review.md) into a concrete implementation sequence.

The plan is optimized for:
- improving maintainability
- reducing future bug risk
- minimizing unnecessary churn
- respecting code dependencies and migration order

This is a single master plan. It is intentionally sequenced so that higher-leverage, lower-risk changes happen before larger store refactors.

**Goals**

- Establish selector-first Zustand usage in React code.
- Reduce coupling between consumers and broad store shapes.
- Remove hidden backend side effects from generic store actions.
- Improve store responsibility boundaries.
- Standardize persistence and mutation policy.
- Improve test reset discipline and coverage for risky flows.
- Only use `useShallow` and Immer where they materially improve maintainability.

**Sequencing Principles**

- Fix read patterns before splitting stores.
- Separate side effects from generic store actions before redesigning store boundaries.
- Split the clearest UI/domain boundaries first.
- Standardize persistence only after the intended state boundaries are clearer.
- Improve tests alongside each change, not only at the end.
- Treat Immer as a late readability improvement, not an architectural fix.

**High-Level Phase Order**

1. Remove whole-store subscriptions and stabilize store consumption.
2. Introduce a selector-first read layer and add `useShallow` where appropriate.
3. Separate backend side effects from generic store actions.
4. Split the broadest stores by responsibility.
5. Decide chat-session workflow failure policy.
6. Refactor `projectStore` into clearer layers.
7. Standardize persistence boundaries.
8. Standardize store reset/testing patterns and close coverage gaps.
9. Optionally adopt Immer for nested update-heavy stores.

**Priority Snapshot**

**Highest priority**
- Phase 1: whole-store subscriptions
- Phase 2: selector-first read layer
- Phase 3: hidden backend side effects in `chatSessionStore`

**Medium priority**
- Phase 4: split `agentStore` UI state and `chatSessionStore` UI state
- Phase 5: decide chat-session workflow failure policy
- Phase 6: refactor `projectStore`
- Phase 8: testing/reset cleanup for touched stores

**Lower priority but useful**
- Phase 7: persistence standardization rollout
- Phase 9: selective Immer adoption

**Dependency Logic**

- Phases 1 and 2 come first because they reduce consumer coupling and make later store splits safer.
- Phase 3 comes before major store splits because current action semantics are one of the biggest bug risks.
- Phase 4 is safer after read patterns and action semantics are clearer.
- Phase 5 follows Phase 4 so archive/unarchive workflow policy can be decided after session-store boundaries are clearer.
- Phase 6 depends on lessons from earlier store cleanup and should not be the first major refactor.
- Phase 7 depends on clearer decisions about what state is truly durable.
- Phase 8 should happen incrementally, but a dedicated cleanup pass is still needed.
- Phase 9 is intentionally last because it improves update ergonomics, not architecture.

**Phase 1: Remove Whole-Store Subscriptions**

**Goal**
- Replace broad bound-store subscriptions with explicit selectors.

**Why first**
- This is the highest-leverage, lowest-risk improvement.
- It reduces rerender noise and decouples components/hooks from full store shapes.
- It makes later store changes less invasive.

**Primary files**
- [AppShell.tsx](/Users/lifei/Development/goose/ui/goose2/src/app/AppShell.tsx:80)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:103)
- [usePersonas.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/usePersonas.ts:12)
- [useChat.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChat.ts:108)

**Detailed tasks**
- `AppShell.tsx`
  - Replace `useChatStore()`, `useChatSessionStore()`, `useAgentStore()`, and `useProjectStore()` broad subscriptions with specific selectors.
  - Avoid selecting whole store objects just to access a few fields or actions.
  - Keep legitimate `getState()` usage in async helper callbacks where necessary.
- `Sidebar.tsx`
  - Replace whole `chatStore`, `agentStoreState`, and `projectStoreState` subscriptions with targeted selectors.
  - Avoid broad subscription combined with heavy render-time derivation.
- `usePersonas.ts`
  - Replace `const store = useAgentStore()` with explicit state/action selectors.
  - Stop relying on the “store object is stable” assumption in hook callbacks.
- `useChat.ts`
  - Replace whole-store subscription with session-scoped selectors for messages/runtime plus specific actions.

**Success criteria**
- No high-level React component or hook uses `useSomeStore()` with no selector.
- Components subscribe only to the fields/actions they actually need.

**Phase 2: Introduce a Selector-First Read Layer**

**Goal**
- Standardize how React code reads Zustand state.

**Why second**
- After Phase 1, repeated selector logic will become visible.
- This is the right time to define reusable selectors and add `useShallow` only where needed.

**Primary files to add or refactor**
- `src/features/chat/stores/chatSelectors.ts`
- `src/features/chat/stores/chatSessionSelectors.ts`
- `src/features/agents/stores/agentSelectors.ts`
- `src/features/projects/stores/projectSelectors.ts`
- [useProviderInventory.ts](/Users/lifei/Development/goose/ui/goose2/src/features/providers/hooks/useProviderInventory.ts:29)

**Detailed tasks**
- Create selector helpers for repeated reads:
  - session lists and active session id
  - session runtime by id
  - visible messages for session
  - project list and project lookup helpers
  - agent/persona/provider simple reads
  - provider selection state
- Keep active session, Home session, active project, and similar composed values derived from existing state. Do not add duplicate store attributes just to make reads easier.
- Introduce grouped selectors with `useShallow` where components need multiple values together.
- Keep primitive selectors simple and do not wrap them in `useShallow`.
- Refactor any repetitive inline lookup logic into selector helpers or pure `lib/` helpers if it is not inherently store-specific.

**Where `useShallow` should be used**
- grouped object selectors in high-level consumers
- derived array/object selectors where reference churn would otherwise trigger unnecessary rerenders

**Where `useShallow` should not be used**
- primitive selectors
- as a substitute for selector discipline
- to compensate for weak store boundaries

**Success criteria**
- Common reads have reusable selector helpers.
- `useShallow` appears only on object/array selectors where it adds value.
- React read patterns are visibly more consistent across features.

**Phase 3: Separate Backend Side Effects From Generic Store Actions**

**Goal**
- Make mutation semantics explicit and reduce hidden side effects.

**Why third**
- This is one of the highest bug-risk areas.
- It is easier to split stores safely after action semantics are cleaned up.

**Primary file**
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:195)

**Current problem**
- `updateSession` looks like a local patch action but can also trigger backend rename/project-update side effects.

**Detailed tasks**
- Split local state patching from backend persistence behavior.
- Keep a truly local session patch action in the store.
- Move remote mutation behavior into explicit orchestration functions or hooks.
- Introduce clearly named operations for:
  - rename session and persist
  - update session project and persist
  - archive session
  - unarchive session
- Audit current call sites that assume `updateSession` is the main entry point:
  - [useChat.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChat.ts:215)
  - [useResolvedAgentModelPicker.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useResolvedAgentModelPicker.ts:279)
  - [useChatSessionController.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChatSessionController.ts:314)
  - [useChatSessionController.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChatSessionController.ts:711)

**Possible target modules**
- `src/features/chat/api/` for backend-facing operations
- `src/features/chat/hooks/` for orchestration wrappers

**Success criteria**
- Generic local patch actions are local only.
- Backend writes happen through explicitly named orchestration functions.
- Mutation semantics are easier to reason about at call sites.

**Phase 4: Split the Broadest Stores by Responsibility**

**Goal**
- Reduce mixed responsibilities inside stores and isolate UI-only state from domain state where practical.

**Why fourth**
- Consumer coupling and action semantics should be cleaner before changing boundaries.

**AgentStore Boundary Work**

**Primary file**
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:34)

**Current mix**
- personas
- agents
- providers
- selected provider
- active agent
- persona editor modal state

**Detailed tasks**
- Keep catalog/domain concerns together initially:
  - personas
  - agents
  - providers
  - selected provider
  - active agent if still truly shared
- Move editor/modal concerns out:
  - `personaEditorOpen`
  - `editingPersona`
  - `personaEditorMode`
- Update affected consumers:
  - [AgentsView.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/agents/ui/AgentsView.tsx:44)
  - [useChatSessionController.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChatSessionController.ts:601)

**Possible target files**
- `agentCatalogStore.ts`
- `agentUiStore.ts`

**ChatSessionStore Boundary Work**

**Primary file**
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:55)

**Current mix**
- session records
- active session selection
- context panel open state
- active workspace UI state

**Detailed tasks**
- Keep session data in the session store.
- Move UI-only keyed state out:
  - `contextPanelOpenBySession`
  - `activeWorkspaceBySession`
- Update affected consumers:
  - [ChatView.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/chat/ui/ChatView.tsx:30)
  - [ContextPanel.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/chat/ui/ContextPanel.tsx:44)
  - [useChatSessionController.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChatSessionController.ts:56)
  - [useChat.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChat.ts:386)

**Possible target files**
- `chatSessionStore.ts`
- `chatSessionUiStore.ts`

**ChatStore Re-Evaluation**

**Primary file**
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:39)

**Current mix**
- messages
- runtime
- queue
- drafts
- connection state
- loading/replay state
- scroll targeting
- cleanup

**Detailed tasks**
- Do not split this immediately unless the earlier Phase 4 boundary work is already complete.
- Reassess whether the app needs a separation between:
  - message state
  - runtime state
  - composer/draft state
- Base the split on actual selector usage after cleanup, not on theoretical purity.

**Success criteria for Phase 4**
- UI-only state is no longer co-located with unrelated domain-heavy stores where that separation is clear and useful.
- Store boundaries reflect actual shared-state responsibilities more closely.

**Phase 5: Decide Chat Session Workflow Failure Policy**

**Goal**
- Make chat-session workflow failure behavior explicit after Phase 4 clarifies session-store boundaries.

**Why this phase matters**
- Phase 3 removes hidden side effects from generic session patching.
- Phase 4 clarifies which session state should remain in `chatSessionStore`.
- Archive/unarchive still have API calls in the store and currently use optimistic-without-rollback behavior.
- Failure-policy changes are user-visible, so they should be handled separately from the mechanical side-effect extraction.

**Detailed tasks**
- Review title/project workflows introduced in Phase 3.
- Revisit archive/unarchive after Phase 4.
- Decide policy per workflow:
  - backend-first
  - optimistic without rollback
  - optimistic with rollback
  - refresh-on-failure
  - optimistic with user-visible error
- Add focused tests for selected success/failure behavior.

**Success criteria**
- Each chat-session workflow has one clear implementation point.
- Local/backend consistency behavior is intentional and tested.

**Phase 6: Refactor `projectStore` Into Clearer Layers**

**Goal**
- Reduce architectural overreach in the project feature state layer.

**Why this phase matters**
- `projectStore` currently combines state, cache hydration, persistence, CRUD orchestration, and optimistic mutation policy.
- That makes project behavior harder to reason about and harder to test than the other feature stores.
- This phase should happen after the earlier selector and store-boundary cleanup so the project refactor is not fighting broad consumer coupling at the same time.

**Primary file**
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:73)

**Current mix**
- project data
- loading state
- local cache hydration/persistence
- CRUD orchestration
- optimistic reorder behavior
- active selection

**Detailed tasks**
- Keep project state transitions in the store.
- Move API orchestration into explicit project commands/hooks.
- Make reorder policy explicit.
- Decide and document whether reorder is:
  - optimistic with rollback
  - optimistic with refresh
  - pessimistic
- Reduce direct persistence coupling in the store so persistence can later be standardized.

**Likely support files**
- `src/features/projects/hooks/useProjectCommands.ts`
- `src/features/projects/stores/projectSelectors.ts`

**Likely consumer files to revisit**
- [ProjectsView.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/projects/ui/ProjectsView.tsx:89)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:192)
- [SettingsModal.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/settings/ui/SettingsModal.tsx:107)

**Success criteria**
- Clearer source-of-truth behavior for projects.
- Explicit mutation policy for reorder and CRUD flows.
- Less orchestration logic living in the store itself.

**Phase 7: Standardize Persistence Boundaries**

**Goal**
- Replace ad hoc per-store durability choices with a clearer persistence policy.

**Why this phase matters**
- Persistence choices currently live in scattered store-local helpers and are not governed by a shared rule.
- That makes hydration behavior, migration behavior, and “what should survive reloads” decisions inconsistent.
- This phase comes after boundary cleanup because persistence should reflect the intended final ownership of state, not the current mixed-responsibility shape.

**Primary files**
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:11)
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:5)
- [draftPersistence.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/draftPersistence.ts:1)

**Detailed tasks**
- Decide what state is truly durable.
- Move eligible store-backed durability to Zustand `persist` where appropriate.
- Use `partialize` and versioning for persisted stores.
- Keep non-store persistence explicit if it truly belongs outside Zustand.

**Good persistence candidates**
- selected provider
- chat drafts if intentionally durable
- possibly project cache if cached bootstrap remains a product decision

**Avoid persisting by default**
- connection state
- transient runtime state
- temporary UI interaction state

**Success criteria**
- durability rules are explicit
- persisted state is minimal and intentional
- persistence code is no longer scattered ad hoc through stores

**Phase 8: Standardize Test Reset Patterns and Close Coverage Gaps**

**Goal**
- Make store tests safer and add coverage to the riskiest state-management paths.

**Why this phase matters**
- The current tests prove some important store behavior, but reset discipline is inconsistent and some of the riskiest stores are still under-covered.
- As the earlier phases change boundaries and mutation semantics, test quality becomes more important, not less.
- This phase ensures the refactor leaves the state layer easier to trust and easier to evolve.

**Primary files**
- [agentStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/__tests__/agentStore.test.ts:37)
- [usePersonas.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/__tests__/usePersonas.test.ts:53)
- [chatStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/__tests__/chatStore.test.ts:22)
- additional chat hook tests that use partial `setState(...)` setup patterns

**Detailed tasks**
- Introduce initial-state reset helpers for each store.
- Stop relying on partial shallow-merge resets as the standard setup pattern.
- Apply the same reset discipline to hook tests that manipulate stores directly.

**Coverage gaps to address**
- `projectStore`
  - cache hydration
  - reorder semantics
  - mutation failure/reconciliation policy
- `providerInventoryStore`
  - merge semantics
  - loading behavior
- session mutation orchestration introduced in Phase 3

**Success criteria**
- tests reset stores from a known initial state
- risky persistence and mutation behaviors have direct coverage

**Phase 9: Optional Immer Adoption for Nested Update Ergonomics**

**Goal**
- Improve readability where nested immutable updates remain noisy after structural cleanup.

**Why this phase matters**
- Immer is not a structural fix, so it should not be introduced before the higher-value boundary and side-effect problems are addressed.
- After the earlier phases, it becomes much easier to judge whether nested update boilerplate is still a real maintainability cost.
- Keeping this phase late prevents the team from mistaking update ergonomics for architectural improvement.

**Primary candidate**
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:104)

**Secondary candidate**
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:137)

**Detailed tasks**
- Reassess nested update-heavy store actions after earlier phases.
- Only adopt Immer if update readability remains a real pain point.
- Do not use Immer as a justification to keep a broad or mixed-responsibility store intact.

**Success criteria**
- update logic is easier to read without obscuring ownership boundaries

**Suggested PR Breakdown**

1. PR 1: selector cleanup in `AppShell`, `Sidebar`, `usePersonas`, `useChat`
2. PR 2: selector helpers + selective `useShallow`
3. PR 3: split `chatSessionStore` local patching from backend side effects
4. PR 4: extract `agentUiStore` from `agentStore`
5. PR 5: extract `chatSessionUiStore` from `chatSessionStore`
6. PR 6: refactor `projectStore` orchestration and mutation policy
7. PR 7: persistence standardization
8. PR 8: reset helpers + coverage expansion
9. PR 9: optional Immer cleanup for nested update-heavy stores

**Plan Usage**

- Use this as the master sequencing document.
- Do not try to execute every phase in a single refactor.
- For implementation, create smaller phase-specific execution notes only when a phase is about to start.
- Re-check the plan after Phase 3 and again after Phase 6, because store boundaries and actual consumer patterns may change enough to adjust later phases.
