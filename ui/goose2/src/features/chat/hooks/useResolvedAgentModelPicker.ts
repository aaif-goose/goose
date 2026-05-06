import { useEffect, useMemo, useState } from "react";
import type { AcpProvider } from "@/shared/api/acp";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import { resolveAgentProviderCatalogIdStrictFromEntries } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { getClient } from "@/shared/api/acpConnection";
import { acpSetModel } from "@/shared/api/acp";
import {
  useChatSessionStore,
  type ChatSession,
} from "../stores/chatSessionStore";
import { useAgentModelPickerState } from "./useAgentModelPickerState";
import {
  clearStoredModelPreference,
  getStoredModelPreference,
  setStoredModelPreference,
} from "../lib/modelPreferences";
import { resolveSelectedAgentId } from "../lib/agentProviderResolution";

const MODEL_ALIAS_IDS = new Set(["current", "default"]);

export type PreferredModelSelection = {
  id: string;
  name: string;
  providerId?: string;
  source: "default" | "explicit";
};

interface UseResolvedAgentModelPickerOptions {
  providers: AcpProvider[];
  selectedProvider: string;
  sessionId: string | null;
  session?: ChatSession;
  pendingModelSelection: PreferredModelSelection | null | undefined;
  setPendingProviderId: (providerId: string | undefined) => void;
  setPendingModelSelection: (
    selection: PreferredModelSelection | null | undefined,
  ) => void;
  setGlobalSelectedProvider: (providerId: string) => void;
  prepareSelectedProvider: (
    providerId: string,
    modelSelection?: PreferredModelSelection | null,
  ) => Promise<void>;
}

function isModelAlias(modelId?: string | null): boolean {
  return modelId != null && MODEL_ALIAS_IDS.has(modelId);
}

