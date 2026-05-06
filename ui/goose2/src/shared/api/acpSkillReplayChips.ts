import { parseSkillInstructionPrompt } from "@/features/skills/lib/skillChatPrompt";
import {
  ensureReplayBuffer,
  getBufferedMessage,
} from "@/features/chat/hooks/replayBuffer";
import type { MessageChip, TextContent } from "@/shared/types/messages";

const pendingReplayChips = new Map<string, Map<string, MessageChip[]>>();

export function getPendingReplayChips(sessionId: string, messageId: string) {
  const byMessage = pendingReplayChips.get(sessionId);
  return byMessage?.get(messageId) ?? [];
}

export function setPendingReplayChips(
  sessionId: string,
  messageId: string,
  chips: MessageChip[],
) {
  if (chips.length === 0) return;
  const byMessage = pendingReplayChips.get(sessionId) ?? new Map();
  byMessage.set(messageId, chips);
  pendingReplayChips.set(sessionId, byMessage);
}

export function clearPendingReplayChips(sessionId: string, messageId: string) {
  const byMessage = pendingReplayChips.get(sessionId);
  if (!byMessage) return;
  byMessage.delete(messageId);
  if (byMessage.size === 0) {
    pendingReplayChips.delete(sessionId);
  }
}

export function skillInstructionToChips(text: string): MessageChip[] {
  return parseSkillInstructionPrompt(text).map((label) => ({
    label,
    type: "skill" as const,
  }));
}

export function handleReplayUserMessageChunk(
  sessionId: string,
  messageId: string,
  content: { text: string },
  created?: number,
): void {
  const buffer = ensureReplayBuffer(sessionId);
  const existing = getBufferedMessage(sessionId, messageId);
  const ann = getTextAnnotations(content);

  if (isAssistantOnly(ann)) {
    const chips = skillInstructionToChips(content.text);
    if (chips.length > 0) {
      attachReplayChips(sessionId, messageId, existing, chips);
    }
    return;
  }

  const textBlock = makeTextBlock(content.text, ann);
  const chips = getPendingReplayChips(sessionId, messageId);
  if (!existing) {
    buffer.push({
      id: messageId,
      role: "user",
      created: created ?? Date.now(),
      content: [textBlock],
      metadata: {
        userVisible: true,
        agentVisible: true,
        ...(chips.length > 0 ? { chips } : {}),
      },
    });
  } else {
    if (created !== undefined) {
      existing.created = created;
    }
    existing.content.push(textBlock);
    attachReplayChips(sessionId, messageId, existing, chips);
  }
  clearPendingReplayChips(sessionId, messageId);
}

export function clearSkillReplayChips(): void {
  pendingReplayChips.clear();
}

function getTextAnnotations(content: {
  text: string;
  annotations?: unknown;
}): TextContent["annotations"] | undefined {
  const rawAnn = content.annotations;
  return typeof rawAnn === "object" && rawAnn !== null
    ? (rawAnn as TextContent["annotations"])
    : undefined;
}

function isAssistantOnly(ann?: TextContent["annotations"]) {
  return Boolean(
    ann?.audience && ann.audience.length > 0 && !ann.audience.includes("user"),
  );
}

function attachReplayChips(
  sessionId: string,
  messageId: string,
  existing: ReturnType<typeof getBufferedMessage>,
  chips: MessageChip[],
) {
  if (chips.length === 0) return;
  if (existing) {
    existing.metadata = {
      ...existing.metadata,
      chips: [...(existing.metadata?.chips ?? []), ...chips],
    };
  } else {
    setPendingReplayChips(sessionId, messageId, chips);
  }
}

function makeTextBlock(
  text: string,
  ann?: TextContent["annotations"],
): TextContent {
  return ann
    ? { type: "text", text, annotations: ann }
    : { type: "text", text };
}
