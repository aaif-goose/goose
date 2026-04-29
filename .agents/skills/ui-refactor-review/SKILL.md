---
name: ui-refactor-review
description: >-
  Review ui/goose2 frontend changes specifically for refactoring opportunities
  and long-term maintainability. Use when the user wants cleanup feedback or
  wants to improve structure, decomposition, layering, type hygiene,
  duplication, dead code, readability, and extensibility in goose2 React +
  TypeScript + Tauri UI code.
---

# UI Refactor Quality

Use this skill for `ui/goose2` pull requests.

Keep the focus on behavior-preserving frontend improvement. Favor the repo's
existing architecture and patterns over generic frontend advice.

## Goals

- Review changed code for refactor quality, not just correctness.
- Bias toward detecting maintainability smells even when the code is still functionally correct.
- Review the final shape of the changed code, not whether it is better than what came before.
- Judge changes by whether they leave the code easier to maintain and extend in future work, not just whether they are correct today.
- Produce an actionable checklist instead of vague feedback.
- Ask for approval before making code changes unless the user explicitly asks for fixes.
- Preserve `ui/goose2` boundaries: `ui/`, `hooks/`, `api/`, `lib/`, `stores/`, and `shared/`.

## Workflow

1. Determine the review scope.
   - Review only the changed lines in the branch or working tree.
   - If both committed and uncommitted changes exist, clarify which scope to review when needed.
   - If the change mixes feature work with refactoring, call that out explicitly and review the feature changes separately from the cleanup quality.
2. Inspect only the changed lines in that scope, but follow each changed code path into surrounding modules when needed to judge whether the shape is clean.
3. Run the `Smell Checklist` below before looking for bugs.
   - Do not require a user-visible bug before calling out a maintainability problem.
   - Prefer concrete findings like "this responsibility is misplaced" or "this logic should be extracted" over generic style commentary.
   - If a Smell Checklist item is true, turn it into an `Issue`.
   - Do not leave a confirmed smell unaccounted for in the final review.
   - Do not suppress an `Issue` because the PR already improved the previous version.
   - Do not treat partial cleanup as resolution. A smell that remains in the post-PR code is still an `Issue`.
4. Evaluate the changed code against the `Rules` below and identify what the PR already improved and what should still be refactored or cleaned up.
5. Review the changed code separately for these buckets:
   - decomposition
   - layering
   - hooks/effects
   - pure helpers
   - type shapes
   - duplication
   - tests
   - feature wiring
   - For each bucket, explicitly determine whether zero, one, or multiple `Issues` remain.
   - If any remain, report all distinct `Issues` you can verify in that bucket.
6. Verify each non-trivial issue against the actual code before turning it into a task.
   - Trace the relevant code path end to end.
   - Check whether the issue is already handled elsewhere.
   - Confirm the suggested cleanup would actually simplify the code.
   - Keep the finding if the maintainability problem is real, local, and behavior-preserving to fix, even when the code still works.
   - Drop speculative or preference-only findings.
7. Before finalizing the review, run a second pass focused only on finding issues not already listed.
   - Discover issues before prioritizing them.
   - Do not stop after the first few findings.
   - Do not omit a verified issue because it is lower priority than other findings.
8. Produce review output in this order:
   - `Applied Well`
   - `Issues`
   - one ordered `Checklist` for the whole reviewed scope
   - Do not stop after the findings until the ordered checklist is complete.
9. Stop after the checklist and ask for approval before making code changes, unless the user explicitly asked to implement fixes.
10. Fix approved checklist items in order, using the `Rules` below as the quality bar for the implementation.
   - State the main maintainability problem in one sentence.
   - Fix the highest-value items first.
   - Make the smallest behavior-preserving change that clearly improves the code.
11. Summarize what changed, what remains, and what verification ran.

## Strict Mode

- Any confirmed smell in the changed code must be reported as an `Issue`.
- Review the post-PR shape. Do not grade on a curve.
- Partial extraction, partial deduplication, or partial cleanup does not clear a remaining smell.
- If multiple distinct smells remain in one file, report each distinct responsibility problem separately.

## Smell Checklist

Before finalizing the review, explicitly ask:

- Is any view or page component still doing too many jobs?
- Is any pure derivation logic still trapped in a component instead of `lib/`?
- Is any repeated async UI workflow ready for a focused hook?
- Are helpers duplicated or living in the wrong layer?
- Are any inline object shapes large enough to deserve a named type?
- Did logic move without moving or adding the right tests?
- Did the refactor preserve feature wiring while improving structure?

## Rules

### Size And Decomposition

- Treat these as smell thresholds, not hard limits:
  - components around 200 lines
  - functions around 40 lines
  - files around 300 lines
  - JSX nesting around 4 levels
