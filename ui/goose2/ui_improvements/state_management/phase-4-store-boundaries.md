**Phase 4: Split The Clearest Store Boundaries**

**Status**
- Not started.

**Goal**
- Move clearly UI-only state out of domain-heavy stores.
- Keep store splits narrow and behavior-preserving.

**Scope**
- `ui/goose2/src/features/agents/stores/agentStore.ts`
- New `ui/goose2/src/features/agents/stores/agentUiStore.ts`
- `ui/goose2/src/features/chat/stores/chatSessionStore.ts`
- New `ui/goose2/src/features/chat/stores/chatSessionUiStore.ts`
- Consumers of persona editor state, context panel state, and active workspace state.

**Out Of Scope**
- Do not split personas, agents, providers, or selected provider yet.
- Do not move `activeSessionId` unless a later review proves it is safe.
- Do not split `chatStore` immediately.
- Do not refactor project orchestration in this phase.

**Execution Steps**

1. Split `agentStore` UI state first.
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

3. Validate the `agentStore` split.
   - Tests for domain state should not need editor fields.
   - Add minimal `agentUiStore` tests if behavior is not trivially covered by UI tests.

4. Split `chatSessionStore` UI state second.
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

6. Re-evaluate `chatStore` only after the two safe splits.
   - Do not split message/runtime/draft state in this phase unless the selector data from Phases 1 and 2 shows a very clear boundary.
   - Record any follow-up observations in the progress tracker.

**Validation**
- `rg "personaEditorOpen|editingPersona|personaEditorMode|openPersonaEditor|closePersonaEditor" ui/goose2/src`
- `rg "contextPanelOpenBySession|activeWorkspaceBySession|setContextPanelOpen|setActiveWorkspace|clearActiveWorkspace" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- agentStore chatSessionStore useChatSessionController`

**Success Criteria**
- Persona editor UI state is no longer in `agentStore`.
- Context panel and active workspace UI state are no longer in `chatSessionStore`.
- Domain store behavior remains unchanged.
