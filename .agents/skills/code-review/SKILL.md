---
name: code-review
description: >-
  Senior engineer code review focused on catching issues before they become PR
  comments. Reviews only changed lines, categorizes issues by priority, and fixes
  them one by one. Includes a focused ui/goose2 refactor-quality pass for
  maintainability, decomposition, layering, type hygiene, duplication, and dead
  code. Use when the user says "code review", "review my code", "review this
  branch", or wants pre-PR feedback.
---

# Pre-PR Code Review

You are a senior engineer conducting a thorough code review. Review **only the lines that changed** in this branch (via `git diff main...HEAD`) and provide actionable feedback on code quality. Do not flag issues in unchanged code.

## Determine Files to Review

**Before starting the review**, identify which files to review by checking:

1. **Run git commands** to check both:
   - Committed changes: `git diff --name-only main...HEAD`
   - Unstaged/staged changes: `git status --short`

2. **Ask the user which set to review** if both exist:
   - If there are both committed changes AND unstaged/staged changes, ask: "I see you have both committed changes and unstaged/staged changes. Which would you like me to review?"
     - **Option A**: Committed changes in this branch (compare against main)
     - **Option B**: Current unstaged/staged changes
     - **Option C**: Both

3. **Proceed automatically** if only one set exists:
   - If only committed changes exist → review those
   - If only unstaged/staged changes exist → review those
   - If neither exist → inform the user there are no changes to review

4. **Get the file list** based on the user's choice:
   - For committed changes: Use `git diff --name-only main...HEAD`
   - For unstaged/staged: Use `git diff --name-only` and `git diff --cached --name-only`
   - Filter to only include files that exist (some may be deleted)

**Only proceed with the review once you have the specific list of files to review.**

## Review Passes

Run these as passes, then consolidate findings before presenting them. A finding should appear once, even if multiple sections support it.

- Use the baseline pass for correctness, regressions, async state, API/backend contracts, accessibility, i18n completeness, CI failures, and obvious cleanup.
- For `ui/goose2` maintainability, use `UI Refactor Quality` as the authoritative pass for decomposition, layering, hooks vs helpers, type hygiene, duplication, naming, module boundaries, and refactor structure.
- Do not duplicate the same underlying concern across the baseline pass and the `UI Refactor Quality` pass. Prefer the `UI Refactor Quality` framing for `ui/goose2` maintainability issues.

### Baseline Safety Pass

#### React Best Practices
- **Components**: Are functional components with hooks used consistently?
- **State Management**: Is `useState` and `useEffect` used properly? Any unnecessary re-renders?
- **Props**: Are prop types properly defined with TypeScript interfaces?
- **Keys**: Are list items using proper unique keys (not array indices)?
- **Hooks Rules**: Are hooks called at the top level and in the correct order?

#### TypeScript Best Practices
- **const vs let vs var**: Is `const` used by default? Is `let` only used when reassignment is needed? Is `var` avoided entirely?
- **Type Safety**: Are types explicit and avoiding `any`? Are proper interfaces/types defined?
- **Type Assertions**: Are type assertions (`as`) used sparingly and only when necessary?
- **Non-null Assertions**: Are non-null assertions (`!`) avoided? They bypass TypeScript's null safety and hide bugs. Use proper null checks or optional chaining instead.
- **React Ref Types**: Are React refs properly typed as nullable (`useRef<T>(null)` with `RefObject<T | null>`)? Refs are null on first render and during unmount.
- **Optional Chaining**: Is optional chaining (`?.`) used appropriately for potentially undefined values?
- **Enums vs Union Types**: Are union types preferred over enums where appropriate?

#### Design System & Styling
- **Component Usage**: Are design system components used instead of raw HTML elements (`<Button>` not `<button>`, `<Input>` not `<input>`)?
- **No Custom Styling**: Is custom inline styling or CSS avoided in favor of design system utilities?
- **Tailwind Classes**: Are Tailwind utility classes used properly and consistently?
- **Tailwind JIT Compilation**: Are Tailwind classes using static strings? JavaScript variables in template literals (e.g., `` `max-w-[${variable}]` ``) break JIT compilation. Use static strings or conditional logic instead (e.g., `condition ? 'max-w-[100px]' : 'max-w-[200px]'`).
- **Theme Tokens**: Are theme tokens used for colors that adapt to light/dark mode (e.g., `text-foreground`, `bg-card`, `text-muted-foreground`) instead of hardcoded colors (e.g., `text-black`, `bg-white`)?
- **Variants**: Could any components benefit from additional variants or properties in the design system?
- **Light and Dark Mode Support**: Are colors working properly in both light and dark modes? No broken colors?
- **Responsive Layout**: Does the layout work correctly at all breakpoints? No broken layout on mobile, tablet, or desktop?

