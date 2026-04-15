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

// Content block types
export interface TextContent {
  type: "text";
  text: string;
}

export interface ImageContent {
  type: "image";
  source:
    | { type: "base64"; mediaType: string; data: string }
    | { type: "url"; url: string };
}

export type ToolCallStatus =
  | "pending"
  | "executing"
  | "completed"
  | "error"
  | "stopped";

export type MessageCompletionStatus =
  | "inProgress"
  | "completed"
  | "error"
  | "stopped";

export interface ToolRequestContent {
  type: "toolRequest";
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  status: ToolCallStatus;
  /** Epoch ms when the tool call started executing (set on event receipt). */
  startedAt?: number;
}

export interface ToolResponseContent {
  type: "toolResponse";
  id: string;
  name: string;
  result: string;
  isError: boolean;
}

export interface ThinkingContent {
  type: "thinking";
  text: string;
}

export interface RedactedThinkingContent {
  type: "redactedThinking";
}

export interface ReasoningContent {
  type: "reasoning";
  text: string;
}

export interface ActionRequiredContent {
  type: "actionRequired";
  id: string;
  actionType: "toolConfirmation" | "elicitation";
  message?: string;
  toolName?: string;
  arguments?: Record<string, unknown>;
  schema?: Record<string, unknown>;
}

export interface SystemNotificationContent {
  type: "systemNotification";
  notificationType: "compaction" | "info" | "warning" | "error";
  text: string;
}

export type MessageContent =
  | TextContent
  | ImageContent
  | ToolRequestContent
  | ToolResponseContent
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
