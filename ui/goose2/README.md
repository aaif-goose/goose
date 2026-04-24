# Goose2

Goose2 is a Tauri 2 + React 19 desktop app.

## Getting Started

1. If your shell cannot find `just`, `pnpm`, or `lefthook`, activate Hermit.
   bash/zsh: `source ./bin/activate-hermit`
   fish: `source ./bin/activate-hermit.fish`
2. Install git hooks: `lefthook install`
3. Prepare workspace dependencies: `just setup`
4. Start the app: `just dev`

`just clean` removes Rust build artifacts, `dist`, and `node_modules`. Run `just setup` again before `just dev`.

`just setup` installs UI workspace dependencies, builds the SDK package, and builds the local debug `goose` CLI binary. `just dev` exports `GOOSE_BIN` to that local binary and loads `src-tauri/tauri.dev.conf.json`, which clears the production `externalBin` requirement during development.

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
