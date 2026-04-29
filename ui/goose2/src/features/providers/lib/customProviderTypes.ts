import type {
  ProviderConfigStatusDto,
  RefreshProviderInventoryResponse,
} from "@aaif/goose-sdk";

export type CustomProviderFormat = "openai" | "anthropic" | "ollama";

export type CustomProviderEngine =
  | "openai_compatible"
  | "anthropic_compatible"
  | "ollama_compatible";

export interface CustomProviderHeaderDraft {
  key: string;
  value: string;
}

export interface CustomProviderDraft {
  providerId?: string;
  editable: boolean;
  engine: CustomProviderEngine;
  displayName: string;
  apiUrl: string;
  basePath: string;
  apiKey: string;
  modelsInput: string;
  models: string[];
  requiresAuth: boolean;
  supportsStreaming: boolean;
  headers: CustomProviderHeaderDraft[];
  catalogProviderId?: string;
}

export interface ProviderCatalogEntryDto {
  providerId: string;
  name: string;
  format: string;
  apiUrl: string;
  modelCount: number;
  docUrl: string;
  envVar: string;
}

export interface ProviderModelTemplateDto {
  id: string;
  name: string;
  contextLimit: number;
  capabilities: {
    toolCall: boolean;
    reasoning: boolean;
    attachment: boolean;
    temperature: boolean;
  };
  deprecated: boolean;
}

export interface ProviderTemplateDto {
  providerId: string;
  name: string;
  format: string;
  apiUrl: string;
  models?: ProviderModelTemplateDto[];
  supportsStreaming: boolean;
  envVar: string;
  docUrl: string;
}

export interface CustomProviderUpsertRequest {
  engine: CustomProviderEngine;
  displayName: string;
  apiUrl: string;
  apiKey: string;
  models: string[];
  supportsStreaming?: boolean;
  headers?: Record<string, string>;
  requiresAuth: boolean;
  catalogProviderId?: string;
  basePath?: string;
}

export interface CustomProviderCreateResponse {
  providerId: string;
  status: ProviderConfigStatusDto;
  refresh: RefreshProviderInventoryResponse;
}

export interface CustomProviderConfigDto {
  providerId: string;
  engine: string;
  displayName: string;
  apiUrl: string;
  models?: string[];
  supportsStreaming?: boolean | null;
  headers?: Record<string, string>;
  requiresAuth: boolean;
  catalogProviderId?: string | null;
  basePath?: string | null;
  apiKeyEnv?: string | null;
  apiKeySet: boolean;
}

export interface CustomProviderReadResponse {
  provider: CustomProviderConfigDto;
  editable: boolean;
  status: ProviderConfigStatusDto;
}

export interface CustomProviderUpdateResponse {
  providerId: string;
  status: ProviderConfigStatusDto;
  refresh: RefreshProviderInventoryResponse;
}

export interface CustomProviderDeleteResponse {
  providerId: string;
  refresh: RefreshProviderInventoryResponse;
}

export type { ProviderConfigStatusDto, RefreshProviderInventoryResponse };
