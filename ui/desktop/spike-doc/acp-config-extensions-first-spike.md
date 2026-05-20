# ACP First Spike: Configured Extensions List

## Purpose

Use configured extensions as the first `ui/desktop` REST-to-ACP migration slice.

This spike is intentionally smaller than chat or sessions. It proves that desktop can talk directly to `/acp` through `goosed` during migration, without depending on ACP session ID behavior or chat streaming semantics.

## Scope

Migrate the read-only configured extensions list behind a feature flag.

```text
Current REST:
  GET /config/extensions

Target ACP:
  _goose/config/extensions
```

Do not migrate add/update/delete in this first slice. Those can remain REST until the read path and ACP client plumbing are proven.

## Why This Slice

- No session ID dependency.
- No chat streaming or prompt lifecycle.
- No tool approval.
- No message-shape conversion.
- Exercises direct renderer WebSocket to `/acp`.
- Exercises ACP initialization and custom `_goose/*` request dispatch.
- Easy to compare REST and ACP responses.

## Relevant Existing Code

Desktop REST read path:

- `ui/desktop/src/components/ConfigContext.tsx`
  - imports `getExtensions as apiGetExtensions`
  - `refreshExtensions()` calls `apiGetExtensions()`
  - initial provider load also calls `apiGetExtensions()`

Desktop UI consumer:

- `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx`
  - consumes `extensionsList` from `useConfig()`
  - calls `getExtensions(true)` to refresh

REST backend:

- `crates/goose-server/src/routes/config_management.rs`
  - `GET /config/extensions`
  - `POST /config/extensions`
  - `DELETE /config/extensions/{name}`

ACP backend:

- `crates/goose/src/acp/server/extensions.rs`
  - `_goose/config/extensions`
  - `_goose/config/extensions/add`
  - `_goose/config/extensions/remove`
  - `_goose/config/extensions/toggle`

ACP client reference:

- `ui/goose2/src/shared/api/createWebSocketStream.ts`
- `ui/goose2/src/shared/api/acpConnection.ts`
- `ui/goose2/src/shared/api/acpApi.ts`

Treat `ui/goose2` as a reference only. Do not share runtime code with it because `ui/goose2` is expected to move out of this repo.

## Current Gap Found

REST `GET /config/extensions` filters hidden extensions:

```rust
goose::config::get_all_extensions()
    .into_iter()
    .filter(|ext| !goose::agents::extension_manager::is_hidden_extension(&ext.config.name()))
```

ACP `_goose/config/extensions` currently calls `crate::config::extensions::get_all_extensions()` and does not apply the same hidden-extension filter.

Recommendation: fix ACP to match REST before enabling this feature flag. The goal is to prove the migration path without introducing UI-visible behavior differences.

## Backend Plan

## Step-By-Step Review Plan

Work in small reviewable slices. After each step, stop and review before moving to the next one.

### Step 1: Inspect router/auth shape

Goal: confirm the exact `goosed` router composition and ACP transport shape before editing.

Review points:

- where REST `X-Secret-Key` middleware is applied
- whether ACP can be mounted as a separate branch
- whether `goose::acp::transport::create_router` causes route collisions
- where ACP token auth should live
- whether Cargo dependencies already allow `goose-server` to call ACP router code

Expected outcome: a precise patch plan for backend mounting.

### Step 2: Add ACP-only router helper

Goal: avoid route collisions by exposing only `/acp` routes for embedding in `goosed`.

Likely file:

- `crates/goose/src/acp/transport/mod.rs`

Reason: existing `create_router(...)` includes `/health`, `/status`, and MCP app proxy routes. `goosed` already owns some of those routes.

Expected outcome: a helper such as `create_acp_router(...)` or equivalent that only mounts:

```text
/acp POST
/acp GET
/acp DELETE
```

### Step 3: Mount `/acp` in `goosed`

Goal: serve REST and ACP from the same `goosed` process during migration.

Likely files:

- `crates/goose-server/src/commands/agent.rs`
- maybe `crates/goose-server/src/routes/mod.rs`

Expected shape:

```text
goosed
  REST routes protected by X-Secret-Key
  /acp protected by ACP token auth
```

### Step 4: Expose ACP URL/token to renderer

Goal: let renderer connect directly to `/acp` over WebSocket.

Likely files:

- `ui/desktop/src/main.ts`
- `ui/desktop/src/preload.ts`
- related Electron type declarations if present

Expected renderer-facing value:

```text
ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
```

### Step 5: Add minimal desktop ACP client

Goal: create enough client plumbing to initialize ACP and call one custom method.

Suggested files:

```text
ui/desktop/src/acp/createWebSocketStream.ts
ui/desktop/src/acp/acpConnection.ts
ui/desktop/src/acp/acpApi.ts
```

