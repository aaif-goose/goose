# Interactive home canvas — design spec

**Date:** 2026-04-29
**Branch:** `tulsi/visual-design`
**Author:** Tulsi
**Implementer:** Codex (via follow-up implementation plan)
**Status:** ready for plan

---

## Goal

Replace the static editorial home page with an **interactive widget canvas** that demos as a personal, customizable surface. Users see a small set of pre-installed widgets on first load and can:

- Drag any widget freely on the canvas
- Right-click a widget to remove it
- Double-click empty canvas space to open a widget picker
- Click an example in the picker to spawn a new widget at the cursor

Layout persists across reloads. Demo-grade fidelity — most widget *content* is mock/static; the *system* (drag, picker, persistence, removal, click-to-front, animated cube) is real.

---

## Why this shape

The brand-driven editorial home has visual richness but no interactivity. For the upcoming demo we need home to feel like a personal surface the user owns, not a curated illustration. The demo arc is "home is lived-in on first load → user adds a few widgets → home is uniquely theirs."

The three-category taxonomy (tile / app / pin) maps the conceptual surface area Goose2 should expose:

- **Tiles** — outputs of agent runs / scheduled briefs (e.g. "Monday morning brief")
- **Apps** — self-contained mini-tools (weather, sticky note, the animated cube)
- **Pins** — references to existing surfaces (an agent to chat with, a chat to resume)

This taxonomy is also the spine for future home-extensibility: agent-generated tiles, third-party apps, user-pinned anything.

---

## Architecture

Three concerns kept clean:

1. **Catalog** — static registry of widget *types*. Each type knows its render component, default size, default content. Defined in code at `src/features/home/widgets/catalog.ts`.

2. **Layout state** — array of widget *instances*. Each instance has a unique id, references a catalog type, and holds `{x, y, z, state?}`. Persisted to `localStorage`. Single source of truth for "what's on the home page right now." Lives in a Zustand store at `src/features/home/stores/homeWidgetStore.ts`.

3. **Picker** — programmatically-opened Radix `<Popover>` anchored at the user's double-click coordinates. Three sections (Tile / App / Pin) showing catalog examples. Click an example → spawns instance into layout state.

Drag, persistence, right-click removal, and bring-to-front are all expressed as updates to layout state. The rendering layer is purely `layoutState + catalog → React tree`.

---

## Data model

```ts
type WidgetCategory = "tile" | "app" | "pin";

interface WidgetCatalogEntry {
  id: string;                         // stable type id, e.g. "weather"
  category: WidgetCategory;
  label: string;                      // shown in picker
  description?: string;               // optional secondary line in picker
  defaultSize: { width: number; height: number };
  Component: React.ComponentType<WidgetRenderProps>;
}

interface WidgetInstance {
  id: string;                         // crypto.randomUUID()
  type: string;                       // catalog entry id
  x: number;                          // px from canvas top-left
  y: number;
  z: number;                          // stacking — bumped on click/drag
  state?: Record<string, unknown>;    // per-instance state
}

interface WidgetRenderProps {
  instance: WidgetInstance;
  onUpdateState: (next: Record<string, unknown>) => void;
}
```

---

## Catalog content

Eight widget types:

| ID | Category | Label | Component | Notes |
|---|---|---|---|---|
| `cube` | App | "Cube" | `CubeWidget.tsx` | Wraps cube-explo animated cube |
| `clock` | App | "Clock" | `ClockWidget.tsx` | Real-time, repurposes existing `HomeClock` time-update logic |
| `weather` | App | "Weather" | `WeatherWidget.tsx` | Static mock 3-day forecast |
| `stickyNote` | App | "Sticky note" | `StickyNoteWidget.tsx` | Real interactive textarea, content in `instance.state.text` |
| `mondayBrief` | Tile | "Monday morning brief" | `MondayBriefTile.tsx` | Static mock card |
| `weeklyHighlights` | Tile | "Weekly highlights" | `WeeklyHighlightsTile.tsx` | Static mock card |
| `agentPin` | Pin | "Pin an agent" | `AgentPinWidget.tsx` | `instance.state.agentId` → avatar+name; click opens chat |
| `chatPin` | Pin | "Pin a chat" | `ChatPinWidget.tsx` | `instance.state.sessionId` → title preview; click jumps to chat |

Pin examples in the picker pre-fill `state` with sensible defaults — first available agent for `agentPin`, most recent chat for `chatPin`. No "pick which one" UI for the demo.

---

## Default layout

Used when `localStorage` is empty (first-load state):

