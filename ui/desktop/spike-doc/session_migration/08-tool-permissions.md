# 08 Tool Permissions

## Goal

Wire ACP `requestPermission` into the existing desktop tool approval UX.

## Files

- `ui/desktop/src/acp/acpConnection.ts`
- `ui/desktop/src/acp/sessionNotificationAdapter.ts`
- `ui/desktop/src/hooks/useChatStream.ts`
- Existing tool approval UI files as needed

## Implementation Steps

1. Define a permission request state shape that the UI can render:

   - session ID
   - tool call ID
   - tool name/title
   - arguments or display content
   - available ACP permission options

2. In `acpConnection.ts`, have `requestPermission` call the registered
   permission handler and await the user's decision.

3. Bridge the permission handler into the current chat UI approval mechanism.
   If the current approval UI is tightly coupled to REST
   `actionRequired/toolConfirmation` messages, adapt ACP permission requests into
   the same display model at the adapter boundary.

4. Map user decisions back to ACP:

   - approve once -> selected ACP allow option
   - approve always -> selected ACP allow-always option, if provided
   - reject once -> selected ACP reject option
   - reject always -> selected ACP reject-always option, if provided
   - cancel/dismiss -> ACP cancelled outcome

5. Do not assume fixed option IDs unless the backend guarantees them. Prefer
   choosing from `args.options`.

6. If the UI offers fewer choices than ACP provides, map to the safest matching
   option and document that limitation in `progress.md` follow-up.

## Completion Criteria

- Permission request is visible to the user.
- Approve/reject maps to ACP response options.
- Cancellation maps to ACP cancelled outcome.
- Approved tool calls continue.
- Rejected tool calls are handled cleanly.

## Risks

- ACP permission options may not match the current desktop approval button set.
- Multiple permission requests may overlap.
- Returning the wrong option ID can approve/reject the wrong scope.
- Permission requests may arrive before the visible chat UI is ready to display
  them.

### Permission Handler Readiness

ACP `requestPermission` is a client request from the backend to the desktop UI.
The backend waits for a `RequestPermissionResponse` before it can continue the
tool call. If a permission request arrives before the chat UI has registered a
permission handler, Step 1's temporary fallback returns `cancelled`. That is
acceptable before live ACP chat is enabled, but it would become a user-visible
failure after migration.

Likely symptoms:

- a tool call is cancelled immediately without showing an approval prompt
- the assistant reports that permission was denied or unavailable
- a background session requiring approval stalls or fails silently
- the request appears in logs but no modal/inline approval UI appears

Mitigation:

- register a durable app-level permission bridge before `session/prompt` can
  trigger tool calls
- route permission requests by `sessionId`, similar to session notifications
- keep pending permission requests in shared state so they can be displayed when
  the relevant chat view mounts or becomes visible
- avoid clearing the global permission handler during individual chat unmounts
- only return `cancelled` when the user cancels, the prompt turn is cancelled,
  or no UI route can reasonably surface the request
- add tests for requests arriving before the chat component is mounted and for
  requests targeting a hidden/background session

### Permission Request Lifetime

Permission requests can outlive the exact component instance that was visible
when the tool call started. Navigation, session switching, cancellation, or
window focus changes should not orphan the pending decision.

Likely symptoms:

- approval buttons disappear while the backend is still waiting
- approving after navigation resolves the wrong request
- stale approval UI remains after cancellation

Mitigation:

- key pending permission state by ACP session ID and tool call ID
- make resolve/reject paths idempotent
- clear pending requests when the matching ACP prompt is cancelled or completes
- ensure hidden sessions can surface an unread/needs-attention state in
  navigation
