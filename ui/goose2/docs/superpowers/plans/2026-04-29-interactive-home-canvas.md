# Interactive Home Canvas Implementation Plan

> Source spec: `docs/superpowers/specs/2026-04-29-interactive-home-canvas-design.md`
>
> Status: review + implementation plan. This plan intentionally stops before code changes to the homepage.

## Summary

Replace `HomeView` with a persisted, draggable widget canvas while leaving `HomeScreen` unchanged. The core spec is strong: catalog, instance layout, picker, and persistence are cleanly separated, and the existing app already has most of the needed libraries (`motion`, Radix Popover, Radix ContextMenu, Zustand).

The main things to settle before implementation are:

- Cube source access: `https://github.com/anaghavi/cube-explo` was not discoverable publicly, and no exact `cube-explo` npm package showed up in search.
- Agent pin semantics: the spec alternates between "opens chat" and "opens agent"; the app currently has a direct `onOpenAgent` handler and a direct `onSelectSession` handler, but no dedicated "start chat with this persona" callback.
- Default layout seeding: `DEFAULT_INSTANCES` cannot know the canvas size or async-loaded persona list at module initialization time.
- Localization: the home namespace already exists, so new picker/menu/widget UI copy should use `react-i18next` instead of hardcoded English.

## Review Feedback

### 1. Use `motion/react`, not `framer-motion`

The spec mentions framer-motion, but the codebase imports from `motion/react` in `ChatView`, `ChatContextPanel`, and `LoadingGoose`. Implement `WidgetFrame` and `AnimatePresence` with `motion/react` to match the existing dependency.

### 2. Make default instances viewport-safe without making persistence complicated

The spec models widget positions as pixels, but also asks for percentage-based first-load placement. Keep persisted positions as pixels. For first load, create defaults with calibrated pixel positions for the home content area, then clamp positions to the current canvas before render and after drag. This keeps storage simple and avoids schema churn.

If we want smarter first-load placement, implement a `createDefaultInstances(canvasRect, defaultPersonaId)` helper and seed only when the persisted storage key is absent. Do not seed by checking `instances.length === 0`, because an intentionally empty persisted layout must stay empty.

### 3. Keep agent pins resilient to async persona loading

Do not require `DEFAULT_INSTANCES` to contain the default Goose persona id up front. The persona list loads asynchronously through `useAppStartup`. Let `AgentPinWidget` resolve its display target this way:

1. Use `instance.state.agentId` if it matches a loaded persona.
2. Fall back to the first built-in persona.
3. Fall back to a generic "Goose" label while personas are still loading.

Picker-created agent pins can pre-fill `state.agentId` when a persona is available.

### 4. Decide what "Pin an agent" does

Current `AppShellContent` renders `<HomeView />` without routing props. To make pins real, pass routing callbacks into `HomeView`.

Recommended demo behavior:

- `chatPin` calls existing `onSelectSession(sessionId)`.
- `agentPin` calls existing `onOpenAgent(agentId)`, opening the agent details surface.

If the desired demo is "click agent pin to start a chat with that persona," add an explicit app-shell callback such as `onStartChatWithPersona(personaId)` rather than overloading `onOpenAgent`. That callback should create or reuse a draft session with `personaId` set.

### 5. Use Radix ContextMenu in its natural shape

The spec says `onContextMenu` opens a Radix context menu anchored at cursor. Radix already anchors context menus to the native context-menu event when using `ContextMenuTrigger asChild`. Prefer:

```tsx
<ContextMenu>
  <ContextMenuTrigger asChild>
    <motion.div ... />
  </ContextMenuTrigger>
  <ContextMenuContent>
    <ContextMenuItem onSelect={...}>...</ContextMenuItem>
  </ContextMenuContent>
</ContextMenu>
```

This avoids custom cursor anchoring state.

### 6. Localize new visible UI strings

Add stable keys under `src/shared/i18n/locales/en/home.json` and `src/shared/i18n/locales/es/home.json` for:

- Picker section labels and item labels/descriptions
- Context menu "Remove"
- Widget mock labels/content that is rendered as app UI
- Empty/fallback labels such as "Recent chat" or "Goose"

The mock content can still be static, but it should not be raw English in migrated home UI.

### 7. Add a small demo recovery affordance only if wanted

The spec says defaults return only after clearing localStorage. That is acceptable, but for demos it is easy to remove every widget and get stuck with a blank canvas. Optional follow-up: add an empty-canvas context menu item or small hidden developer action for "Reset layout." This is not required for the first implementation.

## Implementation Plan

## Phase 0 - Pre-flight

