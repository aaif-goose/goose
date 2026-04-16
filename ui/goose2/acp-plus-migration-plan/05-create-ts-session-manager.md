# Step 05: Create the TypeScript Session Manager

## Objective

Port session state management and ACP operations from the Rust `session_ops.rs`, `command_dispatch.rs`, and `registry.rs` to TypeScript. This module orchestrates all ACP calls: prepare session, send prompt, cancel, load, list, export, import, fork, set model, and list providers.

## Why

The Rust `GooseAcpManager` + `session_ops` is the core orchestration layer that:
1. Tracks which goose sessions are prepared (composite key → goose session ID)
2. Creates or loads goose sessions on demand
3. Sets provider/model/working-dir on sessions
4. Sends prompts and coordinates with the notification handler for streaming
5. Handles cancellation
6. Provides session CRUD (list, export, import, fork)

All of this is pure protocol logic with no native OS access — it belongs in TypeScript.

## New File

### `src/shared/api/acpSessionManager.ts`

## Key Data Structures

### Prepared Session Cache

```typescript
interface PreparedSession {
  gooseSessionId: string;
  providerId: string;
  workingDir: string;
}

/** Maps composite key (sessionId or sessionId__personaId) → PreparedSession */
const preparedSessions = new Map<string, PreparedSession>();
```

### Composite Key Helpers

```typescript
export function makeCompositeKey(sessionId: string, personaId?: string): string {
  if (personaId && personaId.length > 0) {
    return `${sessionId}__${personaId}`;
  }
  return sessionId;
}

export function splitCompositeKey(key: string): { sessionId: string; personaId: string | null } {
  const idx = key.indexOf("__");
  if (idx >= 0) {
    const personaId = key.substring(idx + 2);
    if (personaId.length > 0) {
      return { sessionId: key.substring(0, idx), personaId };
    }
  }
  return { sessionId: key, personaId: null };
}
```

### Running Session Tracking

```typescript
interface RunningSession {
  compositeKey: string;
  providerId: string;
  startedAt: number; // Date.now()
  assistantMessageId: string | null;
  abortController: AbortController;
}

const runningSessions = new Map<string, RunningSession>();
```

## Core Operations

### 1. `prepareSession`

This is the most complex function. It:

1. Checks if a session is already prepared for this composite key
2. If yes, reuses it (updating working dir / provider if changed)
3. If no, tries to load an existing goose session by ID
4. If that fails, creates a new goose session via `client.newSession()`
5. Binds the goose session ID to the local session ID in the notification handler
6. Sets the provider via `client.setSessionConfigOption()` if needed
7. Emits model state to the session store

```typescript
export async function prepareSession(
  compositeKey: string,
  localSessionId: string,
  providerId: string,
  workingDir: string,
): Promise<string> {
  const client = await getClient();

  const existing = preparedSessions.get(compositeKey) ?? preparedSessions.get(localSessionId);
  if (existing) {
    bindSession(existing.gooseSessionId, localSessionId, providerId);
    if (existing.workingDir !== workingDir) {
      await client.goose.gooseWorkingDirUpdate({ sessionId: existing.gooseSessionId, workingDir });
      // ... update cache
    }
    if (existing.providerId !== providerId) {
      const response = await client.setSessionConfigOption({
        sessionId: existing.gooseSessionId,
        optionId: "provider",
        value: providerId,
      });
      // ... emit model state from response.configOptions
    }
    return existing.gooseSessionId;
  }

  let gooseSessionId: string | null = null;
  try {
    const loadResponse = await client.loadSession({
      sessionId: localSessionId,
      workingDir,
    });
    gooseSessionId = localSessionId;
    bindSession(gooseSessionId, localSessionId, providerId);
    // ... handle model state from loadResponse
    // ... update provider if needed
  } catch {
    // Session doesn't exist — create new
  }

  if (!gooseSessionId) {
    const meta: Record<string, unknown> = {};
    if (providerId !== "goose") {
      meta.provider = providerId;
    }
    const newResponse = await client.newSession({
      workingDir,
      ...(Object.keys(meta).length > 0 ? { meta } : {}),
    });
    gooseSessionId = newResponse.sessionId;
    bindSession(gooseSessionId, localSessionId, providerId);
    // ... handle model state from newResponse
  }

  const prepared: PreparedSession = { gooseSessionId, providerId, workingDir };
  preparedSessions.set(compositeKey, prepared);
  preparedSessions.set(localSessionId, prepared);

  return gooseSessionId;
}
```

