# AGENTS.md

Guidelines for AI agents (and developers) working on this codebase.

## Project Overview

Goose2 is a Tauri 2 + React 19 desktop app. It uses TypeScript strict mode, Vite, and Tailwind CSS 3. The codebase follows a feature-sliced architecture organized under `src/app/`, `src/features/`, and `src/shared/`.

## First Steps

Treat this repo as partially Hermit-managed. Do not assume `just`, `pnpm`, `node`, or `lefthook` are available globally.

- In bash/zsh, run `source ./bin/activate-hermit` before using repo tools if the shell cannot find `just`, `pnpm`, or other managed binaries.
- In fish, run `source ./bin/activate-hermit.fish`.
- If PATH still looks wrong or you want to avoid shell assumptions, prefer repo-local binaries such as `./bin/just`, `./bin/pnpm`, and `./bin/lefthook`.
- Biome is installed from `package.json` devDependencies, not from Hermit. Run it through `pnpm`, `pnpm exec biome`, or `npx biome` after `just setup`.
- On a fresh clone, a newly created worktree, or after `just clean`, run `just setup` before relying on `pnpm`, Biome, or app-local tooling.
- In new clones and worktrees, ensure git hooks are installed early with `lefthook install`. If `lefthook` is not on PATH, use `./bin/lefthook install`.
- Agents starting in a fresh clone or worktree should do the setup and hook-install steps proactively rather than assuming the environment is already bootstrapped.
- Use `just dev` for the normal desktop workflow. Use `just dev-frontend` only when you intentionally want the Vite app without Tauri.

## Common Commands

- `just setup` installs frontend dependencies with `pnpm install` and builds the Rust backend once.
- `just dev` starts the desktop app in dev mode and wires Tauri to the local Vite server.
- `just check` runs Biome checks and file-size checks.
- `just test` runs the Vitest suite.
- `just tauri-check` runs `cargo check` in `src-tauri`.
- `just ci` is the main local verification gate.
- `just clean` removes Rust build artifacts, `dist`, and `node_modules`, so `just setup` is required again before `just dev`.

## Architecture & File Structure

```
src/
  app/           — App shell, entry point, top-level providers
  features/      — Feature modules (see Feature Organization below)
    <feature>/
      ui/        — React components (required)
      hooks/     — Custom React hooks for feature logic (when needed)
      stores/    — Zustand state management (when feature needs shared state)
      api/       — Backend API integration (when feature calls backend)
      types.ts   — Feature-specific type definitions (when needed)
  shared/
    types/       — Canonical shared type definitions (single source of truth)
      agents.ts  — Agent, Persona, Provider types
      chat.ts    — ChatState, TokenState, Session, SSE events
      messages.ts — Message, MessageContent, type guards
    ui/          — Reusable UI components (button, etc.)
    lib/         — Utilities (cn.ts for class merging)
    theme/       — Theme provider, appearance settings
    styles/      — Global CSS, design tokens
    hooks/       — Shared hooks
    api/         — API integration
    constants/   — Shared constants
    context/     — Shared contexts
```

### Feature Organization

Not every feature needs every subdirectory. Use only what the feature requires:

| Pattern              | Structure                        | Examples             |
|----------------------|----------------------------------|----------------------|
| **Full-featured**    | `stores/` + `hooks/` + `ui/`    | agents, chat         |
| **Data-driven**      | `stores/` + `api/` + `ui/`      | projects             |
| **API features**     | `api/` + `ui/`                   | skills               |
| **Simple features**  | `ui/` only                       | home, settings, sidebar, status |
| **Tabs**             | `ui/` + `types.ts`               | tabs                 |

### Import Rules for Features

- Shared types live in `src/shared/types/` — this is the single source of truth for cross-feature types.
- There should be NO root-level `src/stores/` or `src/types/` directories.
- Feature stores use feature-relative imports (e.g., `../stores/featureStore`).
- Cross-feature imports use `@/features/*/stores/` or `@/shared/types/`.

## Coding Conventions

