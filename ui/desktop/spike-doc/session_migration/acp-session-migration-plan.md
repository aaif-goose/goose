# Desktop ACP Session Migration Plan

## Goal

Move `ui/desktop` session and chat runtime behavior from REST/OpenAPI to ACP.

This plan assumes the end goal is ACP-only for migrated session behavior. It does
not use a feature flag for each migrated call. REST remains only for areas that
have not moved yet.

## Scope

In scope:

- Session creation.
- Session loading and conversation replay.
- Live prompt streaming.
- Prompt cancellation.
- ACP session update handling.
- ACP tool permission handling.
- Desktop-side REST session import cleanup after the UI no longer uses those calls.

Out of scope:

- Extension list migration.
- Extension mutation migration.
- Removing server REST endpoints in the same change, unless usage is clearly gone
  and the change remains easy to verify.

## Current REST Session Shape

Important desktop files:

- `ui/desktop/src/sessions.ts`
  - `createSession`
  - `startNewSession`
  - `resumeSession`
- `ui/desktop/src/hooks/useChatStream.ts`
  - `resumeAgent`
  - `getSession`
  - `sessionReply`
  - `sessionCancel`
  - `updateFromSession`
- `ui/desktop/src/hooks/useSessionEvents.ts`
  - `GET /sessions/{id}/events` SSE
  - `ActiveRequests`
  - `request_id` / `chat_request_id` routing

The current UI expects REST/SSE events such as:

- `Message`
- `Finish`
- `Error`
- `Notification`
- `UpdateConversation`
- `ActiveRequests`

## Target ACP Session Shape

Use the existing ACP WebSocket client path from `ui/desktop/src/acp`.

ACP methods to use:

- `session/new`
- `session/load`
- `session/prompt`
- `session/cancel`
- `session/list`, if session list is included in the change

ACP notifications to handle:

- `session/update`
- `agent_message_chunk`
- `user_message_chunk`
- `agent_thought_chunk`
- `tool_call`
- `tool_call_update`
- `usage_update`
- `session_info_update`
- `config_option_update`

ACP client requests to handle:

- `requestPermission`

## No Feature Flag Rule

Do not add a REST/ACP feature flag for the same migrated behavior.

Instead:

- A behavior that has moved to ACP should call ACP directly.
- REST remains only for behaviors not yet migrated.
- Do not add automatic REST fallback for ACP chat failures.
- ACP errors should surface as ACP errors so parity gaps are fixed directly.

This avoids keeping REST response objects and ACP response objects alive at the
same call site.

## One PR Plan

The migration can land in one PR, but it should still be structured internally as
clear phases.

### 1. Harden ACP Client

Detailed plan: `01-harden-acp-client.md`

Update `ui/desktop/src/acp/acpConnection.ts` so ACP session notifications and
permission requests have explicit integration points.

### 2. Add ACP Session API Wrapper

Detailed plan: `02-acp-session-wrapper.md`

Add `ui/desktop/src/acp/sessions.ts` with desktop-facing wrappers for ACP
session methods.

### 3. Add ACP Notification Adapter

Detailed plan: `03-notification-adapter.md`

Add a protocol adapter that translates ACP `session/update` notifications into
the current desktop chat state model.

### 4. Migrate Session Creation

Detailed plan: `04-session-creation.md`

Update `ui/desktop/src/sessions.ts` so new sessions use ACP `session/new`.

Before switching, check whether the current REST creation behavior needs parity
for:

- working directory
- recipe deeplinks
- recipe IDs
- extension overrides

If any of these are required for the initial ACP cutover and ACP does not support
them yet, add the missing behavior to ACP first. Do not keep a hidden REST
fallback for only those cases unless the migration scope is intentionally reduced.

### 5. Migrate Conversation Load

Detailed plan: `05-conversation-load.md`

Replace REST `getSession`/`resumeAgent` loading with ACP `session/load`.

ACP `session/load` replays conversation content through `session/update`
notifications. The adapter should collect those updates and produce the same
message state the UI expects today.

Important behavior to preserve:

- loading state
- session load errors
- initial conversation display
- token state
- tool call history
- session name/info

### 6. Migrate Live Prompt Streaming

Detailed plan: `06-live-prompt-streaming.md`

Replace REST `sessionReply` plus `useSessionEvents` with ACP `session/prompt`.

The new flow should be:

```text
user submits message
  -> call ACP session/prompt
  -> receive session/update notifications on the WebSocket
  -> adapter updates Message[] / TokenState / ChatState
  -> prompt response resolves with final stop reason
  -> mark stream finished
```

Do not carry over REST request ID routing. ACP session notifications are scoped
by ACP `sessionId`.

### 7. Migrate Cancellation

Detailed plan: `07-cancellation.md`

Replace REST `sessionCancel` with ACP `session/cancel`.

Keep the existing stop button behavior:

- user can cancel an active prompt
- UI returns to idle or cancelled state
- no stale active request state remains

### 8. Wire Tool Permission Requests

Detailed plan: `08-tool-permissions.md`

ACP tool approval uses `requestPermission`, not the REST
`/action-required/tool-confirmation` path.

The ACP client callback should bridge permission requests into the existing UI
approval pattern, then return the selected ACP permission outcome.

Minimum acceptable behavior for live chat:

- permission request is visible to the user
- approve/reject maps to ACP response options
- cancellation maps to ACP cancelled outcome

### 9. Remove Desktop REST Session Usage

Detailed plan: `09-rest-cleanup.md`

After the ACP flow works, remove unused desktop REST session imports and code
paths.

Keep unrelated REST APIs untouched.

Do not manually edit `ui/desktop/openapi.json`.

## Main Risks

### Message Conversion

The highest-risk part is converting ACP session updates into the current desktop
message model. The raw ACP method calls are comparatively small.

### Tool Permission

If `requestPermission` is not wired, tool calls that require approval will fail
or cancel. This should be treated as a blocker for live ACP chat.

### Recipe and Extension Override Parity

Current REST session creation supports recipe and extension override inputs. ACP
session creation may need backend additions before it can fully replace REST
creation for all desktop entry points.

### Reattach Semantics

The REST path has `ActiveRequests` and request ID based reattach logic. ACP uses
the session-scoped WebSocket notification stream instead. If reattach after view
remount is required, design it around ACP session state rather than preserving
REST request IDs.

### Session Name Refresh

The REST path refreshes session names with `getSession` after early replies. ACP
should prefer `session_info_update` or another ACP-backed session metadata path.

## Suggested Verification

Manual checks:

- Create a new session.
- Load an existing session with text, thinking, and tool calls.
- Send a prompt and receive streamed assistant text.
- Run a prompt that triggers tool approval.
- Approve and reject a tool request.
- Cancel a running prompt.
- Navigate away from and back to an active or recently completed session.
- Confirm session name updates still appear.

Automated checks:

- Unit test the ACP notification adapter with representative update sequences.
- Add tests for text chunk accumulation.
- Add tests for tool call and tool call update conversion.
- Add tests for usage update conversion.
- Add tests for permission request outcome mapping.

## Review Boundary

The PR should present one clear architecture change:

```text
Desktop session/chat runtime moves from REST/SSE to ACP WebSocket.
ACP adapter owns protocol translation.
Existing UI components continue to consume desktop chat state.
```

Keep UI component changes minimal where possible. Most complexity should live in
the ACP session wrapper and notification adapter.