- [ ] Confirm working tree and current branch.
- [ ] Re-read the source spec and this plan.
- [ ] Confirm `HomeView` is the only source importer of retired home assets, then delete those assets only during the implementation phase.
- [ ] Confirm cube path:
  - [ ] If `cube-explo` source is provided or accessible, copy the relevant source into `src/features/home/widgets/cube/`.
  - [ ] If it requires Three/R3F, evaluate dependency cost before adding packages.
  - [ ] If source remains unavailable, implement a lightweight CSS/DOM animated cube fallback and document that the cube source remains blocked.
- [ ] Decide agent pin behavior:
  - [ ] Recommended: `agentPin` opens agent details with `onOpenAgent`.
  - [ ] Alternative: add `onStartChatWithPersona`.

## Phase 1 - Types, Catalog, and Store

Files:

- Create: `src/features/home/widgets/types.ts`
- Create: `src/features/home/widgets/catalog.ts`
- Create: `src/features/home/stores/homeWidgetStore.ts`

Tasks:

- [ ] Define `WidgetCategory`, `WidgetCatalogEntry`, `WidgetInstance`, and `WidgetRenderProps`.
- [ ] Add an optional `defaultState?: () => Record<string, unknown> | undefined` concept to catalog entries, or keep state resolution in the picker layer. Prefer picker-layer state for pins because it depends on current stores.
- [ ] Build the 8-entry catalog in the order expected by the picker: tiles, apps, pins.
- [ ] Implement store actions:
  - [ ] `addWidget(type, x, y, state?)`
  - [ ] `moveWidget(id, x, y)`
  - [ ] `bumpZ(id)`
  - [ ] `removeWidget(id)`
  - [ ] `updateWidgetState(id, state)`
- [ ] Use `persist` middleware with `name: "goose2:home-widgets"` and `version: 1`.
- [ ] Add a shared clamp helper so add/move/render can keep widgets inside the canvas when dimensions are known.
- [ ] Preserve intentionally empty persisted layouts; do not auto-restore defaults just because the array is empty.

Implementation notes:

- Use `crypto.randomUUID()` for new instances, consistent with existing code.
- `updateWidgetState` should merge the existing `instance.state` with the patch, not replace it wholesale, so widgets can add future fields safely.
- Unknown catalog ids in persisted state should be filtered out or rendered as a small fallback frame. Prefer filtering during selector/render to avoid crashing the home route after catalog edits.

## Phase 2 - Shell, Canvas, Frame, and Picker

Files:

- Modify: `src/app/ui/AppShellContent.tsx`
- Modify: `src/features/home/ui/HomeView.tsx`
- Create: `src/features/home/ui/WidgetCanvas.tsx`
- Create: `src/features/home/ui/WidgetFrame.tsx`
- Create: `src/features/home/ui/WidgetPicker.tsx`
- Modify: `src/shared/i18n/locales/en/home.json`
- Modify: `src/shared/i18n/locales/es/home.json`

Tasks:

- [ ] Update `AppShellContent` to pass `onOpenAgent` and `onSelectSession` into `HomeView`.
- [ ] Refactor `HomeView` into a thin shell that renders the canvas and no longer imports decorative home assets.
- [ ] Implement `WidgetCanvas`:
  - [ ] Own a `ref` for drag constraints.
  - [ ] Open picker on double-click only when `event.target === event.currentTarget`.
  - [ ] Convert `clientX/clientY` to canvas-relative coordinates.
  - [ ] Render the existing `bg-dot-grid` aesthetic through the home route container.
- [ ] Implement `WidgetFrame`:
  - [ ] Use `motion.div` from `motion/react`.
  - [ ] Use `drag`, `dragConstraints={canvasRef}`, and `dragMomentum={false}`.
  - [ ] Persist final position from drag offsets.
  - [ ] Bump z on pointer down.
  - [ ] Wrap the frame in Radix `ContextMenu` with one remove item.
  - [ ] Apply width/height from catalog default size.
  - [ ] Use `AnimatePresence` around the rendered instance list.
- [ ] Implement `WidgetPicker`:
  - [ ] Use `Popover`, `PopoverAnchor`, and `PopoverContent`.
  - [ ] Position an invisible anchor at the captured canvas-relative coordinate.
  - [ ] Render Tile, App, Pin sections.
  - [ ] Use real `<button type="button">` rows for picker options.
  - [ ] On select, call `addWidget` centered on the original cursor coordinate.
- [ ] Add i18n keys for all visible picker/menu labels.

## Phase 3 - Widget Components

Files:

