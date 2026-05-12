**Phase 7: Standardize Persistence Boundaries**

**Status**
- Not started.

**Goal**
- Make durable state intentional and consistent.
- Reduce ad hoc localStorage handling where Zustand `persist` or isolated persistence helpers provide a cleaner boundary.

**Scope**
- `ui/goose2/src/features/projects/stores/projectStore.ts`
- `ui/goose2/src/features/agents/stores/agentStore.ts`
- `ui/goose2/src/features/chat/stores/draftPersistence.ts`
- Any persistence introduced or clarified in earlier phases.

**Out Of Scope**
- Do not persist transient runtime state.
- Do not persist UI-only modal/open state by default.
- Do not convert isolated, well-tested persistence helpers just for consistency.
- Do not change product decisions about what survives reload without recording the decision.

**Execution Steps**

1. Inventory durable state.
   - Selected provider.
   - Chat drafts.
   - Project cache.
   - Home session id if relevant to this area.
   - Model preferences if relevant to this area.

2. Classify each durable value.
   - Product preference.
   - Cache for faster bootstrap.
   - Draft/user input recovery.
   - Runtime state that should not be persisted.
   - UI state that should not be persisted.

3. Decide persistence mechanism per value.
   - Use Zustand `persist` only when the persisted data is clearly store-owned.
   - Keep separate helper modules for persistence that is intentionally outside a store or already isolated.
   - Use `partialize` and versioning when adopting `persist`.

4. Migrate one persistence area at a time.
   - Selected provider is the smallest candidate.
   - Project cache should wait until project store ownership is clear.
   - Draft persistence can remain as-is if the helper is clearer than `persist`.

5. Add tests around migration behavior.
   - Hydration/default behavior.
   - Invalid stored data fallback.
   - Version/migration behavior if `persist` is used.
   - Non-persistence of transient state.

**Validation**
- `rg "localStorage|getItem|setItem|persist\\(" ui/goose2/src/features`
- `cd ui/goose2 && pnpm test -- chatStore agentStore project`

**Success Criteria**
- Durable state choices are documented and minimal.
- Persistence code has clear ownership.
- No runtime or temporary UI state is accidentally persisted.
