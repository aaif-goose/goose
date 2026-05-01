import { useCallback, useMemo } from "react";
import { useProviderInventoryStore } from "../stores/providerInventoryStore";
import type { ModelOption } from "@/features/chat/types";
import type {
  ProviderInventoryEntryDto,
  ProviderInventoryModelDto,
} from "@aaif/goose-sdk";
import { getModelProviders } from "../providerCatalog";
import { useDistroStore } from "@/features/settings/stores/distroStore";
import { filterModelProvidersForDistro } from "../distroProviderConstraints";

function isConfiguredGooseModelProvider(
  entry: ProviderInventoryEntryDto,
  modelProviderIds: Set<string>,
): boolean {
  if (!entry.configured) {
    return false;
  }

  if (entry.providerType === "Custom") {
    return entry.providerId.startsWith("custom_");
  }

  return modelProviderIds.has(entry.providerId);
}

function inventoryModelToOption(
  model: ProviderInventoryModelDto,
  provider?: Pick<ProviderInventoryEntryDto, "providerId" | "providerName">,
): ModelOption {
  return {
    id: model.id,
    name: model.name,
    displayName: model.name !== model.id ? model.name : undefined,
    provider: model.family ?? undefined,
    providerId: provider?.providerId,
    providerName: provider?.providerName,
    contextLimit: model.contextLimit ?? undefined,
    recommended: model.recommended ?? false,
  };
}

export function useProviderInventory() {
  const entries = useProviderInventoryStore((s) => s.entries);
  const loading = useProviderInventoryStore((s) => s.loading);
  const distro = useDistroStore((s) => s.manifest);

  const getEntry = useCallback(
    (providerId: string) => entries.get(providerId),
    [entries],
  );

  const getModelsForProvider = useCallback(
    (providerId: string): ModelOption[] => {
      const entry = entries.get(providerId);
      if (!entry) return [];
      return entry.models.map((model) => inventoryModelToOption(model, entry));
    },
    [entries],
  );

  const modelProviderIds = useMemo(
    () =>
      new Set(
        filterModelProvidersForDistro(getModelProviders(), distro).map(
          (provider) => provider.id,
        ),
      ),
    [distro],
  );

  const configuredModelProviderEntries = useMemo(
    () =>
      [...entries.values()].filter((entry) =>
        isConfiguredGooseModelProvider(entry, modelProviderIds),
      ),
    [entries, modelProviderIds],
  );

  const getModelsForAgent = useCallback(
    (agentId: string): ModelOption[] => {
      if (agentId !== "goose") {
        return getModelsForProvider(agentId);
      }

      return configuredModelProviderEntries.flatMap((entry) =>
        entry.models.map((model) => inventoryModelToOption(model, entry)),
      );
    },
    [configuredModelProviderEntries, getModelsForProvider],
  );

  const configuredProviderIds = useMemo(
    () =>
      [...entries.values()]
        .filter((entry) => entry.configured)
        .map((entry) => entry.providerId),
    [entries],
  );

  return {
    entries,
    loading,
    getEntry,
    configuredModelProviderEntries,
    getModelsForAgent,
    getModelsForProvider,
    configuredProviderIds,
  };
}
