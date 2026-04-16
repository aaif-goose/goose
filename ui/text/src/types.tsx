import type { ContentChunk, ToolCall, RequestPermissionResponse } from "@agentclientprotocol/sdk";

export interface PendingPermission {
  toolTitle: string;
  options: Array<{ optionId: string; name: string; kind: string }>;
  resolve: (response: RequestPermissionResponse) => void;
}

export type ResponseItem =
  | (ContentChunk & { itemType: "content_chunk" })
  | (ToolCall & { itemType: "tool_call" })
  | { itemType: "error"; message: string };

export interface Turn {
  userText: string;
  responseItems: ResponseItem[];
  toolCallsById: Map<string, number>;
}
