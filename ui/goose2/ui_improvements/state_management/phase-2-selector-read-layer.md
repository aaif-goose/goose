**Phase 2: Introduce A Selector-First Read Layer**

**Status**
- Complete.

**Goal**
- Standardize repeated Zustand reads after Phase 1 exposes selector duplication.
- Add `useShallow` only where object or array selector results benefit from shallow comparison.
- Use selectors to reveal state boundaries, not hide weak store or component boundaries.
- Treat selector files as the current read-layer map, not as a final architecture. Later phases must revisit selector files when state moves across store boundaries.

**Scope**
- `ui/goose2/src/features/chat/stores/chatSelectors.ts`
- `ui/goose2/src/features/chat/stores/chatSessionSelectors.ts`
- `ui/goose2/src/features/agents/stores/agentSelectors.ts`
- `ui/goose2/src/features/projects/stores/projectSelectors.ts`
- `ui/goose2/src/features/providers/hooks/useProviderInventory.ts`
- Any Phase 1 call sites that now duplicate selector logic.

**Out Of Scope**
- Do not split stores.
- Do not change action semantics.
- Do not move backend calls.
- Do not add `useShallow` around primitive selectors.
- Do not introduce selectors for one-off reads unless they improve clarity.
- Do not create selector files mechanically for every store field.
- Do not add stored attributes for values that can be safely derived.

**Execution Steps**

1. Review and classify selectors from Phase 1.
   - Look for repeated active session, active project, active agent, message, runtime, and provider-selection reads.
   - Classify each candidate as one of:
     - common simple read
     - derived value
     - symptom of a store with too many responsibilities
     - symptom of a component doing too much orchestration
     - one-off inline selector
   - Keep one-off selectors inline unless abstraction clearly improves readability.
   - Record store-boundary and component-boundary issues as follow-ups instead of hiding them behind selectors.

2. Add selector helpers only for common simple reads.
   - Start with pure selector functions.
   - Prefer names that describe the value, not the component that consumes it.
   - Keep selector modules close to the store they read.
   - When a later phase moves state out of a store, move or delete the related selectors in the same phase so selector files do not preserve old boundaries by accident.
   - Selector fallbacks must be stable. Do not return fresh `[]` or `{}` values directly from selectors; use module-level constants or derive outside the selector.
   - Good candidates:
     - chat: `selectMessagesBySession`, `selectSessionStateById`
     - chat sessions: `selectSessions`, `selectActiveSessionId`, `selectHasHydratedSessions`, `selectSessionsLoading`
     - agents: `selectPersonas`, `selectPersonasLoading`, `selectSelectedProvider`
     - projects: `selectProjects`, `selectProjectsLoading`

3. Prefer pure derived helpers for derived values.
   - Active session should be derived from `sessions + activeSessionId`; do not store a second `activeSession` attribute unless there is a strong reason.
   - Project lookup by id can be a pure helper over `projects`.
   - Sidebar session grouping may belong in a pure helper or dedicated selector-like utility, but should be designed carefully because it combines sessions, messages, runtime state, and project ids.
   - Sidebar: evaluate whether derived sidebar session items should be produced by selector/helper code instead of selecting the whole `sessionStateById` record during render.
   - `AppShell` active/home session derivation should remain derived from selected session state for now; do not add duplicate store attributes.

4. Revisit hooks that mix read state and command functions.
   - `usePersonas()` currently returns `personas` and `isLoading` as well as command functions.
   - `AgentsView` already reads `personas` and `personasLoading` directly with store selectors and only uses `usePersonas()` for command functions.
   - Decide whether to keep the current contract or split toward a clearer read-selector plus command-hook pattern.
   - Do not make this contract change in Phase 1.

5. Introduce `useShallow` selectively.
   - Use it for grouped object selectors in high-level consumers.
   - Use it for derived array/object selectors where reference churn causes unnecessary rerenders.
   - Do not use it for primitive values or single function selectors.
   - Do not use `useShallow` to compensate for a broad store or overly broad component dependency.
   - Re-check `useProviderInventory` during this phase. It currently reads `entries` and `loading` separately; only group them with `useShallow` if that improves clarity without introducing object churn.
   - Phase 2 assessment: keep `useProviderInventory` as separate `entries` and `loading` selectors for now. Grouping them with `useShallow` would mostly reduce two store subscriptions to one, but would not narrow rerenders or simplify consumers.

6. Refactor consumers to use selector helpers where they remove duplication.
   - Keep call sites readable.
   - Avoid turning every selector into an abstraction.
   - If a consumer needs many selectors, decide whether that indicates a later component decomposition or store-boundary task.
   - Start with small chat-session selector helpers in `AppShell.tsx` and `Sidebar.tsx`; do not refactor every selector candidate at once.
   - Record `AppShell` as a later component-decomposition candidate rather than trying to hide its orchestration breadth behind selector helpers.
   - Record `Sidebar` derived session grouping as a possible pure helper extraction, not as store state.
   - For `agentStore`, add selectors only for stable repeated domain reads such as personas. Do not add selector helpers for persona editor UI state in this phase, because that state is a Phase 4 extraction candidate.
   - If selector extraction makes a feature look syntactically cleaner but still leaves many store dependencies, record that as a component or store boundary problem instead of adding more selectors.
   - Before closing Phase 2, list selector files created and the later phases that must revisit them.

**Validation**
- `rg "useShallow|zustand/shallow" ui/goose2/src`
- `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar useProviderInventory`

**Success Criteria**
- Common read patterns have reusable selector helpers.
- `useShallow` appears only on object or array selectors where it adds value.
- React store consumption is more consistent without changing behavior.
- Phase 3-5 follow-ups are clearer because selector audit findings distinguish simple read cleanup from deeper architecture problems.