- Use `cn()` from `@/shared/lib/cn` for Tailwind class merging.
- Import paths use the `@/` alias (maps to `./src`).
- Components are controlled where possible (state lifted to parent).
- Use `@tabler/icons-react` for icons (transitioning from `lucide-react`; existing `lucide-react` usage is fine until migrated).
- All `<button>` elements must have `type="button"` to prevent form submission.
- Use semantic HTML (`<aside>`, `<nav>`, `<header>`, `<main>`).

## Localization

- UI copy should go through `react-i18next`, not hardcoded English strings, for app areas that are already on i18n.
- Shared localization lives in `src/shared/i18n/`; use `useTranslation()` for text and the helpers in `src/shared/i18n/format.ts` for dates, times, numbers, currency, and relative time.
- Keep translations in feature-scoped JSON namespaces under `src/shared/i18n/locales/<locale>/` instead of one large file, and use stable keys rather than English sentences as keys.
- Do not translate user-authored content, agent/model output, or backend-only strings unless they are rendered directly as Goose UI.
- `pnpm check` includes `check:i18n`, which flags obvious new raw UI strings in migrated surfaces. Use a narrow `i18n-check-ignore` comment only when the string should stay literal.

## Theming System

ThemeProvider manages three axes:

| Axis         | Values                          | Persistence     | Mechanism                                    |
|--------------|---------------------------------|-----------------|----------------------------------------------|
| Theme mode   | `light`, `dark`, `system`       | localStorage    | `.dark` class on `<html>`                    |
| Accent color | Any hex value                   | localStorage    | `--color-accent` CSS variable                |
| Density      | `compact`, `comfortable`, `spacious` | localStorage | `--density-spacing` CSS variable (0.75/1/1.25) |

- CSS variables are defined in `globals.css` with light/dark variants.
- Tailwind config maps CSS variables to semantic color names.
- Color palette tokens: `background` (primary/secondary/tertiary), `foreground` (primary/secondary/tertiary), `border`, `ring`, plus semantic variants (`info`, `danger`, `success`, `warning`).

## Component Patterns

- Small, focused components — aim for under 200 lines.
- Props interfaces live in the component file, or in `types.ts` for shared types.
- Use `forwardRef` for components that need ref forwarding (React 19 makes this optional, but the pattern is still used).
- Animations: CSS transitions via Tailwind classes; respect `prefers-reduced-motion`.
- Entrance animations: use the `isLoaded` state pattern with `useEffect` + short timeout.

## Accessibility

- ARIA roles on interactive elements (`role="tab"`, `role="tablist"`, `role="status"`).
- `aria-label` on icon-only buttons.
- `aria-hidden` on visually hidden content.
- `aria-selected` on selectable items.
- Color-only indicators must have text alternatives.
- `prefers-reduced-motion` is respected globally.

## Tauri Integration

- The window starts hidden and is shown via `getCurrentWindow().show()` after React mounts.
- Use `data-tauri-drag-region` on header areas for window dragging.
- Title bar uses `titleBarStyle: "Overlay"` with `hiddenTitle: true` for a custom titlebar.
- `tauri-plugin-window-state` persists window size and position.
- Traffic light offset: `pl-20` (80px) to accommodate macOS window controls.

## Architecture

**All frontend ↔ backend communication in goose2 flows through a single path:**

```
React UI  ──►  @aaif/goose-sdk (TS)  ──►  goose-acp  (WebSocket, ACP)  ──►  goose (core)
```

- The Tauri shell spawns a long-lived `goose serve` process and exposes its WebSocket URL via the `get_goose_serve_url` Tauri command. That is essentially the only Tauri command the frontend needs for backend work — it is how the renderer discovers the ACP endpoint.
- The frontend opens a WebSocket to `goose serve` and talks to it using `@aaif/goose-sdk` (published from `ui/sdk/`). The SDK is generated from the ACP custom-method definitions in `crates/goose-sdk/src/custom_requests.rs`, so every backend method has a typed TypeScript client method.
- `goose-acp` (`crates/goose-acp/src/server.rs`) is the server side of the WebSocket. It implements handlers for the custom ACP methods and calls into the `goose` core crate to do the actual work (providers, config, sessions, dictation, etc.).
- `goose` is the pure domain crate. It knows nothing about Tauri or WebSockets — it just exposes Rust APIs that `goose-acp` handlers invoke.

