import { useCallback, useMemo } from "react";
import type { AcpProvider } from "@/shared/api/acp";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import {
  getCatalogEntry,
  resolveAgentProviderCatalogIdStrict,
} from "@/features/providers/providerCatalog";
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
  const {
    entries: providerInventoryEntries,
    getEntry: getProviderInventoryEntry,
    configuredModelProviderEntries,
    getModelsForAgent,
    loading: providerInventoryLoading,
  } = useProviderInventory();

  const selectedAgentId = selectedProvider
    ? (resolveAgentProviderCatalogIdStrict(selectedProvider) ?? "goose")
    : "goose";
  const selectedProviderInventory = getProviderInventoryEntry(selectedAgentId);

  const pickerAgents = useMemo(() => {
    const visible = new Map<string, { id: string; label: string }>();

    visible.set("goose", {
      id: "goose",
      label: getCatalogEntry("goose")?.displayName ?? "Goose",
    });

    for (const provider of providers) {
      const agentId = resolveAgentProviderCatalogIdStrict(provider.id);
      if (!agentId || agentId === "goose") {
        continue;
      }

      const inventoryEntry = providerInventoryEntries.get(agentId);
      if (!inventoryEntry?.configured && agentId !== selectedAgentId) {
        continue;
      }

      visible.set(agentId, {
        id: agentId,
        label: getCatalogEntry(agentId)?.displayName ?? provider.label,
      });
    }

    if (!visible.has(selectedAgentId)) {
      visible.set(selectedAgentId, {
        id: selectedAgentId,
        label: getCatalogEntry(selectedAgentId)?.displayName ?? selectedAgentId,
      });
    }

    return [...visible.values()];
  }, [providerInventoryEntries, providers, selectedAgentId]);

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
    (modelId: string) => {
      const selectedModel = availableModels.find(
        (model) => model.id === modelId,
      );
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

  return {
    selectedAgentId,
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
  };
}
