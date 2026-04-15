# Step 07: Rewire `src/shared/api/acp.ts` to Use the TypeScript ACP Client

## Objective

Replace all `invoke()` calls in `src/shared/api/acp.ts` with calls to the TypeScript ACP session manager (Step 05) and search module (Step 06). Keep the same public API signatures so consumers don't need to change. Use the feature flag from Step 03 to route between old and new paths.

## Why

`src/shared/api/acp.ts` is the single import point for all ACP operations in the frontend. Currently every function calls `invoke("acp_*")`, which goes through Tauri IPC → Rust → WebSocket → goose serve. After this step, they call the TypeScript session manager, which goes directly through WebSocket → goose serve.

The feature flag (`useDirectAcp`) allows both paths to coexist. This means:
- The swap is gradual and reversible
- We can test the new path per-user without affecting everyone
- Instant rollback by flipping the flag

## Changes

### `src/shared/api/acp.ts`

Keep the existing `invoke()` implementations. Add the new direct-ACP implementations alongside them. Route via the feature flag.

**Pattern:**
```typescript
import { invoke } from "@tauri-apps/api/core";
import { useDirectAcp } from "./acpFeatureFlag";

// Lazy imports to avoid loading the new modules when flag is off
async function getSessionManager() {
  return import("./acpSessionManager");
}

export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  if (useDirectAcp()) {
    const { listProviders } = await getSessionManager();
    return listProviders();
  }
  return invoke("discover_acp_providers");
}
```

Once validated, a follow-up removes the `invoke()` branches and the feature flag (Step 09).

### Function-by-function rewiring

#### `discoverAcpProviders`

```typescript
export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  if (useDirectAcp()) {
    const { listProviders } = await getSessionManager();
    return listProviders();
  }
  return invoke("discover_acp_providers");
}
```

#### `acpSendMessage`

```typescript
export async function acpSendMessage(
  sessionId: string,
  providerId: string,
  prompt: string,
  options: AcpSendMessageOptions = {},
): Promise<void> {
  return sendPrompt(sessionId, providerId, prompt, {
    workingDir: options.workingDir,
    systemPrompt: options.systemPrompt,
    personaId: options.personaId,
    personaName: options.personaName,
    images: options.images,
  });
}
```

#### `acpPrepareSession`

```typescript
export async function acpPrepareSession(
  sessionId: string,
  providerId: string,
  options: AcpPrepareSessionOptions = {},
): Promise<void> {
  const { makeCompositeKey } = await import("./acpSessionManager");
  const compositeKey = makeCompositeKey(sessionId, options.personaId);
  const workingDir = options.workingDir ?? "~/.goose/artifacts";
  await prepareSession(compositeKey, sessionId, providerId, workingDir);
}
```

#### `acpSetModel`

```typescript
export async function acpSetModel(
  sessionId: string,
  modelId: string,
): Promise<void> {
  return setModel(sessionId, modelId);
}
```

#### `acpListSessions`

```typescript
export async function acpListSessions(): Promise<AcpSessionInfo[]> {
  return listSessions();
}
```

#### `acpSearchSessions`

```typescript
export async function acpSearchSessions(
  query: string,
  sessionIds: string[],
): Promise<AcpSessionSearchResult[]> {
  return searchSessionsViaExports(query, sessionIds);
}
```

#### `acpLoadSession`

```typescript
export async function acpLoadSession(
  sessionId: string,
  gooseSessionId: string,
  workingDir?: string,
): Promise<void> {
  return loadSession(sessionId, gooseSessionId, workingDir ?? "~/.goose/artifacts");
}
```

#### `acpExportSession`

```typescript
export async function acpExportSession(sessionId: string): Promise<string> {
  return exportSession(sessionId);
}
```

#### `acpImportSession`

```typescript
export async function acpImportSession(json: string): Promise<AcpSessionInfo> {
  return importSession(json);
}
```

#### `acpDuplicateSession`

```typescript
export async function acpDuplicateSession(sessionId: string): Promise<AcpSessionInfo> {
  return forkSession(sessionId);
}
```

#### `acpCancelSession`

```typescript
export async function acpCancelSession(
  sessionId: string,
  personaId?: string,
): Promise<boolean> {
  return cancelSession(sessionId, personaId);
}
```

### Interface types

`AcpSendMessageOptions` and `AcpPrepareSessionOptions` remain defined in this file since they are specific to this API surface. Types originating from the session manager and search module are re-exported:

```typescript
export type { AcpProvider, AcpSessionInfo } from "./acpSessionManager";
export type { SessionSearchResult as AcpSessionSearchResult } from "@/features/sessions/lib/sessionContentSearch";

export interface AcpSendMessageOptions {
  systemPrompt?: string;
  workingDir?: string;
  personaId?: string;
  personaName?: string;
  images?: [string, string][];
}

export interface AcpPrepareSessionOptions {
  workingDir?: string;
  personaId?: string;
}
```

### `src/shared/api/index.ts`

No changes needed — it already re-exports from `./acp`:

```typescript
export * from "./acp";
```

## Consumers

These files import from `@/shared/api/acp` and require no changes since the public API is unchanged:

| File | Imports Used |
|------|-------------|
| `src/features/chat/hooks/useChat.ts` | `acpSendMessage`, `acpCancelSession`, `acpPrepareSession`, `acpSetModel` |
| `src/features/chat/stores/chatSessionStore.ts` | `acpListSessions`, `AcpSessionInfo` |
| `src/features/sessions/hooks/useSessionSearch.ts` | `acpSearchSessions` |
| `src/features/sessions/lib/buildSessionSearchResults.ts` | `AcpSessionSearchResult` |
| `src/app/AppShell.tsx` | `acpPrepareSession`, `acpLoadSession` |
| `src/app/hooks/useAppStartup.ts` | `discoverAcpProviders` |

## Remove `invoke` Import

The file no longer imports from `@tauri-apps/api/core`. Other files (agents, git, system, etc.) still use `invoke()` for non-ACP commands.

## Verification

1. `pnpm typecheck` passes — all consumers type-check against the same API.
2. `pnpm check` passes.
3. `pnpm test` passes — existing tests that mock `invoke()` need updating (check `src/features/chat/hooks/__tests__/useAcpStream.test.ts` and `src/features/chat/hooks/__tests__/useChat.test.ts`).
4. Manual testing: start the app, confirm sessions load, messages send, and search works.

## Files Modified

| File | Change |
|------|--------|
| `src/shared/api/acp.ts` | Replace all `invoke()` calls with session manager / search calls |

## Dependencies

- Step 05 (`acpSessionManager.ts` — all session operations)
- Step 06 (`sessionContentSearch.ts` — search)

## Notes

- After this step, the frontend no longer calls any `acp_*` Tauri commands. The only remaining Tauri invoke for ACP infrastructure is `get_goose_serve_url`, called by `acpConnection.ts`.
- The old Rust ACP commands still exist and are registered but are no longer called. They are removed in Step 09.
- The `@tauri-apps/api/core` import is removed from this file entirely.
