import {
  ensureReplayBuffer,
  getBufferedMessage,
} from "@/features/chat/hooks/replayBuffer";
import type { Message } from "@/shared/types/messages";

const replayAssistantMessageIds = new Map<string, string>();

export function getTrackedReplayAssistantMessageId(
  sessionId: string,
): string | null {
  return replayAssistantMessageIds.get(sessionId) ?? null;
}

export function ensureReplayAssistantMessage(
  sessionId: string,
  preferredMessageId?: string | null,
  created?: number,
): Message {
  const trackedMessageId = replayAssistantMessageIds.get(sessionId);

  if (preferredMessageId) {
    const preferredMessage = getBufferedMessage(sessionId, preferredMessageId);
    if (preferredMessage?.role === "assistant") {
      if (created !== undefined) {
        preferredMessage.created = created;
      }
      replayAssistantMessageIds.set(sessionId, preferredMessageId);
      return preferredMessage;
    }
  }

  if (trackedMessageId) {
    const trackedMessage = getBufferedMessage(sessionId, trackedMessageId);
    if (trackedMessage?.role === "assistant") {
      if (preferredMessageId && trackedMessage.id !== preferredMessageId) {
        trackedMessage.id = preferredMessageId;
        replayAssistantMessageIds.set(sessionId, preferredMessageId);
      }
      if (created !== undefined) {
        trackedMessage.created = created;
      }
      return trackedMessage;
    }
  }

  const messageId = preferredMessageId ?? crypto.randomUUID();
  const buffer = ensureReplayBuffer(sessionId);
  const message: Message = {
    id: messageId,
    role: "assistant",
    created: created ?? Date.now(),
    content: [],
    metadata: {
      userVisible: true,
      agentVisible: true,
      completionStatus: "inProgress",
    },
  };
  buffer.push(message);
  replayAssistantMessageIds.set(sessionId, messageId);
  return message;
}

export function clearReplayAssistantMessage(sessionId: string): void {
  replayAssistantMessageIds.delete(sessionId);
}

export function clearReplayAssistantTracking(): void {
  replayAssistantMessageIds.clear();
}
