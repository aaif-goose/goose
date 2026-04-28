# Project-tinted chat canvas — design spec

**Date:** 2026-04-28
**Branch:** `tulsi/visual-design`
**Status:** Approved (brainstorm complete) — ready for implementation plan

## Summary

When a chat belonging to a project is the active view, tint the app's dot-grid canvas toward the project's assigned color. The effect is subtle and ambient — closer to iPhone's tinted-icon mode than a colored panel. Today, a project's color is stored on the project record and rendered only as a small dot indicator (sidebar, session cards, workspace widget); the surrounding canvas remains neutral grey regardless of which project the active chat belongs to. This spec gives that color a second, environmental role.

## Goals

- Make the active project's identity ambient and immediate from a glance, without competing with chat content.
- Keep the visual change "tasteful": low-saturation, low-intensity, single-layer.
- Reuse the existing project color (set in `CreateProjectDialog`) — no new data model.
- Single-source-of-truth implementation: change CSS variable inputs to `bg-dot-grid`, not the renderer.

## Non-goals

- Tinting inside the chat card (`--surface-card`) or the chat input pill.
- Per-color intensity tuning table.
- A user-facing toggle to enable/disable tinting.
- Changes to the existing project color picker (`COLOR_OPTIONS` in `CreateProjectDialog.tsx`).
- Dark-mode tint behavior — **deferred**. (Today, `--canvas` and `--dot-color` are not redefined under `.dark`, so this spec treats them as theme-shared base values. When dark mode is revisited, the same `color-mix()` machinery will work against whatever new bases are introduced.)

## Behavior

| Situation | Canvas tint |
|---|---|
| Active session is bound to a project | Project color, ~7% fill / ~12% dots, 240ms cross-fade |
| Active session has no project (No-Project chat) | Neutral — base canvas, no tint |
| Active view is not the chat (Home, Projects list, Agents, Skills, Search) | Neutral — base canvas, no tint, even if a project-bound session is still in the store |
| Active project's record is deleted while its chat is open | Falls back to neutral automatically |
| User has `prefers-reduced-motion: reduce` set | Tint still applies, but instant (no transition) |

The tint covers the entire app canvas (`bg-dot-grid` wrapper at `AppShell.tsx`), including the dot-grid area beneath and around the sidebar — matching the reference image. The sidebar's own surfaces (rows, dividers, pinned/recents groupings) keep their existing colors and are unaffected.

## Architecture

### CSS-variable composition

Today, `globals.css` defines two base canvas tokens:

```css
--canvas: #dedede;
--dot-color: rgba(37, 37, 37, 0.15);
```

…and `.bg-dot-grid` consumes them directly:

```css
.bg-dot-grid {
  background-color: var(--canvas);
  background-image: radial-gradient(
    circle,
    var(--dot-color) var(--dot-size),
    transparent var(--dot-size)
  );
  background-size: var(--dot-spacing) var(--dot-spacing);
}
```

We rename `--canvas` → `--canvas-base` and `--dot-color` → `--dot-color-base` so the names declare their role as "un-tinted defaults," and we rewrite `.bg-dot-grid` to derive its actual fill and dot color from the base values mixed with a `--project-tint` variable:

```css
.bg-dot-grid {
  background-color: color-mix(
    in oklch,
    var(--project-tint, transparent) 7%,
    var(--canvas-base)
  );
  background-image: radial-gradient(
    circle,
    color-mix(
      in oklch,
      var(--project-tint, transparent) 12%,
      var(--dot-color-base)
    ) var(--dot-size),
    transparent var(--dot-size)
  );
  background-size: var(--dot-spacing) var(--dot-spacing);
  transition: background-color 240ms ease, background-image 240ms ease;
}

@media (prefers-reduced-motion: reduce) {
  .bg-dot-grid {
    transition: none;
  }
}
```

When `--project-tint` is unset or set to `transparent`, both `color-mix()` calls resolve cleanly back to their base values — i.e., the canvas is identical to today's. This is what guarantees zero visual regression for non-project surfaces.

The 7%/12% starting weights are tunable by eye during implementation. The dot weight is intentionally higher than the fill weight so dots remain visible against a tinted fill rather than vanishing into it.

`oklch` is chosen as the mixing color space because the project palette includes both saturated reds/pinks and cool blues/teals; mixing in `oklch` keeps the perceived weight consistent across hues, where `srgb` would muddy bright hues toward grey.

### Tint-resolution hook

A new hook `useActiveProjectTint()` lives at `src/features/chat/hooks/useActiveProjectTint.ts`:

- Reads the active session id from `useChatSessionStore`.
- Resolves the session's `projectId` (or null if the session has none, or there is no active session).
- Looks up the project in `useProjectStore` and returns `project.color` if found, otherwise `null`.
- Returns `string | null` (the hex color, or `null`).

