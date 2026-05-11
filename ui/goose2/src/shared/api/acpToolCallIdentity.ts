import type { SessionUpdate } from "@agentclientprotocol/sdk";

export interface ToolCallIdentity {
  toolName?: string;
  extensionName?: string;
}

export interface ToolChainSummary {
  summary: string;
  count: number;
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

/**
 * Extract a chain summary from `_meta.goose.toolChainSummary` of a tool-call
 * SessionUpdate. Returns `undefined` when the meta is missing, malformed, or
 * carries a non-positive count.
 *
 * The server attaches this to the FIRST tool call in a multi-tool chain once
 * every step has completed; replays after reload re-emit it on the initial
 * `ToolCall` notification so the chain header is correct on first paint.
 */
export function getToolChainSummary(
  update: SessionUpdate,
): ToolChainSummary | undefined {
  if (!isRecord(update._meta)) return undefined;
  const goose = update._meta.goose;
  if (!isRecord(goose)) return undefined;
  const chain = goose.toolChainSummary;
  if (!isRecord(chain)) return undefined;

  const summary = chain.summary;
  const count = chain.count;
  if (typeof summary !== "string" || summary.length === 0) return undefined;
  if (typeof count !== "number" || !Number.isFinite(count) || count <= 0) {
    return undefined;
  }
  return { summary, count: Math.trunc(count) };
}
