**Phase 2: Introduce A Selector-First Read Layer**

**Status**
- Not started.

**Goal**
- Standardize repeated Zustand reads after Phase 1 exposes selector duplication.
- Add `useShallow` only where object or array selector results benefit from shallow comparison.

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

**Execution Steps**

1. Review duplicated selectors from Phase 1.
   - Look for repeated active session, active project, active agent, message, runtime, and provider-selection reads.
   - Keep one-off selectors inline.

2. Add small selector modules.
   - Start with pure selector functions.
   - Prefer names that describe the value, not the component that consumes it.
   - Keep selector modules close to the store they read.

3. Add selector helpers for repeated reads.
   - Chat: messages by session, runtime by session, active session id, visible messages.
   - Chat sessions: sessions, active session, active session id, archived sessions.
   - Agents: selected provider, active agent, persona lookup inputs.
   - Projects: project list, active project id, active project.

4. Introduce `useShallow` selectively.
   - Use it for grouped object selectors in high-level consumers.
   - Use it for derived array/object selectors where reference churn causes unnecessary rerenders.
   - Do not use it for primitive values or single function selectors.

5. Refactor consumers to use selector helpers where they remove duplication.
   - Keep call sites readable.
   - Avoid turning every selector into an abstraction.

**Validation**
- `rg "useShallow|zustand/shallow" ui/goose2/src`
- `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar useProviderInventory`

**Success Criteria**
- Common read patterns have reusable selector helpers.
- `useShallow` appears only on object or array selectors where it adds value.
- React store consumption is more consistent without changing behavior.
