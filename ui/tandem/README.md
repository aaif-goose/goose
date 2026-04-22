# Tandem

Custom Tauri + React + Vite desktop UI for Goose. Lives next to `goose2`
so both can be compared side by side; `goose2` must be left untouched.

Based on the **Tandem UI** design bundle (see `design-ref/`).

## Layout

```
ui/tandem/
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
    components/           Ported Tandem components
      Icon.tsx
      sidebar/            Ribbon, LeftHeader, SectionSwitcher, sections, LeftFooter
      chat/               TabBar, Composer, EmptyState, ChatView, Message, popovers
      right/              RightPanel (notes list + editor)
      palette/            CommandPalette
      StatusBar.tsx
      Toast.tsx
    services/             ACP client (createWebSocketStream, acpConnection, acp)
  src-tauri/
    Cargo.toml            Rust crate `tandem` (bin `tandem-tauri`)
    build.rs
    tauri.conf.json       productName "Tandem", identifier com.tandem.app
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
pnpm --filter tandem dev            # vite only (browser, no Tauri shell)
pnpm --filter tandem tauri:dev      # full Tauri desktop dev
pnpm --filter tandem tauri:build    # production desktop build
```

## Roadmap

- **Phase 0 (done)** — scaffold, rename myui \u2192 tandem.
- **Phase 1 (done)** — static shell ported from design-ref.
- **Phase 2 (this commit)** — composer wired through `@aaif/goose-sdk` (ACP).
  Tauri backend spawns a long-lived `goose serve` (via sidecar or `$GOOSE_BIN`
  override), exposes `get_goose_serve_url` so the frontend can open a WebSocket
  to `/acp`. Frontend streams `agent_message_chunk` into the current tab's
  assistant message and surfaces `tool_call` / `tool_call_update` events as
  pills. First send in a tab lazily creates a new ACP session.
- **Phase 3 (done)** — folder-backed Memory + Projects (paths in Settings),
  Workflows wired to Goose recipes, Settings modal for folder paths.
- **Phase 4 (this commit)** — UI state (tabs, active tab, section, collapse
  flags, open note) persists via `tauri-plugin-store` under
  `$APP_CONFIG_DIR/state.json`. Hydrates on mount; saves debounced 500ms
  after changes. Transient runtime data (streaming flags, `gooseSessionId`)
  is stripped from disk so the app never resumes in an inconsistent state.
- **Future** — MCP surface in Settings; recipe parameter prompts; real ACP
  session resume (`client.loadSession`) so restored tabs continue their
  prior conversation instead of starting fresh.

## Running with a real goose binary

`tauri:dev` and `tauri:build` resolve `goose` via the sidecar declared in
`src-tauri/tauri.conf.json > bundle > externalBin`. Build the binary first:

```
cargo build --release -p goose-cli   # or whatever the workspace crate is
```

Or point Tandem at an arbitrary binary at runtime:

```
GOOSE_BIN=/path/to/goose pnpm --filter tandem tauri:dev
```

## Design variants not implemented

The design-ref bundle includes a dev-only "Tweaks panel" (accent swatches,
alternate empty-state / switcher / message / token-popover / footer-order /
right-panel variants). It was intentionally removed for v1 \u2014 only the default
variants are shipped.

## Workspace wiring

- Registered in `ui/pnpm-workspace.yaml` and `ui/package.json` workspaces.
- `ui/tandem/src-tauri` is added to the root `Cargo.toml` workspace `exclude`
  list so the Rust workspace does not try to build the Tauri crate.
