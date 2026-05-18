# Desktop ACP Migration Spike

## Goal

`ui/desktop` currently talks to `goosed agent` through REST/OpenAPI. We want to migrate the desktop app to talk to Goose through ACP, while keeping the migration gradual and avoiding a large bundle-size increase.

The preferred migration direction is:

- Keep bundling only `goosed` during migration to avoid shipping both large binaries.
- Add ACP serving at `/acp` inside `goosed agent` as a temporary bridge.
- Let `ui/desktop` talk directly to `/acp` for migrated surfaces.
- Keep existing REST routes only for unmigrated areas.
- Once all REST behavior has moved to ACP, stop bundling/running `goosed` and switch desktop to the existing `goose serve` ACP server.

## Current Architecture

### Desktop backend

`goosed` is an Axum HTTP server.

Relevant files:

- `crates/goose-server/src/main.rs`
- `crates/goose-server/src/commands/agent.rs`
- `crates/goose-server/src/routes/mod.rs`
- `ui/desktop/src/goosed.ts`

`ui/desktop/src/goosed.ts` finds and spawns the bundled `goosed` binary:

```ts
const spawnCommand = goosedPath;
const spawnArgs = ['agent'];
```

The renderer configures the generated OpenAPI client against the `goosed` URL in `ui/desktop/src/renderer.tsx`.

### Desktop REST usage

`ui/desktop` imports generated API methods from `ui/desktop/src/api`.

Important chat/session paths today:

- `ui/desktop/src/sessions.ts`
  - `startAgent` creates sessions.
- `ui/desktop/src/hooks/useChatStream.ts`
  - `resumeAgent`
  - `sessionReply`
  - `sessionCancel`
  - `getSession`
  - `updateFromSession`
- `ui/desktop/src/hooks/useSessionEvents.ts`
  - opens `GET /sessions/{id}/events` as SSE.

The current REST streaming model is Goose-specific:

- `POST /sessions/{id}/reply`
- `GET /sessions/{id}/events`
- event types such as `Message`, `Finish`, `Error`, `Notification`, `ActiveRequests`
- request routing through `request_id` / `chat_request_id`

## Existing ACP Implementation

Goose already has ACP support.

Relevant files:

- ACP agent implementation: `crates/goose/src/acp/server.rs`
- ACP transport: `crates/goose/src/acp/transport/mod.rs`
- ACP custom methods: `crates/goose/src/acp/server/custom_dispatch.rs`
- Custom request types: `crates/goose-sdk/src/custom_requests.rs`
- CLI entry points: `crates/goose-cli/src/cli.rs`

There are two existing ACP modes:

### `goose acp`

Runs ACP over stdio:

```rust
Some(Command::Acp { builtins }) => goose::acp::server::run(builtins).await
```

This is standard ACP, but in Electron it would require the main process to own stdio and bridge to the renderer.

### `goose serve`

Runs ACP over HTTP/SSE/WebSocket:

```rust
Some(Command::Serve { host, port, builtins }) => handle_serve_command(host, port, builtins).await
```

The transport router registers:

```rust
/health
/status
/acp  POST
/acp  GET
/acp  DELETE
```

`GET /acp` upgrades to WebSocket when requested. Otherwise it behaves as SSE. `POST /acp` accepts JSON-RPC messages.

## Bundle Size Finding

Local release binary sizes:

```text
target/release/goose   230M
target/release/goosed  218M
```

Bundling both would add roughly:

```text
goosed + goose = 448M uncompressed
```

`ui/desktop/src/bin` currently contains both binaries locally:

```text
ui/desktop/src/bin/goose   230M
ui/desktop/src/bin/goosed  218M
```

This is too large as a long-term plan.

## Recommended Backend Strategy

Do not bundle both `goose` and `goosed` for production migration. The temporary bridge is to mount `/acp` into `goosed`; the final backend is `goose serve`.

Migration backend:

