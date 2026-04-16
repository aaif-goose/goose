# Step 04: Create the TypeScript Notification Handler

## Objective

Port the Rust `SessionEventDispatcher` (in `src-tauri/src/services/acp/manager/dispatcher.rs`) to TypeScript. This module receives ACP `SessionNotification` events and updates Zustand stores directly — replacing the current Tauri event bus (`acp:text`, `acp:tool_call`, etc.) and the `useAcpStream` hook.

## Why

Currently, ACP notifications flow through three layers:
1. Rust `SessionEventDispatcher` receives the ACP callback
2. Rust emits Tauri events (`acp:text`, `acp:done`, etc.)
3. TypeScript `useAcpStream` hook listens to those events and updates stores

By handling notifications directly in TypeScript, we eliminate the Tauri event bus intermediary and the `useAcpStream` hook entirely.

## New File

### `src/shared/api/acpNotificationHandler.ts`

This file implements the `AcpNotificationHandler` interface from Step 03 and contains all the logic currently split between `dispatcher.rs`, `writer.rs`, and `useAcpStream.ts`.

## Key Data Structures to Port

### Session Route Map

The Rust `dispatcher.rs` maintains a `HashMap<String, SessionRoute>` that maps goose session IDs to local session IDs. Port this as:

```typescript
interface SessionRoute {
  localSessionId: string;
  providerId: string | null;
  activeMessageId: string | null;
  canceled: boolean;
  personaId: string | null;
  personaName: string | null;
}

const routes = new Map<string, SessionRoute>();
```

### Replay Buffer

During `loadSession`, notifications arrive for historical messages. These are buffered and flushed as a single `store.setMessages()` call when `replay_complete` is signaled.

```typescript
import type { Message, MessageContent, ToolRequestContent } from "@/shared/types/messages";

const replayBuffers = new Map<string, Message[]>();
```

## Notification Dispatch Logic

Port the `session_notification` method from `dispatcher.rs`. The Rust code handles these `SessionUpdate` variants:

### 1. `SessionInfoUpdate`

```typescript
function handleSessionInfoUpdate(localSessionId: string, info: SessionInfoUpdate): void {
  const session = useChatSessionStore.getState().getSession(localSessionId);
  if (info.title && !session?.userSetName) {
    useChatSessionStore.getState().updateSession(localSessionId, {
      title: info.title,
    }, { persistOverlay: false });
  }
}
```

### 2. `ConfigOptionUpdate` (model state)

Extract model options from `SessionConfigSelectOptions` (ungrouped or grouped) and update the session store:

```typescript
import type { ModelOption } from "@/features/chat/types";

function extractModelOptionsFromConfigOptions(
  options: SessionConfigOption[],
): { currentModelId: string; currentModelName: string | null; availableModels: ModelOption[] } | null {
  const modelOption = options.find(
    (opt) => opt.category === "model"
  );
  if (!modelOption || modelOption.kind.type !== "select") return null;

  const select = modelOption.kind;
  const currentModelId = select.currentValue;
  const availableModels: ModelOption[] = [];

  if (select.options.type === "ungrouped") {
    for (const value of select.options.values) {
      availableModels.push({ id: value.value, name: value.name });
    }
  } else if (select.options.type === "grouped") {
    for (const group of select.options.groups) {
      for (const value of group.options) {
        availableModels.push({ id: value.value, name: value.name });
      }
    }
  }

  const currentModelName = availableModels.find(m => m.id === currentModelId)?.name ?? null;
  return { currentModelId, currentModelName, availableModels };
}

function handleModelState(
  localSessionId: string,
  providerId: string | null,
  modelState: { currentModelId: string; currentModelName: string | null; availableModels: ModelOption[] },
): void {
  const sessionStore = useChatSessionStore.getState();
  if (providerId) {
    sessionStore.cacheModelsForProvider(providerId, modelState.availableModels);
  }
  const session = sessionStore.getSession(localSessionId);
  const sessionProvider = session?.providerId;
  if (providerId && sessionProvider && providerId !== sessionProvider) {
    return;
  }
  const modelName = modelState.currentModelName ?? modelState.currentModelId;
  sessionStore.setSessionModels(localSessionId, modelState.availableModels);
  if (!providerId && session?.modelId) {
    return;
  }
  sessionStore.updateSession(localSessionId, {
    modelId: modelState.currentModelId,
    modelName,
  }, { persistOverlay: false });
}
```