Reference, not shared dependency:

- `ui/goose2/src/shared/api/createWebSocketStream.ts`
- `ui/goose2/src/shared/api/acpConnection.ts`
- `ui/goose2/src/shared/api/acpApi.ts`

### Step 6: Migrate configured extensions read path behind a flag

Goal: call `_goose/config/extensions` for read-only extension listing when enabled.

Primary integration file:

- `ui/desktop/src/components/ConfigContext.tsx`

Keep writes on REST in this slice:

- add extension
- remove extension
- toggle extension
- bundled extension sync/prune

### Step 7: Validate parity

Goal: prove ACP and REST return equivalent visible extension data.

Validation checklist is below.

## Backend Details

### Mount `/acp` in `goosed`

Add the ACP Axum router to `goosed agent`.

Likely files:

- `crates/goose-server/src/commands/agent.rs`
- `crates/goose-server/src/routes/mod.rs`

Use existing ACP pieces:

```rust
goose::acp::server_factory::{AcpServer, AcpServerFactoryConfig}
goose::acp::transport::create_router
```

Keep REST and ACP as separate route branches.

Watch for route collisions:

- `/status`
- `/mcp-app-proxy`
- `/mcp-app-guest`

### Add ACP token auth

The renderer should connect through direct WebSocket:

```text
ws(s)://127.0.0.1:<port>/acp?token=<acp-token>
```

Do not put `/acp` behind the REST `X-Secret-Key` middleware because browser WebSocket cannot set arbitrary headers.

Guardrails:

- Use a random ACP token for the `goosed` process.
- Prefer an ACP-specific token over reusing the raw REST `X-Secret-Key`.
- Never log full `/acp?token=...` URLs.
- Accept ACP token auth only for the local desktop backend.
- Keep REST `X-Secret-Key` for REST routes during migration.

### Fix `_goose/config/extensions` parity

Update ACP `_goose/config/extensions` to match REST behavior:

- filter hidden extensions
- preserve warnings
- preserve the response shape needed by desktop

The ACP response currently injects a `config_key` field. Verify the desktop shape expected by `ExtensionEntry` and normalize on the client if needed.

## Desktop Plan

### Add ACP client files

Suggested files:

```text
ui/desktop/src/acp/createWebSocketStream.ts
ui/desktop/src/acp/acpConnection.ts
ui/desktop/src/acp/acpApi.ts
```

Base these on `ui/goose2`, but adapt to Electron.

Differences from `ui/goose2`:

- URL lookup should use Electron, not Tauri.
- Build ACP URL from the existing `goosed` host.
- Append the ACP token.
- Initialize ACP once and reconnect on close.

Example URL shape:

```ts
const baseUrl = await window.electron.getGoosedHostPort();
const acpUrl = baseUrl.replace(/^http/, 'ws') + '/acp?token=' + encodeURIComponent(token);
```

If `goosed` is running HTTPS, this becomes `wss://.../acp?...`.

### Add ACP extensions API wrapper

Add a wrapper such as:

```ts
getConfigExtensionsViaAcp(): Promise<ExtensionResponse>
```

It should call:

```text
_goose/config/extensions
```

Normalize the response to the current desktop `ExtensionResponse` shape:

```ts
type ExtensionResponse = {
  extensions: ExtensionEntry[];
  warnings: string[];
};
```

### Add a feature flag

Suggested name:

```text
acpConfigExtensions
```

The exact flag mechanism should follow existing desktop feature-flag conventions if present. If there is no suitable framework, use a local env/config gate for the spike.

### Route read path through ACP only when enabled

Primary integration file:

- `ui/desktop/src/components/ConfigContext.tsx`

Candidate call sites:

- `refreshExtensions()`
- initial load effect that fetches extensions after bundled sync

Keep these mutation paths on REST for the first spike:

- `addExtension`
- `removeExtension`
- `toggleExtension`
- bundled extension sync/prune

After REST mutations complete, refresh can use ACP when the flag is enabled.

### Add fallback behavior

For the spike, if ACP connection or `_goose/config/extensions` fails, log the error and fall back to REST.

This fallback is only for the spike. Once a feature area is fully migrated, the matching REST endpoint should be removed.

## Validation Checklist

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

## Removal Rule

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

## Done Criteria

- `goosed` exposes token-authenticated `/acp`.
- `ui/desktop` can open a direct renderer WebSocket to `/acp`.
- ACP initialize succeeds.
- `_goose/config/extensions` returns REST-equivalent visible extension data.
- Feature flag can switch extension list reads between REST and ACP.
- Existing extension settings UI behaves the same under the ACP read path.
