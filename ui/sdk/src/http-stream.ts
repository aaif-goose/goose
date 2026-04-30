import type { AnyMessage, Stream } from "@agentclientprotocol/sdk";

const ACP_CONNECTION_HEADER = "Acp-Connection-Id";
const ACP_SESSION_HEADER = "Acp-Session-Id";

// Enable via `globalThis.ACP_DEBUG = true`, `localStorage.ACP_DEBUG = "1"`,
// or `ACP_DEBUG=1` in the environment.
function acpDebug(label: string, payload: unknown): void {
  const g = globalThis as {
    ACP_DEBUG?: unknown;
    localStorage?: { getItem?: (k: string) => string | null };
    process?: { env?: Record<string, string | undefined> };
  };
  const on =
    g.ACP_DEBUG === true ||
    g.ACP_DEBUG === "1" ||
    !!g.localStorage?.getItem?.("ACP_DEBUG") ||
    !!g.process?.env?.ACP_DEBUG;
  if (!on) return;
  // eslint-disable-next-line no-console
  console.debug(`[acp] ${label}`, payload);
}

// Methods that are scoped to a session and require an Acp-Session-Id header.
const SESSION_SCOPED_METHODS = new Set<string>([
  "session/prompt",
  "session/cancel",
  "session/load",
  "session/set_mode",
  "session/set_model",
]);

function messageMethod(msg: AnyMessage): string | null {
  const m = msg as { method?: unknown };
  return typeof m.method === "string" ? m.method : null;
}

function messageParams(msg: AnyMessage): unknown {
  return (msg as { params?: unknown }).params;
}

function isRequest(msg: AnyMessage): boolean {
  const m = msg as { method?: unknown; id?: unknown };
  return typeof m.method === "string" && m.id !== undefined && m.id !== null;
}

function isNotification(msg: AnyMessage): boolean {
  const m = msg as { method?: unknown; id?: unknown };
  return typeof m.method === "string" && (m.id === undefined || m.id === null);
}

function extractSessionId(value: unknown): string | null {
  if (value && typeof value === "object" && "sessionId" in value) {
    const sid = (value as { sessionId?: unknown }).sessionId;
    if (typeof sid === "string") return sid;
  }
  return null;
}

/**
 * Create a Stream that speaks the Streamable HTTP ACP transport.
 *
 * Protocol summary:
 * - The first outbound message must be an `initialize` request, sent as a
 *   regular POST. The server responds synchronously with 200 OK, a JSON body
 *   containing the initialize response, and an `Acp-Connection-Id` header.
 * - After initialize, we open a single long-lived GET SSE stream carrying
 *   all server → client messages (responses, notifications, server-initiated
 *   requests) for every session on the connection.
 * - All subsequent POSTs carry `Acp-Connection-Id` and return 202 Accepted.
 *   Session-scoped methods must also carry `Acp-Session-Id`.
 * - On close we send DELETE /acp with the connection header.
 */
