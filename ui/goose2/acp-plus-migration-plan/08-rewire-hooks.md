# Step 08: Remove `useAcpStream`, Update Hooks and App Initialization

## Objective

Remove the `useAcpStream` hook (which listens to Tauri events) since the notification handler (Step 04) now updates stores directly. Update app initialization to set up the new ACP connection and notification handler. Update `AppShell` to use the new code paths.

## Why

With the notification handler updating Zustand stores directly from ACP callbacks, the Tauri event bus is no longer in the loop. The `useAcpStream` hook — which listens to `acp:text`, `acp:done`, `acp:tool_call`, etc. — is now dead code.

## Changes

### 1. Remove `useAcpStream` from `AppShell`

**File:** `src/app/AppShell.tsx`

Remove the import and call:

```diff
- import { useAcpStream } from "@/features/chat/hooks/useAcpStream";

  // Inside the component:
- useAcpStream(true);
```

### 2. Initialize the ACP connection and notification handler on startup

**File:** `src/app/hooks/useAppStartup.ts`

Add ACP initialization as the first step. The notification handler must be registered before any ACP calls so that session notifications from `loadSessions` are handled.

```typescript
import { useEffect } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";

export function useAppStartup() {
  useEffect(() => {
    (async () => {
      // Step 1: Initialize ACP connection and notification handler.
      // This must happen before any ACP calls.
      try {
        const { getClient, setNotificationHandler } = await import(
          "@/shared/api/acpConnection"
        );
        const notificationHandler = await import(
          "@/shared/api/acpNotificationHandler"
        );
        setNotificationHandler(notificationHandler);

        // Trigger connection initialization (fetches URL, creates client, handshake).
        // This blocks until goose serve is ready.
        await getClient();
      } catch (err) {
        console.error("Failed to initialize ACP connection:", err);
        // The app can still show the UI, but ACP operations will fail.
        // Individual operations will retry getClient() and show errors.
      }

      // Step 2: Load data in parallel (same as before, but now using the TS ACP client)
      const store = useAgentStore.getState();

      const loadPersonas = async () => {
        store.setPersonasLoading(true);
        try {
          const { listPersonas } = await import("@/shared/api/agents");
          const personas = await listPersonas();
          store.setPersonas(personas);
        } catch (err) {
          console.error("Failed to load personas on startup:", err);
        } finally {
          store.setPersonasLoading(false);
        }
      };

      const loadProviders = async () => {
        store.setProvidersLoading(true);
        try {
          const { discoverAcpProviders } = await import("@/shared/api/acp");
          const providers = await discoverAcpProviders();
          store.setProviders(providers);
        } catch (err) {
          console.error("Failed to load ACP providers on startup:", err);
        } finally {
          store.setProvidersLoading(false);
        }
      };

      const loadSessionState = async () => {
        const t0 = performance.now();
        console.log("[perf:startup] loadSessionState start");
        const { loadSessions, setActiveSession } =
          useChatSessionStore.getState();
        await loadSessions();
        console.log(
          `[perf:startup] loadSessions done in ${(performance.now() - t0).toFixed(1)}ms`,
        );
        setActiveSession(null);
      };

      await Promise.allSettled([
        loadPersonas(),
        loadProviders(),
        loadSessionState(),
      ]);
    })();
  }, []);
}
```

### 3. `AppShell.loadSessionMessages` — no changes needed

**File:** `src/app/AppShell.tsx`

The `loadSessionMessages` callback dynamically imports `acpLoadSession` from `@/shared/api/acp`. This still works because Step 07 rewired that function to go through the TS session manager.

```typescript
const { acpLoadSession } = await import("@/shared/api/acp");
```

### 4. `useChat` — no changes needed

**File:** `src/features/chat/hooks/useChat.ts`

This hook imports `acpSendMessage`, `acpCancelSession`, `acpPrepareSession`, `acpSetModel` from `@/shared/api/acp`. Step 07 kept the same API surface, so no changes are needed.

```typescript
import {
  acpSendMessage,
  acpCancelSession,
  acpPrepareSession,
  acpSetModel,
} from "@/shared/api/acp";
```

### 5. Handle app shutdown

**File:** `src/app/AppShell.tsx` or `src/app/App.tsx`

Add cleanup on window close to cancel running sessions:

```typescript
import { useEffect } from "react";

useEffect(() => {
  const handleBeforeUnload = () => {
    import("@/shared/api/acpSessionManager").then(({ cancelAll }) => {
      cancelAll();
    }).catch(() => {});
  };

  window.addEventListener("beforeunload", handleBeforeUnload);
  return () => window.removeEventListener("beforeunload", handleBeforeUnload);
}, []);
```