1. Add ACP serving to `goosed agent`.
2. Keep REST routes available on the same process during migration.
3. Mount `/acp` in the existing `goosed` Axum app.
4. Gradually migrate desktop feature areas from REST to ACP.
5. Remove each relevant REST endpoint once its feature area is migrated.

Migration shape:

```text
ui/desktop renderer
  -> http(s)://127.0.0.1:<port>/acp
       goosed process
         existing REST routes temporarily
         ACP /acp
```

This keeps the backend bundle around current `goosed` size plus a small ACP wiring delta, instead of adding a second 230M binary.

Final backend:

```text
ui/desktop renderer
  -> ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
       goose serve
         standard ACP methods
         _goose/* custom ACP methods
```

In the final state `goosed` is not bundled or spawned by the desktop app.

Before deleting the `goosed` bridge, verify that the final `goose serve` path has the same
desktop-specific ACP behavior that the bridge depended on during migration. In particular:

- config and data directories match the desktop app's expected Goose paths
- builtin extension setup matches what desktop needs
- ACP initialization uses the correct desktop platform identity
- any state initialized today by `goosed` startup is either no longer needed or is initialized by
  the final desktop `goose serve` launch path

## Migration Bridge vs Final Server

Mounting `/acp` in `goosed` should be treated as a bridge, not the destination.

```text
During migration:
  ui/desktop
    -> REST for unmigrated features
    -> /acp for migrated features

  bundled backend:
    goosed
      REST routes
      temporary /acp route
```

```text
After migration:
  ui/desktop
    -> /acp only

  bundled backend:
    goose serve
      standard ACP
      _goose/* custom ACP methods
```

The migration rule is:

```text
When a feature moves to /acp:
  remove its corresponding REST endpoint from goosed
```

The final cutover from `goosed agent` to `goose serve` is blocked until no desktop runtime feature depends on REST/OpenAPI.

Expected effort:

- Mounting `/acp` in `goosed`: relatively easy because both servers are Axum and the ACP router already exists.
- Removing `goosed` later: medium effort, because every REST-only desktop capability must first be moved into standard ACP or `_goose/*` custom ACP methods.

## Alternative Considered: Move REST Into `goose serve`

It is possible to make `goose serve` also mount `goose-server` REST routes. That would make `goose-cli` depend on `goose-server`.

Tradeoffs:

- Simpler single `goose` binary packaging.
- But the general-purpose CLI binary gets desktop REST/OpenAPI/tunnel/gateway/server dependencies.
- Likely increases `goose` binary size.
- Blurs the CLI and desktop backend boundary.

The cleaner migration bridge is the reverse: add ACP to `goosed` temporarily. The final target remains `goose serve`, not `goosed`.

## Streaming Differences

Adding `/acp` to `goosed` does not make the current desktop streaming code work unchanged.

### Current REST streaming

The current desktop chat stream expects:

- `GET /sessions/{id}/events`
- Goose-specific `MessageEvent` objects
- `ActiveRequests`
- `request_id` / `chat_request_id`
- `Message`, `Finish`, `Error`, `Notification`

### ACP streaming

ACP streaming is protocol-level:

- Client sends JSON-RPC `session/prompt`.
- Server emits `session/update` notifications.
- Updates include:
  - agent message chunks
  - user message chunks
  - tool calls
  - tool call updates
  - usage updates
  - session info updates
  - config option updates

Tool approval also changes:

- REST uses `/action-required/tool-confirmation`.
- ACP sends `RequestPermissionRequest`.
- The client must respond on the ACP connection.

## Recommended Client Strategy

Use WebSocket ACP directly from the renderer.

Preferred shape:

```text
renderer -> wss://127.0.0.1:<port>/acp
send initialize
send session/new
send session/load
send session/prompt
receive session/update notifications continuously
respond to permission requests
```

WebSocket is preferable to HTTP POST + SSE because it gives one bidirectional connection for requests, responses, notifications, and permission responses. Do not add an Electron-main IPC transport layer for normal ACP chat traffic.

## Existing Reference: `ui/goose2`

`ui/goose2` already has a client pattern that can be reused or adapted.