- Create: `src/features/home/widgets/ClockWidget.tsx`
- Create: `src/features/home/widgets/WeatherWidget.tsx`
- Create: `src/features/home/widgets/StickyNoteWidget.tsx`
- Create: `src/features/home/widgets/MondayBriefTile.tsx`
- Create: `src/features/home/widgets/WeeklyHighlightsTile.tsx`
- Create: `src/features/home/widgets/AgentPinWidget.tsx`
- Create: `src/features/home/widgets/ChatPinWidget.tsx`
- Create/modify: `src/features/home/widgets/CubeWidget.tsx`

Tasks:

- [ ] Move the clock logic from current `HomeView` into `ClockWidget`.
- [ ] Build weather and tile widgets as polished static mock cards.
- [ ] Build `StickyNoteWidget` as a controlled textarea:
  - [ ] Value from `instance.state.text`.
  - [ ] `onChange` calls `onUpdateState({ text })`.
  - [ ] `onPointerDown` stops propagation so text selection works.
- [ ] Build `AgentPinWidget`:
  - [ ] Read personas from `useAgentStore`.
  - [ ] Resolve selected persona with fallback behavior from Review Feedback.
  - [ ] On click, use the chosen agent behavior from Phase 0.
- [ ] Build `ChatPinWidget`:
  - [ ] Read sessions from `useChatSessionStore`.
  - [ ] Filter to visible, unarchived sessions using `getVisibleSessions` and `useChatStore().messagesBySession`.
  - [ ] Resolve selected session from state or fall back to most recent visible chat.
  - [ ] On click, call `onSelectSession`.
- [ ] Keep widget styling restrained and canvas-native: no page-section cards inside cards.

## Phase 4 - Cube

Preferred path:

- [ ] Copy accessible cube source into `src/features/home/widgets/cube/`.
- [ ] Adapt imports and sizing so `CubeWidget` fills its catalog size.
- [ ] Use `prefers-reduced-motion` or `useReducedMotion` if the source exposes a clean pause/reduce hook.
- [ ] Avoid adding Three/R3F dependencies unless the source truly needs them and the visual payoff is worth the package cost.

Fallback path if source remains unavailable:

- [ ] Build a CSS/DOM animated cube in `CubeWidget`.
- [ ] Keep the public widget contract identical so it can be replaced by the real cube later.
- [ ] Note in the final handoff that the cube is a fallback, not the `cube-explo` integration.

## Phase 5 - Asset Cleanup

Files:

- Delete, if still unused:
  - `src/assets/home/world-cube.png`
  - `src/assets/home/clock.svg`
  - `src/assets/home/person-1.png`
  - `src/assets/home/person-2.png`
  - `src/assets/home/sticky-note.svg`

Tasks:

- [ ] Re-run `rg "assets/home|world-cube|clock.svg|person-2|sticky-note|person-1" src`.
- [ ] Delete only assets no longer imported from `src`.
- [ ] Leave historical docs references alone.

## Phase 6 - Tests and Verification

Focused tests:

- [ ] Add store tests for add, move, bump, remove, state update, and persisted empty layout behavior.
- [ ] Add component tests for:
  - [ ] Picker opens on canvas double-click.
  - [ ] Picker does not open from widget double-click.
  - [ ] Selecting an item adds an instance.
  - [ ] Remove context menu removes an instance.
  - [ ] Sticky note updates persisted state.

Manual verification:

- [ ] First load shows cube, clock, and agent pin.
- [ ] Dragging clamps to canvas bounds and persists after reload.
- [ ] Right-click remove animates out and persists after reload.
- [ ] Picker shows 2 Tile, 4 App, and 2 Pin entries.
- [ ] Agent pin and chat pin route using the chosen callbacks.
- [ ] Existing chat empty state still renders from `HomeScreen`.

Commands:

- [ ] `./bin/pnpm exec tsc --noEmit`
- [ ] `./bin/pnpm exec biome check .`
- [ ] `./bin/pnpm test -- src/features/home`
- [ ] If asked for broader verification: `./bin/just check` and `./bin/just test`

## Suggested Implementation Order

1. Land types, catalog, and store first.
2. Build canvas/frame/picker with placeholder widget bodies.
3. Fill in the 8 widgets.
4. Wire routing callbacks for pins.
5. Integrate or fallback the cube.
6. Delete retired assets.
7. Add focused tests.
8. Run typecheck, Biome, and home tests.

## Risks

- The cube source may be private or otherwise unavailable.
- Agent/persona concepts are named inconsistently in the spec and code; settle click behavior before implementation.
- Persisted layouts can survive catalog changes, so unknown widget types need graceful handling.
- Widgets can be off-canvas after a large window resize; the spec accepts this, but clamping on next render/drag reduces demo awkwardness without adding full reflow.

