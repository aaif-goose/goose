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

Default to reporting what needs to be fixed. Do not include an "Applied Well" or praise section unless the user explicitly asks for positive feedback.

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

## Review Checklist

### React Best Practices
- **Components**: Are functional components with hooks used consistently?
- **State Management**: Is `useState` and `useEffect` used properly? Any unnecessary re-renders?
- **Props**: Are prop types properly defined with TypeScript interfaces?
- **Keys**: Are list items using proper unique keys (not array indices)?
- **Hooks Rules**: Are hooks called at the top level and in the correct order?
- **Custom Hooks**: Could repeated stateful/effectful logic be extracted into a focused reusable hook?
- **Pure Helpers**: Could pure derivation, filtering, grouping, parsing, or formatting logic move out of render-heavy components into `lib/` helpers?
- **Component Shape**: Does any changed view/page still own too many unrelated responsibilities across loading, derivation, mutation, and rendering orchestration?
- **Component Size**: Treat these as smell thresholds, not hard limits: components around 200 lines, functions around 40 lines, files around 300 lines, JSX nesting around 4 levels.

### TypeScript Best Practices
- **const vs let vs var**: Is `const` used by default? Is `let` only used when reassignment is needed? Is `var` avoided entirely?
- **Type Safety**: Are types explicit and avoiding `any`? Are proper interfaces/types defined?
- **Type Assertions**: Are type assertions (`as`) used sparingly and only when necessary?
- **Non-null Assertions**: Are non-null assertions (`!`) avoided? They bypass TypeScript's null safety and hide bugs. Use proper null checks or optional chaining instead.
- **React Ref Types**: Are React refs properly typed as nullable (`useRef<T>(null)` with `RefObject<T | null>`)? Refs are null on first render and during unmount.
- **Optional Chaining**: Is optional chaining (`?.`) used appropriately for potentially undefined values?
- **Enums vs Union Types**: Are union types preferred over enums where appropriate?
- **Named Shapes**: Are repeated or verbose inline object shapes extracted to named types when that makes the contract easier to read?
- **Shared Types**: Are canonical cross-feature types kept in `src/shared/types/` instead of duplicated across features?

### Design System & Styling
- **Component Usage**: Are design system components used instead of raw HTML elements (`<Button>` not `<button>`, `<Input>` not `<input>`)?
- **No Custom Styling**: Is custom inline styling or CSS avoided in favor of design system utilities?
- **Tailwind Classes**: Are Tailwind utility classes used properly and consistently?
- **Tailwind JIT Compilation**: Are Tailwind classes using static strings? JavaScript variables in template literals (e.g., `` `max-w-[${variable}]` ``) break JIT compilation. Use static strings or conditional logic instead (e.g., `condition ? 'max-w-[100px]' : 'max-w-[200px]'`).
- **Theme Tokens**: Are theme tokens used for colors that adapt to light/dark mode (e.g., `text-foreground`, `bg-card`, `text-muted-foreground`) instead of hardcoded colors (e.g., `text-black`, `bg-white`)?
- **Variants**: Could any components benefit from additional variants or properties in the design system?
- **Light and Dark Mode Support**: Are colors working properly in both light and dark modes? No broken colors?
- **Responsive Layout**: Does the layout work correctly at all breakpoints? No broken layout on mobile, tablet, or desktop?
- **Semantic Structure**: Are semantic elements like `<main>`, `<nav>`, `<header>`, and `<aside>` used where they clarify page structure?

### Localization (i18n)
- **New Keys**: When new translation keys are added to one locale (e.g., `en`), are all other supported locales updated too? i18next falls back gracefully, but incomplete locales should be flagged.
- **Removed Keys**: When UI text is removed, are the corresponding translation keys removed from all locale files?
- **Raw Strings**: Are user-facing strings wrapped in `t()` calls instead of hardcoded in JSX? Non-translatable symbols (icons, punctuation, HTML entities) are acceptable with an `i18n-check-ignore` annotation.
- **Stable Keys**: Are translation keys stable and domain-specific instead of mirroring incidental English copy?
- **Catch Blocks**: Are user-facing errors routed through translation keys instead of raw English strings in `catch` blocks?

### Code Simplicity (DRY Principle)
- **Duplication**: Is there any repeated code that could be extracted into functions or components?
- **Complexity**: Are there overly complex functions that could be broken down?
- **Logic**: Is the logic straightforward and easy to follow?
- **Abstractions**: Are abstractions appropriate (not too early, not too late)?
- **Guard Clauses**: Are early-return guards used to keep code shallow and readable?
- **Stable Extraction**: If duplication exists in two or more call sites, is the shared shape stable enough to extract without making callers handle a shifting contract?
- **Post-PR Shape**: For refactors, review the final code shape. Do not clear a smell just because the PR improved the previous version.
- **Partial Cleanup**: Do not treat partial extraction, partial deduplication, or partial cleanup as resolution if a verified smell remains in the changed code.

### Naming Reveals Intent
- **Domain Terms**: Are names based on domain meaning instead of generic placeholders like `data`, `value`, or `handler`?
- **Helper Names**: Do helper names describe what they return or decide, not how they compute it?
- **Misleading Names**: Are misleading functions, components, or variables renamed instead of explained with comments?

