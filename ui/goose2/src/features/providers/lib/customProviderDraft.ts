import {
  formatCustomProviderModels,
  parseCustomProviderModels,
} from "./customProviderModels";
import {
  headerDraftsToRecord,
  recordToHeaderDrafts,
} from "./customProviderHeaders";
import type {
  CustomProviderDraft,
  CustomProviderEngine,
  CustomProviderReadResponse,
  CustomProviderUpsertRequest,
  ProviderTemplateDto,
} from "./customProviderTypes";

const FORMAT_ENGINE_MAP: Record<string, CustomProviderEngine> = {
  openai: "openai_compatible",
  anthropic: "anthropic_compatible",
  ollama: "ollama_compatible",
};

const ENGINE_MAP: Record<string, CustomProviderEngine> = {
  openai: "openai_compatible",
  openai_compatible: "openai_compatible",
  anthropic: "anthropic_compatible",
  anthropic_compatible: "anthropic_compatible",
  ollama: "ollama_compatible",
  ollama_compatible: "ollama_compatible",
};

export function isCustomProviderEngine(
  engine: string | undefined,
): engine is CustomProviderEngine {
  if (!engine) {
    return false;
  }
  const normalized = engine.trim().toLowerCase();
  return Boolean(ENGINE_MAP[normalized]);
}

export function normalizeCustomProviderEngine(
  engine: string | undefined,
): string {
  if (!engine) {
    return "";
  }
  const normalized = engine.trim().toLowerCase();
  return ENGINE_MAP[normalized] ?? normalized;
}

export function engineForCustomProviderFormat(
  format: string | undefined,
): CustomProviderEngine {
  return FORMAT_ENGINE_MAP[format ?? ""] ?? "openai_compatible";
}

export function createEmptyCustomProviderDraft(): CustomProviderDraft {
  return {
    editable: true,
    engine: "openai_compatible",
    displayName: "",
    apiUrl: "",
    basePath: "",
    apiKey: "",
    apiKeySet: false,
    modelsInput: "",
    models: [],
    authInitiallyEnabled: true,
    requiresAuth: true,
    supportsStreaming: true,
    headers: [],
  };
}

export function templateToCustomProviderDraft(
  template: ProviderTemplateDto,
): CustomProviderDraft {
  const models = (template.models ?? [])
    .filter((model) => !model.deprecated)
    .map((model) => model.id);

  return {
    editable: true,
    engine: engineForCustomProviderFormat(template.format),
    displayName: template.name,
    apiUrl: template.apiUrl,
    basePath: "",
    apiKey: "",
    apiKeySet: false,
    modelsInput: formatCustomProviderModels(models),
    models,
    authInitiallyEnabled: true,
    requiresAuth: true,
    supportsStreaming: template.supportsStreaming,
    headers: [],
    catalogProviderId: template.providerId,
  };
}

export function readToCustomProviderDraft(
  response: CustomProviderReadResponse,
): CustomProviderDraft {
  const provider = response.provider;
  const models = parseCustomProviderModels(provider.models ?? []);

  return {
    providerId: provider.providerId,
    editable: response.editable,
    engine: normalizeCustomProviderEngine(provider.engine),
    displayName: provider.displayName,
    apiUrl: provider.apiUrl,
    basePath: provider.basePath ?? "",
    apiKey: "",
    apiKeySet: provider.apiKeySet,
    modelsInput: formatCustomProviderModels(models),
    models,
    authInitiallyEnabled: provider.requiresAuth,
    requiresAuth: provider.requiresAuth,
    supportsStreaming: provider.supportsStreaming ?? true,
    headers: recordToHeaderDrafts(provider.headers),
    catalogProviderId: provider.catalogProviderId ?? undefined,
  };
}

export function customProviderDraftToUpsertRequest(
  draft: CustomProviderDraft,
): CustomProviderUpsertRequest {
  const models = parseCustomProviderModels(
    draft.models.length > 0 ? draft.models : draft.modelsInput,
  );

  const apiKey = draft.requiresAuth ? draft.apiKey.trim() : "";
  const request: CustomProviderUpsertRequest = {
    engine: normalizeCustomProviderEngine(draft.engine) as CustomProviderEngine,
    displayName: draft.displayName.trim(),
    apiUrl: draft.apiUrl.trim(),
    models,
    supportsStreaming: draft.supportsStreaming,
    headers: headerDraftsToRecord(draft.headers),
    requiresAuth: draft.requiresAuth,
    catalogProviderId: draft.catalogProviderId,
    basePath: draft.basePath.trim() || undefined,
  };

  if (apiKey) {
    request.apiKey = apiKey;
  }

  return request;
}
