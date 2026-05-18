# 01 Harden ACP Client

## Goal

Update `ui/desktop/src/acp/acpConnection.ts` so ACP session notifications and
permission requests have explicit integration points.

## Files

- `ui/desktop/src/acp/acpConnection.ts`

## Implementation Steps

1. Add an `AcpNotificationHandler` interface:

   ```ts
   export interface AcpNotificationHandler {
     handleSessionNotification(notification: SessionNotification): Promise<void>;
   }
   ```

2. Add module-level handler state:

   ```ts
   let notificationHandler: AcpNotificationHandler | null = null;
   ```

3. Export a setter:

   ```ts
   export function setAcpNotificationHandler(handler: AcpNotificationHandler | null): void;
   ```

4. Update `createClientCallbacks()` so `sessionUpdate` forwards every
   `SessionNotification` to the current handler.

5. Add a permission bridge instead of returning `cancelled` unconditionally.
   The bridge can start as a minimal module-level callback:

   ```ts
   export function setAcpPermissionHandler(handler: AcpPermissionHandler | null): void;
   ```

6. If no permission handler is registered, return `cancelled` with an explicit
   warning. That fallback is acceptable only before live ACP chat is enabled.

7. Keep reconnect behavior as-is: when `client.closed` resolves or rejects,
   clear `resolvedClient` and `clientPromise` so the next API call reconnects.

## Completion Criteria

- ACP connection initialization remains shared by all ACP APIs.
- Session notifications can be consumed by chat state code.
- Permission requests have a deliberate integration point.
- `requestPermission` is not silently auto-cancelling once live ACP chat is
  enabled.

## Risks

- Dropping notifications before the chat hook registers a handler.
- Multiple active sessions competing for one global notification handler.
- Permission requests arriving before the UI is ready to display them.
