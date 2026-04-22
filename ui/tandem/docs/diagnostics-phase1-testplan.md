# Diagnostics Phase 1 — Test Plan

Verify that the bug-report button in the composer and the "Report a bug" command-palette entry both open a modal that can download a diagnostics ZIP and open a pre-filled GitHub issue.

Reference: `ui/tandem/docs/diagnostics-plan.md`. Files changed in Phase 1:

- `ui/tandem/src-tauri/Cargo.toml` — added `zip`, `chrono`, `os_info`.
- `ui/tandem/src-tauri/src/commands/diagnostics.rs` *(new)*
- `ui/tandem/src-tauri/src/commands/mod.rs` — registers the new module.
- `ui/tandem/src-tauri/src/lib.rs` — two new commands in `invoke_handler!`.
- `ui/tandem/src/services/diagnostics.ts` *(new)*
- `ui/tandem/src/components/diagnostics/DiagnosticsModal.tsx` *(new)*
- `ui/tandem/src/components/chat/Composer.tsx` — bug button `onClick`.
- `ui/tandem/src/App.tsx` — state, modal mount, palette wiring, `onReportBug` prop.

---

## 0. Pre-flight

### 0.1 Clean target before first run (Windows)

`cargo check` failed during implementation with `Access is denied` inside `tauri-build`. If you hit the same, close any running Tandem/Tauri process and delete the stale build artifacts:

```bash
# Close all Tandem windows first.
taskkill /IM tandem-tauri.exe /F
taskkill /IM tandem.exe /F
# Optional nuke if builds still fail:
rm -rf ui/tandem/src-tauri/target/debug/build/tandem-*
```

### 0.2 Launch

```bash
cd ui/tandem
pnpm install          # picks up any lockfile shifts
pnpm tauri dev
```

First build will take a couple of minutes (pulling `zip`, `os_info`, `chrono`). Subsequent HMR is fast.

**Pass criteria:** Tandem window opens, no red errors in the Tauri dev console, no warnings about unknown commands.

### 0.3 DevTools

Open DevTools in the webview: `Ctrl+Shift+I`. Keep the **Console** tab visible for every test — any unhandled promise rejection from `invoke(...)` or `openUrl(...)` will land there.

---

## 1. Smoke test (2 minutes)

1. Open Tandem. Verify the **bug icon** is visible at the right edge of the composer footer (after MCP).
2. Hover the icon — tooltip says "Report a bug".
3. Click it — the **Report a problem** modal appears, scrim behind it.
4. Click **Cancel** — modal closes, no toast, no console error.
5. Press `Ctrl+K` (command palette), type `bug`, press Enter — same modal opens.
6. Press `Esc` — behaviour is currently **click-outside closes**, not `Esc`. That's expected for Phase 1.
7. Click outside the modal card — closes.

**Pass criteria:** All open/close paths work with no console errors.

---

## 2. Golden path — Download ZIP

### 2.1 Empty session