Relevant files:

- `ui/goose2/src/shared/api/createWebSocketStream.ts`
- `ui/goose2/src/shared/api/acpConnection.ts`
- `ui/goose2/src/shared/api/acpApi.ts`
- `ui/goose2/src-tauri/src/services/acp/goose_serve.rs`

`ui/goose2`:

- gets a `/acp` WebSocket URL from Tauri
- creates a WebSocket stream
- creates a `GooseClient`
- initializes ACP with client capabilities
- routes `sessionUpdate` notifications through a handler
- exposes APIs such as:
  - `listSessions`
  - `newSession`
  - `loadSession`
  - `prompt`
  - `cancelSession`
  - `setProvider`
  - `setModel`
  - `_goose/*` custom methods

`ui/desktop` can use the same pattern, replacing Tauri URL lookup with Electron URL lookup.
Because `ui/goose2` is expected to move out of this repo in the future, desktop should not share runtime code with it. Treat `ui/goose2` as a reference implementation and copy/adapt the small ACP client pieces into `ui/desktop`.

Example URL derivation:

```ts
const baseUrl = await window.electron.getGoosedHostPort();
const acpUrl = baseUrl.replace(/^http/, 'ws') + '/acp';
```

If `goosed` is running HTTPS, this becomes `wss://.../acp`.

## ACP Auth Decision

Current `goosed` REST uses `X-Secret-Key`.

Browser WebSocket does not support arbitrary request headers. If `/acp` is mounted behind the same auth middleware, direct renderer WebSocket may fail.

Chosen direction: `/acp` should have ACP-compatible token auth.

During migration:

```text
REST routes:
  X-Secret-Key header

ACP route:
  ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
```

After REST is removed:

```text
ACP only:
  ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
```

This preserves a security boundary for the long-term desktop API while still allowing direct renderer WebSocket connections.

Guardrails:

- Use a random ACP token for the `goosed` process.
- Prefer an ACP-specific token over reusing the raw REST `X-Secret-Key`.
- Never log full `/acp?token=...` URLs.
- Keep accepting `X-Secret-Key` only for REST during migration.
- Accept ACP token auth only for the local desktop backend.
- Keep REST and ACP as separate route branches so auth policy and endpoint removal stay clear.

Alternatives considered:

1. Mount `/acp` outside `X-Secret-Key` auth, relying on localhost binding.
2. Allow auth through a query parameter for `/acp`, for example `/acp?token=...`.
3. Open the WebSocket from Electron main, where headers are easier, and bridge to renderer via IPC.
4. Use HTTP/SSE transport where headers are possible, though this is less ergonomic for ACP permission/request-response flow.

Option 2 is the preferred approach. Option 1 matches current `ui/goose2` behavior, but it is weaker as the final desktop backend shape.

## Migration Routing

Keep REST and ACP side-by-side only for feature areas that have not moved yet.

Example shape:

```ts
const backend = {
  sessions: flags.acpSessions ? acpSessions : restSessions,
  chat: flags.acpChat ? acpChat : restChat,
  providers: flags.acpProviders ? acpProviders : restProviders,
};
```

Rules:

- `goosed` remains default for unmigrated surfaces.
- ACP is opt-in per feature area.
- New functionality should prefer ACP unless blocked.
- Once a feature area is migrated to `/acp`, remove the corresponding REST endpoint from `goosed` rather than keeping a permanent fallback.
- Any missing behavior discovered during migration should be added to ACP before removing the REST endpoint.
- Final milestone is no runtime dependency on generated REST APIs.

## Migration Order

### 1. Mount `/acp` in `goosed`

Add ACP Axum router to the `goosed agent` app.

Likely places:

- `crates/goose-server/src/commands/agent.rs`
- `crates/goose-server/src/routes/mod.rs`

Use:

```rust
goose::acp::server_factory::{AcpServer, AcpServerFactoryConfig}
goose::acp::transport::create_router
```

Need to decide:

- route merge order
- whether ACP MCP app proxy should be included once or deduplicated against existing goosed MCP app proxy routes