export function useResolvedAgentModelPicker({
  providers,
  selectedProvider,
  sessionId,
  session,
  pendingModelSelection,
  setPendingProviderId,
  setPendingModelSelection,
  setGlobalSelectedProvider,
  prepareSelectedProvider,
}: UseResolvedAgentModelPickerOptions) {
  const catalogEntries = useProviderCatalogStore((state) => state.entries);
  const catalogLoaded = useProviderCatalogStore((state) => state.loaded);
  const { getEntry: getProviderInventoryEntry } = useProviderInventory();
  const [gooseDefaultSelection, setGooseDefaultSelection] =
    useState<PreferredModelSelection | null>(null);

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
  const concreteSelectedProviderId = useMemo(() => {
    const resolvedAgentId = resolveAgentProviderCatalogIdStrictFromEntries(
      catalogEntries,
      selectedProvider,
    );
    if (resolvedAgentId) {
      return null;
    }

    if (!catalogLoaded) {
      const inventoryEntry = getProviderInventoryEntry(selectedProvider);
      return inventoryEntry?.category === "model" ? selectedProvider : null;
    }

    return selectedProvider;
  }, [
    catalogEntries,
    catalogLoaded,
    getProviderInventoryEntry,
    selectedProvider,
  ]);
  const storedModelPreference = useMemo(
    () => getStoredModelPreference(selectedAgentId),
    [selectedAgentId],
  );

  const getPreferredSelectionForAgent = useMemo(
    () => (agentId: string, fallbackProviderId?: string) => {
      const preferredModel = getStoredModelPreference(agentId);
      if (preferredModel) {
        return {
          id: preferredModel.modelId,
          name: preferredModel.modelName,
          providerId: preferredModel.providerId ?? fallbackProviderId,
          source: "explicit" as const,
        };
      }

      if (agentId === "goose") {
        if (!gooseDefaultSelection) {
          return null;
        }

        return {
          ...gooseDefaultSelection,
          providerId: gooseDefaultSelection.providerId ?? fallbackProviderId,
        };
      }

      const inventoryEntry = getProviderInventoryEntry(agentId);
      if (!inventoryEntry) {
        return null;
      }

      const resolvedInventoryModel =
        inventoryEntry.models.find((model) => model.recommended) ??
        inventoryEntry.models.find((model) => !isModelAlias(model.id)) ??
        inventoryEntry.models[0];

      if (!resolvedInventoryModel) {
        return null;
      }

      return {
        id: resolvedInventoryModel.id,
        name: resolvedInventoryModel.name,
        providerId:
          inventoryEntry.providerId === agentId
            ? inventoryEntry.providerId
            : fallbackProviderId,
        source: "default" as const,
      };
    },
    [getProviderInventoryEntry, gooseDefaultSelection],
  );

  useEffect(() => {
    if (selectedAgentId !== "goose") {
      setGooseDefaultSelection(null);
      return;
    }

    let cancelled = false;

    const loadGooseDefaultSelection = async () => {
      try {
        const client = await getClient();
        const defaults = await client.goose.GooseDefaultsRead({});

        if (cancelled) {
          return;
        }

        const providerId = defaults.providerId ?? undefined;
        const modelId = defaults.modelId ?? undefined;

        if (!modelId) {
          setGooseDefaultSelection(null);
          return;
        }

        setGooseDefaultSelection({
          id: modelId,
          name: modelId,
          providerId,
          source: "default",
        });
      } catch {
        if (!cancelled) {
          setGooseDefaultSelection(null);
        }
      }
    };

    void loadGooseDefaultSelection();

    return () => {
      cancelled = true;
    };
  }, [selectedAgentId]);

  const {
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
    handlePickerOpen,
  } = useAgentModelPickerState({
    providers,
    selectedProvider,
    onProviderSelected: (providerId) => {
      const requestedAgentId = resolveAgentProviderCatalogIdStrictFromEntries(
        catalogEntries,
        providerId,
      );
      const resolvedRequestedAgentId =
        requestedAgentId ??
        resolveSelectedAgentId({
          catalogEntries,
          catalogLoaded,
          selectedProvider: providerId,
          getProviderInventoryEntry,
        });
      const preferredModelSelection = getPreferredSelectionForAgent(
        resolvedRequestedAgentId,
        providerId,
      );
      const nextProviderId = requestedAgentId
        ? (preferredModelSelection?.providerId ?? providerId)
        : providerId;
      const nextModelSelection =
        !requestedAgentId &&
        preferredModelSelection?.providerId &&
        preferredModelSelection.providerId !== providerId
          ? undefined
          : preferredModelSelection
            ? {
                ...preferredModelSelection,
                providerId:
                  requestedAgentId == null
                    ? providerId
                    : preferredModelSelection.providerId,
              }
            : undefined;

      if (!sessionId) {
        setGlobalSelectedProvider(nextProviderId);
        setPendingProviderId(nextProviderId);
        setPendingModelSelection(nextModelSelection);
        return;
      }

      useChatSessionStore
        .getState()
        .switchSessionProvider(sessionId, nextProviderId);
      setGlobalSelectedProvider(nextProviderId);
      void prepareSelectedProvider(nextProviderId, nextModelSelection).catch(
        (error) => {
          console.error("Failed to update ACP session provider:", error);
        },
      );
    },
    onModelSelected: (model) => {
      const modelId = model.id;
      const modelName = model.displayName ?? model.name ?? model.id;
      const nextProviderId = model.providerId ?? selectedProvider;
      const nextStoredModelPreference = {
        modelId,
        modelName,
        providerId: nextProviderId,
      };

      if (!sessionId) {
        if (nextProviderId && nextProviderId !== selectedProvider) {
          setPendingProviderId(nextProviderId);
          setGlobalSelectedProvider(nextProviderId);
        }
        setPendingModelSelection({
          id: modelId,
          name: modelName,
          providerId: nextProviderId,
          source: "explicit",
        });
        return;
      }

      if (
        !session ||
        (modelId === session.modelId &&
          (!nextProviderId || nextProviderId === session.providerId))
      ) {
        return;
      }

      const previousStoredModelPreference =
        getStoredModelPreference(selectedAgentId);
      const previousProviderId = session.providerId;
      const previousModelId = session.modelId;
      const previousModelName = session.modelName;
      const providerChanged =
        Boolean(nextProviderId) && nextProviderId !== session.providerId;

      if (providerChanged && nextProviderId) {
        useChatSessionStore
          .getState()
          .switchSessionProvider(sessionId, nextProviderId);
        setGlobalSelectedProvider(nextProviderId);
      }

      useChatSessionStore.getState().updateSession(sessionId, {
        modelId,
        modelName,
      });

      void (async () => {
        try {
          if (providerChanged && nextProviderId) {
            await prepareSelectedProvider(nextProviderId);
          }
          await acpSetModel(sessionId, modelId);
          setStoredModelPreference(selectedAgentId, nextStoredModelPreference);
        } catch (error) {
          console.error("Failed to set model:", error);
          if (providerChanged && previousProviderId) {
            setGlobalSelectedProvider(previousProviderId);
          }
          if (previousStoredModelPreference) {
            setStoredModelPreference(
              selectedAgentId,
              previousStoredModelPreference,
            );
          } else {
            clearStoredModelPreference(selectedAgentId);
          }
          useChatSessionStore.getState().updateSession(sessionId, {
            providerId: previousProviderId,
            modelId: previousModelId,
            modelName: previousModelName,
          });
          void (async () => {
            try {
              if (providerChanged && previousProviderId) {
                await prepareSelectedProvider(previousProviderId);
              }
              if (previousModelId) {
                await acpSetModel(sessionId, previousModelId);
              }
            } catch (rollbackError) {
              console.error(
                "Failed to restore previous provider/model after setModel failure:",
                rollbackError,
              );
            }
          })();
        }
      })();
    },
  });

  const preferredModelSelection =
    useMemo<PreferredModelSelection | null>(() => {
      if (storedModelPreference) {
        const matchingStoredModel =
          availableModels.find(
            (model) =>
              model.id === storedModelPreference.modelId &&
              (!storedModelPreference.providerId ||
                !model.providerId ||
                model.providerId === storedModelPreference.providerId) &&
              (!concreteSelectedProviderId ||
                !model.providerId ||
                model.providerId === concreteSelectedProviderId),
          ) ?? null;
        const storedSelectionCompatible =
          !concreteSelectedProviderId ||
          storedModelPreference.providerId === concreteSelectedProviderId;

        if (
          matchingStoredModel ||
          ((availableModels.length === 0 || modelsLoading) &&
            storedSelectionCompatible)
        ) {
          return {
            id: storedModelPreference.modelId,
            name:
              matchingStoredModel?.displayName ??
              matchingStoredModel?.name ??
              storedModelPreference.modelName,
            providerId:
              matchingStoredModel?.providerId ??
              storedModelPreference.providerId,
            source: "explicit",
          };
        }
      }

      const inventoryDefaultSelection = getPreferredSelectionForAgent(
        selectedAgentId,
        selectedProvider,
      );

      if (!inventoryDefaultSelection) {
        return null;
      }

      const matchingDefaultModel =
        availableModels.find(
          (model) =>
            model.id === inventoryDefaultSelection.id &&
            (!inventoryDefaultSelection.providerId ||
              !model.providerId ||
              model.providerId === inventoryDefaultSelection.providerId) &&
            (!concreteSelectedProviderId ||
              !model.providerId ||
              model.providerId === concreteSelectedProviderId),
        ) ?? null;
      const defaultSelectionCompatible =
        !concreteSelectedProviderId ||
        inventoryDefaultSelection.providerId === concreteSelectedProviderId;

      if (!matchingDefaultModel && !defaultSelectionCompatible) {
        return null;
      }

      return {
        id: inventoryDefaultSelection.id,
        name:
          matchingDefaultModel?.displayName ??
          matchingDefaultModel?.name ??
          inventoryDefaultSelection.name,
        providerId:
          matchingDefaultModel?.providerId ??
          inventoryDefaultSelection.providerId,
        source: "default",
      };
    }, [
      availableModels,
      getPreferredSelectionForAgent,
      modelsLoading,
      concreteSelectedProviderId,
      selectedProvider,
      selectedAgentId,
      storedModelPreference,
    ]);

  const sessionModelSelection = useMemo<PreferredModelSelection | null>(() => {
    if (!session?.modelId) {
      return null;
    }

    const matchingSessionModel =
      availableModels.find(
        (model) =>
          model.id === session.modelId &&
          (!session.providerId ||
            !model.providerId ||
            model.providerId === session.providerId),
      ) ?? null;

    if (matchingSessionModel) {
      return {
        id: matchingSessionModel.id,
        name:
          matchingSessionModel.displayName ??
          matchingSessionModel.name ??
          session.modelName ??
          session.modelId,
        providerId: matchingSessionModel.providerId ?? session.providerId,
        source: "explicit",
      };
    }

    if (isModelAlias(session.modelId)) {
      return null;
    }

    return {
      id: session.modelId,
      name: session.modelName ?? session.modelId,
      providerId: session.providerId,
      source: "explicit",
    };
  }, [availableModels, session]);

  const effectiveModelSelection =
    pendingModelSelection !== undefined
      ? pendingModelSelection
      : (sessionModelSelection ?? preferredModelSelection);

  return {
    selectedAgentId,
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
    handlePickerOpen,
    effectiveModelSelection,
  };
}