#### Localization (i18n)
- **New Keys**: When new translation keys are added to one locale (e.g., `en`), are all other supported locales updated too? i18next falls back gracefully, but incomplete locales should be flagged.
- **Removed Keys**: When UI text is removed, are the corresponding translation keys removed from all locale files?
- **Raw Strings**: Are user-facing strings wrapped in `t()` calls instead of hardcoded in JSX? Non-translatable symbols (icons, punctuation, HTML entities) are acceptable with an `i18n-check-ignore` annotation.
- **Stable Keys**: Are translation keys stable and domain-specific instead of mirroring incidental English copy?
- **Catch Blocks**: Are user-facing errors routed through translation keys instead of raw English strings in `catch` blocks?

#### Code Simplicity (DRY Principle)
- **Duplication**: Is there any repeated code that could be extracted into functions or components?
- **Complexity**: Are there overly complex functions that could be broken down?
- **Logic**: Is the logic straightforward and easy to follow?
- **Abstractions**: Are abstractions appropriate (not too early, not too late)?
- **Guard Clauses**: Are early-return guards used to keep code shallow and readable?

#### Code Cleanliness
- **Comments**: Are there unnecessary comments explaining obvious code? (Remove them)
- **Console Logs**: Are there leftover `console.log` statements? (Remove them)
- **Dead Code**: Is there unused code, commented-out code, or unused imports?
- **Cross-Boundary Dead Data**: Are there struct/interface fields computed on one side of a boundary (e.g., Rust backend) but never consumed on the other (e.g., TypeScript frontend)? This wastes computation and adds noise to data contracts.
- **Naming**: Are variable and function names clear and descriptive?
- **Magic Numbers**: Are there magic numbers without explanation? Should they be named constants?

#### Animation & UI Polish
- **Race Conditions**: Are there any animation race conditions or timing issues?
- **Single Source of Truth**: Is state managed in one place to avoid conflicts?
- **AnimatePresence**: Is it used properly with unique keys for dialog/modal transitions?
- **Reduced Motion**: Is `useReducedMotion()` respected for accessibility?

#### Async State, Defaults & Persistence
- **Async Source of Truth**: During async provider/model/session mutations, does UI/session/localStorage state update only after the backend accepts the change? If the UI updates optimistically, is there an explicit rollback path?
- **UI/Backend Drift**: Could the UI show provider/model/project/persona X while the backend is still on Y after a failed mutation, delayed prepare, or pending-to-real session handoff?
- **Requested vs Fallback Authority**: Do explicit user or caller selections stay authoritative over sticky defaults, saved preferences, aliases, or fallback resolution?
- **Dependent State Invalidation**: When a parent selection changes (provider/project/persona/workspace/etc.), are dependent values like `modelId`, `modelName`, defaults, or cached labels cleared or recomputed so stale state does not linger?
- **Persisted Preference Validation**: Are stored selections validated against current inventory/capabilities before reuse, and do stale values fail soft instead of breaking creation flows?
- **Compatibility of Fallbacks**: Are default or sticky selections guaranteed to remain compatible with the active concrete provider/backend, instead of leaking across providers?
- **Best-Effort Lookups**: Do inventory/config/default-resolution lookups degrade gracefully on transient failure, or can they incorrectly block a primary flow that should still work with a safe fallback?
- **Draft/Home/Handoff Paths**: If the product has draft, Home, pending, or pre-created sessions, did you review those handoff paths separately from the already-active session path?

#### General Code Quality
- **Error Handling**: Are errors handled gracefully with user-friendly messages?
- **Notifications**: Are success and error messages routed through the app's shared notification primitive instead of one-off notification UI?
- **Loading States**: Are loading states shown during async operations?
- **Accessibility**: Are ARIA labels, keyboard navigation, and screen reader support considered?
- **Performance**: Are there any obvious performance issues (unnecessary re-renders, heavy computations)?
- **Git Hygiene**: Are there any files that shouldn't be committed (env files, etc.)?
- **Unrelated Changes**: Are there any stray files or changes that don't relate to the branch's main purpose? (Accidental commits, unrelated fixes)

### UI Refactor Quality
Use this focused pass for `ui/goose2` changes, especially when the user asks about cleanup, maintainability, decomposition, layering, type hygiene, duplication, dead code, readability, or extensibility.

Keep the focus on behavior-preserving frontend improvement. Favor the repo's existing architecture and patterns over generic frontend advice.

