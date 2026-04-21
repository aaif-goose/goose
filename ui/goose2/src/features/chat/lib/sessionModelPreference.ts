import { resolveAgentProviderCatalogIdStrict } from "@/features/providers/providerCatalog";
import { getStoredModelPreferenceForProvider } from "./modelPreferences";

interface SessionModelPreferenceOptions {
  providerId: string;
  preferredModel?: string;
}

export interface SessionModelPreference {
  providerId: string;
  modelId?: string;
  modelName?: string;
}

interface ProviderInventoryEntryLike {
  models: Array<{
    id: string;
  }>;
}

export function resolveSessionModelPreference({
  providerId,
  preferredModel,
}: SessionModelPreferenceOptions): SessionModelPreference {
  if (preferredModel) {
    return {
      providerId,
      modelId: preferredModel,
      modelName: preferredModel,
    };
  }

  const storedModelPreference = getStoredModelPreferenceForProvider(providerId);
  if (!storedModelPreference) {
    return { providerId };
  }

  if (resolveAgentProviderCatalogIdStrict(providerId)) {
    return {
      providerId: storedModelPreference.providerId ?? providerId,
      modelId: storedModelPreference.modelId,
      modelName: storedModelPreference.modelName,
    };
  }

  if (
    storedModelPreference.providerId &&
    storedModelPreference.providerId !== providerId
  ) {
    return { providerId };
  }

  return {
    providerId,
    modelId: storedModelPreference.modelId,
    modelName: storedModelPreference.modelName,
  };
}

export function sanitizeSessionModelPreference(
  preference: SessionModelPreference,
  inventoryEntry?: ProviderInventoryEntryLike | null,
): SessionModelPreference {
  if (!preference.modelId || !inventoryEntry) {
    return preference;
  }

  if (inventoryEntry.models.some((model) => model.id === preference.modelId)) {
    return preference;
  }

  return {
    providerId: preference.providerId,
  };
}
