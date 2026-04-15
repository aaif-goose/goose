import type { ModelOption } from "../types";

export interface AcpMessageCreatedPayload {
  sessionId: string;
  messageId: string;
  personaId?: string;
  personaName?: string;
}

export interface AcpTextPayload {
  sessionId: string;
  messageId: string;
  text: string;
}

export interface AcpDonePayload {
  sessionId: string;
  messageId: string;
}

export interface AcpToolCallPayload {
  sessionId: string;
  messageId: string;
  toolCallId: string;
  title: string;
}

export interface AcpToolTitlePayload {
  sessionId: string;
  messageId: string;
  toolCallId: string;
  title: string;
}

export interface AcpToolResultPayload {
  sessionId: string;
  messageId: string;
  content: string;
}

export interface AcpSessionInfoPayload {
  sessionId: string;
  title?: string;
}

export interface AcpSessionBoundPayload {
  sessionId: string;
  gooseSessionId: string;
}

export interface AcpModelStatePayload {
  sessionId: string;
  providerId?: string | null;
  currentModelId: string;
  currentModelName?: string;
  availableModels: ModelOption[];
}

export interface AcpUsageUpdatePayload {
  sessionId: string;
  used: number;
  size: number;
}

export interface AcpReplayCompletePayload {
  sessionId: string;
}
