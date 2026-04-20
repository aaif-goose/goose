# Talos

Custom Tauri + React + Vite desktop UI for Goose. Lives next to `goose2`
so both can be compared side by side; `goose2` must be left untouched.

Based on the **Talos UI** design bundle (see `design-ref/`).

## Layout

```
ui/talos/
  index.html              Vite entry
  package.json            Frontend deps and scripts
  tsconfig.json
  vite.config.ts          Dev server on port 1521 (goose2 uses 1520)
  src/
    main.tsx              React root; loads fonts + global CSS
    App.tsx               Full shell (ribbon + 3 panes + statusbar)
    data.ts               Sample data (typed)
    types.ts              Shared TypeScript types
    design/               Ported design tokens + app CSS + icons
      colors_and_type.css
      app.css
      assets/{icon,logo}.svg
    components/           Ported Talos components
      Icon.tsx
      sidebar/            Ribbon, LeftHeader, SectionSwitcher, sections, LeftFooter
      chat/               TabBar, Composer, EmptyState, ChatView, Message, popovers
      right/              RightPanel (notes list + editor)
      palette/            CommandPalette
      StatusBar.tsx
      Toast.tsx
  src-tauri/
    Cargo.toml            Rust crate `talos` (bin `talos-tauri`)
    build.rs
    tauri.conf.json       productName "Talos", identifier com.talos.app
    src/
      main.rs             Thin entry
      lib.rs              Tauri Builder
    capabilities/
      default.json        Minimal permission set
    icons/                Placeholder icons (copied from goose2)
  design-ref/             Original Claude Design bundle (read-only reference)
```

## Scripts

```
pnpm install                       # from repo root or ui/
pnpm --filter talos dev            # vite only (browser, no Tauri shell)
pnpm --filter talos tauri:dev      # full Tauri desktop dev
pnpm --filter talos tauri:build    # production desktop build
```

## Roadmap

- **Phase 0 (done)** — scaffold, rename myui \u2192 talos.
- **Phase 1 (this commit)** — full static shell from design-ref ported to React:
  ribbon, collapsible panels, tabs, empty-state, composer, chat view with mock reply,
  right-panel notes list + editor, command palette, status bar, toasts.
- **Phase 2** — wire composer through `@aaif/goose-sdk` (ACP). Stream real
  assistant messages + tool-use events.
- **Phase 3** — folder-backed Memory + Projects (paths in Settings). Map
  Workflows \u2192 Goose recipes. Settings surface for folder paths and MCP.
- **Phase 4** — persistence (tabs, chat history, prefs) via `tauri-plugin-store`
  or SQLite.

## Design variants not implemented

The design-ref bundle includes a dev-only "Tweaks panel" (accent swatches,
alternate empty-state / switcher / message / token-popover / footer-order /
right-panel variants). It was intentionally removed for v1 \u2014 only the default
variants are shipped.

## Workspace wiring

- Registered in `ui/pnpm-workspace.yaml` and `ui/package.json` workspaces.
- `ui/talos/src-tauri` is added to the root `Cargo.toml` workspace `exclude`
  list so the Rust workspace does not try to build the Tauri crate.