This is a thin selector — no new state, no new store, no provider. It exists as a hook (rather than inline in `AppShell`) only so the resolution rule has one named place to live and one named place to test.

### Application point

`AppShell.tsx` already tracks `activeView: AppView` (state at the top of the component, with values including `"home"`, `"chat"`, `"search"`, `"projects"`, `"agents"`, `"skills"`). The tint is applied only when the chat view is active — so navigating to Home, Projects, Agents, or Skills always yields a neutral canvas, even if the chat session store still references a project-bound session in the background.

`AppShell.tsx` calls the hook, gates by `activeView`, and applies the result as an inline CSS variable on the existing `bg-dot-grid` wrapper element:

```tsx
const projectTint = useActiveProjectTint();
const tint = activeView === "chat" ? projectTint : null;
// ...
<div
  className="flex h-screen w-screen flex-col overflow-hidden bg-dot-grid text-[var(--text-default-alex)]"
  style={{ "--project-tint": tint ?? "transparent" } as React.CSSProperties}
>
```

The hook stays purely a resolution selector (session → project → color). The route gate lives at the application point, where `activeView` is already in scope. No new wrapper element, no overlay, no provider.

## Data flow

1. User selects (or creates) a chat belonging to a project.
2. `useChatSessionStore` updates the active session id; `setActiveView("chat")` runs as part of the same navigation.
3. `useActiveProjectTint()` re-resolves and returns the project's hex color.
4. `AppShell` gates by `activeView === "chat"` and re-renders with `--project-tint` set on the root.
5. `.bg-dot-grid` recomputes `background-color` and `background-image` via `color-mix()`.
6. CSS transitions both properties over 240ms (or instantly under `prefers-reduced-motion`).

When the user navigates to a non-chat route, opens a No-Project chat, or the active project is deleted, the gate or the hook returns `null`, the inline style sets `--project-tint: transparent`, and the canvas cross-fades back to its base values.

## Files touched

- `src/shared/styles/globals.css` — rename `--canvas` → `--canvas-base`, `--dot-color` → `--dot-color-base`; rewrite `.bg-dot-grid` to use `color-mix()`; add reduced-motion override. Also update any other reference to the renamed variables (audit step in the plan).
- `src/features/chat/hooks/useActiveProjectTint.ts` — new hook (described above).
- `src/features/chat/hooks/__tests__/useActiveProjectTint.test.ts` — new unit tests.
- `src/app/AppShell.tsx` — call the hook and apply `style={{ "--project-tint": ... }}` on the existing `bg-dot-grid` wrapper.

A grep audit during implementation will catch any remaining direct references to `--canvas` or `--dot-color` (e.g., the `--color-canvas-alex: var(--canvas)` alias at `globals.css:572`) and rename them in lockstep.

## Testing

### Unit — `useActiveProjectTint`

- In-project session → returns the project's hex.
- Active session has `projectId: null` → returns `null`.
- No active session → returns `null`.
- Active session references a `projectId` no longer present in the project store → returns `null` (graceful fallback).

### Component — `AppShell`

- Renders with `--project-tint` set to the project's hex when `activeView === "chat"` and the active session is in-project.
- Renders with `--project-tint: transparent` when `activeView !== "chat"` (e.g., on Home, Projects, Agents, Skills routes), even if a project-bound session is still in the chat session store.
- Renders with `--project-tint: transparent` when the active session is no-project.
- Snapshot or attribute assertion only — no visual regression test required.

### E2E (optional, low priority)

- Playwright: open a project chat, assert the computed `--project-tint` CSS variable on the root element matches the project's color. Open a no-project chat, assert it resolves to `transparent`.

## Risks & open questions

- **Visual tuning:** The 7%/12% mix percentages are eyeball-tested in the spec. They will likely need a brief polish pass during implementation across all 14 palette colors — particularly the saturated red, hot pink, and magenta. The plan should reserve a tuning step before merge.
- **Variable rename surface area:** Any unrelated stylesheet or component that reads `--canvas` directly will need to be updated to `--canvas-base`. The audit is a one-time grep but worth a dedicated task in the plan.
- **`color-mix()` browser support:** Tauri ships a Chromium WebView; `color-mix()` is supported. No fallback needed.

## Acceptance

- Opening any chat assigned to a project tints the entire dot-grid canvas toward that project's color, subtly.
- Opening a No-Project chat or a non-chat route shows the existing neutral canvas.
- Switching between sessions in different projects cross-fades the canvas over ~240ms.
- `prefers-reduced-motion: reduce` makes the change instantaneous.
- No visual change to non-canvas surfaces (sidebar rows, chat card interior, input pill, context panel, modals).
- All existing tests still pass; new unit + component tests cover the resolution rule and the application point.