### 2. `sendPrompt`

```typescript
export async function sendPrompt(
  sessionId: string,
  providerId: string,
  prompt: string,
  options: {
    workingDir?: string;
    systemPrompt?: string;
    personaId?: string;
    personaName?: string;
    images?: [string, string][]; // [base64, mimeType]
  } = {},
): Promise<void> {
  const client = await getClient();
  const compositeKey = makeCompositeKey(sessionId, options.personaId);

  const effectivePrompt = buildEffectivePrompt(prompt, options.systemPrompt);

  const abort = new AbortController();
  const assistantMessageId = crypto.randomUUID();
  runningSessions.set(compositeKey, {
    compositeKey,
    providerId,
    startedAt: Date.now(),
    assistantMessageId,
    abortController: abort,
  });

  try {
    const workingDir = options.workingDir ?? defaultArtifactsWorkingDir();
    const gooseSessionId = await prepareSession(compositeKey, sessionId, providerId, workingDir);

    attachWriter(gooseSessionId, sessionId, providerId, assistantMessageId, options.personaId, options.personaName);

    const content: ContentBlock[] = [{ type: "text", text: effectivePrompt }];
    for (const [data, mimeType] of (options.images ?? [])) {
      content.push({ type: "image", data, mimeType });
    }

    await client.prompt({
      sessionId: gooseSessionId,
      content,
    });

    clearWriter(gooseSessionId);
    finalizeMessage(sessionId, assistantMessageId);
  } catch (error) {
    clearWriter(/* gooseSessionId */);
    throw error;
  } finally {
    runningSessions.delete(compositeKey);
  }
}
```

### 3. `cancelSession`

```typescript
export async function cancelSession(sessionId: string, personaId?: string): Promise<boolean> {
  const compositeKey = makeCompositeKey(sessionId, personaId);
  const running = runningSessions.get(compositeKey);

  const prepared = preparedSessions.get(compositeKey) ?? preparedSessions.get(sessionId);
  if (!prepared) {
    return running !== undefined; // still preparing
  }

  markCanceled(prepared.gooseSessionId);

  try {
    const client = await getClient();
    await client.cancel({ sessionId: prepared.gooseSessionId });
  } catch {
    // Best-effort cancellation
  }

  return true;
}
```

### 4. `listSessions`

```typescript
export interface AcpSessionInfo {
  sessionId: string;
  title: string | null;
  updatedAt: string | null;
  messageCount: number;
}

export async function listSessions(): Promise<AcpSessionInfo[]> {
  const client = await getClient();
  const response = await client.unstable_listSessions({});
  return response.sessions.map((info) => ({
    sessionId: info.sessionId,
    title: info.title ?? null,
    updatedAt: info.updatedAt ?? null,
    messageCount: (info.meta?.messageCount as number) ?? 0,
  }));
}
```

### 5. `loadSession`

```typescript
export async function loadSession(
  localSessionId: string,
  gooseSessionId: string,
  workingDir: string,
): Promise<void> {
  const client = await getClient();

  bindSession(gooseSessionId, localSessionId);
  startReplayBuffer(localSessionId);

  const response = await client.loadSession({
    sessionId: gooseSessionId,
    workingDir,
  });

  // The backend sends replay notifications asynchronously.
  // The notification handler flushes the replay buffer on replay_complete.

  if (response.models) {
    handleModelState(localSessionId, null, /* extract from response.models */);
  }
  if (response.configOptions) {
    const modelState = extractModelOptionsFromConfigOptions(response.configOptions);
    if (modelState) handleModelState(localSessionId, null, modelState);
  }

  preparedSessions.set(localSessionId, {
    gooseSessionId,
    providerId: "goose", // updated on next prepare
    workingDir,
  });
}
```

### 6. `exportSession`, `importSession`, `forkSession`

```typescript
export async function exportSession(sessionId: string): Promise<string> {
  const client = await getClient();
  const result = await client.goose.gooseSessionExport({ sessionId });
  return result.data;
}

export async function importSession(json: string): Promise<AcpSessionInfo> {
  const client = await getClient();
  return await client.goose.gooseSessionImport({ data: json });
}

export async function forkSession(sessionId: string): Promise<AcpSessionInfo> {
  const client = await getClient();
  const response = await client.unstable_forkSession({
    sessionId,
    workingDir: defaultArtifactsWorkingDir(),
  });
  return {
    sessionId: response.sessionId,
    title: (response.meta?.title as string) ?? null,
    updatedAt: null,
    messageCount: (response.meta?.messageCount as number) ?? 0,
  };
}
```

