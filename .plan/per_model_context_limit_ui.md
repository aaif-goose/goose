# Per-Model Context Limit UI in Context Popover

## Goal

Let users override the context limit for dynamic models directly from the context usage popover (near the "Compact now" button). The override applies to the current session and persists across future sessions for that provider+model combination.

## Background

- Declarative providers (lmstudio, llama_swap, omlx) now have a `context_limit` field in their JSON config as a provider-level default (e.g., 1024K).
- Users may need per-model overrides that differ from the provider default.
- The local inference provider already has a `ModelSettingsPanel` with a `context_size` field — the declarative provider UI should offer a lighter-weight equivalent in the popover.

---

## Backend (Rust)

### 1. ACP custom methods

**File:** `crates/goose-sdk/src/custom_requests.rs`

Add three new request/response structs:

```
_get goose/model/context_limit/set    — Store a per-model context limit override
_get goose/model/context_limit/get    — Read the stored override for a provider+model
_get goose/model/context_limit/reset  — Remove the override (revert to provider/default)
```

Request struct for set/get:
```rust
pub struct ModelContextLimitRequest {
    pub provider: String,
    pub model: String,
}
```

Request struct for set:
```rust
pub struct ModelContextLimitSetRequest {
    pub provider: String,
    pub model: String,
    pub context_limit: Option<usize>,  // None to reset
}
```

Response:
```rust
pub struct ModelContextLimitResponse {
    pub provider: String,
    pub model: String,
    pub context_limit: Option<usize>,
}
```

### 2. Handler implementation

**File:** `crates/goose/src/acp/server.rs`

Implement handlers for the three methods. Persist overrides using the existing preferences system:
- Key format: `model_context_limit:{provider}:{model}`
- Value: the usize limit as a string

### 3. Apply override during model resolution

**File:** `crates/goose/src/acp/server.rs` (`set_model_and_extensions`)

After resolving the provider default context limit, check for a stored override:
1. Build the preference key from current provider + model
2. Read via `config.get_param::<String>(&key)`
3. If found and valid, use it as the `context_limit`

---

## Frontend (goose2)

### 4. Regenerate SDK

Run the SDK generation step so the new ACP methods appear as typed `GooseClient` methods.

### 5. Feature API module

**File:** `ui/goose2/src/features/chat/api/contextLimit.ts` (new)

Thin wrappers around the new ACP methods:
- `getModelContextLimit(provider: string, model: string): Promise<number | null>`
- `setModelContextLimit(provider: string, model: string, limit: number | null): Promise<void>`
- `resetModelContextLimit(provider: string, model: string): Promise<void>`

### 6. Context limit editor in popover

**File:** `ui/goose2/src/features/chat/ui/ChatInputToolbar.tsx`

Add a new section below the "Compact now" / settings row in the context popover:

```
+------------------------------------------+
|  Context Window                          |
|  [====================>] 78%            |
|  78K / 1024K tokens                      |
|  [Compact now] [Settings]                |
|                                          |
|  Context limit:  [1024K v]               |  ← new
|  Default: 1024K  [Reset]                 |
+------------------------------------------+
```

**Components:**
- **Preset selector**: Dropdown with "Default", "256K", "512K", "1024K", "2048K"
- **Custom input**: When user types a value not in presets, show "Custom" badge
- **Reset button**: Reverts to provider default, calls the reset ACP method
- **Default label**: Shows the provider's built-in default for reference
- Only visible for providers that support dynamic models (declarative providers)

**State flow:**
1. On popover open: fetch current override via `getModelContextLimit`
2. On change: call `setModelContextLimit` → update `chatStore.tokenState.contextLimit`
3. On reset: call `resetModelContextLimit` → revert to provider default

### 7. Wire into chatStore

**File:** `ui/goose2/src/features/chat/stores/chatStore.ts`

Add an action to update context limit for a session:
```ts
setSessionContextLimit: (sessionId, contextLimit) =>
  set((state) => ({
    sessionStateById: {
      ...state.sessionStateById,
      [sessionId]: {
        ...state.sessionStateById[sessionId],
        tokenState: {
          ...state.sessionStateById[sessionId]?.tokenState,
          contextLimit,
        },
      },
    },
  })),
```

---

## Files to modify

| File | Action |
|------|--------|
| `crates/goose-sdk/src/custom_requests.rs` | Add 3 new ACP methods |
| `crates/goose/src/acp/server.rs` | Implement handlers + apply override in model resolution |
| `ui/goose2/src/features/chat/api/contextLimit.ts` | **New** — API wrappers |
| `ui/goose2/src/features/chat/ui/ChatInputToolbar.tsx` | Add context limit editor to popover |
| `ui/goose2/src/features/chat/stores/chatStore.ts` | Add `setSessionContextLimit` action |
| `ui/sdk/src/generated/` | Regenerate from SDK defs |

## Design notes

- Control is unobtrusive — inline input, no modal
- "Custom" badge shown when override differs from provider default
- Only shown for declarative providers with dynamic models (check provider type)
- For local inference models, existing `ModelSettingsPanel` already handles `context_size` — no duplicate UI needed
- Quick presets cover common ranges; custom input accepts any value
