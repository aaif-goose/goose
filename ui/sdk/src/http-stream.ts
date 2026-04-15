import type { AnyMessage, Stream } from "@agentclientprotocol/sdk";

const ACP_SESSION_HEADER = "Acp-Session-Id";

export function createHttpStream(serverUrl: string): Stream {
  let sessionId: string | null = null;
  const incoming: AnyMessage[] = [];
  const waiters: Array<() => void> = [];
  const sseAbort = new AbortController();

  function pushMessage(msg: AnyMessage) {
    incoming.push(msg);
    const w = waiters.shift();
    if (w) w();
  }

  function waitForMessage(): Promise<void> {
    if (incoming.length > 0) return Promise.resolve();
    return new Promise<void>((r) => waiters.push(r));
  }

  async function consumeSSE(response: Response) {
    if (!response.body) return;
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const parts = buffer.split("\n\n");
        buffer = parts.pop() || "";

        for (const part of parts) {
          for (const line of part.split("\n")) {
            if (line.startsWith("data: ")) {
              try {
                const msg = JSON.parse(line.slice(6)) as AnyMessage;
                pushMessage(msg);
              } catch {
                // ignore malformed JSON
              }
            }
          }
        }
      }
    } catch (e: unknown) {
      if (e instanceof DOMException && e.name === "AbortError") return;
    }
  }

  let isFirstRequest = true;

  const readable = new ReadableStream<AnyMessage>({
    async pull(controller) {
      await waitForMessage();
      while (incoming.length > 0) {
        controller.enqueue(incoming.shift()!);
      }
    },
  });

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      const isRequest =
        "method" in msg &&
        "id" in msg &&
        msg.id !== undefined &&
        msg.id !== null;

      const headers: Record<string, string> = {
        "Content-Type": "application/json",
        Accept: "application/json, text/event-stream",
      };
      if (sessionId) {
        headers[ACP_SESSION_HEADER] = sessionId;
      }

      if (isFirstRequest && isRequest) {
        isFirstRequest = false;

        const response = await fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
          signal: sseAbort.signal,
        });

        const sid = response.headers.get(ACP_SESSION_HEADER);
        if (sid) sessionId = sid;

        consumeSSE(response);
      } else if (isRequest) {
        const abort = new AbortController();
        fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
          signal: abort.signal,
        }).catch(() => {});
        setTimeout(() => abort.abort(), 200);
      } else {
        await fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
        });
      }
    },

    close() {
      sseAbort.abort();
    },
  });

  return { readable, writable };
}
