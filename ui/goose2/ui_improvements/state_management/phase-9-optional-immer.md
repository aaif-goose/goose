**Phase 9: Optional Immer Adoption**

**Status**
- Not started.

**Goal**
- Improve nested update readability only where structural cleanup did not solve the pain.
- Avoid using Immer as a substitute for better store boundaries.

**Scope**
- `ui/goose2/src/features/chat/stores/chatStore.ts`
- Possibly `ui/goose2/src/features/chat/stores/chatSessionStore.ts`

**Out Of Scope**
- Do not add Immer before earlier phases are complete.
- Do not use Immer to justify keeping a broad store.
- Do not convert flat stores where object spread is already clear.
- Do not make dependency changes manually in `Cargo.toml`; for frontend packages follow the repo's package workflow.

**Execution Steps**

1. Reassess update complexity after Phases 1 through 7.
   - Check whether `chatStore` nested updates are still hard to read.
   - Check whether store splits or selectors already reduced the pain enough.

2. Decide whether Immer is worth the dependency and pattern change.
   - Adopt only if it materially improves readability.
   - Skip this phase if the remaining updates are acceptable.

3. If adopting, start with one store.
   - Prefer `chatStore` first.
   - Convert a small group of related nested updates.
   - Keep behavior exactly the same.

4. Add or update focused tests.
   - Messages update correctly.
   - Runtime state updates correctly.
   - Drafts and cleanup still work.
   - No accidental mutation leaks between sessions.

5. Reassess before expanding.
   - Only convert `chatSessionStore` if there is still meaningful nested update boilerplate.

**Validation**
- `cd ui/goose2 && pnpm test -- chatStore chatSessionStore useChat`
- Focused code review for accidental mutation or changed reference behavior.

**Success Criteria**
- Update logic is easier to read.
- Behavior is unchanged.
- Immer is used selectively and not as an architectural workaround.
