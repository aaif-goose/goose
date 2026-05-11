**Goose2 Zustand Review: Code Quality + Architecture**

If the bar is “easy to maintain later” and “low future bug surface,” the current Zustand usage in `ui/goose2` does **not** meet that bar yet.

This review combines two perspectives:
- strict Zustand usage and code-quality review
- higher-level architectural review of how Zustand is being used in the frontend

The conclusions are based on reading the current `ui/goose2` stores, major consumers, and store tests.

Primary reference points from the official Zustand docs:
- Intro and selector usage: https://zustand.docs.pmnd.rs/getting-started/introduction
- Flux-inspired practice: https://zustand.docs.pmnd.rs/learn/guides/flux-inspired-practice
- Slices pattern: https://zustand.docs.pmnd.rs/learn/guides/slices-pattern
- Auto-generating selectors: https://zustand.docs.pmnd.rs/learn/guides/auto-generating-selectors
- `useShallow`: https://zustand.docs.pmnd.rs/learn/guides/prevent-rerenders-with-use-shallow
- `persist`: https://zustand.docs.pmnd.rs/reference/middlewares/persist
- Reset pattern: https://zustand.docs.pmnd.rs/learn/guides/how-to-reset-state
- `Map` / `Set` usage: https://zustand.docs.pmnd.rs/learn/guides/maps-and-sets-usage

**Overall Verdict**

`ui/goose2` uses Zustand well enough to ship features, but not strictly enough for long-term maintainability.

The biggest problems are:
1. broad subscriptions to entire stores in React
2. stores with weak responsibility boundaries
3. state stores doubling as workflow and backend side-effect layers
4. ad hoc persistence and inconsistent durability rules
5. insufficiently explicit mutation/reconciliation policy
6. inconsistent testing/reset discipline

This is not a case of “Zustand is wrong.” It is a case of “the current architecture makes Zustand too easy to use as a global bucket for state, persistence, and orchestration.”

**What Is Good**

- State updates mostly go through `set` and are implemented immutably.
- `Map` and `Set` usage is correct where present, because updates create new instances. Examples: [providerInventoryStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/providers/stores/providerInventoryStore.ts:19), [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:452).
- Some consumers already use narrow selectors correctly, for example [useProviderSelection.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/useProviderSelection.ts:1).
- `getState()` usage in non-React async/event code is acceptable and not a problem by itself.
- Important stores like chat and agent stores already have meaningful tests.

**Detailed Findings**

1. **Whole-store subscriptions are the clearest Zustand misuse**

In React code, bound store hooks should not be called without selectors.

Concrete examples:
- [AppShell.tsx](/Users/lifei/Development/goose/ui/goose2/src/app/AppShell.tsx:80)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:103)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:191)
- [usePersonas.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/usePersonas.ts:12)
- [useChat.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChat.ts:108)

Why this is a problem:
- unrelated store updates can rerender large components/hooks
- consumers become tightly coupled to the full store shape
- store refactors become expensive because consumers depend on broad internals

Strict standard:
- no `useSomeStore()` without a selector in React components or hooks

2. **`useShallow` is underused where it matters**

There is effectively no `useShallow` usage in the `src/` tree.

That is not the primary problem, but it is a real gap after selector discipline is fixed.

Where it would matter:
- grouped selectors returning objects
- computed selectors returning arrays/objects
- high-level components that need a few store fields together

Good target pattern:
```ts
const { sessions, activeSessionId } = useChatSessionStore(
  useShallow((s) => ({
    sessions: s.sessions,
    activeSessionId: s.activeSessionId,
  })),
);
```

Strict standard:
- first adopt selector-first consumption
- then use `useShallow` on object/array selectors
- do not use `useShallow` on primitive selectors

More specifically, `useShallow` is good for:
- replacing whole-store subscriptions with grouped object selectors in high-level consumers such as [AppShell.tsx](/Users/lifei/Development/goose/ui/goose2/src/app/AppShell.tsx:80), [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:103), [usePersonas.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/usePersonas.ts:12), and [useChat.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/hooks/useChat.ts:108)
- derived object/array selectors in places like [useProviderInventory.ts](/Users/lifei/Development/goose/ui/goose2/src/features/providers/hooks/useProviderInventory.ts:29)

`useShallow` is not for:
- primitive selectors like `loading`, `activeSessionId`, or `selectedProvider`
- compensating for weak store boundaries or hidden store side effects
- replacing selector discipline with a blanket optimization

Maintainability conclusion:
- `useShallow` is a good tactical improvement after selector cleanup
- it does not replace the higher-value fixes in this review: selector-first reads, clearer store boundaries, explicit side-effect boundaries, and standardized persistence/reconciliation rules

3. **Stores are too broad in responsibility**

Separate stores are not automatically wrong, but several individual stores are carrying too many concerns.

Examples:
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:39)
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:34)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:55)
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:37)

