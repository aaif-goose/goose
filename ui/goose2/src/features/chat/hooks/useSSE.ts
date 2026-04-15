import { useEffect, useRef, useState, useCallback } from "react";

interface UseSSEOptions {
  onMessage?: (event: MessageEvent) => void;
  onError?: (error: Event) => void;
  onOpen?: () => void;
  enabled?: boolean;
}

interface UseSSEReturn {
  close: () => void;
  readyState: number;
}

/**
 * Hook for managing an EventSource (SSE) connection lifecycle.
 *
 * Automatically connects when `enabled` is true (default) and disconnects
 * on unmount or when `enabled` becomes false.
 */
export function useSSE(url: string, options?: UseSSEOptions): UseSSEReturn {
  const { onMessage, onError, onOpen, enabled = true } = options ?? {};
  const sourceRef = useRef<EventSource | null>(null);
  const [readyState, setReadyState] = useState<number>(EventSource.CLOSED);

  // Stable refs for callbacks to avoid reconnecting when handlers change
  const onMessageRef = useRef(onMessage);
  onMessageRef.current = onMessage;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const onOpenRef = useRef(onOpen);
  onOpenRef.current = onOpen;

  const close = useCallback(() => {
    if (sourceRef.current) {
      sourceRef.current.close();
      sourceRef.current = null;
      setReadyState(EventSource.CLOSED);
    }
  }, []);

  useEffect(() => {
    if (!enabled || !url) {
      close();
      return;
    }

    const source = new EventSource(url);
    sourceRef.current = source;

    source.onopen = () => {
      setReadyState(EventSource.OPEN);
      onOpenRef.current?.();
    };

    source.onmessage = (event) => {
      onMessageRef.current?.(event);
    };

    source.onerror = (event) => {
      setReadyState(source.readyState);
      onErrorRef.current?.(event);
    };

    setReadyState(EventSource.CONNECTING);

    return () => {
      source.close();
      sourceRef.current = null;
    };
  }, [url, enabled, close]);

  return { close, readyState };
}
