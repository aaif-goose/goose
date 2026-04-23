import { resolveAgentProviderCatalogIdStrict } from "@/features/providers/providerCatalog";

const MODEL_PREFERENCES_STORAGE_KEY = "goose:preferredModelsByAgent";

export interface StoredModelPreference {
  modelId: string;
  modelName: string;
  providerId?: string;
}

type StoredModelPreferences = Record<string, StoredModelPreference>;

function readStoredModelPreferences(): StoredModelPreferences {
  if (typeof window === "undefined") {
    return {};
  }

  try {
    const stored = window.localStorage.getItem(MODEL_PREFERENCES_STORAGE_KEY);
    if (!stored) {
      return {};
    }

    const parsed = JSON.parse(stored);
    if (!parsed || typeof parsed !== "object") {
      return {};
    }

    return parsed as StoredModelPreferences;
  } catch {
    return {};
  }
}

function persistStoredModelPreferences(
  preferences: StoredModelPreferences,
): void {
  if (typeof window === "undefined") {
    return;
  }

  try {
    if (Object.keys(preferences).length === 0) {
      window.localStorage.removeItem(MODEL_PREFERENCES_STORAGE_KEY);
      return;
    }

    window.localStorage.setItem(
      MODEL_PREFERENCES_STORAGE_KEY,
      JSON.stringify(preferences),
    );
  } catch {
    // localStorage may be unavailable
  }
}

export function getStoredModelPreference(
  agentId: string,
): StoredModelPreference | null {
  return readStoredModelPreferences()[agentId] ?? null;
}

export function getStoredModelPreferenceForProvider(
  providerId: string,
): StoredModelPreference | null {
  const agentId = resolveAgentProviderCatalogIdStrict(providerId) ?? "goose";
  return getStoredModelPreference(agentId);
}

export function setStoredModelPreference(
  agentId: string,
  preference: StoredModelPreference,
): void {
  const next = readStoredModelPreferences();
  next[agentId] = preference;
  persistStoredModelPreferences(next);
}

export function clearStoredModelPreference(agentId: string): void {
  const next = readStoredModelPreferences();
  if (!(agentId in next)) {
    return;
  }
  delete next[agentId];
  persistStoredModelPreferences(next);
}