### 7. `setModel`

```typescript
export async function setModel(localSessionId: string, modelId: string): Promise<void> {
  const client = await getClient();

  for (const [key, prepared] of preparedSessions) {
    const { sessionId } = splitCompositeKey(key);
    if (sessionId !== localSessionId) continue;

    const response = await client.setSessionConfigOption({
      sessionId: prepared.gooseSessionId,
      optionId: "model",
      value: modelId,
    });
    const modelState = extractModelOptionsFromConfigOptions(response.configOptions);
    if (modelState) handleModelState(localSessionId, prepared.providerId, modelState);
  }
}
```

### 8. `listProviders`

```typescript
const DEPRECATED_PROVIDER_IDS = new Set(["claude-code", "codex", "gemini-cli"]);

export interface AcpProvider {
  id: string;
  label: string;
}

export async function listProviders(): Promise<AcpProvider[]> {
  const client = await getClient();
  const result = await client.goose.gooseProvidersList({});
  return result.providers
    .filter((p: { id: string }) => !DEPRECATED_PROVIDER_IDS.has(p.id))
    .map((p: { id: string; label: string }) => ({ id: p.id, label: p.label }));
}
```

### 9. `listRunning`

```typescript
export interface AcpRunningSession {
  sessionId: string;
  personaId: string | null;
  providerId: string;
  runningForSecs: number;
}

export function listRunning(): AcpRunningSession[] {
  const now = Date.now();
  return [...runningSessions.values()].map((entry) => {
    const { sessionId, personaId } = splitCompositeKey(entry.compositeKey);
    return {
      sessionId,
      personaId,
      providerId: entry.providerId,
      runningForSecs: Math.floor((now - entry.startedAt) / 1000),
    };
  });
}
```

### 10. `cancelAll`

```typescript
export function cancelAll(): void {
  for (const entry of runningSessions.values()) {
    entry.abortController.abort();
  }
}
```

## Helper: Build Effective Prompt

```typescript
function buildEffectivePrompt(prompt: string, systemPrompt?: string): string {
  if (!systemPrompt || systemPrompt.trim().length === 0) {
    return prompt;
  }
  return [
    `<persona-instructions>\n${systemPrompt}\n</persona-instructions>`,
    `<user-message>\n${prompt}\n</user-message>`,
  ].join("\n\n");
}
```

## Helper: Default Artifacts Working Dir

```typescript
function defaultArtifactsWorkingDir(): string {
  return "~/.goose/artifacts";
}
```

The `goose serve` backend handles working directory resolution and `~` expansion. This function only needs to supply a reasonable path.

## Imports from Other Modules

```typescript
import { getClient } from "./acpConnection";
import {
  bindSession,
  attachWriter,
  clearWriter,
  markCanceled,
  startReplayBuffer,
  finalizeReplay,
  flushReplayBuffer,
  finalizeMessage,
  handleModelState,
  extractModelOptionsFromConfigOptions,
} from "./acpNotificationHandler";
```

## Concurrency

The Rust code uses per-session `Mutex` locks (`op_locks`) and `pending_cancels` / `preparing_sessions` sets to prevent concurrent mutations and coordinate cancellation during preparation. In single-threaded JS, mutex locks aren't needed for correctness, but a simple promise-based lock prevents concurrent `prepareSession` calls for the same composite key from racing. Port `pending_cancels` and `preparing_sessions` as module-level `Set<string>` variables.

## Generated Client Method Names

The `GooseExtClient` methods (e.g., `client.goose.gooseProvidersList()`) are generated from the ACP schema. Verify actual method names in `ui/acp/src/generated/client.gen.ts` — they use camelCase versions of the `goose/providers/list` method name.

## Streaming Model

The `client.prompt()` call blocks until the agent finishes responding. During this time, `SessionNotification` events stream in via the `Client` callback, handled by the notification handler. This matches the Rust flow.

## Verification

1. `pnpm typecheck` passes.
2. `pnpm check` passes.
3. Unit tests for `makeCompositeKey`, `splitCompositeKey`, `buildEffectivePrompt`.
4. Port relevant tests from `session_ops/tests.rs`.

## Files Created

| File | Purpose |
|------|---------|
| `src/shared/api/acpSessionManager.ts` | Session state management and ACP operations |

## Dependencies

- Step 03 (`acpConnection.ts` — provides `getClient()`)
- Step 04 (`acpNotificationHandler.ts` — provides bind/attach/clear/finalize functions)