```ts
const DEFAULT_INSTANCES: WidgetInstance[] = [
  { id: "default-cube",      type: "cube",     x: ~center,      y: ~center,      z: 1 },
  { id: "default-clock",     type: "clock",    x: ~top-right,   y: ~top,         z: 1 },
  { id: "default-agent-pin", type: "agentPin", x: ~bottom-left, y: ~bottom-left, z: 1,
    state: { agentId: <default Goose persona id> } },
];
```

Specific px coordinates are calibrated by Codex against a typical viewport (suggest 1440×900 as baseline). `<default Goose persona id>` is resolved at implementation time from the existing `useAgentStore` defaults — likely the built-in Goose persona's id.

Once persisted, user removals/edits override these defaults — defaults only show again if `localStorage` is cleared.

---

## File structure

```
src/features/home/
  ui/
    HomeView.tsx              ← refactored (no time/greeting, no static PNG decoration layer)
    WidgetCanvas.tsx          ← new — double-click target + drag bounds reference
    WidgetFrame.tsx           ← new — generic wrapper around every instance
    WidgetPicker.tsx          ← new — programmatically-anchored picker
  widgets/
    types.ts                  ← types defined above
    catalog.ts                ← 8-entry registry
    CubeWidget.tsx
    ClockWidget.tsx
    WeatherWidget.tsx
    StickyNoteWidget.tsx
    MondayBriefTile.tsx
    WeeklyHighlightsTile.tsx
    AgentPinWidget.tsx
    ChatPinWidget.tsx
    cube/                     ← cube-explo source ported here (see Cube integration)
  stores/
    homeWidgetStore.ts        ← Zustand + persist middleware
```

**Existing files affected:**
- `src/features/home/ui/HomeView.tsx` — heavily refactored: removes time/greeting, removes static PNG decorations, becomes a thin shell rendering `<WidgetCanvas>` and `<WidgetPicker>`.
- `src/assets/home/world-cube.png`, `clock.svg`, `person-2.png`, `sticky-note.svg`, `person-1.png` — **delete after confirming no other component imports them.** Currently only `HomeView` imports these, but Codex should grep `src/` to be sure before deletion.
- `src/features/home/ui/HomeScreen.tsx` — **untouched.** This is the chat-empty-state surface, distinct from `HomeView`. Don't change it.

---

## Component responsibilities

**`HomeView`** — pulls `instances` from `homeWidgetStore`, renders `<WidgetCanvas>` containing one `<WidgetFrame>` per instance, plus the `<WidgetPicker>` overlay.

