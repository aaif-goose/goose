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

## Backend Architecture

All AI communication goes through **ACP (Agent Client Protocol)**:
- The Tauri app starts a long-lived `goose serve` process and exposes its WebSocket URL.
- The frontend connects directly to `goose serve` over WebSocket and handles ACP notifications in TypeScript.

For non AI communication, such as configuration:
- Use **Tauri commands** (`invoke()` from `@tauri-apps/api/core`) for request/response operations (sessions, personas, skills, projects, etc.).
- Use **Tauri events** (`listen()` from `@tauri-apps/api/event`) for streaming data from ACP.
- Do **not** add HTTP fetch calls to a backend server, `apiFetch` utilities, or sidecar process management.

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
