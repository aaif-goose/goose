**Phase 1: Remove Whole-Store Subscriptions**

**Status**
- Not started.

**Goal**
- Replace broad bound-store subscriptions with explicit selectors.
- Keep behavior unchanged.
- Reduce coupling before any store boundary changes.

**Scope**
- `ui/goose2/src/app/AppShell.tsx`
- `ui/goose2/src/features/sidebar/ui/Sidebar.tsx`
- `ui/goose2/src/features/agents/hooks/usePersonas.ts`
- `ui/goose2/src/features/chat/hooks/useChat.ts`

**Out Of Scope**
- Do not split stores.
- Do not move backend API calls.
- Do not introduce Zustand `persist`.
- Do not introduce Immer.
- Do not create a broad selector framework unless repeated selectors become obvious.

**Execution Steps**

1. Establish the current baseline.
   - Run `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`.
   - Confirm broad subscriptions are limited to the known Phase 1 targets or document any additional targets.

2. Refactor `AppShell.tsx`.
   - Replace `useChatStore()` with selectors for only the used fields/actions.
   - Replace `useChatSessionStore()` with selectors for only the used fields/actions.
   - Replace `useAgentStore()` with selectors for only the used fields/actions.
   - Replace `useProjectStore()` with selectors for only the used fields/actions.
   - Keep `useStore.getState()` inside async callbacks where the code intentionally needs latest state at call time.

3. Refactor `Sidebar.tsx`.
   - Replace `useChatStore()` with selectors for `messagesBySession` and any action/helper actually needed during render.
   - Replace `useChatSessionStore()` broad destructuring with selectors.
   - Replace `useAgentStore()` and `useProjectStore()` broad reads with targeted selectors or narrow `getState()` usage in callbacks.
   - Keep render output and sorting behavior unchanged.

4. Refactor `usePersonas.ts`.
   - Replace `const store = useAgentStore()` with selected actions/state.
   - Use selected actions in callbacks.
   - Avoid adding dependencies that recreate timers unnecessarily.
   - If a callback needs latest store state rather than reactive state, use `useAgentStore.getState()` explicitly.

5. Refactor `useChat.ts`.
   - Replace `const store = useChatStore()` with selectors for session-specific messages/runtime and selected actions.
   - Preserve existing use of `getState()` where callbacks intentionally read current runtime state.
   - Avoid changing prompt, queue, compaction, or message mutation semantics.

**Validation**
- `rg "use[A-Za-z0-9]+Store\\(\\)" ui/goose2/src`
- `cd ui/goose2 && pnpm test -- useChat usePersonas Sidebar`

**Success Criteria**
- No high-level React component or hook in scope calls a bound Zustand hook without a selector.
- Behavior remains unchanged.
- No store boundary or persistence changes are included.
