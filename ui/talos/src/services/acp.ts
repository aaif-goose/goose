import type { ContentBlock } from "@agentclientprotocol/sdk";
import { getClient, setNotificationHandler, type NotificationHandler } from "./acpConnection";

const DEFAULT_WORKING_DIR = "~/.goose/artifacts";

/** Warm the singleton ACP client and register the session-notification handler. */
export async function initAcp(handler: NotificationHandler): Promise<void> {
  setNotificationHandler(handler);
  await getClient();
}

/**
 * Start a new Goose session. Returns the gooseSessionId which the caller
 * must pass to subsequent sendMessage / cancel calls.
 */
export async function startSession(workingDir: string = DEFAULT_WORKING_DIR): Promise<string> {
  const client = await getClient();
  const response = await client.newSession({ cwd: workingDir, mcpServers: [] });
  return response.sessionId;
}

/**
 * Send a user message to an existing session. Streaming happens via the
 * notification handler registered in `initAcp`. The returned promise resolves
 * when the assistant turn is complete.
 */
export async function sendMessage(gooseSessionId: string, text: string): Promise<void> {
  const client = await getClient();
  const content: ContentBlock[] = [{ type: "text", text }];
  await client.prompt({ sessionId: gooseSessionId, prompt: content });
}

/** Cancel an in-progress prompt on the given session. */
export async function cancelSession(gooseSessionId: string): Promise<void> {
  const client = await getClient();
  await client.cancel({ sessionId: gooseSessionId });
}
