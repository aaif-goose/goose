# GitHub Copilot Code Review Instructions

## Review Philosophy
- Only comment when you have HIGH CONFIDENCE (>80%) that an issue exists
- Be concise: one sentence per comment when possible
- Focus on actionable feedback, not observations
- When reviewing text, only comment on clarity issues if the text is genuinely confusing or could lead to errors. "Could be clearer" is not the same as "is confusing" - stay silent unless HIGH confidence it will cause problems

## Priority Areas (Review These)

### Security & Safety
- Unsafe code blocks without justification
- Command injection risks (shell commands, user input)
- Path traversal vulnerabilities
- Credential exposure or hardcoded secrets
- Missing input validation on external data
- Improper error handling that could leak sensitive info

### Correctness Issues
- Logic errors that could cause panics or incorrect behavior
- Race conditions in async code
- Resource leaks (files, connections, memory)
- Off-by-one errors or boundary conditions
- Incorrect error propagation (using `unwrap()` inappropriately)
- Optional types that don't need to be optional
- Booleans that should default to false but are set as optional
- Error context that doesn't add useful information (e.g., `.context("Failed to do X")` when error already says it failed)
- Overly defensive code that adds unnecessary checks
- Unnecessary comments that just restate what the code already shows (remove them)

### Architecture & Patterns
- Code that violates existing patterns in the codebase
- Missing error handling (should use `anyhow::Result`)
- Async/await misuse or blocking operations in async contexts
- Improper trait implementations

### No Doc Updates with Code Changes
- PRs with code changes shouldn't update `/documentation` - docs deploy on merge, code on release. Use `unlisted: true` or remove/hide docs.

## Project-Specific Context

- This is a Rust project using cargo workspaces
- Core crates: `goose` (agent logic and ACP server), `goose-cli` (CLI), `goose-mcp` (MCP servers)
- Error handling: Use `anyhow::Result`, not `unwrap()` in production code
- Async runtime: tokio
- MCP protocol implementations require extra scrutiny
- Naming convention: In `documentation/docs` and `documentation/blog`, always refer to the project as "goose" (lowercase), never "Goose" (even at the start of sentences)

## CI Pipeline Context

**Important**: You review PRs immediately, before CI completes. Do not flag issues that CI will catch.

### What Our CI Checks (`.github/workflows/ci.yml`)

**Rust checks:**
- `cargo fmt --check` - Code formatting (rustfmt)
- `cargo test --jobs 2` - All tests
- `cargo clippy --all-targets -- -D warnings` - Linting (clippy)

**Desktop app checks:**
- `pnpm install --frozen-lockfile` - Fresh dependency install (in `ui/desktop/`)
- `pnpm run lint:check` - ESLint + Prettier
- `pnpm run test:run` - Vitest tests

**Setup steps CI performs:**
- Installs system dependencies (libdbus, gnome-keyring, libxcb)
- Activates hermit environment (`source bin/activate-hermit`)
- Caches Cargo and pnpm dependencies
- Runs `pnpm install --frozen-lockfile` before any pnpm scripts (ensures all packages are installed)

**Key insight**: Commands like `npx` check local `node_modules` first, which CI installs via `pnpm install --frozen-lockfile`. Don't flag these as broken unless you can explain why CI setup wouldn't handle it.

## Skip These (Low Value)

Do not comment on:
- **Style/formatting** - CI handles this (rustfmt, prettier)
- **Clippy warnings** - CI handles this (clippy)
- **Test failures** - CI handles this (full test suite)
- **Missing dependencies** - CI handles this (pnpm install will fail)
- **Minor naming suggestions** - unless truly confusing
- **Suggestions to add comments** - for self-documenting code
- **Refactoring suggestions** - unless there's a clear bug or maintainability issue
- **Multiple issues in one comment** - choose the single most critical issue
- **Logging suggestions** - unless for errors or security events (the codebase needs less logging, not more)
- **Pedantic accuracy in text** - unless it would cause actual confusion or errors. No one likes a reply guy

## Response Format

When you identify an issue:
1. **State the problem** (1 sentence)
2. **Why it matters** (1 sentence, only if not obvious)
3. **Suggested fix** (code snippet or specific action)

Example:
```
This could panic if the vector is empty. Consider using `.get(0)` or add a length check.
```

## When to Stay Silent

If you're uncertain whether something is an issue, don't comment. False positives create noise and reduce trust in the review process.