Auth model: use token-authenticated `/acp` for direct renderer WebSocket, separate from REST `X-Secret-Key`.

### 2. Add desktop ACP client

Add something like:

```text
ui/desktop/src/acp/createWebSocketStream.ts
ui/desktop/src/acp/acpConnection.ts
ui/desktop/src/acp/acpApi.ts
```

This can be based on `ui/goose2`.

### 3. Migrate session list/create/load

Map:

```text
REST /agent/start       -> ACP session/new
REST /agent/resume      -> ACP session/load
REST /sessions          -> ACP session/list
REST /sessions/{id}     -> ACP session/load plus replay/session metadata
```

This proves:

- session IDs
- history replay
- session metadata
- current model/provider/mode state

### 4. Migrate chat streaming

Map:

```text
REST /sessions/{id}/reply
REST /sessions/{id}/events
  -> ACP session/prompt + session/update notifications
```

Prefer moving the chat state toward ACP-native events and data structures rather than preserving the old goosed `MessageEvent` model. A temporary adapter into existing desktop `Message` and `TokenState` shapes is acceptable only as a short bridge if it substantially lowers rollout risk.

### 5. Migrate tool approval and tool display

Map:

```text
REST /action-required/tool-confirmation
  -> ACP RequestPermissionRequest response
```

Tool display should move toward ACP-native `tool_call` / `tool_call_update` state. Any old desktop message-shape adapter should be treated as temporary migration glue.

### 6. Migrate provider/model/mode

Use ACP session config options:

```text
setSessionConfigOption({ configId: "provider" })
setSessionConfigOption({ configId: "model" })
setSessionConfigOption({ configId: "mode" })
```

Use existing Goose custom methods for provider inventory and setup:

```text
_goose/providers/list
_goose/providers/config/read
_goose/providers/config/save
_goose/providers/config/status
_goose/providers/custom/*
_goose/providers/catalog/*
```

### 7. Migrate extensions/tools/resources

Existing custom ACP methods cover:

```text
_goose/extensions/add
_goose/extensions/remove
_goose/config/extensions
_goose/config/extensions/add
_goose/config/extensions/remove
_goose/config/extensions/toggle
_goose/session/extensions
_goose/tools
_goose/tool/call
_goose/resource/read
_goose/working_dir/update
```

### 8. Migrate settings and secondary surfaces

Existing ACP custom methods cover many settings/product surfaces:

```text
_goose/preferences/*
_goose/defaults/*
_goose/onboarding/import/*
_goose/sources/*
_goose/dictation/*
```

## First Spike Plan: Configured Extensions List

Before the ACP session-list PR lands, use configured extensions as the first migration slice. This avoids session ID semantics and chat streaming complexity while still proving the core `/acp` path.

### Scope

Migrate the read-only configured extensions list behind a feature flag.

```text
Current REST:
  GET /config/extensions

Target ACP:
  _goose/config/extensions
```

Do not migrate add/update/delete in this first slice. Those can remain REST until the read path and ACP client plumbing are proven.

### Why This Is A Good First Slice

- No session ID dependency.
- No chat streaming or prompt lifecycle.
- No tool approval.
- No message-shape conversion.
- Exercises direct renderer WebSocket to `/acp`.
- Exercises ACP initialization and custom `_goose/*` request dispatch.
- Easy to compare REST and ACP responses.

### Current Gap Found

REST `GET /config/extensions` filters hidden extensions:

```rust
goose::config::get_all_extensions()
    .into_iter()
    .filter(|ext| !goose::agents::extension_manager::is_hidden_extension(&ext.config.name()))
```

ACP `_goose/config/extensions` currently calls `crate::config::extensions::get_all_extensions()` and does not apply the same hidden-extension filter.

For this spike, either:

1. Fix ACP to match REST before enabling the feature flag, or
2. Allow the mismatch only in local spike mode and track it as the first ACP gap.

Recommendation: fix ACP to match REST. The goal is to prove migration, not introduce UI-visible behavior differences.

