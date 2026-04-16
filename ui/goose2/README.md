# Goose2

Goose2 is a Tauri 2 + React 19 desktop app.

## Getting Started

1. If your shell cannot find `just`, `pnpm`, or `lefthook`, activate Hermit.
   bash/zsh: `source ./bin/activate-hermit`
   fish: `source ./bin/activate-hermit.fish`
2. Install git hooks: `lefthook install`
3. Install dependencies: `just setup`
4. Start the app: `just dev`

`just clean` removes Rust build artifacts, `dist`, and `node_modules`. Run `just setup` again before `just dev`.

`just setup` bootstraps a shared managed goose checkout in a home-level cache directory when it does not exist, fast-forwards it, builds a local `goose` binary, and stamps the exact branch/commit it used. `just dev` only does a lightweight preflight against that shared stamp; if the managed checkout is missing, stale, or built from the wrong branch, it warns and tells you to rerun `just setup`. By default the helper uses `~/Library/Caches/goose2-dev` on macOS, or `$XDG_CACHE_HOME/goose2-dev` / `~/.cache/goose2-dev` elsewhere. It prefers `origin/baxen/goose2` and falls back to `origin/main` when that branch does not exist yet.

Override the shared cache root or branch with `GOOSE_DEV_ROOT=/path/to/cache` and `GOOSE_DEV_BRANCH=my/integration-branch`. You can also override the checkout path directly with `GOOSE_DEV_REPO=/path/to/goose`, or the clone source with `GOOSE_DEV_CLONE_URL=...`.

Run `just` to list available commands, or see [justfile](./justfile) for the full recipe definitions.

## Important Files

- [AGENTS.md](./AGENTS.md) repo conventions and agent guidance
- [justfile](./justfile) local setup, dev, test, and CI commands
- [CODEOWNERS](./CODEOWNERS) code ownership
- [.github/workflows/ci.yml](./.github/workflows/ci.yml) CI checks
- [.github/ISSUE_TEMPLATE/](./.github/ISSUE_TEMPLATE/) issue templates
- [GOVERNANCE.md](./GOVERNANCE.md) project governance
- [LICENSE](./LICENSE) license terms

Project leads should keep this README, [CODEOWNERS](./CODEOWNERS), and the issue templates current. If this repo grows beyond the quick-start flow above, add a `CONTRIBUTING.md` and link it here once it exists.
