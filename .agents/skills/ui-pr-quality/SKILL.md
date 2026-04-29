---
name: ui-pr-quality
description: >-
  Review and improve ui/goose2 frontend pull requests. Use when the user asks
  to review a PR, assess code quality, make a cleanup plan, or fix readability,
  layering, naming, type hygiene, duplication, or dead-code issues in goose2
  React + TypeScript + Tauri UI code.
---

# UI Refactor Quality

Use this skill for `ui/goose2` pull requests.

Keep the focus on behavior-preserving frontend improvement. Favor the repo's
existing architecture and patterns over generic frontend advice.

## Goals

- Review changed code for refactor quality, not just correctness.
- Produce an actionable checklist instead of vague feedback.
- Fix issues in a safe order by default.
- Preserve `ui/goose2` boundaries: `ui/`, `hooks/`, `api/`, `lib/`, `stores/`, and `shared/`.

## Steps

1. Determine the review scope.
   - Review only the changed lines in the branch or working tree.
   - If both committed and uncommitted changes exist, clarify which scope to review when needed.
   - If the change mixes feature work with refactoring, call that out explicitly and review the feature changes separately from the cleanup quality.
2. Inspect only the changed lines in that scope.
3. Evaluate the changed code against the `Rules` below and identify what the PR already improved and what should still be refactored or cleaned up.
4. Verify each non-trivial issue against the actual code before turning it into a task.
   - Trace the relevant code path end to end.
   - Check whether the issue is already handled elsewhere.
   - Confirm the suggested cleanup would actually simplify the code.
   - Drop speculative or preference-only findings.
5. Produce review output in this order:
   - `Applied Well`
   - `Issues`
   - one flat `Checklist` for the whole reviewed scope
   - `Suggested Order`
   - `Verdict`
6. Fix checklist items in order, using the `Rules` below as the quality bar for the implementation.
   - State the main maintainability problem in one sentence.
   - Fix the highest-value items first.
   - Make the smallest behavior-preserving change that clearly improves the code.
7. Summarize what changed, what remains, and what verification ran.

## Rules

### Size And Decomposition

- Treat these as smell thresholds, not hard limits:
  - components around 200 lines
  - functions around 40 lines
  - files around 300 lines
  - JSX nesting around 4 levels
- If a component does more than its name claims, rename it or split it.
- Split by responsibility, not by arbitrary line count.

### Naming Reveals Intent

- Use names that describe intent, not implementation trivia.
- Prefer domain terms over generic placeholders like `data`, `value`, or `handler`.
- A helper name should describe what it returns or decides, not how it computes it.
- Rename misleading functions before adding comments to explain them.

### Layer Discipline

- `ui/`: rendering and light view logic only.
- `hooks/`: glue between React state/effects and lower layers.
- `api/`: backend transport wrappers and DTO adaptation only.
- `lib/`: pure functions and domain helpers only.
- `stores/`: shared feature state only.
- Keep business logic out of render-heavy components when a hook or utility would make it clearer.
- Do not move simple local state into a store unless multiple consumers truly need it.
- Keep `api/` free of UI imports, path logic, and unrelated domain policy.
- Keep `lib/` free of React, DOM, `window`, and I/O.

### Module Encapsulation

- Export the minimum surface a module needs to share.
- Keep helpers, constants, and intermediate transforms private unless another module genuinely needs them.
- Treat removing stale exports as a quality improvement.

### DRY And Hooks

- Extract shared behavior once the duplication is clear and the shared abstraction is stable.
- Two call sites can be enough when the shared shape is obvious and both call sites become simpler.
- Prefer a hook when the shared logic is stateful or effectful.
- Keep each hook focused on one job.
- Keep hook return shapes stable so callers are not forced to handle shifting contracts.

### Type Hygiene

- Keep canonical cross-feature types in `src/shared/types/`.
- Do not duplicate types across features when one shared type should exist.
- Give inline object types with 3 or more fields a name when they start obscuring the code.
- Prefer `Pick`, `Omit`, and `Partial` over restating shapes by hand.
- Avoid `any`, unchecked `as`, non-null assertions, and string-encoded pseudo-unions when a discriminated union would be clearer.

### React And UI

- Prefer straight-line render logic, guard clauses, and early returns over deep nesting.
- Prefer controlled components where practical.
- Use semantic HTML like `<main>`, `<nav>`, `<header>`, and `<aside>`.
- Every plain `<button>` must include `type="button"`.
- Use `cn()` from `@/shared/lib/cn` for Tailwind class merging.
- Prefer existing shared UI primitives before creating new one-off markup patterns.
- Avoid inline styles except for truly dynamic values.
- Respect reduced-motion behavior when touching animation.

### Notifications, Localization, And Accessibility

- Route success and error feedback through the app's shared notification primitive.
- Route user-facing Goose UI copy through `react-i18next` in already-migrated surfaces.
- Prefer stable translation keys over inline English strings.
- Avoid raw user-facing strings inside `catch` blocks.
- Add text alternatives for icon-only or color-only affordances.
- Keep interactive semantics explicit with labels, roles, and selected state where applicable.

### Tauri And Backend Boundaries

- Frontend-to-core communication goes through `SDK -> ACP -> goose`.
- Do not add ad hoc `fetch()` calls for goose core behavior.
- Do not add `invoke()` calls as proxies to goose core behavior; reserve them for desktop-shell concerns.
- Do not call ACP clients directly from UI components; keep backend access in `shared/api/` or `features/*/api/`.

### Errors, State Drift, And Dead Code

- Handle errors explicitly and close to the source.
- Keep the happy path easy to see.
- In async UI flows, keep local state, persisted state, and backend-confirmed state from drifting apart.
- Delete unused exports, imports, parameters, fields, and commented-out code.
- Remove tests that only protect deleted internals rather than user-visible behavior.

## Review Output

### Applied Well

- List what the PR already improved.
- Use concrete examples with file references.
- Skip generic praise.

### Issues

- List only issues that are actually in scope for the changed code.
- For each issue, explain:
  - what is wrong
  - why it matters
  - the smallest change that would improve it
- Only include issues that survived a verification pass against the actual code.

### Checklist

- End with one flat actionable checklist for the whole reviewed scope.
- Do not create a separate checklist per issue.
- Each item should be specific enough to implement directly.
- Each item should be small enough to fix as one unit.
- If an item would require sub-steps, split it into multiple checklist items instead of nesting.

### Suggested Order

- Put that checklist in the order it should be fixed:
  - boundary and layering issues first
  - naming and decomposition next
  - type and hook cleanup after that
  - dead code and polish last

### Verdict

- End with one short line describing the PR's current quality level and the next logical cleanup pass.

## Done Criteria

- No unresolved in-scope boundary violations remain.
- The code is clearer without changing intended behavior.
- No new dead code or needless exports were introduced.
- Naming and decomposition are improved where the review identified them.
- Review findings were verified before being turned into fix tasks.
- Verification was run when appropriate, or explicitly called out if not run.
