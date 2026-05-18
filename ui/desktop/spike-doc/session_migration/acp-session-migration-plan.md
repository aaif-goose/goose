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

## Vertical Slice Plan

The migration can land in one PR, but implementation should proceed as vertical
slices. Each slice should prove one user-visible path end to end before adding
more ACP surface area.

Working agreement: `working-agreement.md`

This is intentionally different from building all wrappers first. ACP session
behavior is notification-driven, so a wrapper can typecheck without proving that
chat actually works. The first useful proof is a narrow `session/load` path that
exercises the wrapper, notification router, adapter, and React state together.

### 1. Harden ACP Client

Detailed plan: `01-harden-acp-client.md`

Update `ui/desktop/src/acp/acpConnection.ts` so ACP session notifications and
permission requests have explicit integration points.

### 2. Text-Only Conversation Load Slice

Detailed plan: `02-acp-session-wrapper.md`

Build the minimum ACP path needed to load an existing text-only session:

- add `loadAcpSession` in `ui/desktop/src/acp/sessions.ts`
- add the session-scoped ACP notification router
- add the minimal notification adapter for `user_message_chunk`,
  `agent_message_chunk`, and session metadata needed by load
- add server support for an initial `session_info_update` during
  `session/load`, so desktop has session metadata without depending on REST
  `resumeAgent.session`
- wire conversation load in the chat hook
- verify an existing text-only session renders through ACP

This slice should prove:

```text
React hook subscribes by sessionId
  -> calls ACP session/load
  -> ACP sends session/update: session_info_update
  -> ACP sends session/update: replayed message chunks
  -> ACP sends session/update: usage_update
  -> router delivers notifications to the matching session
  -> adapter converts metadata, usage, and text chunks
  -> UI renders messages
```

Do not make `useChatStream` depend on both `resumeAgent.session.conversation`
and ACP replay as competing conversation sources. REST may remain temporarily
for setup behavior that ACP does not yet expose, but conversation and session
metadata for the load slice should come from ACP notifications.

### 3. Live Text Prompt Slice

Detailed plans: `03-notification-adapter.md`, `06-live-prompt-streaming.md`

Add `promptAcpSession` and migrate the live text submit path. Reuse the router
and text adapter path proven by conversation load.

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

### 4. Cancellation Slice

Detailed plan: `07-cancellation.md`

Add `cancelAcpSession` and replace REST cancellation after live prompt state
exists.

Keep the existing stop button behavior:

- user can cancel an active prompt
- UI returns to idle or cancelled state
- no stale active request state remains

### 5. Tool Call Display Slice

Detailed plan: `03-notification-adapter.md`

Extend the notification adapter beyond text:

- `agent_thought_chunk`
- `tool_call`
- `tool_call_update`
- `usage_update`
- richer `session_info_update`

Verify loading and live prompt sessions with thinking and tool history.

### 6. Tool Permission Slice

Detailed plan: `08-tool-permissions.md`

ACP tool approval uses `requestPermission`, not the REST
`/action-required/tool-confirmation` path.

The ACP client callback should bridge permission requests into the existing UI
approval pattern, then return the selected ACP permission outcome.

Minimum acceptable behavior for live chat:

- permission request is visible to the user
- approve/reject maps to ACP response options
- cancellation maps to ACP cancelled outcome

### 7. Session Creation Slice

Detailed plan: `04-session-creation.md`

Add `createAcpSession` and update `ui/desktop/src/sessions.ts` so new sessions
use ACP `session/new`.

Session creation comes after load/prompt because it has extra parity risk.
Before switching, check whether the current REST creation behavior needs parity
for:

- working directory
- recipe deeplinks
- recipe IDs
- extension overrides

If any of these are required for the initial ACP cutover and ACP does not support
them yet, add the missing behavior to ACP first. Do not keep a hidden REST
fallback for only those cases unless the migration scope is intentionally reduced.

### 8. Optional Session List Slice

Detailed plan: `02-acp-session-wrapper.md`

Add `listAcpSessions()` only if session list migration is included in this PR.
Otherwise leave session list on REST until cleanup scope is explicit.

### 9. Remove Desktop REST Session Usage

Detailed plan: `09-rest-cleanup.md`

After the ACP flow works, remove unused desktop REST session imports and code
paths.

Keep unrelated REST APIs untouched.

Do not manually edit `ui/desktop/openapi.json`.

## Legacy Phase Docs

The detailed files still split work by subsystem because that is useful while
implementing. Use them inside the vertical slices above:

- `02-acp-session-wrapper.md` for the wrapper and router.
- `03-notification-adapter.md` for conversion logic.
- `04-session-creation.md` for the later creation slice.
- `05-conversation-load.md` for load integration details.
- `06-live-prompt-streaming.md` for prompt integration details.
- `07-cancellation.md` for cancellation.
- `08-tool-permissions.md` for permission bridging.
- `09-rest-cleanup.md` for cleanup after replacement behavior is proven.

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