What that looks like:
- `chatStore` mixes messages, runtime, drafts, queue, connection state, loading/replay state, scroll targeting, and cleanup
- `agentStore` mixes personas, agents, providers, selected provider, active agent, and persona editor UI state
- `chatSessionStore` mixes session records, active session selection, context-panel state, and workspace UI state
- `projectStore` mixes project records, loading state, persistence, CRUD orchestration, optimistic reordering, and active selection

Why this is a problem:
- unrelated state changes share subscriber surfaces
- tests become broader than necessary
- small changes require too much knowledge of one store’s internals

Immer is relevant here only as an update-readability tool, not as a boundary fix. It may improve maintainability in nested update-heavy stores such as [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:104), where updates to `messagesBySession`, `sessionStateById`, `draftsBySession`, and `scrollTargetMessageBySession` currently require substantial object spread boilerplate, and to a lesser extent in [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:137). It would not solve the higher-value problems in this review: broad store responsibilities, workflow/state mixing, hidden backend side effects, or ad hoc persistence. It is also unlikely to add much value to already-flat stores like [providerInventoryStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/providers/stores/providerInventoryStore.ts:19).

4. **Domain state and UI state are mixed together**

This is one of the clearest architectural problems.

Examples:
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:56)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:60)

Specifically:
- `agentStore` holds persona/agent/provider data and also persona editor modal state
- `chatSessionStore` holds session data and also context panel open state plus per-session workspace UI state

Why this is a problem:
- UI interaction state and domain data are forced into the same lifecycle
- unrelated UI churn can affect store consumers interested in durable feature data
- state boundaries are harder to reason about

Strict standard:
- if state is only for one screen or interaction, prefer local state or a focused UI store
- if state is shared across views or sessions, use Zustand
- do not default to putting modal/open-state UI into domain-heavy stores

5. **Store actions hide backend and persistence side effects**

The clearest example is `chatSessionStore.updateSession`.

Code:
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:195)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:211)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:224)

Why this is a problem:
- `updateSession` sounds like a local patch, but it may also rename a backend session or update the backend project association
- action names stop being truthful about side effects
- rollback, retry, and reconciliation responsibilities become unclear

Strict standard:
- local patch actions should be local only
- side-effectful operations should be explicit, for example `renameSessionAndPersist`
- transport and orchestration should stay in `api/` modules or dedicated hooks/commands

6. **Zustand is being used as a workflow layer, not just a state layer**

This is the main architectural synthesis.

Examples:
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:73)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:195)
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:415)

What to look for:
- local storage helpers embedded in stores
- API calls inside store actions
- optimistic update behavior living in the store
- store methods that are really workflow/orchestration entry points

Architectural conclusion:
- Zustand should primarily own shared client state
- hooks and command-style modules should own workflow/orchestration
- `api/` should own backend transport

7. **Persistence is hand-rolled where `persist` would provide a cleaner boundary**

Examples:
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:11)
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:5)
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:18)

Why this is a problem:
- repeated `localStorage` code
- each store invents its own hydration semantics
- no versioning, migrations, or `partialize`
- persistence rules are not centralized or explicit

Architectural conclusion:
- durability should be a standard boundary, not a per-store ad hoc decision

8. **Selectors are not a stable architectural read API yet**

The codebase often relies on imperative helper methods instead of a selector-oriented read layer.

Examples:
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:188)
- [agentStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/agentStore.ts:213)
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:181)
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:184)
- [chatStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatStore.ts:191)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:329)

These helpers are acceptable for `getState()` usage in non-React code, but in React they push the app toward imperative reads instead of reactive selectors.

Architectural conclusion:
- the app needs a selector-first read API
- common selectors should be promoted into helper hooks
- React components should not depend on imperative store helper methods by default

9. **Some render paths perform heavy derived work while broadly subscribed**

The clearest example is the sidebar.

Code:
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:103)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:145)
- [Sidebar.tsx](/Users/lifei/Development/goose/ui/goose2/src/features/sidebar/ui/Sidebar.tsx:194)

Why this is a problem:
- broad subscription plus per-render grouping/sorting/runtime lookups compounds rerender cost
- derivation logic is harder to isolate, test, and reuse

Architectural conclusion:
- central derived state patterns should live in selector helpers or pure `lib/` utilities
- avoid render-time traversal of large store structures when the same logic is core to the feature

10. **`projectStore` is the strongest example of architectural overreach**

Code:
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:73)
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:78)
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:157)

It currently acts as:
- project state store
- local cache owner
- CRUD orchestration layer
- optimistic reorder owner
- active selection owner

Why this is a problem:
- one module owns too many failure modes
- source-of-truth rules are muddy
- harder to add retries, rollback, or refresh policies later

11. **Async mutation and reconciliation policy is not explicit enough**

Examples:
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:157)
- [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:251)

