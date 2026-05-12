import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  getReplayBuffer,
  getBufferedMessage,
} from "@/features/chat/hooks/replayBuffer";
import type { McpAppContent, MessageContent } from "@/shared/types/messages";
import { buildMcpAppPayloadFromToolUpdate } from "./mcpAppToolUpdate";

export function findReplayMessageWithToolCall(
  sessionId: string,
  toolCallId: string,
): ReturnType<typeof getBufferedMessage> {
  const buffer = getReplayBuffer(sessionId);
  if (!buffer) {
    return undefined;
  }
  for (let index = buffer.length - 1; index >= 0; index -= 1) {
    const message = buffer[index];
    if (
      message.content.some(
        (content) =>
          content.type === "toolRequest" && content.id === toolCallId,
      )
    ) {
      return message;
    }
  }
  return undefined;
}

export function extractToolResultText(update: {
  // biome-ignore lint/suspicious/noExplicitAny: ACP SDK ToolCallContent type is complex
  content?: Array<any> | null;
  rawOutput?: unknown;
}): string {
  if (update.content && update.content.length > 0) {
    for (const item of update.content) {
      if (item.type === "content" && item.content?.type === "text") {
        return item.content.text;
      }
    }
  }
  if (update.rawOutput !== undefined && update.rawOutput !== null) {
    return typeof update.rawOutput === "string"
      ? update.rawOutput
      : JSON.stringify(update.rawOutput);
  }
  return "";
}

export function extractToolStructuredContent(update: {
  rawOutput?: unknown;
}): unknown | undefined {
  if (Object.hasOwn(update, "rawOutput")) {
    return update.rawOutput;
  }

  return undefined;
}

export function attachMcpAppPayload(
  sessionId: string,
  toolCallId: string,
  toolCallTitle: string,
  update: SessionUpdate,
  isReplay: boolean,
  options?: {
    replayMessageId?: string | null;
  },
): void {
  const payload = buildMcpAppPayloadFromToolUpdate(
    sessionId,
    toolCallId,
    toolCallTitle,
    update,
  );
  if (!payload) {
    return;
  }

  const block: McpAppContent = {
    type: "mcpApp",
    id: toolCallId,
    payload,
  };

  if (isReplay) {
    const message =
      findReplayMessageWithToolCall(sessionId, toolCallId) ??
      (options?.replayMessageId
        ? getBufferedMessage(sessionId, options.replayMessageId)
        : undefined);
    if (message) {
      message.content = insertMcpAppContent(message.content, block);
      return;
    }
  }

  const store = useChatStore.getState();
  const message = [...(store.messagesBySession[sessionId] ?? [])]
    .reverse()
    .find((candidate) =>
      candidate.content.some(
        (content) =>
          content.type === "toolRequest" && content.id === toolCallId,
      ),
    );
  if (!message) {
    return;
  }

  store.updateMessage(sessionId, message.id, (current) => ({
    ...current,
    content: insertMcpAppContent(current.content, block),
  }));
}

function insertMcpAppContent(
  content: MessageContent[],
  block: McpAppContent,
): MessageContent[] {
  if (content.some((item) => item.type === "mcpApp" && item.id === block.id)) {
    return content;
  }

  const insertAfterIndex = findMcpAppAnchorIndex(content, block.id);
  if (insertAfterIndex === -1) {
    return [...content, block];
  }

  return [
    ...content.slice(0, insertAfterIndex + 1),
    block,
    ...content.slice(insertAfterIndex + 1),
  ];
}

function findMcpAppAnchorIndex(
  content: MessageContent[],
  toolCallId: string,
): number {
  for (let index = content.length - 1; index >= 0; index -= 1) {
    const block = content[index];
    if (block.type === "toolResponse" && block.id === toolCallId) {
      return index;
    }
  }

  for (let index = content.length - 1; index >= 0; index -= 1) {
    const block = content[index];
    if (block.type === "toolRequest" && block.id === toolCallId) {
      return index;
    }
  }

  return -1;
}