### 3. `AgentMessageChunk` (live streaming — text)

When a route has an `activeMessageId` (live streaming path):

```typescript
function handleLiveText(localSessionId: string, text: string): void {
  const store = useChatStore.getState();
  store.updateStreamingText(localSessionId, text);
}
```

When in replay mode (no `activeMessageId`, session is loading):

```typescript
function handleReplayText(localSessionId: string, gooseSessionId: string, text: string): void {
  const buffer = replayBuffers.get(localSessionId);
  if (!buffer) return;
  const route = routes.get(gooseSessionId);
  // Find or create the current assistant message in the buffer
  // and append the text chunk to it.
}
```

### 4. `ToolCall` and `ToolCallUpdate`

The live path calls:
```typescript
store.appendToStreamingMessage(sessionId, toolRequest);
```

The replay path appends to the buffer message.

### 5. `UserMessageChunk` (replay only)

During replay, user messages arrive as `UserMessageChunk`. Extract the inner content:

```typescript
function extractUserMessage(raw: string): string {
  const openTag = "<user-message>\n";
  const closeTag = "\n</user-message>";
  const startIdx = raw.indexOf(openTag);
  if (startIdx >= 0) {
    const innerStart = startIdx + openTag.length;
    if (raw.substring(innerStart).endsWith(closeTag)) {
      return raw.substring(innerStart, raw.length - closeTag.length);
    }
  }
  return raw;
}
```

### 6. Done / Finalize

When a streaming message completes (the `prompt()` call resolves), the session manager (Step 05) calls a finalize method:

```typescript
export function finalizeMessage(localSessionId: string, messageId: string): void {
  const store = useChatStore.getState();
  store.updateMessage(localSessionId, messageId, (message) => {
    const content = message.content.map((block) =>
      block.type === "toolRequest" && block.status === "executing"
        ? { ...block, status: "completed" as const }
        : block,
    );
    return {
      ...message,
      content,
      metadata: { ...message.metadata, completionStatus: "completed" },
    };
  });
  store.setStreamingMessageId(localSessionId, null);
  store.setChatState(localSessionId, "idle");
}
```

## Public API

```typescript
/** Register a goose session ID → local session ID binding. */
export function bindSession(gooseSessionId: string, localSessionId: string, providerId?: string): void;

/** Attach a "writer" for live streaming — sets the active message ID. */
export function attachWriter(gooseSessionId: string, localSessionId: string, providerId: string | null, messageId: string, personaId?: string, personaName?: string): void;

/** Clear the active writer after streaming completes. */
export function clearWriter(gooseSessionId: string): void;

/** Mark a session as cancelled. */
export function markCanceled(gooseSessionId: string): boolean;

/** Start replay buffering for a session. */
export function startReplayBuffer(localSessionId: string): void;

/** Finalize replay — flush buffer to store. */
export function finalizeReplay(gooseSessionId: string): void;

/** Flush the replay buffer for a session (called when loading completes). */
export function flushReplayBuffer(localSessionId: string): void;

/** Finalize a completed streaming message. */
export function finalizeMessage(localSessionId: string, messageId: string): void;

/** The main notification handler — implements AcpNotificationHandler from Step 03. */
export function handleSessionNotification(notification: SessionNotification): Promise<void>;
```

## Porting Checklist

