## Clarified Problem Statement

**Goal:** Replace the desktop app's auto-update flow with an in-place, restart-and-replace mechanism (like pingdotgg/t3code) so updating does not leave a new zip/duplicate app bundle but overwrites the running app and restarts it.

**Constraints:**
- Must preserve macOS code-signing / notarization and Windows signing; cannot break Gatekeeper / SmartScreen.
- Must work when app is in `/Applications` (macOS), `Program Files` (Windows), `/opt` (Linux deb/rpm) with appropriate permissions (prompt/fail gracefully if not writable).
- Must not break existing Electron Forge + `electron-updater` release pipeline (GitHub Releases via `@electron-forge/publisher-github`).
- Must handle **macOS and Windows only** (Linux deb/rpm/flatpak out of scope, delegated to package manager).
- Restart via **"Restart now" Notification** (not silent auto-restart): on `update-downloaded` show Notification, click triggers swap + `app.relaunch`/`app.quit`.

**Non-goals:**
- Changing release channels / feed URL logic or adding delta/differential download optimization (unless required for in-place).
- Reworking Linux package-manager updates (apt/dnf/flatpak) beyond the existing deb/rpm/flatpak makers — those stay as-is.
- Background silent update scheduling / rollout % / telemetry redesign (keep existing analytics hooks).
- CLI (`goose-cli`) update mechanism.

**Success criteria:**
- Triggering "Check for updates" downloads once, **keeps zip cached for rollback** in `app.getPath('userData')/update-cache` (not `~/Downloads`), replaces files at the current install path, and relaunches the same `Goose.app` / `Goose.exe` / `/opt/Goose` binary on next launch (verified by `app.getVersion()` bump).
- No duplicate `.app` / `.zip` left in `~/Downloads` or install parent dir after update; cache dir holds last 1-2 zips for rollback, auto-pruned.
- Manual test on macOS (Apple Silicon + Intel) and Windows shows: install vN, update to vN+1 via in-app updater, app restarts automatically, `ps` shows new PID, old version not present.
- Existing `autoUpdater` events (`checking-for-update`, `update-available`, `download-progress`, `update-downloaded`, `error`) still fire and UI reflects them.

## Approaches Considered

### Approach A: Fix the GitHub-fallback in-place swap (minimal, t3code-inspired)
- Sketch: Keep `electron-updater` as primary. Make `ui/desktop/src/utils/githubUpdater.ts` the canonical "zip-free" path: download to `os.tmpdir()`, `extractArchive` with `ditto`/`Expand-Archive`/`unzip`, resolve payload via `resolvePayloadPath`, validate with `REQUIRED_INSTALL_DIRECTORIES` allowlist, then atomic swap via temp `SwapCommand` script + `app.relaunch`/`app.quit`. Delete archive + extract dir on success. Patch `autoUpdater.ts:quitAndInstall` branch to also clean up and use same swap helper when `isUsingGitHubFallback`.
- Affected files: `ui/desktop/src/utils/autoUpdater.ts` (~845 LOC, IPC + event handlers), `ui/desktop/src/utils/githubUpdater.ts` (~746 LOC, `extractArchive`, `runCommand`, `SwapCommand`), `ui/desktop/src/main.ts` (setup at line 2542), `ui/desktop/forge.config.ts` (keep `maker-zip` but no change).
- Tradeoffs: Smallest change; reuses existing safety checks (shared-directory guard, runtime allowlist). Still two codepaths (`electron-updater` vs fallback) — risk of drift. Does not remove zip maker; just cleans up after. Fastest to ship.
- Effort: S (1-2 weeks, mostly testing permissions/signing)

### Approach B: Vendor t3code's updater verbatim (faithful port)
- Sketch: Clone t3code's updater module (shell-script `rsync`/`ditto` over existing bundle while app is quitting, spawned detached). Replace both update paths with a single `inplaceUpdater.ts` that: downloads asset to temp, verifies signature/checksum, spawns a detached helper (`sh -c "sleep 0.5; rsync -a --delete payload/ target/; open -n target/Goose.app"` on macOS, equivalent PowerShell on Windows), then `app.quit()`. `autoUpdater.ts` becomes thin wrapper that only checks version, delegating install to helper. Delete `githubUpdater.ts` or keep as legacy.
- Affected files: new `ui/desktop/src/utils/inplaceUpdater.ts`, `ui/desktop/src/utils/autoUpdater.ts` (shrink to check-only), `ui/desktop/src/utils/githubUpdater.ts` (deprecate/remove), `ui/desktop/forge.config.ts`/`package.json` (add helper scripts, maybe `extraResource` for shell helper), `ui/desktop/src/main.ts`.
- Tradeoffs: Most faithful to t3code UX — true "replace same app, restart". Single install path. Requires vendoring + maintaining foreign shell logic, careful code-sign handling (macOS `ditto` preserves xattrs/signing, `rsync` may not). More risk on Windows (file locks). Larger review surface.
- Effort: M (2-4 weeks, cross-platform QA + signing verification)

