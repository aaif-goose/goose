import { useCallback, useMemo, useRef } from "react";
import type { AcpProvider } from "@/shared/api/acp";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import {
  getCatalogEntryFromEntries,
  resolveAgentProviderCatalogIdStrictFromEntries,
} from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { resolveSelectedAgentId } from "../lib/agentProviderResolution";
import type { ModelOption } from "../types";

interface UseAgentModelPickerStateOptions {
  providers: AcpProvider[];
  selectedProvider?: string;
  onProviderSelected: (providerId: string) => void;
  onModelSelected?: (model: ModelOption) => void;
}

const EMPTY_MODELS: ModelOption[] = [];

export function useAgentModelPickerState({
  providers,
  selectedProvider,
  onProviderSelected,
  onModelSelected,
}: UseAgentModelPickerStateOptions) {
  const catalogEntries = useProviderCatalogStore((state) => state.entries);
  const catalogLoaded = useProviderCatalogStore((state) => state.loaded);
  const {
    entries: providerInventoryEntries,
    getEntry: getProviderInventoryEntry,
    configuredModelProviderEntries,
    getModelsForAgent,
    loading: providerInventoryLoading,
  } = useProviderInventory();

  const selectedAgentId = useMemo(
    () =>
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded,
        selectedProvider,
        getProviderInventoryEntry,
      }),
    [
      catalogEntries,
      catalogLoaded,
      getProviderInventoryEntry,
      selectedProvider,
    ],
  );
  const selectedProviderInventory = getProviderInventoryEntry(selectedAgentId);

  const pickerAgents = useMemo(() => {
    const visible = new Map<string, { id: string; label: string }>();

    visible.set("goose", {
      id: "goose",
      label:
        getCatalogEntryFromEntries(catalogEntries, "goose")?.displayName ??
        "Goose",
    });

    for (const provider of providers) {
      const agentId =
        resolveAgentProviderCatalogIdStrictFromEntries(
          catalogEntries,
          provider.id,
        ) ??
        (!catalogLoaded &&
        providerInventoryEntries.get(provider.id)?.category === "agent"
          ? provider.id
          : null);
      if (!agentId || agentId === "goose") {
        continue;
      }

      const inventoryEntry = providerInventoryEntries.get(agentId);
      if (!inventoryEntry?.configured && agentId !== selectedAgentId) {
        continue;
      }

      visible.set(agentId, {
        id: agentId,
        label:
          getCatalogEntryFromEntries(catalogEntries, agentId)?.displayName ??
          provider.label,
      });
    }

    if (!visible.has(selectedAgentId)) {
      visible.set(selectedAgentId, {
        id: selectedAgentId,
        label:
          getCatalogEntryFromEntries(catalogEntries, selectedAgentId)
            ?.displayName ?? selectedAgentId,
      });
    }

    return [...visible.values()];
  }, [
    catalogEntries,
    catalogLoaded,
    providerInventoryEntries,
    providers,
    selectedAgentId,
  ]);

  const availableModels = useMemo(
    () => getModelsForAgent(selectedAgentId) ?? EMPTY_MODELS,
    [getModelsForAgent, selectedAgentId],
  );

  const modelsLoading = useMemo(() => {
    // Show loading only when we have no models to display yet.
    // If cached models exist, show them immediately — a background refresh
    // will update the list when it completes.
    if (availableModels.length > 0) {
      return false;
    }

    if (providerInventoryLoading) {
      return true;
    }

    if (selectedAgentId === "goose") {
      return (
        configuredModelProviderEntries.length > 0 &&
        configuredModelProviderEntries.some((entry) => entry.refreshing)
      );
    }

    return selectedProviderInventory?.refreshing === true;
  }, [
    availableModels.length,
    configuredModelProviderEntries,
    providerInventoryLoading,
    selectedAgentId,
    selectedProviderInventory?.refreshing,
  ]);

  const modelStatusMessage = useMemo(() => {
    if (availableModels.length > 0) {
      return null;
    }

    if (selectedAgentId === "goose") {
      const entryWithHint = configuredModelProviderEntries.find(
        (entry) => entry.modelSelectionHint || entry.lastRefreshError,
      );
      return (
        entryWithHint?.modelSelectionHint ??
        entryWithHint?.lastRefreshError ??
        null
      );
    }

    return (
      selectedProviderInventory?.modelSelectionHint ??
      selectedProviderInventory?.lastRefreshError ??
      null
    );
  }, [
    availableModels.length,
    configuredModelProviderEntries,
    selectedAgentId,
    selectedProviderInventory?.modelSelectionHint,
    selectedProviderInventory?.lastRefreshError,
  ]);

  const handleProviderChange = useCallback(
    (providerId: string) => {
      if (providerId === (selectedProvider ?? "goose")) {
        return;
      }

      onProviderSelected(providerId);
    },
    [onProviderSelected, selectedProvider],
  );

  const handleModelChange = useCallback(
    (modelId: string, selectedModelOverride?: ModelOption) => {
      const selectedModel =
        selectedModelOverride ??
        availableModels.find((model) => model.id === modelId);
      onModelSelected?.({
        id: modelId,
        name: selectedModel?.name ?? modelId,
        displayName: selectedModel?.displayName ?? modelId,
        provider: selectedModel?.provider,
        providerId: selectedModel?.providerId,
        providerName: selectedModel?.providerName,
        contextLimit: selectedModel?.contextLimit,
        recommended: selectedModel?.recommended,
      });
    },
    [availableModels, onModelSelected],
  );

  const refreshingRef = useRef(false);
  const handlePickerOpen = useCallback(() => {
    if (refreshingRef.current || useProviderInventoryStore.getState().loading) {
      return;
    }
    refreshingRef.current = true;
    import("@/features/providers/api/inventory")
      .then(({ backgroundRefreshInventory }) =>
        backgroundRefreshInventory(useProviderInventoryStore.getState()),
      )
      .catch((err) =>
        console.error("Failed to background-refresh inventory:", err),
      )
      .finally(() => {
        refreshingRef.current = false;
      });
  }, []);

  return {
    selectedAgentId,
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
    handlePickerOpen,
  };
}