**This is the pattern you must follow when adding any new backend-touching feature.** When you are vibecoding in this app, it is very tempting to reach for `invoke()` or add an HTTP fetch — don't. The rule is: if a feature needs to talk to `goose` core, it goes through the SDK → ACP → goose chain above.

### The canonical example: skills-as-sources (PR #8675)

The skills → sources migration in [#8675](https://github.com/block/goose/pull/8675) is the clearest illustration of the rule. **It deleted 319 lines of Tauri-command code in `src-tauri/src/commands/skills.rs` and replaced them with ACP custom methods.** If you find yourself wanting to add an `invoke()` command that proxies to `goose`, that PR is what "doing it the other way" looks like. Copy this shape when adding new endpoints:

1. **Define the request/response in `crates/goose-sdk/src/custom_requests.rs`.** Use the `JsonRpcRequest` / `JsonRpcResponse` derives and the `#[request(method = "_goose/<area>/<action>", response = ...)]` attribute. Sources uses namespaced methods like `_goose/sources/create`, `_goose/sources/list`, `_goose/sources/update`, `_goose/sources/delete`, `_goose/sources/export`, `_goose/sources/import` with paired request/response structs (`CreateSourceRequest` / `CreateSourceResponse`, etc.).
2. **Implement the handler in `crates/goose-acp/src/server.rs`** with `#[custom_method(YourRequest)]`. Keep it thin: unpack the request, call into the `goose` crate, wrap the result. The sources handlers are ~5 lines each — e.g. `on_list_sources` just calls `goose::sources::list_sources(...)` and returns the typed response. Errors map to `sacp::Error::invalid_params()` / `internal_error()`.
3. **Put the real logic in the `goose` crate.** Sources lives in `crates/goose/src/sources.rs` — filesystem CRUD, frontmatter parsing, scope resolution, all of it. `goose-acp` knows nothing about where skills are stored on disk; it just forwards typed arguments. This separation is the point.
4. **Regenerate the SDK.** The TS methods on `GooseClient` are generated into `ui/sdk/src/generated/`. Do not hand-edit generated files.
5. **Call it from the frontend via a feature `api/` module.** See `ui/goose2/src/features/skills/api/skills.ts`. It calls `getClient()` from `acpConnection.ts` and invokes the SDK, then adapts the generic `SourceEntry` shape into a feature-friendly `SkillInfo`:
   ```ts
   export async function listSkills(): Promise<SkillInfo[]> {
     const client = await getClient();
     const raw = await client.extMethod("_goose/sources/list", { type: "skill" });
     const sources = (raw.sources ?? []) as SourceEntry[];
     return sources.map(toSkillInfo);
   }
   ```
   Feature code (hooks, stores, UI) imports from that `api/` module — it never touches the ACP client directly.

**Note on typed vs untyped calls.** Skills currently uses `client.extMethod("_goose/sources/...", ...)` (the untyped escape hatch) because it reshapes a generic `Source` API into skill-specific types. The **preferred** shape for new features is the typed generated methods — `client.goose.GooseFooBar({ ... })` — as used by dictation (`client.goose.GooseDictationTranscribe`) and the provider inventory (`client.goose.GooseProvidersList`). Reach for `extMethod()` only when you have a real reason to bypass the generated types.

For a minimal frontend `api/` wrapper using the typed shape, see `ui/goose2/src/features/providers/api/inventory.ts` — ~30 lines, typed SDK calls, thin adapter. For a fully worked end-to-end feature including OS-keychain handling and progress streaming, see the voice dictation feature ([#8609](https://github.com/block/goose/pull/8609)) and `ui/goose2/src/shared/api/dictation.ts`.

### When `invoke()` is still appropriate

Tauri commands (`invoke()` from `@tauri-apps/api/core`) are reserved for things that genuinely belong to the desktop shell, not to `goose` core. In practice that means:

- `get_goose_serve_url` — bootstrapping the ACP connection.
- Secret storage owned by the OS keychain (e.g. `save_provider_field`, `delete_provider_config` — note dictation still uses these for writing API keys into the OS keychain, because that's a shell concern).
- Window state, filesystem dialogs, and other Tauri-plugin-backed capabilities.

If the thing you're building is "get data from goose" or "tell goose to do something," it is **not** one of these cases. Add a custom ACP method instead.

### Don't

- Don't add HTTP `fetch` calls to a `goose` HTTP API, or reintroduce an `apiFetch` utility. There is no HTTP API for goose2 — the backend is the ACP WebSocket.
- Don't manage a sidecar `goose` process from the renderer. The Tauri shell owns that lifecycle.
- Don't add a new `invoke()` command in `src-tauri/` as a proxy to `goose` core. Add an ACP custom method instead.
- Don't hand-edit `ui/sdk/src/generated/`. Regenerate.
- Don't call the ACP client (`getClient()`) directly from UI components or stores. Go through a `shared/api/*.ts` (or `features/<feature>/api/*.ts`) module so the SDK surface is mockable in tests.

## Tooling

| Tool        | Purpose                                        |
|-------------|-------------------------------------------------|
| **Hermit**  | Manages repo binaries such as `node`, `pnpm`, `just`, and `lefthook` |
| **Just**    | Task runner (`just dev`, `just build`, `just check`) |
| **Lefthook**| Git hooks (pre-commit, pre-push)               |
| **Biome**   | Linting and formatting                          |
| **pnpm**    | Package manager                                 |

Additional tooling notes:

- Prefer repo-managed binaries over global tools when there is any ambiguity about PATH.
- Hermit manages `node`, `pnpm`, `just`, and `lefthook`, while Biome comes from `node_modules` after `just setup`.
- Tauri backend commands still rely on a working Rust/Cargo toolchain.
- Pre-commit hooks run formatting plus `just check`.
- Pre-push hooks run `just fmt-check`, `just clippy`, `just check`, `just test`, `just build`, and `just tauri-check`.
- Do not use `--no-verify` to bypass hooks. Fix the underlying issue instead.

## Performance Logging

- Frontend perf logs use `perfLog()` from `@/shared/lib/perfLog`. Messages are tagged `[perf:<channel>]` (startup, conn, load, newtab, prepare, send, api, stream, replay, chatview). Enabled automatically in Vite dev mode, or opt-in via `localStorage.setItem("goose.perf", "1")` in a release build.
- Backend perf logs live in `crates/goose-acp/src/server.rs` under `target: "perf"` at `debug!` level. Off by default; enable with `RUST_LOG=perf=debug,info` on the `goose serve` process.
- `just dev` and `just dev-debug` export `RUST_LOG=perf=debug,info` so the child `goose serve` emits perf logs without extra setup. Override by setting `RUST_LOG` in the environment before invoking `just`.

## Testing & Verification

- Unit/component tests use Vitest and Testing Library via `just test` or `pnpm test`.
- E2E tests use Playwright via `just test-e2e` and `just test-e2e-all`.
- File size enforcement runs through `pnpm check:file-sizes` and is included in `just check`.
- Before handing off a change, run the smallest relevant verification step. Use `just ci` when you need the full local gate.
- GitHub Actions also runs desktop-oriented checks, including Playwright coverage, that are broader than the local pre-push hook.

## Key Dependencies

- `react` 19.1, `react-dom` 19.1
- `@tauri-apps/api` 2.x
- `@tanstack/react-query` 5.x
- `tailwindcss` 3.x with `tailwindcss-animate`
- `@tabler/icons-react` for icons (migrating from `lucide-react`)
- `class-variance-authority` for component variants
- `clsx` + `tailwind-merge` for class merging
- `@radix-ui/react-slot` for polymorphic components

## Don'ts

- Don't import from `../` across feature boundaries — use `@/` paths.
- Don't put business logic in UI components — extract to hooks or utilities.
- Don't use inline styles except for dynamic values (like animation delays).
- Don't add dependencies without checking if an existing one covers the need.
- Don't skip `type="button"` on buttons.
- Don't use color-only indicators without text alternatives.
- Never use `--no-verify` when pushing — fix the underlying lint/hook issues.
- Don't create root-level `src/types/` or `src/stores/` directories — types belong in `src/shared/types/`, stores belong in `src/features/<feature>/stores/`.
- Don't duplicate type definitions across files — each type has one canonical location.