export function createHttpStream(serverUrl: string): Stream {
  const base = serverUrl.replace(/\/+$/, "");
  const endpoint = `${base}/acp`;

  let connectionId: string | null = null;
  let getStreamAbort: AbortController | null = null;
  let closed = false;

  // Readable-stream plumbing: enqueue-with-buffer until the consumer pulls.
  const inbox: AnyMessage[] = [];
  let pullResolve: (() => void) | null = null;

  function deliver(msg: AnyMessage) {
    inbox.push(msg);
    if (pullResolve) {
      const r = pullResolve;
      pullResolve = null;
      r();
    }
  }

  function waitForInbox(): Promise<void> {
    if (inbox.length > 0) return Promise.resolve();
    return new Promise<void>((r) => {
      pullResolve = r;
    });
  }

  async function openGetStream() {
    if (!connectionId) return;
    getStreamAbort = new AbortController();

    const response = await fetch(endpoint, {
      method: "GET",
      headers: {
        Accept: "text/event-stream",
        [ACP_CONNECTION_HEADER]: connectionId,
      },
      signal: getStreamAbort.signal,
    });

    if (!response.ok || !response.body) {
      throw new Error(
        `Failed to open ACP GET stream: ${response.status} ${response.statusText}`,
      );
    }

    void consumeSSE(response.body).catch((err) => {
      if (closed) return;
      // eslint-disable-next-line no-console
      console.error("ACP GET stream error:", err);
    });
  }

  async function consumeSSE(body: ReadableStream<Uint8Array>) {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        let idx: number;
        while ((idx = buffer.indexOf("\n\n")) >= 0) {
          const event = buffer.slice(0, idx);
          buffer = buffer.slice(idx + 2);
          handleSseEvent(event);
        }
      }
      if (buffer.length > 0) handleSseEvent(buffer);
    } catch (e: unknown) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      throw e;
    }
  }

  function handleSseEvent(event: string) {
    const dataLines: string[] = [];
    for (const line of event.split("\n")) {
      if (line.startsWith("data:")) {
        dataLines.push(line.slice(5).replace(/^ /, ""));
      }
    }
    if (dataLines.length === 0) return;
    const data = dataLines.join("\n");
    let msg: AnyMessage;
    try {
      msg = JSON.parse(data) as AnyMessage;
    } catch {
      return;
    }

    acpDebug("SSE → client", msg);
    deliver(msg);
  }

  async function sendInitialize(msg: AnyMessage) {
    acpDebug("initialize → agent", msg);
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify(msg),
    });

    if (!response.ok) {
      throw new Error(
        `ACP initialize failed: ${response.status} ${response.statusText}`,
      );
    }

    const connId = response.headers.get(ACP_CONNECTION_HEADER);
    if (!connId) {
      throw new Error(
        `ACP initialize response missing ${ACP_CONNECTION_HEADER} header`,
      );
    }
    connectionId = connId;

    const body = (await response.json()) as AnyMessage;
    acpDebug("initialize response", body);
    await openGetStream();
    // Deliver the initialize response to the SDK *after* the GET stream is
    // up, so any immediate server-initiated messages won't be missed.
    deliver(body);
  }

  async function sendPost(msg: AnyMessage) {
    if (!connectionId) {
      throw new Error("ACP POST attempted before initialize");
    }

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json",
      [ACP_CONNECTION_HEADER]: connectionId,
    };

    if (isRequest(msg) || isNotification(msg)) {
      const sid = extractSessionId(messageParams(msg));
      if (sid) {
        headers[ACP_SESSION_HEADER] = sid;
      } else if (isRequest(msg)) {
        const method = messageMethod(msg);
        if (method && SESSION_SCOPED_METHODS.has(method)) {
          throw new Error(`ACP method ${method} requires sessionId in params`);
        }
      }
    }

    acpDebug("POST → agent", msg);
    const response = await fetch(endpoint, {
      method: "POST",
      headers,
      body: JSON.stringify(msg),
    });

    if (response.status !== 202 && !response.ok) {
      throw new Error(
        `ACP POST failed: ${response.status} ${response.statusText}`,
      );
    }
    // Drain the body so the connection can be reused.
    await response.arrayBuffer().catch(() => undefined);
  }

  async function sendDelete() {
    if (!connectionId) return;
    try {
      await fetch(endpoint, {
        method: "DELETE",
        headers: { [ACP_CONNECTION_HEADER]: connectionId },
      });
    } catch {
      // best-effort
    }
  }

  const readable = new ReadableStream<AnyMessage>({
    async pull(controller) {
      await waitForInbox();
      while (inbox.length > 0) {
        controller.enqueue(inbox.shift()!);
      }
      if (closed && inbox.length === 0) {
        controller.close();
      }
    },
    async cancel() {
      closed = true;
      await sendDelete();
      getStreamAbort?.abort();
      if (pullResolve) {
        const r = pullResolve;
        pullResolve = null;
        r();
      }
    },
  });

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      if (
        !connectionId &&
        isRequest(msg) &&
        messageMethod(msg) === "initialize"
      ) {
        await sendInitialize(msg);
        return;
      }
      if (!connectionId) {
        throw new Error(
          "ACP transport: first outgoing message must be `initialize`",
        );
      }
      await sendPost(msg);
    },
    async close() {
      closed = true;
      await sendDelete();
      getStreamAbort?.abort();
      // Unblock any pending pull so the readable can close.
      if (pullResolve) {
        const r = pullResolve;
        pullResolve = null;
        r();
      }
    },
    async abort() {
      closed = true;
      await sendDelete();
      getStreamAbort?.abort();
      if (pullResolve) {
        const r = pullResolve;
        pullResolve = null;
        r();
      }
    },
  });

  return { readable, writable };
}