### Approach C: Go full electron-updater differential + autoInstallOnAppQuit=false (upstream-aligned)
- Sketch: Remove GitHub-fallback zip path entirely. Configure `electron-updater` for differential updates (`autoUpdater.autoDownload`, `autoInstallOnAppQuit=false`, `forceDevUpdateConfig` handling) and rely on `app-update.yml` + GitHub Releases feed. `quitAndInstall(false, true)` already does in-place Squirrel replacement on macOS/Windows; fix is to ensure it does not stage a zip side-by-side and to clean staging dir. For Linux, keep deb/rpm makers but disable in-app updater (delegate to package manager). Add explicit `before-quit-for-update` hook that deletes staging zip.
- Affected files: `ui/desktop/src/utils/autoUpdater.ts` (remove fallback branching, simplify to ~300 LOC), `ui/desktop/forge.config.ts` (tune `packagerConfig.extraResource`, makers), `ui/desktop/src/utils/githubUpdater.ts` (delete), `ui/desktop/package.json` (`electron-updater` config, `build.publish`), `ui/desktop/src/updates.ts`.
- Tradeoffs: Cleanest long-term, least custom code, leverages Squirrel.Mac/Windows tested path. Loses the GitHub fallback that helps when `electron-updater` feed fails. Requires confidence that `electron-updater` quitAndInstall is truly in-place on all platforms (it is on macOS/Win, but Linux is no-op). Not a t3code clone — violates literal request but achieves same UX with less shell.
- Effort: M (2-3 weeks, but risky if feed/Squirrel edge cases regress)

## Recommendation

**Approach A** — fix the existing `githubUpdater` swap to be the zip-free, restart-in-place path and make `autoUpdater` delegate to it when fallback is used. It is the smallest change that literally satisfies "no new zip, replace same app, restart" while preserving the current Forge pipeline and safety guards (`REQUIRED_INSTALL_DIRECTORIES`, `SHARED_DIRECTORY_NAMES`). It can be evolved into B later if you want to fully vendor t3code's helper. If you are willing to drop the fallback, C is the cleaner second choice.

## Decisions (from brainstorm Q&A 2026-09-02)

- **Q1 Platform:** macOS + Windows in-scope; Linux explicitly out.
- **Q2 t3code reference:** **My take:** don't vendor t3code verbatim. Use hybrid: `electron-updater.quitAndInstall(false,true)` for primary path (Squirrel already in-place), and for `githubUpdater` fallback use `ditto -x -k` (macOS) / `Expand-Archive` (Windows) + `app.relaunch` + `app.quit` with detached helper only if file lock. `ditto` preserves code-signing xattrs; `rsync -a --delete` does not. If you want literal t3code, that's Approach B — but adds maintenance for no UX gain.
- **Q3 Restart:** "Restart now" Notification (user-confirmed), not silent. Matches `autoUpdater.ts:641` `update-downloaded` handler.
- **Q4 Permissions:** **Default to graceful fail** (no UAC/osascript escalation in v1). Pre-flight `fs.access(target, W_OK)`; if not writable, show error Notification + log, keep cached zip, don't swap. Escalation can be added later if needed — keeps v1 small.
- **Q5 Cache:** Keep zip cached for rollback in `userData/update-cache/<version>.zip` (or `app.getPath('temp')` staging), prune to last 2. Delete extract dir after swap.

## Open questions

- **Permission escalation follow-up:** If graceful-fail (Q4) causes support tickets for non-writable `/Applications` / `Program Files`, should v2 add `osascript with administrator privileges` (macOS) / UAC elevation (Windows) helper? Defer until data.
- **Codesign verification:** Log `codesign --verify --deep` after swap but don't block relaunch — ok, or should failure block restart and rollback to cached zip?