| Rust Source | Rust Function/Method | TS Equivalent |
|-------------|---------------------|---------------|
| `dispatcher.rs` | `SessionEventDispatcher::session_notification` | `handleSessionNotification()` |
| `dispatcher.rs` | `SessionEventDispatcher::bind_session` | `bindSession()` |
| `dispatcher.rs` | `SessionEventDispatcher::attach_writer` | `attachWriter()` |
| `dispatcher.rs` | `SessionEventDispatcher::clear_writer` | `clearWriter()` |
| `dispatcher.rs` | `SessionEventDispatcher::mark_canceled` | `markCanceled()` |
| `dispatcher.rs` | `SessionEventDispatcher::finalize_replay` | `finalizeReplay()` |
| `dispatcher.rs` | `SessionEventDispatcher::emit_session_info` | `handleSessionInfoUpdate()` |
| `dispatcher.rs` | `SessionEventDispatcher::emit_model_state` | `handleModelState()` |
| `dispatcher.rs` | `SessionEventDispatcher::emit_model_state_from_options` | `handleModelState()` via `extractModelOptionsFromConfigOptions()` |
| `dispatcher.rs` | `SessionEventDispatcher::emit_replay_complete` | `flushReplayBuffer()` + `store.setSessionLoading(false)` |
| `dispatcher.rs` | `extract_user_message` | `extractUserMessage()` |
| `dispatcher.rs` | `extract_content_preview` | `extractContentPreview()` |
| `writer.rs` | `TauriMessageWriter::append_text` | Handled inline in `handleSessionNotification` |
| `writer.rs` | `TauriMessageWriter::record_tool_call` | Handled inline in `handleSessionNotification` |
| `writer.rs` | `TauriMessageWriter::record_tool_result` | Handled inline in `handleSessionNotification` |
| `writer.rs` | `TauriMessageWriter::finalize` | `finalizeMessage()` |
| `useAcpStream.ts` | All event listeners | Replaced by `handleSessionNotification()` |
| `replayBuffer.ts` | Buffer management | Inlined or imported |

## Store Methods Used

The notification handler calls these existing Zustand store methods (no changes needed to the stores):

**`useChatStore`:**
- `addMessage(sessionId, message)`
- `updateMessage(sessionId, messageId, updater)`
- `setMessages(sessionId, messages)` — for replay buffer flush
- `updateStreamingText(sessionId, text)`
- `appendToStreamingMessage(sessionId, content)`
- `setStreamingMessageId(sessionId, id)`
- `setChatState(sessionId, state)`
- `setPendingAssistantProvider(sessionId, null)`
- `setSessionLoading(sessionId, loading)`
- `markSessionUnread(sessionId)`
- `setError(sessionId, error)`

**`useChatSessionStore`:**
- `updateSession(sessionId, patch, opts)`
- `setSessionAcpId(sessionId, acpSessionId)`
- `setSessionModels(sessionId, models)`
- `cacheModelsForProvider(providerId, models)`
- `getSession(sessionId)`

## Registration

During app initialization (Step 08), register the handler with the connection manager:

```typescript
import { setNotificationHandler } from "@/shared/api/acpConnection";
import * as notificationHandler from "@/shared/api/acpNotificationHandler";

setNotificationHandler(notificationHandler);
```

## Verification

1. `pnpm typecheck` passes.
2. `pnpm check` passes.
3. Unit tests for `extractUserMessage` and `extractContentPreview` (port the Rust tests from `dispatcher_tests.rs`).

## Files Created

| File | Purpose |
|------|---------|
| `src/shared/api/acpNotificationHandler.ts` | ACP notification handler — replaces dispatcher.rs + writer.rs + useAcpStream.ts |

## Dependencies

- Step 03 (`acpConnection.ts` must exist for the `AcpNotificationHandler` interface)
- Zustand stores (`useChatStore`, `useChatSessionStore`) — no changes needed

## Notes

- In single-threaded JS, a plain `Map` replaces the Rust `Arc<Mutex<HashMap>>` for route storage.
- Replay buffering relies on the `replay_complete` signal from the backend. The `loadSession` RPC resolves, the backend sends remaining notifications, then sends `replay_complete`. The handler flushes the buffer at that point.
- The `SessionNotification` type from `@agentclientprotocol/sdk` has a `sessionId` field (the goose session ID) and an `update` field with the variant. Check the SDK types for the exact shape.
- Port the `shouldTrackStreamingEvent` guard from `useAcpStream.ts` — it prevents stale events from updating already-completed messages.
