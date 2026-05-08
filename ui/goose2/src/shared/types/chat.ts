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
  hasUsageSnapshot: boolean;
  streamingMessageId: string | null;
  pendingAssistantProviderId: string | null;
  error: string | null;
  hasUnread: boolean;
}

export const INITIAL_SESSION_CHAT_RUNTIME: SessionChatRuntime = {
  chatState: "idle",
  tokenState: INITIAL_TOKEN_STATE,
  hasUsageSnapshot: false,
  streamingMessageId: null,
  pendingAssistantProviderId: null,
  error: null,
  hasUnread: false,
};

// Session
export interface Session {
  id: string;
  title: string;
  projectId?: string | null;
  providerId?: string;
  personaId?: string;
  modelId?: string;
  modelName?: string;
  workingDir?: string | null;
  createdAt: string;
  updatedAt: string;
  archivedAt?: string;
  messageCount: number;
  userSetName?: boolean;
}