**`WidgetCanvas`** — full-bleed `<div>` filling the home route. Owns:
- `onDoubleClick` handler that fires only when `event.target === event.currentTarget` (the canvas itself, not a child widget) → opens picker at `{event.clientX, event.clientY}` mapped to canvas-relative coords (subtract canvas's `getBoundingClientRect()`)
- `dragConstraints` ref passed down to children
- The `bg-dot-grid` background (preserves the home aesthetic)

**`WidgetFrame`** — generic, renders one instance:
- `<motion.div drag dragConstraints={canvasRef} dragMomentum={false} onDragEnd={...} />`
- `onPointerDown` → calls `bumpZ(instance.id)` if `instance.z < currentMaxZ`
- `onContextMenu` → opens Radix `<ContextMenu>` anchored at cursor with single "Remove" item
- Looks up `catalog[instance.type].Component` and renders it with `{instance, onUpdateState}` props
- Applies `position: absolute`, `transform: translate(x, y)`, `z-index: z`, width/height from `defaultSize`
- Wrapped at parent level in `<AnimatePresence>` for spawn/exit animations

**`WidgetPicker`** — receives `{open, x, y, onSelect, onClose}` props. Internals:
- Invisible `<PopoverAnchor>` positioned absolutely at `{x, y}` size `0×0`
- Popover content has 3 sections (Tile / App / Pin); each section maps catalog entries to clickable rows with label + description
- `onSelect(catalogId)` → store action `addWidget(catalogId, x, y)` → close picker

**Widget components (8)** — all receive `{instance, onUpdateState}`:
- Most static; ignore `onUpdateState`
- `StickyNoteWidget`: `<textarea>` whose value comes from `instance.state.text ?? ""`, calls `onUpdateState({ text: next })` on change. The textarea's `onPointerDown` calls `e.stopPropagation()` so framer-motion doesn't intercept text-selection presses.
- `AgentPinWidget` / `ChatPinWidget`: read `instance.state.agentId` / `sessionId`, render avatar+label, click handler calls existing chat-routing functions (the same handlers the sidebar uses to open agents / jump to chats).

---

## Store API

```ts
interface HomeWidgetStore {
  instances: WidgetInstance[];
  addWidget: (type: string, x: number, y: number, state?: Record<string, unknown>) => void;
  moveWidget: (id: string, x: number, y: number) => void;
  bumpZ: (id: string) => void;       // sets z = max(currentZ) + 1
  removeWidget: (id: string) => void;
  updateWidgetState: (id: string, state: Record<string, unknown>) => void;
}
```

`addWidget` centers the widget on the click point: `{x: clickX - defaultSize.width / 2, y: clickY - defaultSize.height / 2}` so widgets materialize *around* the cursor.

---

## Interactions

### Double-click → picker
- Listener on `WidgetCanvas`'s top-level `<div>`
- Guard: only fires when `event.target === event.currentTarget`
- Captures click coords relative to canvas, opens picker

### Picker → spawn
- Click an example row → `addWidget(catalogId, x, y, defaultState)`
- Picker closes simultaneously
- New widget enters with `motion`'s `initial={{scale: 0.9, opacity: 0}}` → `animate={{scale: 1, opacity: 1}}` spring (~250ms)
- Z-index implicit: new widgets get `z = currentMax + 1`

### Drag
- `<motion.div drag dragMomentum={false} dragConstraints={canvasRef} onDragEnd={...} />`
- `dragConstraints={ref}` clamps so widget edges stay within canvas (motion handles the size math automatically)
- `dragMomentum={false}` prevents inertia overshoot
- `onDragEnd={(_, info) => moveWidget(id, currentX + info.offset.x, currentY + info.offset.y)}`

### Click-vs-drag separation (load-bearing)

framer-motion's built-in distance threshold (~3px) decides:

- Below threshold → `onClick` fires (relevant for pins)
- Above threshold → drag fires, click is suppressed

Result: tiny mouse jitter never opens a pinned chat by accident; intentional clicks always work. **No custom logic needed — this is "free" from the library.**

### Click to bring to front
- `onPointerDown` on `WidgetFrame` → `if (instance.z < maxZ) bumpZ(instance.id)`
- Pure side effect; doesn't `preventDefault`, doesn't `stopPropagation`
- The same press still flows into either click or drag on the inner widget

### Inner interactive surfaces
- `AgentPinWidget` / `ChatPinWidget`: avatar+name block has `onClick={openChat / jumpToChat}`. Click separation handled by motion's threshold.
- `StickyNoteWidget`: textarea calls `e.stopPropagation()` on `onPointerDown` only, so framer-motion never sees the press. Text selection / caret placement work without triggering drag or z-bump.

### Right-click → remove
- Use Radix `<ContextMenu>` (same package as existing dropdown/popover, no new dep)
- Single menu item: "Remove"
- On click → `removeWidget(instance.id)`
- Widget exit animation: `<AnimatePresence>` wrapping the list of widgets; widget exits with `exit={{scale: 0.9, opacity: 0}}` for symmetry with spawn

---

## Persistence

**Storage key:** `goose2:home-widgets`

**Stored shape:** the `instances` array, JSON-serialized — same shape as in-memory state. No transform layer.

**Wiring** (Zustand `persist` middleware):

```ts
import { create } from "zustand";
import { persist } from "zustand/middleware";

const DEFAULT_INSTANCES: WidgetInstance[] = [/* cube + clock + agentPin */];

export const useHomeWidgetStore = create<HomeWidgetStore>()(
  persist(
    (set, get) => ({
      instances: DEFAULT_INSTANCES,
      // actions...
    }),
    { name: "goose2:home-widgets", version: 1 },
  ),
);
```

**Behavior:**
- First-ever load → no storage entry → `instances` = `DEFAULT_INSTANCES` → middleware writes them on first state change
- Subsequent loads → storage value hydrates over defaults (stored state wins, including emptiness)
- User removes a default widget then reloads → stays removed
- `localStorage` unavailable (privacy mode) → middleware silently falls back to in-memory state; no persistence that session
- Schema migration deferred — `version: 1` is a hatch for future format changes. If shape changes during demo iteration, blow away the storage key.

---

## Cube integration

The cube comes from `https://github.com/anaghavi/cube-explo`. Repo not yet inspected — this spec defines the *interface* the cube widget exposes; Codex decides how to port the source.

**Interface contract:**
- `CubeWidget.tsx` exports a default React component accepting `WidgetRenderProps`
- Renders inside a div matching `defaultSize` (`{width: 320, height: 320}` as starting target — Codex calibrates after seeing the source)
- `instance.state` unused — cube has no per-instance configuration
- Animation runs continuously while mounted; pauses naturally when unmounted

**Integration paths Codex should evaluate, in order of preference:**

1. **Copy cube source into `src/features/home/widgets/cube/`** as a self-contained subfolder. Lift the Three.js / React Three Fiber / shader code (whatever it is) into the goose2 tree. Adjust imports, add necessary deps.
2. **Install cube-explo as an npm dep** if it's published. (Probably isn't — looks like a personal repo — but worth a 10-second check.)
3. **Stub with a static cube image + CSS animation** if the source turns out to be heavy enough that integrating would balloon scope past demo-grade. **This fallback should be flagged in the implementation plan, not chosen silently.**

