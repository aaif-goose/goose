import type {
  GooseMcpAppToolPayload,
  GooseReadResourceResult,
  GooseToolCallUpdateMeta,
  GooseToolMetadata,
} from "@aaif/goose-sdk";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import type { McpAppPayload } from "@/shared/types/messages";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function extractMcpAppPayload(
  update: SessionUpdate,
): GooseMcpAppToolPayload | null {
  if (update.sessionUpdate !== "tool_call_update" || !isRecord(update._meta)) {
    return null;
  }

  const meta = update._meta as GooseToolCallUpdateMeta;
  const payload = meta.goose?.mcpApp;
  return isRecord(payload) ? (payload as GooseMcpAppToolPayload) : null;
}

export function buildMcpAppPayloadFromToolUpdate(
  sessionId: string,
  toolCallId: string,
  toolCallTitle: string,
  update: SessionUpdate,
): McpAppPayload | null {
  const payload = extractMcpAppPayload(update);
  if (!payload) {
    return null;
  }

  return {
    sessionId,
    toolCallId,
    toolCallTitle,
    source: "toolCallUpdateMeta",
    tool: {
      name: payload.toolName,
      extensionName: payload.extensionName,
      resourceUri: payload.resourceUri,
      meta: isRecord(payload.toolMeta)
        ? (payload.toolMeta as GooseToolMetadata)
        : undefined,
    },
    resource: {
      result:
        (payload.resourceResult as GooseReadResourceResult | null) ?? null,
      ...(typeof payload.readError === "string"
        ? { readError: payload.readError }
        : {}),
    },
  };
}
