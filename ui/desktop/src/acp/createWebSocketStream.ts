import type { Stream } from '@aaif/goose-sdk';

export function createWebSocketStream(wsUrl: string): Stream {
  const ws = new window.WebSocket(wsUrl);

  const incoming: unknown[] = [];
  const waiters: Array<() => void> = [];
  let closed = false;

  function pushMessage(message: unknown): void {
    incoming.push(message);
    waiters.shift()?.();
  }

  function waitForMessage(): Promise<void> {
    if (incoming.length > 0 || closed) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => waiters.push(resolve));
  }

  const openPromise = new Promise<void>((resolve, reject) => {
    ws.addEventListener('open', () => resolve(), { once: true });
    ws.addEventListener('error', () => reject(new Error('ACP WebSocket connection failed')), {
      once: true,
    });
  });

  ws.addEventListener('message', (event) => {
    if (typeof event.data !== 'string') {
      return;
    }
    try {
      pushMessage(JSON.parse(event.data));
    } catch {
      // Ignore malformed messages from the transport.
    }
  });

  const closeWaiters = () => {
    closed = true;
    for (const waiter of waiters) {
      waiter();
    }
    waiters.length = 0;
  };

  ws.addEventListener('close', closeWaiters);
  ws.addEventListener('error', closeWaiters);

  const readable = new window.ReadableStream({
    async pull(controller) {
      await waitForMessage();
      while (incoming.length > 0) {
        controller.enqueue(incoming.shift());
      }
      if (closed && incoming.length === 0) {
        controller.close();
      }
    },
  });

  const writable = new window.WritableStream({
    async write(message) {
      await openPromise;
      ws.send(JSON.stringify(message));
    },
    close() {
      ws.close();
    },
    abort() {
      ws.close();
    },
  });

  return { readable, writable } as Stream;
}
