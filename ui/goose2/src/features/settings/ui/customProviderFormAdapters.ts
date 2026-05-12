import { normalizeCustomProviderEngine } from "@/features/providers/lib/customProviderDraft";
import { recordToHeaderDrafts } from "@/features/providers/lib/customProviderHeaders";
import {
  formatCustomProviderModels,
  parseCustomProviderModels,
} from "@/features/providers/lib/customProviderModels";
import type {
  CustomProviderDraft,
  CustomProviderEngine,
  CustomProviderReadResponse,
  ProviderTemplateCatalogEntryDto,
  ProviderTemplateDto,
} from "@/features/providers/lib/customProviderTypes";
import type { CustomProviderMutationInput } from "@/features/providers/ui/CustomProviderDialog";
import type {
  CustomProviderFormValues,
  ProviderTemplate,
} from "@/features/providers/ui/CustomProviderForm";

function engineForCustomProviderFormat(format: string): CustomProviderEngine {
  if (format === "anthropic") {
    return "anthropic_compatible";
  }
  if (format === "ollama") {
    return "ollama_compatible";
  }
  return "openai_compatible";
}

export function templateToFormValue(
  template: ProviderTemplateDto,
): ProviderTemplate {
  const models = (template.models ?? [])
    .filter((model) => !model.deprecated)
    .map((model) => model.id);

  return {
    id: template.providerId,
    displayName: template.name,
    engine: engineForCustomProviderFormat(template.format),
    apiUrl: template.apiUrl,
    requiresAuth: true,
    supportsStreaming: template.supportsStreaming,
    models,
    headers: [],
  };
}

export function catalogEntryToTemplate(
  entry: ProviderTemplateCatalogEntryDto,
): ProviderTemplate {
  return {
    id: entry.providerId,
    displayName: entry.name,
    engine: engineForCustomProviderFormat(entry.format),
    apiUrl: entry.apiUrl,
    requiresAuth: true,
    supportsStreaming: true,
    models: [],
    headers: [],
  };
}

export function readResponseToFormValue(
  response: CustomProviderReadResponse,
): CustomProviderFormValues {
  const provider = response.provider;
  return {
    providerId: provider.providerId,
    displayName: provider.displayName,
    engine: normalizeCustomProviderEngine(provider.engine),
    apiUrl: provider.apiUrl,
    basePath: provider.basePath ?? "",
    requiresAuth: provider.requiresAuth,
    apiKey: "",
    apiKeySet: provider.apiKeySet,
    models: parseCustomProviderModels(provider.models ?? []),
    authInitiallyEnabled: provider.requiresAuth,
    supportsStreaming: provider.supportsStreaming ?? true,
    headers: recordToHeaderDrafts(provider.headers),
    catalogProviderId: provider.catalogProviderId ?? undefined,
  };
}

export function formValueToDraft(
  input: CustomProviderMutationInput,
): CustomProviderDraft {
  const models = parseCustomProviderModels(input.models);
  return {
    providerId: input.providerId,
    editable: true,
    engine: input.engine,
    displayName: input.displayName,
    apiUrl: input.apiUrl,
    basePath: input.basePath,
    apiKey: input.apiKey,
    apiKeySet: input.apiKeySet,
    modelsInput: formatCustomProviderModels(models),
    models,
    authInitiallyEnabled: input.authInitiallyEnabled,
    requiresAuth: input.requiresAuth,
    supportsStreaming: input.supportsStreaming,
    headers: input.headers,
    catalogProviderId: input.catalogProviderId,
  };
}
