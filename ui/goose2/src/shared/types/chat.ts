import type { Message } from "./messages";
import type { Agent } from "./agents";

// Chat state machine
export type ChatState =
  | "idle"
  | "thinking"
  | "streaming"
  | "waiting"
  | "compacting"
  | "error";

// Token tracking
export interface TokenState {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  accumulatedInput: number;
  accumulatedOutput: number;
  accumulatedTotal: number;
  contextLimit: number;
}

export const INITIAL_TOKEN_STATE: TokenState = {
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  accumulatedInput: 0,
  accumulatedOutput: 0,
  accumulatedTotal: 0,
  contextLimit: 0,
};

export interface SessionChatRuntime {
  chatState: ChatState;
  tokenState: TokenState;
  streamingMessageId: string | null;
  pendingAssistantProviderId: string | null;
  error: string | null;
  hasUnread: boolean;
}

export const INITIAL_SESSION_CHAT_RUNTIME: SessionChatRuntime = {
  chatState: "idle",
  tokenState: INITIAL_TOKEN_STATE,
  streamingMessageId: null,
  pendingAssistantProviderId: null,
  error: null,
  hasUnread: false,
};

// Session
export interface Session {
  id: string;
  title: string;
  agentId?: string;
  projectId?: string | null;
  providerId?: string;
  personaId?: string;
  modelId?: string;
  modelName?: string;
  createdAt: string;
  updatedAt: string;
  archivedAt?: string;
  messageCount: number;
  userSetName?: boolean;
}

// SSE event types (from goosed server)
export type MessageEventType =
  | "message"
  | "error"
  | "finish"
  | "modelChange"
  | "notification"
  | "updateConversation"
  | "ping";

export interface MessageEvent {
  type: "message";
  message: Message;
  tokenState: TokenState;
}

export interface ErrorEvent {
  type: "error";
  error: string;
}

export interface FinishEvent {
  type: "finish";
  reason: string;
  tokenState: TokenState;
}

export interface ModelChangeEvent {
  type: "modelChange";
  model: string;
  mode: string;
}

export type StreamEvent =
  | MessageEvent
  | ErrorEvent
  | FinishEvent
  | ModelChangeEvent;

// Chat request
export interface ChatRequest {
  userMessage: Message;
  sessionId: string;
  recipeName?: string;
  overrideConversation?: Message[];
}

// Active chat context
export interface ChatContext {
  sessionId: string;
  agent: Agent;
  messages: Message[];
  chatState: SessionChatRuntime["chatState"];
  tokenState: SessionChatRuntime["tokenState"];
  streamingMessageId: SessionChatRuntime["streamingMessageId"];
  error: SessionChatRuntime["error"];
}
