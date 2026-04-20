# MyUI

Custom Tauri + React + Vite desktop UI for Goose. Lives next to `goose2`
so both can be compared side by side; `goose2` must be left untouched.

## Layout

```
ui/myui/
  index.html              Vite entry
  package.json            Frontend deps and scripts
  tsconfig.json
  vite.config.ts          Dev server on port 1521 (goose2 uses 1520)
  src/
    main.tsx              React root
    App.tsx               App shell
  src-tauri/
    Cargo.toml            Rust crate `myui` (bin `myui-tauri`)
    build.rs
    tauri.conf.json       productName "MyUI", identifier com.myui.app
    src/
      main.rs             Thin entry
      lib.rs              Tauri Builder
    capabilities/
      default.json        Minimal permission set
    icons/                Placeholder icons (copied from goose2)
```

## Scripts

```
pnpm install              # from repo root or ui/
pnpm --filter myui dev           # vite only (browser, no Tauri shell)
pnpm --filter myui tauri:dev     # full Tauri desktop dev
pnpm --filter myui tauri:build   # production desktop build
```

## Talking to Goose

`@aaif/goose-sdk` (workspace package) is already declared as a dependency.
Import it from your React code to speak ACP to the embedded `goose` binary,
same as `goose2` does. Nothing in `ui/goose2/` has been modified.

## Workspace wiring

- Registered in `ui/pnpm-workspace.yaml` and `ui/package.json` workspaces.
- `ui/myui/src-tauri` is added to the root `Cargo.toml` workspace `exclude`
  list so the Rust workspace does not try to build the Tauri crate.
