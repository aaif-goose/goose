import type {
  GooseReadResourceResult,
  GooseToolMetadata,
} from "@aaif/goose-sdk";

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

// Message roles
export type MessageRole = "user" | "assistant" | "system";

/** ACP audience restriction — which roles may see a content block. */
export type Audience = ("user" | "assistant")[];

/** ACP content-block annotations (mirrors the SDK's Annotations shape). */
export interface ContentAnnotations {
  audience?: Audience;
}

// Content block types
export interface TextContent {
  type: "text";
  text: string;
  annotations?: ContentAnnotations;
}

export interface ImageContent {
  type: "image";
  source:
    | { type: "base64"; mediaType: string; data: string }
    | { type: "url"; url: string };
  annotations?: ContentAnnotations;
}

export type ToolCallStatus =
  | "pending"
  | "executing"
  | "completed"
  | "error"
  | "stopped";

export type ToolKind =
  | "read"
  | "edit"
  | "delete"
  | "move"
  | "search"
  | "execute"
  | "think"
  | "fetch"
  | "switch_mode"
  | "other";

export interface ToolCallLocation {
  path: string;
  line?: number | null;
}

export type MessageCompletionStatus =
  | "inProgress"
  | "completed"
  | "error"
  | "stopped";

export interface ToolChainSummary {
  /** Lowercase phrase covering the chain's tool calls (e.g. "applied dark mode polish"). */
  summary: string;
  /** Number of tool calls the summary covers. */
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
  locations?: ToolCallLocation[];
  /** Epoch ms when the tool call started executing (set on event receipt). */
  startedAt?: number;
  annotations?: ContentAnnotations;
  /**
   * Server-generated summary of a multi-tool chain that starts at this tool
   * call. Only set on the FIRST tool call of a chain (>= 2 tools); the rest of
   * the chain has this field undefined.
   */
  chainSummary?: ToolChainSummary;
}

export interface ToolResponseContent {
  type: "toolResponse";
  id: string;
  name: string;
  result: string;
  structuredContent?: unknown;
  isError: boolean;
  annotations?: ContentAnnotations;
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
  annotations?: ContentAnnotations;
}

export interface RedactedThinkingContent {
  type: "redactedThinking";
  annotations?: ContentAnnotations;
}

export interface ReasoningContent {
  type: "reasoning";
  text: string;
  annotations?: ContentAnnotations;
}

export interface ActionRequiredContent {
  type: "actionRequired";
  id: string;
  actionType: "toolConfirmation" | "elicitation";
  message?: string;
  toolName?: string;
  arguments?: Record<string, unknown>;
  schema?: Record<string, unknown>;
  annotations?: ContentAnnotations;
}

export interface SystemNotificationContent {
  type: "systemNotification";
  notificationType: "compaction" | "info" | "warning" | "error";
  text: string;
  annotations?: ContentAnnotations;
}

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
  /** Persona that generated this assistant message (set on send). */
  personaId?: string;
  personaName?: string;
  providerId?: string;
  /** Which persona this user message is addressed to. */
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

// Type guards for content blocks
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

// Helpers
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