---

## Content scope

### Real
- Drag/drop layout (motion + dragConstraints)
- localStorage persistence
- Picker open/close/select
- Right-click context menu → remove
- Sticky note text editing
- Pin click → opens chat / opens agent (calls existing chat-routing handlers — same ones the sidebar uses)
- Cube animation (per cube-explo source)
- Clock real-time display (re-uses `HomeClock` time-update logic from current `HomeView`)

### Mock (static / hardcoded)
- "Monday morning brief" tile content — hardcoded copy, e.g., "3 priorities · 2 meetings · ☕ at 10:30"
- "Weekly highlights" tile content — hardcoded copy
- Weather forecast — hardcoded current conditions + 3-day forecast
- Picker pin examples auto-fill with first available agent / most recent chat — no "which one to pin?" UI

---

## Out of scope

Explicitly *not* part of this work:

- Widget resizing
- Widget collision detection / snapping
- Multi-select drag
- Undo/redo
- Named layouts ("save as 'Focus mode'")
- Cross-device sync
- Touch/mobile gestures (Tauri desktop only)
- Real APIs (no actual weather, no real morning brief generation)
- User-extensible catalog
- Resize-aware reflow when window shrinks (widgets may end up off-canvas until next drag — acceptable)
- Accessibility deep dive — basic semantics free; full a11y audit deferred
- Dragging the existing static decorations being retired (`sticky-note.svg`, `person-2.png`, current `clock.svg`, `world-cube.png`, `person-1.png`) — those are deleted, not made draggable. Their replacements in the catalog are reborn from scratch.

---

## Acceptance criteria

- Home page loads with 3 default widgets visible (cube, clock, agentPin → Goose), positioned per `DEFAULT_INSTANCES`
- Existing time/greeting and static decorative PNGs no longer render
- Dragging any widget moves it; release commits position; bounds clamp to canvas
- Double-click on empty canvas space opens picker at cursor; double-click on a widget does NOT open picker
- Picker shows three sections — Tile (2), App (4), Pin (2) — 8 catalog examples total
- Clicking an example spawns a new widget centered on the click point with a spring entry animation
- Right-clicking any widget opens a context menu with "Remove"; clicking Remove removes widget with exit animation
- Clicking a widget that is behind another brings it forward (z-bump)
- Clicking a pin (no drag) opens the linked chat / agent via existing chat-routing handlers
- Tiny mouse jitter on a pin (< ~3px movement) does NOT spuriously open the chat — motion's threshold handles this
- Layout persists across reloads; user-removed widgets stay removed
- `pnpm typecheck` clean; biome lint clean; i18n strings (if any new) checked
- No regressions to the existing chat home-screen flow (`HomeScreen.tsx` is separate and unchanged)

---

## Open questions for Codex

- **Cube integration path.** Needs investigation of `https://github.com/anaghavi/cube-explo` before implementation strategy can be confirmed. Implementation plan should flag the chosen path explicitly (copy / npm dep / static fallback).
- **Default position calibration.** `~center`, `~top-right`, `~bottom-left` are placeholders. Suggest measuring against a 1440×900 baseline (typical Mac dev resolution) and using percentage-based positioning so behavior at other sizes is graceful.
- **Default Goose persona id.** Resolved at implementation time from `useAgentStore` — likely the built-in Goose persona's id.

---

## Reference

- Codex handoff for the related global composer model picker bug: `docs/codex/2026-04-29-global-composer-model-picker-empty.md` (separate issue, but in the same broader "make the home + composer surfaces work for the demo" sweep).
- Existing animation patterns: `src/features/chat/ui/ChatContextPanel.tsx`, `src/features/chat/ui/LoadingGoose.tsx` (uses `motion.div`, `AnimatePresence`, `useReducedMotion`).
- Existing Radix Popover wrapper: `src/shared/ui/popover.tsx`.
- Existing Zustand patterns: `src/features/agents/stores/agentStore.ts` (no persist middleware) and other feature stores.
