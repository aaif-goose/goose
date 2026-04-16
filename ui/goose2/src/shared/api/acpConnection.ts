import { invoke } from "@tauri-apps/api/core";
import { GooseClient } from "@aaif/goose-sdk";
import {
  PROTOCOL_VERSION,
  type Client,
  type SessionNotification,
  type RequestPermissionRequest,
  type RequestPermissionResponse,
} from "@agentclientprotocol/sdk";
import { createWebSocketStream } from "./createWebSocketStream";

let notificationHandler: AcpNotificationHandler | null = null;

export interface AcpNotificationHandler {
  handleSessionNotification(notification: SessionNotification): Promise<void>;
}

export function setNotificationHandler(handler: AcpNotificationHandler): void {
  notificationHandler = handler;
}

let clientPromise: Promise<GooseClient> | null = null;
let resolvedClient: GooseClient | null = null;

function createClientCallbacks(): () => Client {
  return () => ({
    requestPermission: async (
      args: RequestPermissionRequest,
    ): Promise<RequestPermissionResponse> => {
      const optionId = args.options?.[0]?.optionId ?? "approve";
      return {
        outcome: {
          outcome: "selected",
          optionId,
        },
      };
    },

    sessionUpdate: async (notification: SessionNotification): Promise<void> => {
      if (notificationHandler) {
        await notificationHandler.handleSessionNotification(notification);
      }
    },
  });
}

function monitorConnection(client: GooseClient): void {
  client.closed
    .then(() => {
      console.warn(
        "[acp] Connection closed. Will reconnect on next getClient().",
      );
      resolvedClient = null;
      clientPromise = null;
    })
    .catch(() => {
      console.warn(
        "[acp] Connection error. Will reconnect on next getClient().",
      );
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
    clientInfo: {
      name: "goose2",
      version: "0.1.0",
    },
  });

  monitorConnection(client);

  return client;
}

export async function getClient(): Promise<GooseClient> {
  if (resolvedClient) {
    return resolvedClient;
  }

  if (!clientPromise) {
    clientPromise = initializeConnection()
      .then((client) => {
        resolvedClient = client;
        return client;
      })
      .catch((error) => {
        clientPromise = null;
        throw error;
      });
  }

  return clientPromise;
}

export function isClientReady(): boolean {
  return resolvedClient !== null;
}

export function getClientSync(): GooseClient | null {
  return resolvedClient;
}
