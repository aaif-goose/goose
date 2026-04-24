import type { AnyMessage, Stream } from "@agentclientprotocol/sdk";

export function createWebSocketStream(wsUrl: string): Stream {
  const ws = new WebSocket(wsUrl);

  const incoming: AnyMessage[] = [];
  const waiters: Array<() => void> = [];
  let closed = false;

  function pushMessage(msg: AnyMessage): void {
    incoming.push(msg);
    const waiter = waiters.shift();
    if (waiter) waiter();
  }

  function waitForMessage(): Promise<void> {
    if (incoming.length > 0 || closed) return Promise.resolve();
    return new Promise<void>((resolve) => waiters.push(resolve));
  }

  const openPromise = new Promise<void>((resolve, reject) => {
    ws.addEventListener("open", () => resolve(), { once: true });
    ws.addEventListener(
      "error",
      (event) => {
        reject(new Error(`WebSocket connection failed: ${event}`));
      },
      { once: true },
    );
  });

  ws.addEventListener("message", (event) => {
    if (typeof event.data !== "string") return;
    try {
      const msg = JSON.parse(event.data) as AnyMessage;
      pushMessage(msg);
    } catch {
      // ignore malformed JSON
    }
  });

  ws.addEventListener("close", () => {
    closed = true;
    for (const waiter of waiters) waiter();
    waiters.length = 0;
  });

  ws.addEventListener("error", () => {
    closed = true;
    for (const waiter of waiters) waiter();
    waiters.length = 0;
  });

  const readable = new ReadableStream<AnyMessage>({
    async pull(controller) {
      await waitForMessage();
      while (incoming.length > 0) {
        // biome-ignore lint/style/noNonNullAssertion: length checked in while condition
        controller.enqueue(incoming.shift()!);
      }
      if (closed && incoming.length === 0) {
        controller.close();
      }
    },
  });

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      await openPromise;
      ws.send(JSON.stringify(msg));
    },
    close() {
      ws.close();
    },
    abort() {
      ws.close();
    },
  });

  return { readable, writable };
}