What to look for:
- local state updated first
- backend call later
- failures mostly logged
- no explicit rollback or refresh strategy encoded in the abstraction

Architectural conclusion:
- each mutation should clearly be one of:
  - pessimistic
  - optimistic with rollback
  - optimistic with guaranteed refresh
- that policy should be explicit and consistent

12. **Test reset discipline is not strict enough**

Examples:
- [agentStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/__tests__/agentStore.test.ts:37)
- [usePersonas.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/__tests__/usePersonas.test.ts:53)
- [chatStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/__tests__/chatStore.test.ts:22)

Why this is a problem:
- `setState` shallow-merges
- tests sometimes omit fields when resetting state
- stale state can leak across tests

Strict standard:
- reset each store from its initial state or `getInitialState()` pattern

13. **Coverage is uneven for side-effectful stores**

Well-covered:
- [agentStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/stores/__tests__/agentStore.test.ts:1)
- [chatStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/__tests__/chatStore.test.ts:1)
- [chatSessionStore.test.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/__tests__/chatSessionStore.test.ts:1)

Noticeably under-covered:
- [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:1)
- [providerInventoryStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/providers/stores/providerInventoryStore.ts:1)

Missing high-value coverage areas:
- project cache hydration
- reorder semantics
- optimistic failure policy
- inventory merge semantics

14. **There are no clear house rules for what belongs in Zustand**

This is the meta architectural issue.

Code evidence comes from inconsistency across modules:
- broad whole-store consumer: [AppShell.tsx](/Users/lifei/Development/goose/ui/goose2/src/app/AppShell.tsx:80)
- narrow selector consumer: [useProviderSelection.ts](/Users/lifei/Development/goose/ui/goose2/src/features/agents/hooks/useProviderSelection.ts:1)
- store-embedded persistence: [projectStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/projects/stores/projectStore.ts:11)
- store-embedded backend side effects: [chatSessionStore.ts](/Users/lifei/Development/goose/ui/goose2/src/features/chat/stores/chatSessionStore.ts:195)
- local UI state still handled in component: [AppShell.tsx](/Users/lifei/Development/goose/ui/goose2/src/app/AppShell.tsx:58)

What this means:
- the codebase does not yet have a shared rule set for:
  - local vs shared state
  - ephemeral vs durable state
  - store state vs orchestration logic
  - selector API expectations
  - persistence boundaries

**Clarifications So We Don’t Overstate**

- Multiple separate stores are not automatically wrong.
- `Map` and `Set` are not wrong here; they are used correctly.
- `getState()` is not wrong in non-React async/event code.
- Colocating actions with state is a recommended Zustand pattern. The issue is not action location by itself, but unclear responsibilities and hidden side effects.
- The architecture recommendations below are a synthesis of the current findings. They are not all direct “code defects,” but they are the most defensible target design responses to the issues in the current code.

**Architectural Target Direction**

The clean target architecture is:
- React local state for local view concerns
- Zustand for shared client state needed by multiple consumers
- `api/` modules for backend transport
- hooks or command-style modules for orchestration and reconciliation
- pure `lib/` utilities for derivation and transformation

Zustand should be a small, predictable shared-state layer, not a catch-all workflow layer.

A healthier direction would be to separate concerns more explicitly, for example:
- session data state vs session UI state
- agent/provider catalog state vs editor/modal UI state
- chat message state vs chat composer/runtime state
- workflow commands/hooks for backend mutations and reconciliation

This exact split does not need to be implemented all at once, but the codebase should move in that direction.

**Recommended Standard For `ui/goose2`**

- Never call a bound Zustand hook without a selector in React components or hooks.
- Use `useShallow` only when a selector returns an object or array.
- Keep store actions either:
  - pure local state transitions, or
  - explicitly named async commands with clear side-effect semantics.
- Do not mix domain data state and modal/open-state UI unless truly necessary.
- Use `persist` for durable state, with `partialize` and `version`.
- Prefer selector hooks/helpers for React reads.
- Use `getState()` mainly in non-React async/event code.
- Reset stores in tests from initial state, not partial merge patches.
- When a store grows large, split it by responsibility or by slices.
- For every backend mutation, make the optimistic/pessimistic/reconciliation policy explicit.
- Establish a team rule for what belongs in Zustand vs local state vs hooks vs `api/`.

**Bottom Line**

Strictly judged against Zustand good practices and from an architectural perspective, `ui/goose2` is **adequate for shipping features quickly, but not disciplined enough for long-term maintainability**.

The highest-value issues to fix are:
1. whole-store subscriptions
2. broad mixed-responsibility stores
3. domain state and UI state mixed in the same stores
4. hidden backend side effects in generic store actions
5. Zustand being used as a workflow layer instead of primarily a state layer
6. ad hoc persistence
7. weak selector and test-reset discipline

These are the areas most likely to create future bugs and make the code harder to refactor, reason about, and test.
