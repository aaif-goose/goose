# Branching strategy

This fork tracks `aaif-goose/goose` upstream while carrying a custom Tandem UI.
The branch model below keeps upstream updates flowing in without ever risking
the custom work.

> **Where is this doc?** It lives on `tandem/main` (and every `tandem/feat/*`
> branch inherits it). `main` intentionally has no custom file — see below.

---

## The three branch roles

| Branch | Role | Who may commit |
|---|---|---|
| `main` | Pristine mirror of `aaif-goose/goose:main`. | **Nobody directly.** Only advances via `git merge --ff-only upstream/main`. |
| `tandem/main` | Long-lived integration branch. Carries all Tandem custom work. | Merges from `main` (weekly sync); merges from `tandem/feat/*`. |
| `tandem/feat/<name>` | Short-lived feature branches off `tandem/main`. | You, for day-to-day work. Merged back into `tandem/main` when done. |

---

## If you're on `main`

**Purpose:** zero-drift mirror of `aaif-goose/goose:main`. It exists so every
upstream sync is a guaranteed-safe fast-forward.

**Rules:**
- Do **not** commit here. Do **not** open PRs against `main`.
- Advances *only* via `git merge --ff-only upstream/main`.
- If `git diff upstream/main..origin/main` is ever non-empty, something was
  committed here by mistake — investigate before syncing again.

**Typical commands you'd run from main:**
```bash
git fetch upstream --tags
git checkout main
git merge --ff-only upstream/main
git push origin main
```

---

## If you're on `tandem/main`

**Purpose:** the integration root. This is the branch that actually gets built,
tested, and released as the Tandem desktop app. Custom work from feature
branches merges in here, and upstream updates merge in here too.

**Rules:**
- **Never force-push.** Other feature branches rebase/merge onto it.
- Do **not** make direct commits for new features — branch off to
  `tandem/feat/<name>` instead.
- Hotfixes on `tandem/main` are OK if they're small and urgent.
- All PRs targeting this branch go through the *Tandem isolation guard* CI
  check (`.github/workflows/tandem-isolation.yml`).

**What's allowed to change from here:**
- Everything under `ui/tandem/**`
- The three shared workspace files: `Cargo.toml`, `ui/pnpm-workspace.yaml`,
  `ui/package.json` (additive edits only)
- `.github/workflows/tandem-*.yml`
- `BRANCHING.md` at repo root (this file)

Any edit outside that allowlist must carry `[cross-boundary]` in the commit
message, or the CI guard will fail.

**Weekly upstream sync ritual** (Mondays, or when upstream has something you need):

```bash
# 1. Pull upstream into the mirror
git fetch upstream --tags
git checkout main
git merge --ff-only upstream/main
git push origin main

# 2. Tag a rollback anchor on tandem/main, then merge main in
git checkout tandem/main
git pull --ff-only
git tag tandem/pre-sync-$(date +%Y-%m-%d)
git merge main
#    Conflict surface is tiny: the three shared files listed above.
#    Resolve by keeping both upstream's additions and your `tandem` entries.

# 3. Smoke-test BEFORE pushing (see "Verification" below)

# 4. Publish
git push origin tandem/main
git push origin tandem/pre-sync-$(date +%Y-%m-%d)

# 5. Fan the merge out to every active feature branch
git checkout tandem/feat/<name>
git merge tandem/main
```

**Rollback if a sync turns out to break something:**
```bash
git reset --hard tandem/pre-sync-YYYY-MM-DD
```

---

## If you're on `tandem/feat/<name>`

**Purpose:** short-lived scratch space for a single feature, bug fix, or
experiment. Born from `tandem/main`, dies when merged back.

**Rules:**
- Branch from `tandem/main` (never from `main`, never from another feature).
- Name the branch for what it does: `tandem/feat/command-palette-fuzzy`,
  `tandem/feat/diagnostics-redact-tokens`, `tandem/fix/acp-reconnect`.
- Rebase freely while the branch is private. Once it's pushed / shared, stop
  rebasing and use merge commits instead.
- Keep edits inside the allowlist (same as `tandem/main`). The CI guard runs
  on PRs targeting `tandem/feat/**` too.
- Merge back into `tandem/main` via PR so the isolation guard runs. Delete
  the branch after merge.

**Creating a new feature branch:**
```bash
git checkout tandem/main
git pull --ff-only
git checkout -b tandem/feat/<short-name>
```

**Keeping it current while you work:**
```bash
git checkout tandem/feat/<short-name>
git merge tandem/main          # or: git rebase tandem/main (only if unpublished)
```

---

## Verification (smoke test before pushing `tandem/main`)

Run from repo root after any upstream merge:

1. `pnpm install` — catches workspace/dep regressions.
2. `cargo check -p tandem` — catches Rust-side breakage from upstream Cargo
   changes.
3. `pnpm --filter tandem tauri dev` — launch the app, verify:
   - Window opens
   - Composer connects to goose backend (ACP stream)
   - Settings modal opens and persists
   - Diagnostics modal exports a ZIP

Only push after all four pass.

---

## Remotes

```
origin    = https://github.com/larmax82/goose.git     (your fork — push here)
upstream  = https://github.com/aaif-goose/goose.git   (read-only — never push)
```

The `upstream` push URL is deliberately set to a non-resolving value so
`git push upstream` fails loudly if attempted by accident.

---

## Quick reference — "can I commit this change here?"

| Change touches… | On `main`? | On `tandem/main`? | On `tandem/feat/*`? |
|---|---|---|---|
| `ui/tandem/**` | ❌ | ✅ | ✅ |
| `Cargo.toml` / `ui/pnpm-workspace.yaml` / `ui/package.json` (additive) | ❌ | ✅ | ✅ |
| `.github/workflows/tandem-*.yml` | ❌ | ✅ | ✅ |
| `BRANCHING.md` (this file) | ❌ | ✅ | ✅ |
| Anything else (upstream code) | ❌ | ⚠️ only with `[cross-boundary]` in the commit message | ⚠️ only with `[cross-boundary]` |
| Upstream commits arriving via merge from `main` | ✅ via ff-only merge | ✅ via merge commit | ✅ via merge from `tandem/main` |
