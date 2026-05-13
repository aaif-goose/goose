import type {
  GooseReadResourceResult,
  GooseToolMetadata,
} from "@aaif/goose-sdk";
import type {
  Annotations,
  ImageContent as AcpImageContent,
  Role,
  TextContent as AcpTextContent,
  ToolCallLocation as AcpToolCallLocation,
  ToolCallStatus as AcpToolCallStatus,
  ToolKind,
} from "@agentclientprotocol/sdk";

// ── Wire types (re-exported from ACP SDK) ─────────────────────────────
//
// These are the exact types that come off the ACP WebSocket. We re-export
// them so feature code imports everything from this module. Aliases exist
// only for readability — no reshaping, no field-dropping.

export type { Annotations, Role, ToolKind };
export type ToolCallLocation = AcpToolCallLocation;

/** ACP TextContent with discriminator. */
export type TextContent = AcpTextContent & { type: "text" };

/** ACP ImageContent with discriminator. */
export type ImageContent = AcpImageContent & { type: "image" };

/**
 * Tool call execution status.
 *
 * The four ACP wire values plus `"stopped"`, a renderer-only extension
 * for user-cancelled tool calls.
 */
export type ToolCallStatus = AcpToolCallStatus | "stopped";

// ── Message role ──────────────────────────────────────────────────────

/**
 * ACP defines `Role = "user" | "assistant"`. The renderer adds `"system"`
 * for locally-synthesized notification messages.
 */
export type MessageRole = Role | "system";

// ── Composer attachment drafts ────────────────────────────────────────

export type ChatAttachmentKind = "image" | "file" | "directory";

export interface ChatImageAttachmentDraft {
  id: string;
  kind: "image";
  name: string;
  path?: string;
  mimeType: string;
  base64: string;
  previewUrl: string;
}

export interface ChatFileAttachmentDraft {
  id: string;
  kind: "file";
  name: string;
  path?: string;
  mimeType?: string;
}

export interface ChatDirectoryAttachmentDraft {
  id: string;
  kind: "directory";
  name: string;
  path: string;
}

export type ChatAttachmentDraft =
  | ChatImageAttachmentDraft
  | ChatFileAttachmentDraft
  | ChatDirectoryAttachmentDraft;

// ── Renderer-only content block types ─────────────────────────────────
//
// These types have no ACP equivalent. They are synthesized by the
// notification handler from _meta payloads, tool call reductions, or
// local UI events.

export type MessageCompletionStatus =
  | "inProgress"
  | "completed"
  | "error"
  | "stopped";

export interface ToolChainSummary {
  summary: string;
  count: number;
}

export interface ToolRequestContent {
  type: "toolRequest";
  id: string;
  name: string;
  toolName?: string;
  extensionName?: string;
  arguments: Record<string, unknown>;
  status: ToolCallStatus;
  toolKind?: ToolKind;
  locations?: AcpToolCallLocation[];
  startedAt?: number;
  annotations?: Annotations;
  chainSummary?: ToolChainSummary;
}

export interface ToolResponseContent {
  type: "toolResponse";
  id: string;
  name: string;
  result: string;
  structuredContent?: unknown;
  isError: boolean;
  annotations?: Annotations;
}

export interface McpAppPayload {
  sessionId: string;
  toolCallId: string;
  toolCallTitle: string;
  source: "toolCallUpdateMeta";
  tool: {
    name: string;
    extensionName: string;
    resourceUri: string;
    meta?: GooseToolMetadata;
  };
  resource: {
    result: GooseReadResourceResult | null;
    readError?: string;
  };
}

export interface McpAppContent {
  type: "mcpApp";
  id: string;
  payload: McpAppPayload;
}

export interface ThinkingContent {
  type: "thinking";
  text: string;
  annotations?: Annotations;
}

export interface RedactedThinkingContent {
  type: "redactedThinking";
  annotations?: Annotations;
}

export interface ReasoningContent {
  type: "reasoning";
  text: string;
  annotations?: Annotations;
}

export interface ActionRequiredContent {
  type: "actionRequired";
  id: string;
  actionType: "toolConfirmation" | "elicitation";
  message?: string;
  toolName?: string;
  arguments?: Record<string, unknown>;
  schema?: Record<string, unknown>;
  annotations?: Annotations;
}

export interface SystemNotificationContent {
  type: "systemNotification";
  notificationType: "compaction" | "info" | "warning" | "error";
  text: string;
  annotations?: Annotations;
}

// ── Message ───────────────────────────────────────────────────────────

export type MessageContent =
  | TextContent
  | ImageContent
  | ToolRequestContent
  | ToolResponseContent
  | McpAppContent
  | ThinkingContent
  | RedactedThinkingContent
  | ReasoningContent
  | ActionRequiredContent
  | SystemNotificationContent;

export interface MessageAttachment {
  type: "file" | "url" | "directory";
  name: string;
  path?: string;
  url?: string;
  mimeType?: string;
}

export interface MessageChip {
  label: string;
  type: "skill" | "extension" | "recipe";
}

export interface MessageMetadata {
  userVisible?: boolean;
  agentVisible?: boolean;
  attachments?: MessageAttachment[];
  chips?: MessageChip[];
  personaId?: string;
  personaName?: string;
  providerId?: string;
  targetPersonaId?: string;
  targetPersonaName?: string;
  completionStatus?: MessageCompletionStatus;
}

export interface Message {
  id: string;
  role: MessageRole;
  created: number;
  content: MessageContent[];
  metadata?: MessageMetadata;
}

// ── Type guards ───────────────────────────────────────────────────────

export function isTextContent(c: MessageContent): c is TextContent {
  return c.type === "text";
}
export function isToolRequest(c: MessageContent): c is ToolRequestContent {
  return c.type === "toolRequest";
}
export function isToolResponse(c: MessageContent): c is ToolResponseContent {
  return c.type === "toolResponse";
}
export function isMcpApp(c: MessageContent): c is McpAppContent {
  return c.type === "mcpApp";
}
export function isThinking(c: MessageContent): c is ThinkingContent {
  return c.type === "thinking";
}
export function isReasoning(c: MessageContent): c is ReasoningContent {
  return c.type === "reasoning";
}
export function isActionRequired(
  c: MessageContent,
): c is ActionRequiredContent {
  return c.type === "actionRequired";
}
export function isSystemNotification(
  c: MessageContent,
): c is SystemNotificationContent {
  return c.type === "systemNotification";
}

// ── Helpers ───────────────────────────────────────────────────────────

export function getTextContent(message: Message): string {
  return message.content
    .filter(isTextContent)
    .map((c) => c.text)
    .join("\n");
}

export function createUserMessage(
  text: string,
  attachments?: MessageAttachment[],
  chips?: MessageChip[],
): Message {
  return {
    id: crypto.randomUUID(),
    role: "user",
    created: Date.now(),
    content: [{ type: "text", text }],
    metadata: {
      userVisible: true,
      agentVisible: true,
      ...(attachments ? { attachments } : {}),
      ...(chips && chips.length > 0 ? { chips } : {}),
    },
  };
}

export function createSystemNotificationMessage(
  text: string,
  notificationType: SystemNotificationContent["notificationType"] = "info",
): Message {
  return {
    id: crypto.randomUUID(),
    role: "system",
    created: Date.now(),
    content: [{ type: "systemNotification", notificationType, text }],
    metadata: {
      userVisible: true,
      agentVisible: false,
    },
  };
}
