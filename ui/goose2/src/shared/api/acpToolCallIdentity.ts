import type { SessionUpdate } from "@agentclientprotocol/sdk";

export interface ToolCallIdentity {
  toolName?: string;
  extensionName?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function getToolCallIdentity(update: SessionUpdate): ToolCallIdentity {
  if (!isRecord(update._meta)) {
    return {};
  }
  const goose = update._meta.goose;
  if (!isRecord(goose)) {
    return {};
  }

  const toolCall = isRecord(goose.mcpApp)
    ? goose.mcpApp
    : isRecord(goose.toolCall)
      ? goose.toolCall
      : null;
  if (!toolCall) return {};

  return {
    ...(typeof toolCall.toolName === "string"
      ? { toolName: toolCall.toolName }
      : {}),
    ...(typeof toolCall.extensionName === "string"
      ? { extensionName: toolCall.extensionName }
      : {}),
  };
}
