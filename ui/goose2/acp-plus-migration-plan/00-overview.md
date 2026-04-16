# ACP-Plus Migration Plan: Overview

## Goal

Move all ACP protocol handling from the Rust Tauri backend into the TypeScript/WebView layer, so the frontend communicates directly with `goose serve` over WebSocket. The Rust layer shrinks to a thin native shell responsible only for:

1. Spawning and managing the `goose serve` child process
2. Providing the server URL to the frontend
3. Window management / OS integration

Long-term, config, personas, skills, projects, git, doctor, and all other native operations will also move behind `goose serve` ACP extension methods — eliminating the Rust middleware entirely.

## Current Architecture

```
Frontend (TS)
  → invoke("acp_send_message")          [Tauri IPC]
    → GooseAcpManager                   [Rust singleton, dedicated thread]
      → ClientSideConnection            [Rust ACP client over WebSocket]
        → goose serve ws://127.0.0.1:<port>/acp   [child process]
      ← SessionNotification             [ACP callback in Rust]
    ← TauriMessageWriter                [emits Tauri events]
  ← listen("acp:text", ...)             [Tauri event bus]
    → Zustand store updates
```

## Target Architecture (Phase A)

```
Frontend (TS)
  → GooseClient (WebSocket)
    → goose serve ws://127.0.0.1:<port>/acp   [child process]
  ← Client callbacks → direct Zustand store updates

Tauri Rust shell:
  - Spawn goose serve, expose URL
  - Config/personas/skills/projects/git/doctor (temporary — Phase B removes these)
  - Window management
```

## Target Architecture (Phase B — Long-Term)

```
Frontend (TS)
  → GooseClient (WebSocket)
    → goose serve ws://127.0.0.1:<port>/acp
  ← Client callbacks → direct Zustand store updates

Tauri Rust shell (~200 lines):
  - Spawn goose serve, expose URL
  - Window management
```

## Steps

| Step | File | Summary |
|------|------|---------|
| 01 | `01-expose-goose-serve-url.md` | Add Tauri command to expose the `goose serve` WebSocket URL to the frontend |
| 02 | `02-add-acp-npm-dependencies.md` | Add `@aaif/goose-acp` and `@agentclientprotocol/sdk` to goose2 |
| 03 | `03-create-ts-acp-connection.md` | Create the singleton TypeScript ACP connection manager (WebSocket transport), reconnection logic, and feature flag |
| 04 | `04-create-ts-notification-handler.md` | Port the Rust `SessionEventDispatcher` to TypeScript |
| 05 | `05-create-ts-session-manager.md` | Port session state management and ACP operations to TypeScript |
| 06 | `06-port-session-search.md` | Port session content search from Rust to TypeScript |
| 07 | `07-rewire-shared-api-acp.md` | Replace `invoke()` wrappers in `src/shared/api/acp.ts` with direct TS ACP calls |
| 08 | `08-rewire-hooks.md` | Remove `useAcpStream`, update `useChat`, `useAppStartup`, `AppShell` |
| 09 | `09-delete-rust-acp-code.md` | Delete the Rust ACP middleware and unused dependencies |
| 10 | `10-phase-b-future-native-migration.md` | Plan for moving config/personas/skills/projects/git/doctor to `goose serve` |

## Ordering & Dependencies

```
01 ──┐
     ├──→ 03 ──→ 04 ──→ 05 ──→ 07 ──→ 08 ──→ 09
02 ──┘                    │
                          └──→ 06 ──→ 07
```

- Steps 01 and 02 are independent and can be done in parallel.
- Steps 03–06 build on each other, though 06 can proceed in parallel with 04/05.
- Step 07 wires everything together.
- Step 08 removes the old Tauri event listeners.
- Step 09 is cleanup — only after everything works.
- Step 10 is the Phase B roadmap.

## Key Decisions

1. **WebSocket transport.** `goose serve` exposes a WebSocket endpoint at `/acp`. Each WS text frame is a single JSON-RPC message. This is the same transport the Rust layer already uses — we are moving the WebSocket client from Rust to TypeScript. WebSocket provides true bidirectional streaming with lower overhead than HTTP+SSE.

2. **Direct store updates over event bus.** The notification handler calls Zustand store methods directly instead of emitting Tauri events. This eliminates a layer of indirection and the `useAcpStream` hook.

3. **Reuse `@aaif/goose-acp`.** Already used by `ui/desktop` (Electron) and `ui/text` (Ink TUI). Provides `GooseClient`, generated types, and Zod validators. A `createWebSocketStream` helper will be added (either in `@aaif/goose-acp` or locally in goose2) since the package currently only ships `createHttpStream`.

4. **Auto-approve permissions.** Same as the current Rust implementation — accept the first option on all `request_permission` callbacks.

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Tauri CSP blocks localhost WebSocket | CSP is already `null` (disabled) in `tauri.conf.json` |
| `goose serve` not ready when frontend initializes | Rust still does a readiness check; the URL command only resolves after the server is confirmed ready |
| WebSocket disconnection / reconnection | Implement reconnection logic in the connection manager; `GooseClient.closed` signals when the connection drops |
| Replay timing (notifications arriving after `loadSession` resolves) | Port the drain/stabilization logic from Rust, or rely on the `replay_complete` signal from the backend |
| Session state consistency during migration | Feature flag (`useDirectAcp` in `acpFeatureFlag.ts`) routes between old Tauri IPC and new WebSocket path. Default off, flip per-user to test, flip default to on after validation, remove in Step 09 |
