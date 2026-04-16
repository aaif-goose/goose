# Step 01: Expose the `goose serve` URL to the Frontend

## Objective

Add a Tauri command that returns the WebSocket URL of the running `goose serve` process so the frontend can connect directly via WebSocket.

## Why

The Rust layer currently connects to `goose serve` over WebSocket internally and proxies everything. The frontend never knows the server URL. Exposing it lets the TypeScript ACP client connect directly.

## Changes

### 1. Re-export `GooseServeProcess`

**File:** `src-tauri/src/services/acp/mod.rs`

Add a re-export so the command layer can reference the struct:

```rust
pub(crate) use goose_serve::GooseServeProcess;
```

No changes to `GooseServeProcess` itself — the existing `ws_url()` method already returns `ws://127.0.0.1:<port>/acp`.

### 2. Add the Tauri command

**File:** `src-tauri/src/commands/acp.rs`

Add this command alongside the existing ones:

```rust
use crate::services::acp::goose_serve::GooseServeProcess;

/// Return the WebSocket URL of the running goose serve process.
///
/// This command blocks until the server is confirmed ready. The frontend
/// uses this URL to establish a direct WebSocket ACP connection.
#[tauri::command]
pub async fn get_goose_serve_url() -> Result<String, String> {
    GooseServeProcess::start().await?;
    let process = GooseServeProcess::get()?;
    Ok(process.ws_url())
}
```

### 3. Register the command

**File:** `src-tauri/src/lib.rs`

Add the new command to the `invoke_handler` macro near the other `commands::acp::*` entries:

```rust
commands::acp::get_goose_serve_url,
```

### 4. CSP — no changes needed

**File:** `src-tauri/tauri.conf.json`

CSP is currently disabled (`"csp": null`), so the frontend can open WebSocket connections to `ws://127.0.0.1:*` without restriction.

If CSP is ever re-enabled, add:
```
connect-src 'self' ws://127.0.0.1:*
```

## Verification

1. `cargo check` in `src-tauri/` — confirms compilation.
2. `cargo clippy --all-targets -- -D warnings` in `src-tauri/`.
3. `cargo fmt` in `src-tauri/`.
4. Add a temporary `console.log(await invoke("get_goose_serve_url"))` in the frontend startup — it should print something like `ws://127.0.0.1:54321/acp`.

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/services/acp/mod.rs` | Add `pub(crate) use goose_serve::GooseServeProcess` |
| `src-tauri/src/commands/acp.rs` | Add `get_goose_serve_url` command |
| `src-tauri/src/lib.rs` | Register `get_goose_serve_url` in invoke_handler |

## Notes

- The existing ACP commands remain functional during migration. They are removed in Step 09.
- `GooseServeProcess::start()` is idempotent — the first call spawns the process; subsequent calls return immediately.
- The readiness check (`wait_for_server_ready`) ensures the URL is only returned after the server is accepting connections.
- The URL includes the `/acp` path — the same WebSocket endpoint the Rust layer currently uses in `thread.rs`.
