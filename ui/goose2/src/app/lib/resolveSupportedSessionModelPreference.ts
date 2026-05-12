import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { getProviderInventory } from "@/features/providers/api/inventory";
import {
  resolveSessionModelPreference,
  sanitizeSessionModelPreference,
  type SessionModelPreference,
} from "@/features/chat/lib/sessionModelPreference";
import { getStoredModelPreference } from "@/features/chat/lib/modelPreferences";

export async function resolveSupportedSessionModelPreference(
  providerId: string,
  inventoryEntries: Map<string, ProviderInventoryEntryDto>,
  preferredModel?: string,
): Promise<SessionModelPreference> {
  const sessionModelPreference = resolveSessionModelPreference({
    providerId,
    preferredModel,
  });

  if (!sessionModelPreference.modelId) {
    return sessionModelPreference;
  }

  const exactStoredPreference = preferredModel
    ? null
    : getStoredModelPreference(providerId);
  const shouldPreserveWithoutInventory =
    sessionModelPreference.providerId === providerId &&
    exactStoredPreference?.modelId === sessionModelPreference.modelId &&
    (exactStoredPreference.providerId ?? providerId) ===
      sessionModelPreference.providerId;

  const inventoryEntry =
    inventoryEntries.get(sessionModelPreference.providerId) ??
    (await getProviderInventory([sessionModelPreference.providerId])
      .then(([entry]) => entry)
      .catch(() => undefined));

  if (!inventoryEntry) {
    if (shouldPreserveWithoutInventory) {
      return sessionModelPreference;
    }

    return {
      providerId: sessionModelPreference.providerId,
    };
  }

  return sanitizeSessionModelPreference(sessionModelPreference, inventoryEntry);
}
