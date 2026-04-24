import type { ContentChunk, ToolCall } from "@agentclientprotocol/sdk";

export type ResponseItem =
  | (ContentChunk & { itemType: "content_chunk" })
  | (ToolCall & { itemType: "tool_call" })
  | { itemType: "error"; message: string };

export interface Turn {
  userText: string;
  responseItems: ResponseItem[];
  toolCallsById: Map<string, number>;
}