- Review changed code for refactor quality, not just correctness.
- Bias toward detecting maintainability smells even when the code is still functionally correct.
- Review the final shape of the changed code, not whether it is better than what came before.
- Judge changes by whether they leave the code easier to maintain and extend in future work, not just whether they are correct today.
- Ask for approval before making code changes unless the user explicitly asks for fixes.
- Preserve `ui/goose2` boundaries: `ui/`, `hooks/`, `api/`, `lib/`, `stores/`, and `shared/`.

#### Strict Mode
- Any confirmed smell in the changed code must be reported as an `Issue`.
- Review the post-PR shape. Do not grade on a curve.
- Partial extraction, partial deduplication, or partial cleanup does not clear a remaining smell.
- If multiple distinct smells remain in one file, report each distinct responsibility problem separately.

#### Smell Checklist
Before finalizing the review, explicitly ask:

- Is any view or page component still doing too many jobs?
- Is any pure derivation logic still trapped in a component instead of `lib/`?
- Is any repeated async UI workflow ready for a focused hook?
- Are helpers duplicated or living in the wrong layer?
- Are any inline object shapes large enough to deserve a named type?
- Did logic move without moving or adding the right tests?
- Did the refactor preserve feature wiring while improving structure?

#### Size And Decomposition
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

#### Naming Reveals Intent
- Use names that describe intent, not implementation trivia.
- Prefer domain terms over generic placeholders like `data`, `value`, or `handler`.
- A helper name should describe what it returns or decides, not how it computes it.
- Rename misleading functions before adding comments to explain them.

#### Layer Discipline
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

#### Module Encapsulation
- Export the minimum surface a module needs to share.
- Keep helpers, constants, and intermediate transforms private unless another module genuinely needs them.
- Treat removing stale exports as a quality improvement.
- If a helper is used in only one module, default to keeping it local.
- If similar helpers appear across two modules, default to extracting them.

#### DRY And Hooks
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

#### Type Hygiene
- Keep canonical cross-feature types in `src/shared/types/`.
- Do not duplicate types across features when one shared type should exist.
- Give inline object types with 3 or more fields a name when they start obscuring the code.
- Prefer `Pick`, `Omit`, and `Partial` over restating shapes by hand.
- Avoid `any`, unchecked `as`, non-null assertions, and string-encoded pseudo-unions when a discriminated union would be clearer.
- Treat repeated or verbose inline object shapes as extraction candidates for named types.
- If verbose or repeated inline shapes remain after the PR, report that as an `Issue`.

#### React And UI
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

#### Notifications, Localization, And Accessibility
- Route success and error feedback through the app's shared notification primitive.
- Route user-facing Goose UI copy through `react-i18next` in already-migrated surfaces.
- Prefer stable translation keys over inline English strings.
- Avoid raw user-facing strings inside `catch` blocks.
- Add text alternatives for icon-only or color-only affordances.
- Keep interactive semantics explicit with labels, roles, and selected state where applicable.

#### Tauri And Backend Boundaries
- Frontend-to-core communication goes through `SDK -> ACP -> goose`.
- Do not add ad hoc `fetch()` calls for goose core behavior.
- Do not add `invoke()` calls as proxies to goose core behavior; reserve them for desktop-shell concerns.
- Do not call ACP clients directly from UI components; keep backend access in `shared/api/` or `features/*/api/`.

#### Errors, State Drift, And Dead Code
- Handle errors explicitly and close to the source.
- Keep the happy path easy to see.
- In async UI flows, keep local state, persisted state, and backend-confirmed state from drifting apart.
- Delete unused exports, imports, parameters, fields, and commented-out code.
- Remove tests that only protect deleted internals rather than user-visible behavior.
- When logic moves across modules, expect coverage to move with it rather than disappear.
- Treat coverage loss in refactors as suspicious unless the behavior was intentionally removed.
- If behavior-preserving logic moved but coverage did not move with it, report that as an `Issue`.
- Report redundant props, fields, parameters, and intermediate values as `Issues`.

## Review & Fix Process

### Step 0: Run Quality Checks

Before reading any code, run the project's CI gate to establish a baseline. Use **check-only** commands so the baseline never mutates the working tree — otherwise auto-formatters can introduce unstaged diffs and you'll end up reviewing formatter output instead of the author's actual changes.