- Treat "many unrelated state variables + many handlers + many effects in one view" as a smell even when the line count is still tolerated.
- Treat a file that owns multiple unrelated responsibilities across data loading, derivation, mutation, and rendering orchestration as a smell unless there is a strong reason to keep it together.
- If a component does more than its name claims, rename it or split it.
- Split by responsibility, not by arbitrary line count.
- When a view contains substantial pure derivation logic, prefer extracting it into `lib/` helpers with direct tests.
- When a view contains substantial effectful workflow logic, prefer extracting it into a focused hook.
- Do not suppress a decomposition `Issue` just because the PR already extracted some responsibilities.
- If the remaining file still does too many jobs, report that as an `Issue`.
- File size alone is not the finding. The finding is the number of unrelated responsibilities still owned by the final file.

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
- If a component mixes pure transforms and UI event orchestration, split the pure transforms out first.
- Do not move simple local state into a store unless multiple consumers truly need it.
- Keep `api/` free of UI imports, path logic, and unrelated domain policy.
- Keep `lib/` free of React, DOM, `window`, and I/O.
- Prefer shared domain helpers in `lib/` when the same normalization, formatting, or parsing logic appears in multiple modules.
- If logic lives in the wrong layer after the PR, report that as an `Issue` even if the PR reduced the amount of misplaced logic.

### Module Encapsulation

- Export the minimum surface a module needs to share.
- Keep helpers, constants, and intermediate transforms private unless another module genuinely needs them.
- Treat removing stale exports as a quality improvement.
- If a helper is used in only one module, default to keeping it local.
- If similar helpers appear across two modules, default to extracting them.

### DRY And Hooks

- Extract shared behavior once the duplication is clear and the shared abstraction is stable.
- Two call sites can be enough when the shared shape is obvious and both call sites become simpler.
- Prefer a hook when the shared logic is stateful or effectful.
- Keep each hook focused on one job.
- Keep hook return shapes stable so callers are not forced to handle shifting contracts.
- Do not use a hook as the default extraction target for oversized components.
- If the logic is pure and React-independent, report extraction to `lib/`.
- If the logic coordinates React state, effects, async actions, or UI event orchestration, report extraction to `hooks/`.
- Treat repeated pure UI derivation logic as helper extraction candidates.
- If repeated effectful orchestration remains in the changed code, report that as an `Issue`.
- If repeated pure transforms remain in the changed code, report that as an `Issue`.

### Type Hygiene

- Keep canonical cross-feature types in `src/shared/types/`.
- Do not duplicate types across features when one shared type should exist.
- Give inline object types with 3 or more fields a name when they start obscuring the code.
- Prefer `Pick`, `Omit`, and `Partial` over restating shapes by hand.
- Avoid `any`, unchecked `as`, non-null assertions, and string-encoded pseudo-unions when a discriminated union would be clearer.
- Treat repeated or verbose inline object shapes as extraction candidates for named types.
- If verbose or repeated inline shapes remain after the PR, report that as an `Issue`.

### React And UI

- Prefer straight-line render logic, guard clauses, and early returns over deep nesting.
- Prefer controlled components where practical.
- Use semantic HTML like `<main>`, `<nav>`, `<header>`, and `<aside>`.
- Prefer existing shared UI button primitives over plain `<button>` elements.
- Treat new plain `<button>` usage as a refactor smell unless there is a specific semantic or integration reason.
- If a plain `<button>` is genuinely necessary, it must use `type="button"` in goose2.
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
- When logic moves across modules, expect coverage to move with it rather than disappear.
- Treat coverage loss in refactors as suspicious unless the behavior was intentionally removed.
- If behavior-preserving logic moved but coverage did not move with it, report that as an `Issue`.
- Report redundant props, fields, parameters, and intermediate values as `Issues`.

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

- End with one ordered actionable checklist for the whole reviewed scope.
- Do not create a separate checklist per issue.
- Each item should be specific enough to implement directly.
- Each item should be small enough to fix as one unit.
- If an item would require sub-steps, split it into multiple checklist items instead of nesting.
- Treat `Checklist` as the complete fix inventory for the reviewed scope.
- Do not defer concrete fix items from `Checklist` into later sections.
- Each checklist item must describe a concrete code change, not a high-level goal.
- A user should be able to implement the fix directly from the `Checklist`.
- Order the checklist by implementation sequence:
  - boundary and layering issues first
  - naming and decomposition next
  - type and hook cleanup after that
  - dead code and polish last

## Done Criteria

- No unresolved in-scope boundary violations remain.
- The code is clearer without changing intended behavior.
- No new dead code or needless exports were introduced.
- Naming and decomposition are improved where the review identified them.
- Review findings were verified before being turned into fix tasks.
- Verification was run when appropriate, or explicitly called out if not run.
