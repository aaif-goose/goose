**Phase 4: Split The Clearest Store Boundaries**

**Status**
- Not started.

**Goal**
- Move clearly UI-only state out of domain-heavy stores.
- Keep store splits narrow and behavior-preserving.
- Use the Phase 2 selector audit to distinguish UI-state extraction from deeper domain-model cleanup.
- Revisit component responsibility after UI state moves, especially where Phase 2 identified components that still depend on many store concerns.

**Scope**
- `ui/goose2/src/features/agents/stores/agentStore.ts`
- `ui/goose2/src/features/agents/stores/agentSelectors.ts` if Phase 2 adds it
- New `ui/goose2/src/features/agents/stores/agentUiStore.ts`
- `ui/goose2/src/features/chat/stores/chatSessionStore.ts`
- `ui/goose2/src/features/chat/stores/chatSessionSelectors.ts`
- `ui/goose2/src/features/chat/stores/chatSelectors.ts` if `chatStore` boundaries are revisited
- New `ui/goose2/src/features/chat/stores/chatSessionUiStore.ts`
- Consumers of persona editor state, context panel state, and active workspace state.

**Out Of Scope**
- Do not split personas, agents, providers, or selected provider yet.
- Do not move `activeSessionId` unless this phase's boundary review proves it is safe.
- Do not split `chatStore` immediately.
- Do not refactor project orchestration in this phase.
- Do not solve the full persona vs agent vs provider domain model in the first store split; record the model problem and keep the initial extraction narrow.

**Execution Steps**

1. Split `agentStore` UI state first.
   - Phase 2 identified `agentStore` as mixing personas, agents, providers, selected provider, active agent, and persona editor UI state.
   - Treat persona editor state as the safest first extraction, not as the full solution to the domain model.
   - Move these fields to `agentUiStore`:
     - `personaEditorOpen`
     - `editingPersona`
     - `personaEditorMode`
   - Move these actions to `agentUiStore`:
     - `openPersonaEditor`
     - `closePersonaEditor`
   - Keep catalog/provider state in `agentStore`.

2. Update agent UI consumers.
   - Update `AgentsView.tsx` and related components to read editor state from `agentUiStore`.
   - Update any controller code that opens or closes the persona editor.
   - Keep persona CRUD and provider selection on `agentStore`.
   - Update any Phase 2 selector files so `agentSelectors.ts` contains only state that remains in `agentStore`. If UI selectors were avoided in Phase 2, this should be a small verification step.

3. Validate the `agentStore` split.
   - Tests for domain state should not need editor fields.
   - Add minimal `agentUiStore` tests if behavior is not trivially covered by UI tests.
   - Recheck `agentStore.isLoading`. Phase 2 found it may be a legacy/general loading flag because personas, agents, and providers already have specific loading fields. Remove or defer it only after confirming consumers and tests do not depend on it.
   - Recheck derived helper methods such as `getActiveAgent`, `getAgentsByPersona`, `getBuiltinPersonas`, and `getCustomPersonas`. Keep them for imperative `getState()` usage if useful, but React consumers should prefer selectors or pure helpers.

4. Split `chatSessionStore` UI state second.
   - Phase 2 identified `chatSessionStore` as mixing session records, hydration/loading, active selection, creation/archive/mutation actions, context-panel state, and workspace UI state.
   - This phase should extract only the clearly UI-only context panel and workspace state.
   - Move these fields to `chatSessionUiStore`:
     - `contextPanelOpenBySession`
     - `activeWorkspaceBySession`
   - Move these actions to `chatSessionUiStore`:
     - `setContextPanelOpen`
     - `setActiveWorkspace`
     - `clearActiveWorkspace`
   - Keep session records and active session selection in `chatSessionStore`.

5. Update chat UI consumers.
   - Update `ChatView.tsx`, `ContextPanel.tsx`, `useChatSessionController.ts`, and `useChat.ts` if they read the moved fields/actions.
   - Keep session creation, loading, archive, and rename behavior unchanged.
   - Update `chatSessionSelectors.ts` so selectors for moved UI state live with `chatSessionUiStore` or are removed. Keep session record selectors with `chatSessionStore`.
   - Recheck session-derived values after the split. Active session and Home session should still be derived from `sessions + activeSessionId` or `sessions + homeSessionId`; do not add duplicate stored session objects.
   - Recheck sidebar session grouping after UI state moves. If grouping remains complex, prefer a pure helper or selector-like utility over adding grouped sidebar rows to store state.

6. Re-evaluate `chatStore` only after the two safe splits.
   - Phase 2 identified `chatStore` as mixing messages, runtime, drafts, queue, loading, scroll targets, and cleanup.
   - Phase 2 also found duplicated active-session selection: both `chatStore` and `chatSessionStore` store `activeSessionId`. Decide which store should own active session selection, or explicitly document why both must remain synchronized.
   - Do not split message/runtime/draft state in this phase unless the selector data from Phases 1 and 2 shows a very clear boundary.
   - If any `chatStore` state moves later, update `chatSelectors.ts` in the same change so selector files continue to reflect real store boundaries.
   - Record any follow-up observations in the progress tracker.

7. Record remaining domain-model questions.
   - Clarify whether personas, agents, providers, selected provider, and active agent need separate stores or clearer naming.
   - Defer any larger domain-model split to a later focused plan after the UI-only state extraction is complete.
   - Recheck store helper methods that return derived values, including `chatStore.getActiveMessages`, `chatSessionStore.getActiveSession`, and `chatSessionStore.getArchivedSessions`. Prefer pure helpers/selectors for React reads, but keep imperative helpers if they are still useful in callbacks or non-React code.

8. Recheck component responsibility after the UI-store splits.
   - `AgentsView` should be simpler after persona editor UI state moves out of `agentStore`; if it still mixes list filtering, editor orchestration, import/export, delete confirmation, and mutation flows too heavily, record a focused component-decomposition follow-up.
   - `useChatSessionController`, `ChatView`, and context-panel consumers should be rechecked after context panel and workspace UI state move out of `chatSessionStore`.
   - Recheck chat/session actions that are owned too high in `AppShell` and prop-drilled through multiple surfaces, especially rename from `SidebarChatRow` and `SessionCard`.
   - Recheck whether rename/archive/move actions need a surface-level action hook or controller after state boundaries are clearer.
   - Do not decompose components in this phase unless the split requires it for safe wiring; record the follow-up with concrete files and responsibilities.

**Validation**
- `rg "personaEditorOpen|editingPersona|personaEditorMode|openPersonaEditor|closePersonaEditor" ui/goose2/src`
- `rg "contextPanelOpenBySession|activeWorkspaceBySession|setContextPanelOpen|setActiveWorkspace|clearActiveWorkspace" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- agentStore chatSessionStore useChatSessionController`

**Success Criteria**
- Persona editor UI state is no longer in `agentStore`.
- Context panel and active workspace UI state are no longer in `chatSessionStore`.
- Domain store behavior remains unchanged.
