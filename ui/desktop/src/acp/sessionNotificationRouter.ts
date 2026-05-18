import type { SessionNotification } from '@agentclientprotocol/sdk';
import { type AcpNotificationHandler, setAcpNotificationHandler } from './acpConnection';

export type AcpSessionNotificationListener = (
  notification: SessionNotification
) => Promise<void> | void;

export interface AcpSessionNotificationRouter {
  handler: AcpNotificationHandler;
  route(notification: SessionNotification): Promise<void>;
  subscribe(sessionId: string, listener: AcpSessionNotificationListener): () => void;
}

export function createAcpSessionNotificationRouter(): AcpSessionNotificationRouter {
  const listenersBySessionId = new Map<string, Set<AcpSessionNotificationListener>>();

  const route = async (notification: SessionNotification): Promise<void> => {
    const listeners = listenersBySessionId.get(notification.sessionId);
    if (!listeners) {
      return;
    }

    await Promise.all([...listeners].map((listener) => listener(notification)));
  };

  const subscribe = (sessionId: string, listener: AcpSessionNotificationListener): (() => void) => {
    const listeners = listenersBySessionId.get(sessionId) ?? new Set();
    listenersBySessionId.set(sessionId, listeners);
    listeners.add(listener);

    let unsubscribed = false;

    return () => {
      if (unsubscribed) {
        return;
      }

      unsubscribed = true;
      const currentListeners = listenersBySessionId.get(sessionId);
      currentListeners?.delete(listener);

      if (currentListeners?.size === 0) {
        listenersBySessionId.delete(sessionId);
      }
    };
  };

  return {
    handler: {
      handleSessionNotification: route,
    },
    route,
    subscribe,
  };
}

const acpSessionNotificationRouter = createAcpSessionNotificationRouter();
let installed = false;

export function installAcpSessionNotificationRouter(): void {
  if (installed) {
    return;
  }

  setAcpNotificationHandler(acpSessionNotificationRouter.handler);
  installed = true;
}

export function subscribeToAcpSession(
  sessionId: string,
  listener: AcpSessionNotificationListener
): () => void {
  return acpSessionNotificationRouter.subscribe(sessionId, listener);
}
