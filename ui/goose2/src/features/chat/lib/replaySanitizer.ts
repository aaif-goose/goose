import type { Message } from "@/shared/types/messages";
import { getTextContent } from "@/shared/types/messages";

const MANUAL_COMPACT_TRIGGER = "/compact";
const ALTERNATE_COMPACT_TRIGGERS = new Set(["/summarize"]);

export function isManualCompactReplayArtifact(message: Message): boolean {
  if (message.role !== "user") {
    return false;
  }

  const rawText = getTextContent(message).trim();
  if (!rawText) {
    return false;
  }

  const normalizedText = rawText.replace(/\s+/g, " ").trim().toLowerCase();
  if (ALTERNATE_COMPACT_TRIGGERS.has(normalizedText)) {
    return true;
  }

  const collapsedText = normalizedText.replace(/\s+/g, "");
  return (
    collapsedText.length > 0 &&
    collapsedText.replaceAll(MANUAL_COMPACT_TRIGGER, "").length === 0
  );
}

export function sanitizeReplayMessages(messages: Message[]): Message[] {
  return messages.filter((message) => !isManualCompactReplayArtifact(message));
}
