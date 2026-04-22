# Talos Diagnostics / "Report a Problem" Implementation Plan

## 0. Verified context

- **Repo (for filing issues):** `https://github.com/larmax82/goose` (from `git config --get remote.origin.url`). This is a fork — do NOT use `block/goose` or `aaif-goose/goose` (desktop's hardcoded URL).
- **Goose subprocess logs are discarded.** `ui/talos/src-tauri/src/services/goose_serve.rs` (lines 53-55) currently has `stdout(Stdio::null())` and `stderr(Stdio::null())`. Log capture must be added as part of this feature.
- **No `session_manager`, no `config.yaml`.** Transcript lives in `tabs[].messages` (`App.tsx`); settings are `{ memoryDir, projectsDir }` stored via `tauri-plugin-store` at `$APP_CONFIG_DIR/settings.json`.
- **Plugins already wired:** `opener`, `dialog`, `shell`, `store`. `opener:default` permission is already in `capabilities/default.json`, but **GitHub URL opening may need `opener:allow-open-url`** (verify at implementation time — on Tauri 2 the default scope sometimes whitelists only local files).

---

## 1. Inventory — Desktop vs. Talos

| Concern | Desktop (source) | Talos (destination) | Action |
|---|---|---|---|
| Modal UI | `ui/desktop/src/components/ui/Diagnostics.tsx` | — | **Adapt** (strip i18n, lucide, `Button` primitive; use Talos's `Icon` + `.scrim`/`.palette` CSS pattern from `SettingsModal.tsx`) |
| Bug-report trigger | `ui/desktop/src/components/ChatInput.tsx` (L1684-1710) | `ui/talos/src/components/chat/Composer.tsx` L193-196 (no-op) | **Wire up** (lift `bugModalOpen` into `App.tsx` like `settingsOpen`, or keep local in `Composer` — see §3) |
| ZIP generator (Rust) | `crates/goose/src/session/diagnostics.rs::generate_diagnostics` — reads `Paths::in_state_dir("logs")`, `config.yaml`, `SessionManager::export_session`, templates | — | **Rebuild from scratch** in Talos (no `Paths`, no `config.yaml`, no `session_manager`). Transcript flows from frontend, not Rust. |
| System info | `crates/goose/src/session/diagnostics.rs::SystemInfo` (uses `sys_info`, `chrono`, `Config::global`, `get_enabled_extensions`) | — | **Slim port**: only `os`, `os_version`, `architecture`, `app_version`, talos build id, provider/model (provided by frontend), ACP port. No config/extensions concept. |
| ZIP HTTP delivery | `crates/goose-server/src/routes/status.rs` `GET /diagnostics/{id}` | — | **Not applicable.** Talos has no goose-server HTTP. ZIP is built and saved directly via Tauri command + `plugin-dialog` save-dialog. |
| Session export format | `session_manager.export_session(id)` → JSON | `tabs[].messages: Message[]` in React state | **New serializer**: serialize the current tab's `messages` to JSON on the frontend, pass to Rust as a string. |
| Config snapshot | `config.yaml` | Talos `settings.json` in `$APP_CONFIG_DIR` | **Include** the Talos settings file (paths, no secrets today). Also include `state.json` (the `tauri-plugin-store` file) *optionally* or with PII scrub. |
| Log files | `Paths::in_state_dir("logs")/*.jsonl` + server log dir | None | **New**: tee `goose serve` stdout+stderr to a rotating file under `app_log_dir()`. |
| GitHub URL | `https://github.com/aaif-goose/goose/issues/new` | — | **Replace** with `https://github.com/larmax82/goose/issues/new`. |

---

## 2. Rust work (`ui/talos/src-tauri/`)

### New file: `src/commands/diagnostics.rs`

```text
Pseudo-signatures (no code per instructions):

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    app_version: String,          // env!("CARGO_PKG_VERSION") of talos crate
    app_name: String,             // "Talos"
    tauri_version: String,        // tauri::VERSION
    os: String,                   // std::env::consts::OS
    os_version: String,           // sys_info::os_release() || "unknown"
    architecture: String,         // std::env::consts::ARCH
    hostname: Option<String>,     // sys_info::hostname().ok() — gated by privacy flag; see §7
    goose_binary: Option<String>, // resolved sidecar path or $GOOSE_BIN
    goose_serve_port: Option<u16>,// from GooseServeProcess if already spawned
    timestamp_utc: String,        // chrono::Utc::now().to_rfc3339()
    // Provided by frontend (model + provider come from React state):
    provider: Option<String>,
    model: Option<String>,
    enabled_mcp_servers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    // Pre-serialized JSON string of the current tab's Message[] + title + id.
    // The frontend does the serialization because types already live there.
    session_transcript_json: String,
    session_title: String,
    session_tab_id: String,
    // Frontend-side system info (merged with Rust-side)
    provider: Option<String>,
    model: Option<String>,
    enabled_mcp_servers: Vec<String>,
    // User-chosen save path from the Tauri save-dialog.
    output_zip_path: String,
    // Toggle from the modal: include memoryDir tree? (default false — PII)
    include_memory_dir: bool,
}

#[tauri::command]
pub async fn get_system_info(app_handle: AppHandle) -> Result<SystemInfo, String>;
// Populates the Rust-owned fields. Frontend merges its own (provider/model/mcp).

#[tauri::command]
pub async fn write_diagnostics_zip(
    app_handle: AppHandle,
    request: DiagnosticsRequest,
) -> Result<WriteDiagnosticsResult, String>;
// Returns { bytesWritten, entries: Vec<String> } so the modal can show a summary.

#[tauri::command]
pub async fn read_log_tail(
    app_handle: AppHandle,
    max_bytes: Option<usize>, // default 256 KiB
) -> Result<String, String>;
// Diagnostic helper used by "view logs" buttons if we add them later.
```

### ZIP contents (authoritative list)

- `system.json` — `SystemInfo` serialized pretty.
- `system.txt` — human-readable version (mirrors desktop's `to_text`).
- `session.json` — the frontend-supplied transcript string, verbatim.
- `settings.json` — copy of `$APP_CONFIG_DIR/settings.json` if it exists.
- `state.json` — copy of `$APP_CONFIG_DIR/state.json` (tabs snapshot) with a PII-scrubber pass (see §7).
- `logs/goose-serve.log` — current log file (see §4).
- `logs/goose-serve.log.1` (and `.2`) — rotated tails if present.
- `logs/talos-app.log` — if we adopt `tauri-plugin-log` (optional, phase 4).
- `memory/…` — **only** if `include_memory_dir` is true and `settings.memoryDir` resolves; walked with a 50 MB total-size cap.
- `README.txt` — boilerplate: "This bundle was generated by Talos vX on [date]. Do not post publicly if your transcript contains secrets."

### New file: `src/services/log_capture.rs`

Responsibilities:

- Open/create a log file at `app_handle.path().app_log_dir()?.join("goose-serve.log")`. On Windows this resolves to `%LOCALAPPDATA%\com.talos.app\logs\`.
- Rotate on startup: if the existing file is > 5 MiB, rename `.log` → `.log.1` (evicting `.log.1` → `.log.2`, dropping `.log.2`).
- Expose `pub fn child_stdio(app_handle: &AppHandle) -> Result<(Stdio, Stdio), String>` returning `(stdout, stderr)` backed by `File::try_clone()`-duplicated handles for the spawned child. On Tokio this means `Stdio::from(std::fs::File)`.
- Provide a `pub fn tail_current(max_bytes: usize) -> Result<String, String>` helper used by `read_log_tail`.

### Modified files

- `src/services/goose_serve.rs` (L53-55): replace the two `Stdio::null()` calls with handles from `log_capture::child_stdio(&app_handle)`. Keep `kill_on_drop(true)`. Log the resolved log file path at INFO so QA can find it.
- `src/services/mod.rs`: add `pub mod log_capture;`.
- `src/commands/mod.rs`: add `pub mod diagnostics;`.
- `src/lib.rs`: register the three new commands in `invoke_handler!`.
- `capabilities/default.json`: verify `opener:default` covers URL opening; if not, add `opener:allow-open-url` scoped to `https://github.com/**`.

### Cargo additions (`Cargo.toml`)

- `zip = "2"` — for the ZIP writer.
- `chrono = { version = "0.4", features = ["serde"] }` — timestamps.
- `sys-info = "0.9"` — OS release. (Alternative: `os_info = "3"`; pick whichever compiles cleanly on Windows — `sys-info` may have MSVC quirks; `os_info` is pure-Rust and safer cross-platform, so **prefer `os_info`**.)
- Already present: `serde`, `serde_json`, `serde_yaml`, `log`, `tokio`, `dirs`.

---

## 3. Frontend work (`ui/talos/src/`)

### New file: `src/components/diagnostics/DiagnosticsModal.tsx`

Props:

```text
interface DiagnosticsModalProps {
  open: boolean;
  onClose: () => void;
  currentTab: ChatTab;           // to serialize messages
  model: string;                 // active model id
  mcpServers: McpServer[];       // to compute enabled list
}
```

- Follows the `.scrim` + `.palette` styling pattern from `SettingsModal.tsx`. No lucide, no `intl`. Uses the existing `Icon` component (`bug`, `info`, `file-text`, `settings`, `check`, `x`).
- Internal state: `isDownloading`, `isFilingBug`, `includeMemoryDir` (checkbox), `error`.
- List items: "Basic system info", "Current session messages", "Recent Goose logs", "Talos settings", "UI state snapshot", plus a checkbox for "Include memory folder (may contain personal notes)" — unchecked by default.
- Warning text: same sensitive-info warning and attach-hint as desktop.
- Two buttons: **Download** and **File Bug on GitHub**. Cancel closes.

Download flow:

1. Call `save` from `@tauri-apps/plugin-dialog` with `defaultPath: \`talos-diagnostics-${tabId}-${isoDate}.zip\`` and filter `[{ name: "Zip", extensions: ["zip"] }]`.
2. If user cancels (returns null), abort.
3. Serialize `currentTab.messages` (plus id/title) to JSON.
4. Invoke `write_diagnostics_zip` with the bundle request.
5. On success, show a toast "Diagnostics saved to …" and close.
6. On error, surface `error` in the modal body (don't close).

File-bug flow:

1. `invoke<SystemInfo>("get_system_info")`.
2. Merge in frontend-known `provider`, `model`, `enabled_mcp_servers`.
3. Build issue body (template below).
4. `URLSearchParams` with `template=bug_report.md`, `body`, `labels=bug,talos`.
5. `openUrl` from `@tauri-apps/plugin-opener` (NOT `window.open` — Tauri 2 webview disables `window.open` popups by default).
6. Close modal.

### GitHub issue body template (verbatim)

```text
**Describe the bug**

Before filing, please:
- Download the Talos diagnostics zip from the same dialog and attach it below.
- Check common issues: https://github.com/larmax82/goose/issues?q=is%3Aissue

A clear and concise description of what the bug is.

---

**To Reproduce**
Steps to reproduce the behavior:
1. Go to '...'
2. Click on '....'
3. Scroll down to '....'
4. See error

---

**Expected behavior**
A clear and concise description of what you expected to happen.

---

**Screenshots**
If applicable, add screenshots to help explain your problem.

---

**Environment**
- **App:** Talos ${info.appVersion} (Tauri ${info.tauriVersion})
- **OS & Arch:** ${info.os} ${info.osVersion} ${info.architecture}
- **Provider & Model:** ${providerModel}
- **MCP servers enabled:** ${mcpList}
- **Goose serve port:** ${info.gooseServePort ?? "n/a"}

---

**Additional context**
Add any other context about the problem here.

Please attach the diagnostics zip you just downloaded — it contains logs, the current session transcript, and settings which will speed up triage significantly.
```

Where `providerModel` and `mcpList` fall back to `[e.g. openrouter – claude-opus-4.7]` / `[e.g. filesystem, github]` when empty, mirroring desktop.

### New file: `src/services/diagnostics.ts`

Thin wrappers:

```text
export async function getSystemInfo(): Promise<SystemInfo> { ... }
export async function writeDiagnosticsZip(req: DiagnosticsRequest): Promise<WriteResult> { ... }
export async function promptSaveZip(defaultName: string): Promise<string | null> { ... }
export async function openIssueUrl(url: string): Promise<void> { ... }

// Helper — serialize messages without `streaming` flag etc.
export function serializeTranscript(tab: ChatTab): string { ... }
```

### Modified files

- `src/App.tsx`:
  - Add `diagnosticsOpen` state (like `settingsOpen`).
  - Render `<DiagnosticsModal open={diagnosticsOpen} onClose={…} currentTab={currentTab} model={model} mcpServers={mcpServers} />` beside `SettingsModal`.
  - Add a `setDiagnosticsOpen(true)` run-handler to the "Report a bug" entry in the `commands` array (currently L557, has no `run`).
  - Pass `onReportBug: () => setDiagnosticsOpen(true)` down through `composerProps`.

- `src/components/chat/Composer.tsx`:
  - Add `onReportBug: () => void` to `ComposerProps`.
  - Wire the existing button at L194: `onClick={onReportBug}`.

- `src/types.ts`: no change (ChatTab already has what we need).

- `src/components/Icon.tsx`: add `"download"` and `"github"` to `IconName` and the switch if you want those glyphs inside the modal. Otherwise reuse `bug` + `arrow-up` + `globe`.

### UX states

- **Idle:** description + list + checkbox + two buttons.
- **Downloading:** buttons disabled; Download shows spinner-ish text "Saving…" (reuse the ellipsis convention from SettingsModal "Saving\u2026").
- **Error:** `<div>` below buttons, red text var (`--color-danger`), does not dismiss modal; buttons re-enabled.
- **Success:** toast via `addToast`, modal closes.

---

## 4. Log capture — minimal mechanism

Current state: `goose_serve.rs` discards child stdio. Without changes, the ZIP has no log payload.

**Plan: tee to file under Tauri's app-log dir.**

- Path: `app_handle.path().app_log_dir()?` → on Windows that's `C:\Users\<u>\AppData\Local\com.talos.app\logs\goose-serve.log`, on macOS `~/Library/Logs/com.talos.app/`. On Tauri 2 you must call `std::fs::create_dir_all` since `app_log_dir` doesn't create it.
- On spawn:
  1. Create the dir.
  2. Rotate if current file > 5 MiB (simple one-level rename chain: `.log.2` dropped, `.log.1` → `.log.2`, `.log` → `.log.1`, new `.log` created).
  3. Open file with `OpenOptions::create(true).append(true)`.
  4. `try_clone()` for stderr handle.
  5. `command.stdout(Stdio::from(stdout_handle))`, `.stderr(Stdio::from(stderr_handle))`.
- Keep `kill_on_drop(true)` so the file handle is closed when the child dies.

**Race / file-locking considerations:**

- Windows holds exclusive locks on open files by default. Rotating a file that the child still has open will fail. Mitigation: **rotate only at spawn time, before the child starts.** Since `GOOSE_SERVE` is a `OnceCell` spawned once per app run, this is effectively a per-launch rotation — acceptable.
- The diagnostics command reads the log file while the child is still writing. On Windows, a `File::open` for reading succeeds because the child opened with `GENERIC_WRITE | FILE_SHARE_READ` (default for Rust stdlib `OpenOptions::append`). Double-check by testing: if reads fail with sharing violation, switch to `fs::copy` (which respects share modes differently) or read via `File::open(p)?.read_to_end(...)` with a short retry loop.
- Concurrent diagnostics requests: serialize by wrapping the write path in a `tokio::sync::Mutex<()>` inside `diagnostics.rs`. Not essential for v1 (user can't click Download twice in a row).
- Size bound: hard-cap each log entry added to the ZIP at 10 MiB tail (desktop uses `LOGS_TO_KEEP`; we use a byte cap because we have only one rotating file).

**Alternative considered — in-memory ring buffer:** more work (need a `Mutex<VecDeque<u8>>` and a forwarder task reading from piped child stdio), survives zero restarts, can't be inspected out-of-band. Rejected for v1. File-based is simpler and lets power users `tail -f` the log.

---

## 5. Dependencies

### Rust (`ui/talos/src-tauri/Cargo.toml`)

New:
- `zip = "2"`
- `chrono = { version = "0.4", features = ["serde"] }`
- `os_info = "3"` (preferred over `sys-info` for Windows toolchain stability)

Already present (re-use): `serde`, `serde_json`, `serde_yaml`, `log`, `tokio`, `dirs`, `tauri-plugin-dialog`, `tauri-plugin-opener`, `tauri-plugin-store`.

### Frontend (`ui/talos/package.json`)

No additions needed. `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-opener` are already installed.

### Capabilities (`ui/talos/src-tauri/capabilities/default.json`)

- Already have: `opener:default`, `dialog:allow-open`, `dialog:allow-save`, `store:default`.
- **Verify:** whether `opener:default` permits `open_url` for `https://github.com/...`. If Tauri 2 rejects it, add `opener:allow-open-url` with a URL scope `[{ "url": "https://github.com/**" }]`.

---

## 6. Phasing

**Phase 1 — Thinnest end-to-end slice (day 1)**
- Add `DiagnosticsModal` with list of placeholders and both buttons.
- Wire composer bug button → `App.tsx` → modal.
- Add Rust `write_diagnostics_zip` command that writes a ZIP containing only `system.json` (hardcoded system info) and a `session.json` from the frontend.
- Use `plugin-dialog::save` to pick the path.
- `File Bug` button opens a **static** URL via `plugin-opener::openUrl` (no system info interpolation yet).
- Ship-test: button click → modal → Download → pick path → ZIP appears on disk with 2 files; GitHub button opens browser at correct repo URL.

**Phase 2 — Real system info + issue body interpolation**
- Implement `get_system_info` Rust command with `os_info`.
- Wire frontend side (provider/model/mcp merge) and template interpolation.
- Add `system.txt` to ZIP.
- Add Talos `settings.json` + `state.json` copies to ZIP (with trivial PII pass — strip `composer` drafts, keep message bodies).

**Phase 3 — Log capture**
- Add `services/log_capture.rs`.
- Modify `goose_serve.rs` to tee into `goose-serve.log`.
- Include rotated logs in ZIP.
- Add `read_log_tail` command (not user-visible yet, helps QA).

**Phase 4 — Memory folder inclusion + polish**
- Checkbox in modal for `include_memory_dir`.
- Rust walker with 50 MiB total cap and `.git`/binary-file skip.
- Success toast wording, keyboard shortcuts (`Esc` closes, handled by scrim).
- Add diagnostics entry to `commands` array in `App.tsx` so it shows up in `⌘K` palette.

**Phase 5 (optional) — App-log plumbing**
- Add `tauri-plugin-log`, route `log::info!` macros to a second file under the same log dir, include it in ZIP.
- Add a "View logs" button in the modal that calls `read_log_tail` into a read-only pane.

Each phase is independently shippable and testable.

---

## 7. Risks & open questions

1. **ZIP filename format.** Proposal: `talos-diagnostics-{YYYYMMDD-HHMMSS}-{shortTabId}.zip` where `shortTabId = tab.id.slice(-6)`. Avoids collisions when a user files multiple reports. *Decision needed.*
2. **Transcript persistence.** Today, transcript lives only in `state.json` + React state. If the user clicks Report before the 500ms debounce fires, the on-disk `state.json` is stale. Mitigation: the *frontend* serializes the live `currentTab.messages` and passes it to Rust — so the ZIP always reflects what the user sees, regardless of debounce. We still copy `state.json` for other tabs' context.
3. **Include memoryDir contents?** Off by default. These are the user's personal notes — high PII risk. Gate behind a prominent checkbox with a warning.
4. **PII scrubbing scope.** What do we scrub from `state.json`? Proposal for v1: strip nothing, add the warning banner, trust the user. Later: provide an opt-in "redact file paths" toggle. *Decision needed.*
5. **Windows path handling.** `dirs::home_dir()` and `app_handle.path().app_log_dir()` both return `PathBuf` that works fine on Windows, but ZIP entries must use forward slashes. Remember to convert backslashes before `zip.start_file(name, ...)`. Unit-testable.
6. **macOS bundle vs. cargo-dev.** In dev (`tauri:dev`), `app_config_dir` differs from production bundle. Verify Settings and log paths are consistent across both.
7. **Hostname in SystemInfo.** Including hostname helps debugging (domain-joined machines, multi-user) but is PII. Proposal: include only if Goose is running in a "debug" or opt-in mode. Safer default: **exclude**. *Decision needed.*
8. **`sys-info` vs. `os_info`.** `sys-info` has C FFI and has broken Windows MSVC builds before. `os_info` is pure Rust. Strong default: `os_info`.
9. **Opener URL length.** GitHub's issue body accepts large `body` params but browsers cap URL length around 8 KiB. MCP lists are short and the template is fixed — unlikely to exceed — but worth a soft clamp in the frontend (e.g., truncate `enabled_mcp_servers` display at 20 entries).
10. **ACP session resumption.** Diagnostics captures frontend transcript only. If you later add `client.loadSession`, the Rust side could query Goose for its session state — out of scope for this feature.

---

## 8. Test plan

### Phase 1
- Manual: Click bug icon in composer → modal opens. Click Download → save dialog opens with correct default name → ZIP saved; unzip and confirm 2 files. Click File Bug → browser opens correct repo issue page.
- Negative: cancel save dialog → modal stays open, no error toast. Cancel with `Esc` → modal closes.

### Phase 2
- Manual: modal shows correct OS / version / arch strings on both Windows and macOS dev builds. Body parameter in the GitHub URL contains the right `provider/model` strings after toggling the model picker.
- Unit (Rust): `SystemInfo::collect()` returns non-empty `app_version` and `architecture` on every supported platform.
- Unit (TS): `serializeTranscript` round-trips through `JSON.parse` without `streaming`/`gooseSessionId` leaking.

### Phase 3
- Manual: start Talos, send a message (causing goose-serve activity), click Download → open the ZIP → `logs/goose-serve.log` exists and is non-empty. Kill Talos, relaunch, confirm `.log.1` shows up.
- Manual (Windows-specific): open the ZIP **while** a chat is streaming. Should succeed without "file in use" error.
- Unit (Rust): rotation logic — given a temp dir with `.log` > 5 MiB, after `rotate()` the old file is at `.log.1`, new file is empty.

### Phase 4
- Manual: toggle "Include memory folder" on with a large (~100 MiB) memoryDir → ZIP size capped at ~50 MiB; trailing files omitted with a note file in the ZIP explaining the cap.
- Manual: toggle off → no `memory/` prefix in ZIP.

### Phase 5 (if built)
- Manual: "View logs" button shows last N KB of log; resizing modal respects scroll.

### Cross-cutting
- Command-palette entry "Report a bug" fires the same modal.
- Filing a bug after a long chat produces a URL under browser's URL-length cap.
- On a fresh install with empty settings, ZIP still produces (settings.json + state.json are just absent — log this in README.txt inside the ZIP).

---

## Critical Files for Implementation

- `ui/talos/src-tauri/src/services/goose_serve.rs`
- `ui/talos/src-tauri/src/lib.rs`
- `ui/talos/src-tauri/Cargo.toml`
- `ui/talos/src/App.tsx`
- `ui/talos/src/components/chat/Composer.tsx`