1. Start a fresh Tandem. Do NOT send any messages.
2. Open the bug modal.
3. Click **Download**. The native save-dialog appears.
4. **Default filename check:** expect `tandem-diagnostics-YYYYMMDD-HHMMSS-<6chars>.zip` where the tail is the last 6 chars of `tab.id`. Timestamp should be local-time "now" within a minute.
5. Save to a scratch folder (e.g. `E:\tmp\`).
6. Toast appears: `Diagnostics saved (X.X KB) to <path>`. Modal closes.

### 2.2 Inspect the ZIP

Extract the ZIP. It must contain exactly 3 files:

- `system.json`
- `session.json`
- `README.txt`

Verify:

**`system.json`** (JSON, pretty-printed). All fields present:
```
appName         == "Tandem"
appVersion      == "0.1.0"  (matches Cargo.toml)
tauriVersion    == matches `tauri --version` (minus the "tauri-cli" prefix)
os              == "windows"   (or "macos" / "linux")
osVersion       == non-empty, e.g. "10" / "11" on Windows
architecture    == "x86_64" (or "aarch64", "x86")
timestampUtc    == RFC3339, e.g. "2026-04-21T14:32:07.123456789+00:00"
provider        == null           (Phase 1 — not wired)
model           == "opus-4.7"     (whatever is selected in the composer)
enabledMcpServers == JSON array of on MCP names
```

**`session.json`** (JSON, pretty-printed). Structure:
```json
{
  "tabId": "t...",
  "title": "New chat",
  "gooseSessionId": null,   // empty session
  "messages": []
}
```

**`README.txt`**: human-readable. Confirm it contains:
- `Generated:` followed by the same ISO timestamp as system.json.
- `App: Tandem 0.1.0`
- `Session: New chat (tab id: t...)`
- The sensitive-info warning.

### 2.3 Session with content

1. Send a message ("hello"). Wait for Goose to respond.
2. Toggle a different model in the composer (e.g. Haiku).
3. Open the bug modal. Click Download. Save.
4. Extract. `session.json` should have `messages` array with the user + assistant exchange, and each message should carry `role`, `paragraphs`, and — for the assistant — `tools` with any `Write`/`Terminal` calls that ran.
5. `system.json` → `model` should reflect the model you just switched to.

**Negative assertion:** `session.json` must NOT contain `streaming: true`, `composer` drafts, or `gooseSessionId` leaking into per-message records. Only the top-level `gooseSessionId` field.

### 2.4 Save-dialog cancellation

1. Open modal. Click Download. When the save-dialog appears, press Cancel (or `Esc` in the OS dialog).
2. The modal stays open, no toast, no error banner, no console message.

### 2.5 Save to a locked path

1. Try saving to a read-only folder (e.g. `C:\Windows\System32\readonly.zip`, or any path you don't have write permission for).
2. Expected: red error banner inside the modal body: `Failed to create <path>: ...`. The modal stays open, buttons re-enable.

### 2.6 Overwrite behaviour

1. Download to `E:\tmp\phase1.zip`. Confirm file exists.
2. Download again, choose the same filename. The OS save-dialog will prompt to overwrite — confirm Yes.
3. Open the new ZIP: should be a fresh bundle, not appended/corrupt.

---

## 3. Golden path — File Bug on GitHub

### 3.1 URL opens

1. Open modal. Click **File bug on GitHub**.
2. Expected: your default browser opens a new tab at `https://github.com/larmax82/goose/issues/new?...`.
3. Modal closes. No toast (Phase 1 has no success toast for this path).

### 3.2 Issue body interpolation

In the opened GitHub page, click "Markdown / Preview" to inspect. Confirm the body contains these **exact lines** (Phase 1 uses frontend-known fields only since provider isn't surfaced):

```
- **App:** Tandem 0.1.0 (Tauri <version>)
- **OS & Arch:** windows <version> <arch>
- **Provider & Model:** [e.g. openrouter - claude-opus-4.7]   ← fallback, no provider yet
- **MCP servers enabled:** <actual list, or the fallback>
```

Labels on the issue draft (right sidebar): **bug, tandem**.

### 3.3 Capability check

If the button throws, the most likely cause is a Tauri permissions error. In DevTools console, watch for:

```
forbidden method: opener:default > open_url
```

If you see that, add `opener:allow-open-url` (with a URL scope for `https://github.com/**`) to `ui/tandem/src-tauri/capabilities/default.json` and rebuild. Include a screenshot of the error in the test report.

### 3.4 URL length sanity

1. Send a long message (paste ~2 KB of lorem ipsum). Let Goose reply (long response ok).
2. File a bug. Confirm the browser opens successfully — no "URL too long" error from the browser.
3. (Expected not to break. Phase 2 will interpolate a larger body; test again there.)

---

## 4. Edge cases

### 4.1 Multiple tabs

1. Open 3 tabs via `Ctrl+N`. Send different messages in each.
2. For each tab, open the modal and click Download. Save with distinct names.
3. Each ZIP's `session.json` must correspond to the tab that was active when Download was pressed (matching `tabId`, `title`, `messages`).

### 4.2 Empty composer-draft doesn't leak

1. Type some text into the composer but do NOT send.
2. Open the modal, click Download.
3. `session.json` should NOT contain the draft text. The draft lives in `tab.composer` which is deliberately excluded by `serializeTranscript`.

### 4.3 Double-click protection

1. Open the modal. Click Download. In the brief moment before the save-dialog opens, try clicking **File bug on GitHub** too.
2. Expected: second button visually disabled (grey) during the busy state. Only one in-flight operation at a time.

### 4.4 Scrim while busy

1. Open the modal. Click Download. While the save-dialog is open, DO NOT touch the Tandem window — OS dialog owns focus.
2. After the dialog resolves, click outside the Tandem modal: it should close normally.
3. Try clicking the scrim while `Saving\u2026` is showing (before dialog resolves — you need to throttle via a large session to catch this window). Expected: no-op. Scrim click is disabled when `busy`.

### 4.5 Unicode in title

1. Rename a tab (open an existing chat / recipe) so its title includes emoji or non-ASCII characters.
2. Download. `README.txt` should show the title correctly. `session.json` should round-trip (UTF-8, no `\uXXXX` escapes as long as JSON serialization default doesn't force ASCII — `serde_json::to_vec_pretty` does not).
3. The filename on disk is ASCII (derived from `tab.id`), so no filesystem fuss.

### 4.6 Long paths (Windows)

1. Save to a very deep path (>200 chars total). Windows has `MAX_PATH=260` unless long-path support is enabled.
2. Expected: if write fails, error banner appears with Windows's error text. Not a Phase 1 blocker — just confirm it surfaces cleanly.

### 4.7 Disk full

Not practical to simulate unless you have a small RAM disk. Skip unless you already have one.

---

## 5. Rust-side sanity

### 5.1 Commands registered

In DevTools console, run:

```js
await window.__TAURI__.core.invoke("get_system_info")
```

Should return an object matching §2.2's `system.json` shape. If you get `command not found: get_system_info`, the registration in `lib.rs` didn't take — restart the dev server.

### 5.2 Write to an explicit path

```js
await window.__TAURI__.core.invoke("write_diagnostics_zip", {
  request: {
    sessionTranscriptJson: JSON.stringify({ hi: 1 }),
    sessionTitle: "manual",
    sessionTabId: "manual-test",
    provider: null,
    model: "opus-4.7",
    enabledMcpServers: ["filesystem"],
    outputZipPath: "E:/tmp/manual-phase1.zip",
    includeMemoryDir: false,
  }
})
```

Expected return:
```json
{ "bytesWritten": <nonzero>, "entries": ["system.json","session.json","README.txt"], "outputPath": "E:/tmp/manual-phase1.zip" }
```

Open the ZIP — should be valid.

### 5.3 Path normalization

Repeat §5.2 with `outputZipPath: "E:\\tmp\\backslash.zip"` (Windows-style). It should also work; `File::create` accepts either slash convention.

---

## 6. Regressions to watch for

These are **existing** flows that shouldn't have been touched by Phase 1. A quick sanity click-through:

- Sending a chat message still works and streams.
- Settings modal (`⌘,`) still opens and saves.
- The composer's other footer buttons (context folder picker, paperclip, slash, token counter, model, MCP) still open their popovers without clipping (the earlier `overflow:hidden` fix should be intact).
- Memory panel still refreshes after a Write tool completes (unchanged).

If any of these break, it's likely an accidental side-effect of the composer prop addition. Look at `onReportBug` being plumbed through in `App.tsx` and `Composer.tsx`.

---

## 7. Deliverables per run

For the test report, capture:

1. **One screenshot** of the open modal.
2. **A valid Phase 1 ZIP** (sessionless + session-with-content variants).
3. **A paste of** `system.json` contents.
4. **The generated GitHub URL** (copy from the browser address bar).
5. **Console output** for any error scenarios (§2.5, §3.3).

Known **not-yet-done** (intentional, Phase 2+):

- `provider` is always `null` (not wired from composer state).
- `settings.json` / `state.json` not included.
- `logs/goose-serve.log` not included (log capture is Phase 3).
- `memory/` not included even if the checkbox is toggled.
- Esc-to-close not wired (scrim click is the only "cancel").
- No keyboard focus trap in the modal.

If any of these appear to work, that's a bug, not a feature.

---

## 8. Exit criteria for Phase 1 sign-off

All of the following must pass to approve Phase 1 and move to Phase 2:

- [ ] §1 smoke: open / close from both button and palette.
- [ ] §2.2 ZIP contents correct (3 files, valid JSON, matching fields).
- [ ] §2.3 transcript captures real messages + tools.
- [ ] §2.4 cancel save-dialog is a true no-op.
- [ ] §2.5 error state renders inside the modal, buttons re-enable.
- [ ] §3.1 GitHub URL opens in the default browser at the fork repo.
- [ ] §3.2 body contains App / OS / Arch lines.
- [ ] §4.1 different tabs produce different transcripts.
- [ ] §6 no regressions in existing features.

Anything else (cosmetic, UX polish) → file as an issue rather than a blocker.