### Backend Steps

1. Mount `/acp` in `goosed agent`.
   - Add or merge the ACP Axum router into `crates/goose-server`.
   - Keep REST and ACP as separate route branches.
   - Avoid collisions with existing `/status`, `/mcp-app-proxy`, and `/mcp-app-guest` routes.

2. Add ACP token auth for `/acp`.
   - Direct renderer WebSocket should connect with:
     ```text
     ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
     ```
   - Do not put `/acp` behind REST `X-Secret-Key` middleware.
   - Do not log full token-bearing URLs.

3. Fix `_goose/config/extensions` parity.
   - Apply the same hidden-extension filtering as REST.
   - Preserve `warnings`.
   - Preserve enough shape for existing desktop extension rendering.

### Desktop Client Steps

1. Add ACP client files under `ui/desktop/src/acp/`.
   Suggested files:
   ```text
   ui/desktop/src/acp/createWebSocketStream.ts
   ui/desktop/src/acp/acpConnection.ts
   ui/desktop/src/acp/acpApi.ts
   ```

2. Base these on `ui/goose2`, but copy/adapt rather than share.
   - Replace Tauri URL lookup with Electron lookup.
   - Build ACP URL from the existing goosed host.
   - Append the ACP token.
   - Initialize ACP once and reconnect on close.

3. Add an ACP extensions API wrapper:
   ```ts
   getConfigExtensionsViaAcp(): Promise<ExtensionResponse>
   ```
   It should call:
   ```text
   _goose/config/extensions
   ```
   and normalize the response to the existing desktop `ExtensionResponse` shape.

4. Add a feature flag.
   Suggested name:
   ```text
   acpConfigExtensions
   ```

5. Route only the read path through ACP when the flag is enabled.
   Primary integration point:
   - `ui/desktop/src/components/ConfigContext.tsx`

   Keep mutation paths on REST for this first spike:
   - `addExtension`
   - `removeExtension`
   - `toggleExtension`
   - bundled extension sync/prune

### Validation

Compare REST and ACP for the same local config:

- same visible extension count
- same names
- same `enabled` values
- same extension types (`builtin`, `stdio`, `streamable_http`, etc.)
- same warnings
- hidden extensions do not appear
- settings/extensions UI renders correctly
- add/update/delete still work through REST and refresh the list
- failed ACP connection falls back to REST while the spike flag is enabled

### Removal Rule

Do not remove `GET /config/extensions` after this first read-only spike if writes still use REST sync logic that depends on the endpoint.

Remove the REST extension endpoints only after the full extension config surface has migrated:

```text
GET    /config/extensions
POST   /config/extensions
DELETE /config/extensions/{name}
```

Corresponding ACP methods:

```text
_goose/config/extensions
_goose/config/extensions/add
_goose/config/extensions/remove
_goose/config/extensions/toggle
```

## Known Gaps To Investigate

Likely REST-only or partially covered areas:

- recipe encode/decode/scan/schedule/create-from-session
- schedules
- local inference model management/downloads
- tunnel/gateway
- diagnostics/system info
- telemetry
- session sharing
- app export/import/list app flows
- MCP UI proxy details
- current `ActiveRequests` reattach semantics

During the transition these can remain on REST fallback. The final target is to expose each required capability through Goose custom ACP methods under `_goose/...` unless it maps cleanly to standard ACP.

## Recommended End State

Short term:

```text
goosed exposes REST + temporary /acp
desktop uses REST by default, ACP by feature flag
```

Migration:

```text
desktop moves one feature area at a time to ACP
missing backend behavior is added as standard ACP use or _goose custom methods
matching goosed REST endpoints are removed as each feature migrates
```

End state:

```text
desktop talks to /acp directly
goose serve is the single bundled desktop backend
goosed is no longer bundled or spawned
REST/OpenAPI is removed from desktop runtime behavior
```

## Open Decisions

No major architecture decisions remain from this spike. Implementation details still need validation around route merge order, MCP app proxy deduplication, and exact token plumbing.
