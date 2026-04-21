import { useEffect, useMemo, useState } from "react";
import type { AcpProvider } from "@/shared/api/acp";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import { resolveAgentProviderCatalogIdStrict } from "@/features/providers/providerCatalog";
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

const GOOSE_PROVIDER_CONFIG_KEY = "GOOSE_PROVIDER";
const GOOSE_MODEL_CONFIG_KEY = "GOOSE_MODEL";

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
  const { getEntry: getProviderInventoryEntry } = useProviderInventory();
  const [gooseDefaultSelection, setGooseDefaultSelection] =
    useState<PreferredModelSelection | null>(null);

  const selectedAgentId =
    resolveAgentProviderCatalogIdStrict(selectedProvider) ?? "goose";
  const storedModelPreference = useMemo(
    () => getStoredModelPreference(selectedAgentId),
    [selectedAgentId],
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
        const [providerResponse, modelResponse] = await Promise.all([
          client.goose.GooseConfigRead({ key: GOOSE_PROVIDER_CONFIG_KEY }),
          client.goose.GooseConfigRead({ key: GOOSE_MODEL_CONFIG_KEY }),
        ]);

        if (cancelled) {
          return;
        }

        const providerId =
          typeof providerResponse.value === "string"
            ? providerResponse.value
            : undefined;
        const modelId =
          typeof modelResponse.value === "string"
            ? modelResponse.value
            : undefined;

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
  } = useAgentModelPickerState({
    providers,
    selectedProvider,
    onProviderSelected: (providerId) => {
      const nextAgentId =
        resolveAgentProviderCatalogIdStrict(providerId) ?? "goose";
      const preferredModel = getStoredModelPreference(nextAgentId);
      const nextProviderId = preferredModel?.providerId ?? providerId;

      if (!sessionId) {
        setGlobalSelectedProvider(nextProviderId);
        setPendingProviderId(nextProviderId);
        setPendingModelSelection(
          preferredModel
            ? {
                id: preferredModel.modelId,
                name: preferredModel.modelName,
                providerId: preferredModel.providerId,
                source: "explicit",
              }
            : undefined,
        );
        return;
      }

      useChatSessionStore
        .getState()
        .switchSessionProvider(sessionId, nextProviderId);
      setGlobalSelectedProvider(nextProviderId);
      void prepareSelectedProvider(
        nextProviderId,
        preferredModel
          ? {
              id: preferredModel.modelId,
              name: preferredModel.modelName,
              providerId: preferredModel.providerId,
              source: "explicit",
            }
          : null,
      ).catch((error) => {
        console.error("Failed to update ACP session provider:", error);
      });
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
        setStoredModelPreference(selectedAgentId, nextStoredModelPreference);
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
                model.providerId === storedModelPreference.providerId),
          ) ?? null;

        if (
          matchingStoredModel ||
          availableModels.length === 0 ||
          modelsLoading
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

      const inventoryDefaultSelection =
        selectedAgentId === "goose"
          ? gooseDefaultSelection
          : (() => {
              const inventoryEntry = getProviderInventoryEntry(selectedAgentId);
              if (!inventoryEntry?.defaultModel) {
                return null;
              }

              return {
                id: inventoryEntry.defaultModel,
                name: inventoryEntry.defaultModel,
                providerId:
                  inventoryEntry.providerId === selectedAgentId
                    ? inventoryEntry.providerId
                    : undefined,
                source: "default" as const,
              };
            })();

      if (!inventoryDefaultSelection) {
        return null;
      }

      const matchingDefaultModel =
        availableModels.find(
          (model) =>
            model.id === inventoryDefaultSelection.id &&
            (!inventoryDefaultSelection.providerId ||
              !model.providerId ||
              model.providerId === inventoryDefaultSelection.providerId),
        ) ?? null;

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
      getProviderInventoryEntry,
      gooseDefaultSelection,
      modelsLoading,
      selectedAgentId,
      storedModelPreference,
    ]);

  const effectiveModelSelection =
    pendingModelSelection !== undefined
      ? pendingModelSelection
      : session?.modelId
        ? {
            id: session.modelId,
            name: session.modelName ?? session.modelId,
            providerId: session.providerId,
            source: "explicit" as const,
          }
        : preferredModelSelection;

  return {
    selectedAgentId,
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
    effectiveModelSelection,
  };
}
