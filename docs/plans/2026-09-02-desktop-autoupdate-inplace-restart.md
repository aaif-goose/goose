# Plan: Desktop Auto-Update In-Place Restart (macOS + Windows)

**Source:** `docs/brainstorms/2026-09-02-desktop-autoupdate-inplace-restart.md` (Approach A)
**Goal:** In-place, restart-and-replace auto-update — no new zip/duplicate app, overwrite same install, keep zip cached for rollback, "Restart now" notification.

## Context
- Current: `ui/desktop/src/utils/autoUpdater.ts` (845 LOC) uses `electron-updater` + `quitAndInstall(false, true)` + fallback to `githubUpdater.ts` (746 LOC) which already does `extractArchive`/`resolvePayloadPath`/`SwapCommand` but leaves artifacts.
- Forge: `ui/desktop/forge.config.ts` maker-zip + publisher-github.
- Decisions: macOS+Windows only, graceful-fail permissions (no elevation v1), keep cached zip in `userData/update-cache`, Restart-now UX.

## Implementation Steps

### Step 1: Extend githubUpdater.ts — cached zip, prune, writability pre-check
**Files:** `ui/desktop/src/utils/githubUpdater.ts`
- Add `getUpdateCacheDir()`, `ensureCacheDir()`, `pruneCache(keep=2)` helpers using `app.getPath('userData')/update-cache`.
- Add `isTargetWritable(targetPath)` via `fs.access(targetPath, constants.W_OK)` with macOS `.app` bundle check (check parent + bundle).
- Change download path from `os.tmpdir()`/random to `cacheDir/<version>.zip` — keep after install.
- After successful swap, delete only `extractDir`, not zip; call `pruneCache`.
- Export helpers for testing.

### Step 2: Make swap use app.relaunch + graceful failure + Notification
**Files:** `ui/desktop/src/utils/githubUpdater.ts`, `ui/desktop/src/utils/autoUpdater.ts`
- In `githubUpdater.installUpdate()`: pre-flight writability check; if fails, return `{ success:false, needsPermission:true }` instead of swapping; caller shows error Notification.
- On success: spawn detached? Prefer `app.relaunch()` + `app.quit()` (preserves args). Only use detached shell if file lock detected (Windows).
- Ensure `autoUpdater.ts` fallback path calls `githubUpdater.installUpdate` and handles `needsPermission` case.
- Add code-sign verification log: `codesign --verify --deep` on macOS (non-blocking, log only).

### Step 3: Update autoUpdater.ts — unify Restart-now flow, cleanup
**Files:** `ui/desktop/src/utils/autoUpdater.ts`
- On `update-downloaded`: show `Notification` with "Restart now" button (existing handler at ~641). Click → call `installUpdate` (either `quitAndInstall` or `githubUpdater` swap). Ensure zip not duplicated in Downloads.
- For `isUsingGitHubFallback` branch, use `githubUpdater` swap helper instead of direct `quitAndInstall`.
- Ensure `autoDownloadDisabled` handling still works.
- Clean up staging dir after electron-updater success (delete temp zip if any).
- Keep analytics hooks `trackUpdateInstallInitiated` etc.

### Step 4: Main.ts wiring + IPC
**Files:** `ui/desktop/src/main.ts`
- No major change; ensure `setupAutoUpdater` still called at 2542 after window creation; verify IPC `check-for-updates` returns updated info.
- Optionally add IPC `install-cached-update` for manual rollback (stretch).

### Step 5: Tests & verification
- Unit test: `ui/desktop/src/utils/__tests__/githubUpdater.test.ts` (new) — mock `fs.access`, test `isTargetWritable`, `pruneCache`, cache path.
- Manual smoke: `pnpm --prefix ui/desktop test` or `vitest` ; `cargo fmt` / `cargo clippy` not needed (desktop only) but run `pnpm run typecheck` and `pnpm run lint` if available.
- Runtime smoke: build dev `pnpm --prefix ui/desktop start` and trigger fake update via `forceDevUpdateConfig`.

## Effort
S (1-2 weeks). This plan implements Approach A minimal.

## Risks
- Windows file lock during swap → fallback to `setTimeout` retry or `Update.exe` style.
- Gatekeeper: `ditto` preserves xattrs, verify with `codesign --verify`.

## Verification Plan
- [ ] `ui/desktop/src/utils/githubUpdater.ts` has cache + prune + writability helpers
- [ ] No zip left in Downloads/parent after update; cache has ≤2 zips
- [ ] Notification "Restart now" triggers relaunch and version bumps
- [ ] Typecheck passes: `pnpm --prefix ui/desktop run typecheck`
- [ ] Existing updater tests pass

