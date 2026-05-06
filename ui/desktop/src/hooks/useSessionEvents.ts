import { useEffect, useRef, useState, useCallback } from 'react';
import { sessionEvents, type MessageEvent } from '../api';

/**
 * An SSE event with an optional request_id (added by the server at the
 * SSE framing layer, not part of the generated MessageEvent type).
 */
export type SessionEvent = MessageEvent & {
  request_id?: string;
  /** Chat-level request UUID used for routing events to the correct handler. */
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
      // The reconnect loop runs for the lifetime of the hook and keeps
      // retrying indefinitely so transient disconnects (App Nap, sleep/wake,
      // network blips) recover via the SSE `Last-Event-ID` replay protocol
      // without surfacing a synthetic terminal error (issue #8717).
      //
      // To prevent in-flight chat requests from getting stuck forever when
      // the connection truly cannot recover (e.g. `goosed` crashed or the
      // network stays offline well past any plausible App Nap duration),
      // a scoped fallback synthesises an `Error` event for any active
      // listeners after `TERMINAL_ERROR_AFTER_MS` of continuous failure.
      // `useChatStream`'s event processor maps that to `STREAM_FINISH`,
      // unblocking new submits while the loop continues to retry in the
      // background.
      let retryDelay = 500;
      const MAX_RETRY_DELAY = 10_000;
      // Long enough to absorb typical App Nap / sleep-wake durations, short
      // enough that a stuck chat unblocks within a reasonable user wait.
      const TERMINAL_ERROR_AFTER_MS = 5 * 60 * 1000;
      let lastEventId: string | undefined;
      let failureStreakStartedAt: number | null = null;

      // Synthesise a terminal `Error` event for every active listener once
      // the failure streak has lasted long enough that the outage is no
      // longer plausibly transient. Resets the streak timer afterwards so a
      // continuing outage will fire again for any requests submitted after
      // the previous broadcast (instead of leaving them stuck indefinitely).
      const broadcastTerminalErrorIfStuck = () => {
        if (failureStreakStartedAt === null) return;
        if (Date.now() - failureStreakStartedAt < TERMINAL_ERROR_AFTER_MS) return;
        if (listenersRef.current.size === 0) {
          // No active listeners means no in-flight request is waiting on
          // events; nothing to unblock. Reset so the next broadcast window
          // starts fresh whenever a request is actually submitted.
          failureStreakStartedAt = Date.now();
          return;
        }

        const errorEvent: SessionEvent = {
          type: 'Error',
          error: 'Lost connection to server',
        } as SessionEvent;
        for (const [id, handlers] of listenersRef.current) {
          // Snapshot the handler set: terminal-event handlers in
          // `useChatStream` unsubscribe themselves and would otherwise
          // mutate the set during iteration.
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
            // Disable the inner retry loop so errors surface to our outer
            // loop, which handles backoff.
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

            // Only mark as connected after the first real event arrives,
            // since the HTTP request doesn't happen until iteration starts.
            if (!receivedEvent) {
              receivedEvent = true;
              setConnected(true);
              retryDelay = 500;
              failureStreakStartedAt = null;
            }

            // The server adds chat_request_id (the chat UUID) and request_id
            // to the JSON at the SSE framing layer. Route using chat_request_id
            // so that Notification events (which carry their own MCP tool-call
            // request_id) still reach the correct handler.
            const sessionEvent = event as SessionEvent;
            const routingId = sessionEvent.chat_request_id ?? sessionEvent.request_id;

            // ActiveRequests events notify the client about in-flight requests
            // it can reattach to (e.g. after a remount).
            if (sessionEvent.type === 'ActiveRequests') {
              const ids = (sessionEvent as unknown as { request_ids: string[] }).request_ids;
              activeRequestsHandlerRef.current?.(ids);
              continue;
            }

            // Server-level errors without a request ID (e.g. "client too far
            // behind") affect all active listeners — broadcast to everyone.
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

          // Stream ended. Reconnect unless we were intentionally aborted.
          if (abortController.signal.aborted) break;
          setConnected(false);

          // If the stream ended without delivering any events, the connection
          // likely failed silently (e.g. 404 with sseMaxRetryAttempts: 1).
          // Back off and reconnect; only synthesise a terminal Error after a
          // long, continuous failure streak (see `broadcastTerminalErrorIfStuck`)
          // so transient blips do not regress issue #8717.
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

          // Back off before retrying. We keep reconnecting forever so idle
          // windows transparently resume their SSE stream when the renderer
          // wakes from suspension; the scoped terminal fallback above is the
          // only mechanism that surfaces a stream error to listeners.
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
