**Phase 8: Standardize Test Reset Patterns And Close Coverage Gaps**

**Status**
- Not started.

**Goal**
- Make store and hook tests safer after the state boundaries have stabilized.
- Replace partial shallow-merge resets with known full-state resets.
- Add coverage for risky persistence and mutation policy paths.

**Scope**
- Store tests for chat, chat sessions, agents, projects, providers, and any UI stores introduced in Phase 4.
- Hook tests that directly manipulate Zustand stores.
- Coverage gaps discovered in Phases 3 through 6.

**Out Of Scope**
- Do not rewrite all tests for style.
- Do not add broad snapshot-style coverage.
- Do not introduce a large test utility framework unless duplication is clearly painful.

**Execution Steps**

1. Add full initial-state reset helpers.
   - Prefer one helper per store.
   - Keep helpers easy to update when store shape changes.
   - Include actions only if the store's reset approach requires them.

2. Replace partial resets in existing tests.
   - `agentStore.test.ts`
   - `usePersonas.test.ts`
   - `chatStore.test.ts`
   - Chat hook tests that call `setState` directly.
   - New store tests from earlier phases.

3. Add project store coverage.
   - Cache hydration.
   - Add/edit/remove local state results.
   - Reorder semantics.
   - Reorder failure policy if implemented or documented.

4. Add provider inventory store coverage if still missing.
   - `setEntries` replaces inventory.
   - `mergeEntries` preserves existing providers and updates overlapping providers.
   - Loading behavior.

5. Add session mutation orchestration coverage.
   - Local patch does not call backend APIs.
   - Rename operation calls backend rename.
   - Project update operation calls backend project update.
   - Archive/unarchive operation behavior.

6. Remove stale assumptions from tests.
   - Tests should not rely on fields that moved to UI stores.
   - Tests should reset every store touched by the scenario.
   - Component-test mocks for Zustand bound hooks should support selector-style calls. Phase 1 moved production code from `useStore()` to `useStore((state) => ...)`, so mocks should accept an optional selector and invoke it with the mocked state shape.
   - Tests that import Phase 2 selector files should still match the final store boundaries after Phases 4 and 5. Remove or move selector imports if the selected state moved to a new store.
   - Recheck test reset assumptions around legacy/general fields such as `agentStore.isLoading`. If Phase 4 removes or narrows those fields, reset helpers should not preserve stale state shape.

**Validation**
- `cd ui/goose2 && pnpm test`
- `rg "setState\\(\\{" ui/goose2/src/features ui/goose2/src/shared`

**Success Criteria**
- Store tests reset from known initial state.
- Risky mutation and persistence paths have direct coverage.
- Test setup matches the final store boundaries.
