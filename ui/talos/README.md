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
    services/             ACP client (createWebSocketStream, acpConnection, acp)
  src-tauri/
    Cargo.toml            Rust crate `talos` (bin `talos-tauri`)
    build.rs
    tauri.conf.json       productName "Talos", identifier com.talos.app
                          externalBin = target/release/goose (sidecar)
    src/
      main.rs             Thin entry
      lib.rs              Tauri Builder + get_goose_serve_url command
      commands/acp.rs     Returns ws://127.0.0.1:{port}/acp
      services/goose_serve.rs
                          Spawns `goose serve` (sidecar or $GOOSE_BIN)
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
- **Phase 1 (done)** — static shell ported from design-ref.
- **Phase 2 (this commit)** — composer wired through `@aaif/goose-sdk` (ACP).
  Tauri backend spawns a long-lived `goose serve` (via sidecar or `$GOOSE_BIN`
  override), exposes `get_goose_serve_url` so the frontend can open a WebSocket
  to `/acp`. Frontend streams `agent_message_chunk` into the current tab's
  assistant message and surfaces `tool_call` / `tool_call_update` events as
  pills. First send in a tab lazily creates a new ACP session.
- **Phase 3** — folder-backed Memory + Projects (paths in Settings). Map
  Workflows \u2192 Goose recipes. Settings surface for folder paths and MCP.
- **Phase 4** — persistence (tabs, chat history, prefs) via `tauri-plugin-store`
  or SQLite.

## Running with a real goose binary

`tauri:dev` and `tauri:build` resolve `goose` via the sidecar declared in
`src-tauri/tauri.conf.json > bundle > externalBin`. Build the binary first:

```
cargo build --release -p goose-cli   # or whatever the workspace crate is
```

Or point Talos at an arbitrary binary at runtime:

```
GOOSE_BIN=/path/to/goose pnpm --filter talos tauri:dev
```

## Design variants not implemented

The design-ref bundle includes a dev-only "Tweaks panel" (accent swatches,
alternate empty-state / switcher / message / token-popover / footer-order /
right-panel variants). It was intentionally removed for v1 \u2014 only the default
variants are shipped.

## Workspace wiring

- Registered in `ui/pnpm-workspace.yaml` and `ui/package.json` workspaces.
- `ui/talos/src-tauri` is added to the root `Cargo.toml` workspace `exclude`
  list so the Rust workspace does not try to build the Tauri crate.
