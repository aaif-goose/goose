// Provider types — these map to goose serve provider names.
// All sessions run through goose serve; the provider ID selects which
// backend provider goose uses for inference.  The list is dynamic
// (fetched from the backend via discover_acp_providers) so this is a
// plain string rather than a narrow union.
export type ProviderType = string;

export interface ProviderConfig {
  type: ProviderType;
  name: string;
  description?: string;
  models: ModelInfo[];
  requiresApiKey: boolean;
  apiKeyEnvVar?: string;
}

export interface ModelInfo {
  id: string;
  name: string;
  contextWindow: number;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsThinking: boolean;
}

// Avatar type — either a remote URL or a local file in ~/.goose/avatars/
export type Avatar =
  | { type: "url"; value: string }
  | { type: "local"; value: string };

// Persona types (from sprout)
export interface Persona {
  id: string;
  displayName: string;
  avatar?: Avatar | null;
  systemPrompt: string;
  provider?: ProviderType;
  model?: string;
  isBuiltin: boolean;
  isFromDisk?: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface CreatePersonaRequest {
  displayName: string;
  avatar?: Avatar | null;
  systemPrompt: string;
  provider?: ProviderType;
  model?: string;
}

export interface UpdatePersonaRequest {
  displayName?: string;
  avatar?: Avatar | null;
  systemPrompt?: string;
  provider?: ProviderType;
  model?: string;
}

// Agent types
export type AgentStatus = "online" | "offline" | "starting" | "error";
export type AgentConnectionType = "builtin" | "acp";

export interface Agent {
  id: string;
  name: string;
  personaId?: string;
  persona?: Persona;
  provider: ProviderType;
  model: string;
  systemPrompt?: string;
  connectionType: AgentConnectionType;
  status: AgentStatus;
  isBuiltin: boolean;
  acpEndpoint?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateAgentRequest {
  name: string;
  personaId?: string;
  provider: ProviderType;
  model: string;
  systemPrompt?: string;
  connectionType: AgentConnectionType;
  acpEndpoint?: string;
}

// Session, TokenState, ChatState, and MessageEventType are defined in ./chat.ts
