import { invoke } from "@tauri-apps/api/core";
import { GooseClient } from "@aaif/goose-sdk";
import {
  PROTOCOL_VERSION,
  type Client,
  type RequestPermissionRequest,
  type RequestPermissionResponse,
  type SessionNotification,
} from "@agentclientprotocol/sdk";
import { createWebSocketStream } from "./createWebSocketStream";

export type NotificationHandler = (n: SessionNotification) => void | Promise<void>;

let notificationHandler: NotificationHandler | null = null;

export function setNotificationHandler(h: NotificationHandler): void {
  notificationHandler = h;
}

let clientPromise: Promise<GooseClient> | null = null;
let resolvedClient: GooseClient | null = null;

function createClientCallbacks(): () => Client {
  return () => ({
    requestPermission: async (
      args: RequestPermissionRequest,
    ): Promise<RequestPermissionResponse> => {
      // Auto-approve the first option until a real permission UI lands.
      const optionId = args.options?.[0]?.optionId ?? "approve";
      return { outcome: { outcome: "selected", optionId } };
    },

    sessionUpdate: async (n: SessionNotification): Promise<void> => {
      if (notificationHandler) await notificationHandler(n);
    },
  });
}

function monitorConnection(client: GooseClient): void {
  client.closed
    .then(() => {
      console.warn("[acp] connection closed; will reconnect on next getClient()");
      resolvedClient = null;
      clientPromise = null;
    })
    .catch(() => {
      console.warn("[acp] connection error; will reconnect on next getClient()");
      resolvedClient = null;
      clientPromise = null;
    });
}

async function initializeConnection(): Promise<GooseClient> {
  const wsUrl: string = await invoke("get_goose_serve_url");
  const stream = createWebSocketStream(wsUrl);
  const client = new GooseClient(createClientCallbacks(), stream);

  await client.initialize({
    protocolVersion: PROTOCOL_VERSION,
    clientCapabilities: {},
    clientInfo: { name: "tandem", version: "0.1.0" },
  });

  monitorConnection(client);
  return client;
}

export async function getClient(): Promise<GooseClient> {
  if (resolvedClient) return resolvedClient;
  if (!clientPromise) {
    clientPromise = initializeConnection()
      .then((c) => {
        resolvedClient = c;
        return c;
      })
      .catch((err) => {
        clientPromise = null;
        throw err;
      });
  }
  return clientPromise;
}
