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
  return {
    providerId: storedModelPreference?.providerId ?? providerId,
    modelId: storedModelPreference?.modelId,
    modelName: storedModelPreference?.modelName,
  };
}