### Code Cleanliness
- **Comments**: Are there unnecessary comments explaining obvious code? (Remove them)
- **Console Logs**: Are there leftover `console.log` statements? (Remove them)
- **Dead Code**: Is there unused code, commented-out code, or unused imports?
- **Cross-Boundary Dead Data**: Are there struct/interface fields computed on one side of a boundary (e.g., Rust backend) but never consumed on the other (e.g., TypeScript frontend)? This wastes computation and adds noise to data contracts.
- **Naming**: Are variable and function names clear and descriptive?
- **Magic Numbers**: Are there magic numbers without explanation? Should they be named constants?
- **Redundant Data**: Are redundant props, fields, parameters, intermediate values, or exports removed?
- **Coverage Drift**: When behavior-preserving logic moves, did meaningful test coverage move with it instead of disappearing?

### Animation & UI Polish
- **Race Conditions**: Are there any animation race conditions or timing issues?
- **Single Source of Truth**: Is state managed in one place to avoid conflicts?
- **AnimatePresence**: Is it used properly with unique keys for dialog/modal transitions?
- **Reduced Motion**: Is `useReducedMotion()` respected for accessibility?

### Async State, Defaults & Persistence
- **Async Source of Truth**: During async provider/model/session mutations, does UI/session/localStorage state update only after the backend accepts the change? If the UI updates optimistically, is there an explicit rollback path?
- **UI/Backend Drift**: Could the UI show provider/model/project/persona X while the backend is still on Y after a failed mutation, delayed prepare, or pending-to-real session handoff?
- **Requested vs Fallback Authority**: Do explicit user or caller selections stay authoritative over sticky defaults, saved preferences, aliases, or fallback resolution?
- **Dependent State Invalidation**: When a parent selection changes (provider/project/persona/workspace/etc.), are dependent values like `modelId`, `modelName`, defaults, or cached labels cleared or recomputed so stale state does not linger?
- **Persisted Preference Validation**: Are stored selections validated against current inventory/capabilities before reuse, and do stale values fail soft instead of breaking creation flows?
- **Compatibility of Fallbacks**: Are default or sticky selections guaranteed to remain compatible with the active concrete provider/backend, instead of leaking across providers?
- **Best-Effort Lookups**: Do inventory/config/default-resolution lookups degrade gracefully on transient failure, or can they incorrectly block a primary flow that should still work with a safe fallback?
- **Draft/Home/Handoff Paths**: If the product has draft, Home, pending, or pre-created sessions, did you review those handoff paths separately from the already-active session path?

### UI Refactor Quality
Use this focused pass for `ui/goose2` changes, especially when the user asks about cleanup, maintainability, decomposition, layering, type hygiene, duplication, dead code, readability, or extensibility.

- **Final Shape**: Judge whether the changed code is easier to maintain and extend after the PR, not whether it is better than before.
- **No Bug Required**: Report confirmed maintainability smells even when behavior still works.
- **Confirmed Smells**: Any verified final-shape smell in changed code should become an issue; do not leave it only as a note or mental checklist item.
- **Mixed Scope**: If a PR mixes feature work and refactoring, review the feature behavior separately from the cleanup quality.
- **Layer Discipline**: Keep `ui/` for rendering and light view logic, `hooks/` for React state/effect orchestration, `api/` for backend transport and DTO adaptation, `lib/` for pure helpers, and `stores/` for shared feature state.
- **Library Purity**: Keep `lib/` free of React, DOM, `window`, and I/O.
- **Boundary Discipline**: Keep frontend-to-core behavior on the `SDK -> ACP -> goose` path. Do not add ad hoc `fetch()` calls or `invoke()` proxies for core Goose behavior, and do not call ACP clients directly from UI components.
- **Module Encapsulation**: Export the minimum surface. Keep helpers private unless another module genuinely needs them, and remove stale exports.
- **Hooks vs Helpers**: Prefer hooks for stateful async workflows and `lib/` helpers for React-independent logic. Keep each hook focused on one job with a stable return shape, and do not use a hook as the default extraction target for an oversized component.
- **Distinct Smells**: If multiple distinct smells remain in one file, report each distinct responsibility problem as its own issue.
- **Feature Wiring**: Confirm the refactor preserves user-visible behavior, state updates, persistence, and backend calls.
- **Tests**: Treat coverage loss during refactors as suspicious unless the behavior was intentionally removed.
- **Second Pass**: Before finalizing, re-check decomposition, layering, hooks/effects, pure helpers, type shapes, duplication, tests, and feature wiring for missed issues.

### General Code Quality
- **Error Handling**: Are errors handled gracefully with user-friendly messages?
- **Notifications**: Are success and error messages routed through the app's shared notification primitive instead of one-off notification UI?
- **Loading States**: Are loading states shown during async operations?
- **Accessibility**: Are ARIA labels, keyboard navigation, and screen reader support considered?
- **Performance**: Are there any obvious performance issues (unnecessary re-renders, heavy computations)?
- **Git Hygiene**: Are there any files that shouldn't be committed (env files, etc.)?
- **Unrelated Changes**: Are there any stray files or changes that don't relate to the branch's main purpose? (Accidental commits, unrelated fixes)

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
2. Review **only those changed lines** against the Review Checklist — do not flag issues in unchanged code, but follow changed code paths into surrounding modules when needed to verify the issue
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
     - Recommended fix

  2. Short Issue Title (P3) [Your Call]
     - Description of the issue and why it may or may not need addressing
     - Recommended fix if the user chooses to act on it
  ```

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