The Rust backend's `acp_registry_for_exit.cancel_all()` on `RunEvent::Exit` becomes a no-op after migration (no sessions are registered in the Rust registry). The TS cleanup above replaces it.

### 6. Delete old files

These files are no longer needed:

- `src/features/chat/hooks/useAcpStream.ts` — replaced by `acpNotificationHandler.ts`
- `src/features/chat/hooks/acpStreamTypes.ts` — types moved to the notification handler / SDK imports
- `src/features/chat/hooks/replayBuffer.ts` — logic moved into the notification handler
- `src/features/chat/hooks/useSSE.ts` — only consumer was `useAcpStream`

### 7. Update test files

**Delete:**
- `src/features/chat/hooks/__tests__/useAcpStream.test.ts` — the hook no longer exists

**Update:**
- `src/features/chat/hooks/__tests__/useChat.test.ts` — mocks should target the session manager functions instead of `invoke()`.

Replace any `invoke()`-level mocks:

```typescript
// Before
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// After
vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: vi.fn().mockResolvedValue(undefined),
  acpPrepareSession: vi.fn().mockResolvedValue(undefined),
  acpSetModel: vi.fn().mockResolvedValue(undefined),
  acpCancelSession: vi.fn().mockResolvedValue(true),
}));
```

### 8. Remove Tauri event listener cleanup

The `useAcpStream` hook registered listeners for `acp:text`, `acp:done`, `acp:tool_call`, `acp:tool_title`, `acp:tool_result`, `acp:message_created`, `acp:session_info`, `acp:session_bound`, `acp:model_state`, `acp:usage_update`, `acp:replay_complete`, `acp:replay_user_message`. All of these are gone now.

Confirm no other code listens to these events:

```bash
cd ui/goose2/src
rg "acp:" --include="*.ts" --include="*.tsx" | grep -v "__tests__" | grep -v "node_modules"
```

After this step, the only `acp:` references should be in test files (updated/deleted above).

## Verification

1. `pnpm typecheck` passes.
2. `pnpm check` passes.
3. `pnpm test` passes (after updating/deleting affected tests).
4. Manual testing:
   - App starts and shows the home screen
   - Session list loads
   - Creating a new chat and sending a message works
   - Streaming text appears in real-time
   - Tool calls display correctly
   - Cancelling a running session works
   - Loading a historical session replays messages
   - Session search returns results
   - Model switching works
   - Session export/import/duplicate works

## Files Modified

| File | Change |
|------|--------|
| `src/app/AppShell.tsx` | Remove `useAcpStream(true)`, add shutdown cleanup |
| `src/app/hooks/useAppStartup.ts` | Add ACP connection + notification handler initialization |

## Files Deleted

| File | Reason |
|------|--------|
| `src/features/chat/hooks/useAcpStream.ts` | Replaced by `acpNotificationHandler.ts` |
| `src/features/chat/hooks/acpStreamTypes.ts` | Types moved to notification handler |
| `src/features/chat/hooks/replayBuffer.ts` | Logic moved to notification handler |
| `src/features/chat/hooks/useSSE.ts` | Only consumer was `useAcpStream` |
| `src/features/chat/hooks/__tests__/useAcpStream.test.ts` | Hook deleted |

## Dependencies

- Step 03 (`acpConnection.ts`)
- Step 04 (`acpNotificationHandler.ts`)
- Step 07 (rewired `acp.ts`)

## Notes

- `useAcpStream` was the only consumer of the `acp:*` Tauri events. Once removed, no frontend code listens to those events. The Rust backend still emits them until Step 09 removes the Rust code, but they go nowhere — this is harmless.
- The `useChat` hook's `sendMessage` function sets `chatState` to `"thinking"` before `acpPrepareSession`, then `"streaming"` before `acpSendMessage`. This flow is unchanged — the session manager handles the ACP calls, and the notification handler updates the store as streaming events arrive.
- The `stopGeneration` function in `useChat` calls `acpCancelSession`, which now goes through the TS session manager calling `client.cancel()` directly.
- The `loadSessionMessages` callback in `AppShell` sets `store.setSessionLoading(sessionId, true)` before calling `acpLoadSession`. The notification handler's replay logic checks `loadingSessionIds` to decide whether to buffer. This flow is preserved — the notification handler reads from `useChatStore.getState().loadingSessionIds` just as `useAcpStream` did.
