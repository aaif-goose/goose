// Provider types map to goose serve provider names.
// The provider list is dynamic, so this remains a plain string rather than
// a narrow union.
export type ProviderType = string;

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

// Session, TokenState, and ChatState are defined in ./chat.ts
