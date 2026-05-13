import { useEffect, useRef, useState, useCallback } from 'react';
import { sessionEvents, type MessageEvent } from '../api';

/**
 * An SSE event with an optional request_id (added by the server at the
 * SSE framing layer, not part of the generated MessageEvent type).
 */
export type SessionEvent = MessageEvent & {
  request_id?: string;
  chat_request_id?: string;
};

type EventHandler = (event: SessionEvent) => void;
type ActiveRequestsHandler = (requestIds: string[]) => void;

export function useSessionEvents(sessionId: string) {
  const listenersRef = useRef(new Map<string, Set<EventHandler>>());
  const activeRequestsHandlerRef = useRef<ActiveRequestsHandler | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    if (!sessionId) return;

    const abortController = new AbortController();
    abortRef.current = abortController;

    (async () => {
      let retryDelay = 500;
      const MAX_RETRY_DELAY = 10_000;
      const TERMINAL_ERROR_AFTER_MS = 5 * 60 * 1000;
      let lastEventId: string | undefined;
      let failureStreakStartedAt: number | null = null;

      const broadcastTerminalErrorIfStuck = () => {
        if (failureStreakStartedAt === null) return;
        if (Date.now() - failureStreakStartedAt < TERMINAL_ERROR_AFTER_MS) return;
        if (listenersRef.current.size === 0) {
          failureStreakStartedAt = Date.now();
          return;
        }

        const errorEvent: SessionEvent = {
          type: 'Error',
          error: 'Lost connection to server',
        } as SessionEvent;
        for (const [id, handlers] of listenersRef.current) {
          for (const handler of [...handlers]) {
            handler({ ...errorEvent, request_id: id, chat_request_id: id });
          }
        }
        failureStreakStartedAt = Date.now();
      };

      while (!abortController.signal.aborted) {
        try {
          const { stream } = await sessionEvents({
            path: { id: sessionId },
            signal: abortController.signal,
            headers: lastEventId ? { 'Last-Event-ID': lastEventId } : undefined,
            sseMaxRetryAttempts: 1,
            onSseEvent: (event) => {
              if (event.id) {
                lastEventId = event.id;
              }
            },
          });

          let receivedEvent = false;

          for await (const event of stream) {
            if (abortController.signal.aborted) break;

            if (!receivedEvent) {
              receivedEvent = true;
              setConnected(true);
              retryDelay = 500;
              failureStreakStartedAt = null;
            }

            const sessionEvent = event as SessionEvent;
            const routingId = sessionEvent.chat_request_id ?? sessionEvent.request_id;

            if (sessionEvent.type === 'ActiveRequests') {
              const ids = (sessionEvent as unknown as { request_ids: string[] }).request_ids;
              activeRequestsHandlerRef.current?.(ids);
              continue;
            }

            if (!routingId && sessionEvent.type === 'Error') {
              for (const [id, handlers] of listenersRef.current) {
                for (const handler of handlers) {
                  handler({ ...sessionEvent, request_id: id, chat_request_id: id });
                }
              }
            } else if (routingId) {
              const handlers = listenersRef.current.get(routingId);
              if (handlers) {
                for (const handler of handlers) {
                  handler(sessionEvent);
                }
              }
            }
          }

          if (abortController.signal.aborted) break;
          setConnected(false);

          if (!receivedEvent) {
            if (failureStreakStartedAt === null) failureStreakStartedAt = Date.now();
            broadcastTerminalErrorIfStuck();
            await new Promise((r) => setTimeout(r, retryDelay));
            retryDelay = Math.min(retryDelay * 2, MAX_RETRY_DELAY);
          }
        } catch (error) {
          if (abortController.signal.aborted) break;
          console.warn('SSE connection error, reconnecting:', error);
          setConnected(false);

          if (failureStreakStartedAt === null) failureStreakStartedAt = Date.now();
          broadcastTerminalErrorIfStuck();
          await new Promise((r) => setTimeout(r, retryDelay));
          retryDelay = Math.min(retryDelay * 2, MAX_RETRY_DELAY);
        }
      }

      setConnected(false);
    })();

    const listeners = listenersRef.current;
    return () => {
      abortController.abort();
      abortRef.current = null;
      listeners.clear();
      setConnected(false);
    };
  }, [sessionId]);

  const addListener = useCallback((requestId: string, handler: EventHandler): (() => void) => {
    if (!listenersRef.current.has(requestId)) {
      listenersRef.current.set(requestId, new Set());
    }
    listenersRef.current.get(requestId)!.add(handler);

    return () => {
      const set = listenersRef.current.get(requestId);
      if (set) {
        set.delete(handler);
        if (set.size === 0) {
          listenersRef.current.delete(requestId);
        }
      }
    };
  }, []);

  const setActiveRequestsHandler = useCallback((handler: ActiveRequestsHandler | null) => {
    activeRequestsHandlerRef.current = handler;
  }, []);

  return { connected, addListener, setActiveRequestsHandler };
}
