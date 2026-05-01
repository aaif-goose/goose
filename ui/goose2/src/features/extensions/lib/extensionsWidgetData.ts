import type { Message, ToolRequestContent } from "@/shared/types/messages";
import type { ExtensionEntry, SessionExtensionStatus } from "../types";
import { getToolOwnerSignatureKey } from "./extensionUsage";

export interface ToolUsageSnapshot {
  signature: string;
  messages: Message[];
}

export function toUnavailableSessionExtensionStatus(
  extension: ExtensionEntry,
): SessionExtensionStatus {
  const { enabled: _enabled, ...config } = extension;
  return {
    ...config,
    status: "unavailable",
    tools: [],
  };
}

export function mergeExtensionStatuses(
  sessionExtensions: SessionExtensionStatus[],
  configuredExtensions: ExtensionEntry[],
): SessionExtensionStatus[] {
  const byKey = new Map(
    sessionExtensions.map((extension) => [extension.config_key, extension]),
  );
  for (const extension of configuredExtensions) {
    if (!byKey.has(extension.config_key)) {
      byKey.set(
        extension.config_key,
        toUnavailableSessionExtensionStatus(extension),
      );
    }
  }
  return Array.from(byKey.values());
}

export function getToolUsageSnapshot(
  messages: Message[],
  lastSnapshot: ToolUsageSnapshot,
): ToolUsageSnapshot {
  const signatureParts: string[] = [];
  const toolMessages: Message[] = [];

  for (const message of messages) {
    const toolRequests: ToolRequestContent[] = [];
    for (const content of message.content) {
      if (content.type === "toolRequest") {
        const owner = getToolOwnerSignatureKey(content);
        if (owner) {
          signatureParts.push(`${message.id}:${content.id}:${owner}`);
        }
        toolRequests.push(content);
      }
    }
    if (toolRequests.length > 0) {
      toolMessages.push({ ...message, content: toolRequests });
    }
  }

  const signature = signatureParts.join("|");
  if (signature === lastSnapshot.signature) {
    return lastSnapshot;
  }

  return { signature, messages: toolMessages };
}