Avoid `just check-everything` as the baseline in this repo: that recipe runs `cargo fmt --all` in write mode and will modify the working tree. Run the non-mutating equivalents instead:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
(cd ui/desktop && pnpm run lint:check)
./scripts/check-openapi-schema.sh
```

If the project has a stronger pre-push or CI gate than this helper set, run that fuller gate when the review is meant to be PR-ready, but only after confirming it is also non-mutating (or run it from a clean stash). In this repo, targeted tests for the changed area plus the pre-push checks are often the practical follow-up.

Report the results as pass/fail. Any failures are automatically **P0** issues and should appear at the top of the findings list. Do not skip this step even if the user only wants a quick review.

### Step 1: Conduct Review

For each file in the list:

1. Run `git diff main...HEAD -- <file>` to get the exact lines that changed
2. Review **only those changed lines** against the review passes — do not flag issues in unchanged code, but follow changed code paths into surrounding modules when needed to verify the issue
3. For stateful UI or async flow changes, trace the full path end to end: user selection -> local/session state update -> persistence -> backend prepare/set/update call -> failure/rollback path
4. For `ui/goose2` refactors, run the UI Refactor Quality pass before finalizing findings
5. Note the file path and line numbers from the diff output for each issue found

### Step 2: Categorize Issues

Assign each issue a priority level:
- **P0**: Breaks functionality, TypeScript errors, security issues
- **P1–P2**: Performance problems, accessibility issues, code quality, unnecessary complexity, poor practices, design system violations
- **P3**: Style inconsistencies, minor improvements, missing type safety, animation issues, theme token usage
- **P4**: Cleanup — console logs, unused imports, dead code, unnecessary comments, unrelated changes

If many high-severity issues exist in a file, assess whether a full refactor would be simpler than individual fixes.

### Step 3: Present Findings

After reviewing all files, provide:
- **Summary**: Total files reviewed, overall quality rating (1-5 stars)
- **Issues**: A single numbered list ordered by priority (P0 first, P4 last). Each issue must follow this exact format:

  ```
  1. Short Issue Title (P0) [Must Fix]
     - Description of the issue and why it matters
     - User effect if this ships
     - Recommended fix

  2. Short Issue Title (P3) [Your Call]
     - Description of the issue and why it may or may not need addressing
     - User effect if this ships
     - Recommended fix if the user chooses to act on it
  ```

  Write the user-effect bullet in product language: describe what the user would experience, misunderstand, lose, or be blocked from doing if the issue reached production.

  Use a short, descriptive title (3–6 words max) so issues can be referenced by number (e.g. "fix issue 3").

### Step 3b: Self-Check

Before presenting findings to the user, silently review the issue list three times:

1. **Pass 1**: For each issue, ask — is this genuinely a problem, or could it be intentional/acceptable? Remove false positives.
2. **Pass 2**: For each remaining issue, ask — does the recommended fix actually improve the code, or is it a matter of preference?
3. **Pass 3**: For async state/default-resolution issues, ask — can the UI, persisted state, and backend ever disagree after a failure, fallback, or session handoff?
4. **Pass 4**: For `ui/goose2` refactors, ask — did any confirmed final-shape smell survive in decomposition, layering, hooks/effects, pure helpers, type shapes, duplication, tests, or feature wiring?

After these passes, tag each surviving issue as one of:
- **[Must Fix]** — clear violation, will likely get flagged in PR review
- **[Your Call]** — valid concern but may be intentional or a reasonable tradeoff (e.g. stepping outside the design system for a specific reason). Present it but let the user decide.

Only present issues that survived these passes.

Merge duplicate concerns before presenting findings. If the same underlying issue appears in multiple passes, report it once under the most specific applicable reason. Prefer the `UI Refactor Quality` framing for `ui/goose2` maintainability issues.

Do not include an "Applied Well" section in the review output. If there are no issues, say that clearly and mention any remaining test gaps or residual risk.

### Step 4: Fix Issues

**Before fixing**, ask: "Would you like me to fix these issues in order? Or do you have questions about any of them first? I will fix each issue one by one and ask for approval before moving to the next one."

**When approved**, work through issues one at a time in numbered order (P0 → P4). After each fix:
1. Explain what was changed and why
2. Ask: "Does that look good? Ready to move on to issue [N]?"
3. Wait for confirmation before proceeding to the next issue

**Important**: When adding documentation comments:
- Only add comments for non-obvious things: magic numbers, complex logic, design decisions, or workarounds
- If you call out something as confusing or hard-coded in your review and suggest adding documentation, it's acceptable to add a comment when approved
- Don't add comments that just restate what the code does

Explain each change as you make it. If an issue is too subjective or minor, skip it and note why.

**Remember**: Cleanup tasks like removing comments should always be done LAST, because earlier fixes might introduce new comments that also need removal.

### Step 5: Ready to Ship

Once all issues are fixed, display:

---

**✅ Code review complete! All issues have been addressed.**

Your code is ready to commit and push. Lefthook and CI will run the repo's configured gates when you push.

Next steps: generate a PR summary that explains the intent of this change, what files were modified and why, and how to verify the changes work.

---
