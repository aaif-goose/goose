import { useMemo, useState } from "react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { setStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import {
  getAgentProviders,
  getModelProviders,
} from "@/features/providers/providerCatalog";
import { filterModelProvidersForDistro } from "@/features/providers/distroProviderConstraints";
import { useCredentials } from "@/features/providers/hooks/useCredentials";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { useDistroStore } from "@/features/settings/stores/distroStore";
import { saveDefaults } from "../api/onboarding";
import {
  firstUsableModel,
  PROMOTED_MODEL_ORDER,
} from "../lib/providerDefaults";
import type {
  OnboardingReadiness,
  SelectedSetup,
  TFunctionLike,
  UsableDefaultEntry,
} from "../types";

interface UseOnboardingProviderStepParams {
  readiness: OnboardingReadiness;
  t: TFunctionLike;
  onSelectedSetup: (setup: SelectedSetup) => void;
  onReady: () => void;
}

export function useOnboardingProviderStep({
  readiness,
  t,
  onSelectedSetup,
  onReady,
}: UseOnboardingProviderStepParams) {
  const [providerError, setProviderError] = useState("");
  const [showAllProviders, setShowAllProviders] = useState(false);
  const [selectingProviderId, setSelectingProviderId] = useState<string | null>(
    null,
  );

  const inventoryEntries = useProviderInventoryStore((state) => state.entries);
  const agentStore = useAgentStore();
  const distro = useDistroStore((state) => state.manifest);

  const {
    configuredIds,
    loading: credentialLoading,
    savingProviderIds,
    syncingProviderIds,
    inventoryWarnings,
    getConfig,
    save,
    remove,
    completeNativeSetup,
  } = useCredentials();

  const modelProviders = useMemo(() => {
    const all = filterModelProvidersForDistro(getModelProviders(), distro);
    return [...all].sort((a, b) => {
      const aIndex = PROMOTED_MODEL_ORDER.indexOf(a.id);
      const bIndex = PROMOTED_MODEL_ORDER.indexOf(b.id);
      if (aIndex !== -1 || bIndex !== -1) {
        return (aIndex === -1 ? 99 : aIndex) - (bIndex === -1 ? 99 : bIndex);
      }
      return a.displayName.localeCompare(b.displayName);
    });
  }, [distro]);

  const visibleModelProviders = modelProviders.filter(
    (provider) =>
      showAllProviders ||
      provider.group !== "additional" ||
      configuredIds.has(provider.id) ||
      inventoryEntries.get(provider.id)?.configured,
  );

  const usableModelEntries = useMemo(
    () =>
      [...inventoryEntries.values()]
        .filter((entry) => entry.configured && firstUsableModel(entry))
        .filter((entry) =>
          modelProviders.some((provider) => provider.id === entry.providerId),
        ),
    [inventoryEntries, modelProviders],
  );

  const usableAgentEntries = useMemo(
    () =>
      [...inventoryEntries.values()].filter(
        (entry) =>
          entry.configured &&
          entry.providerId !== "goose" &&
          getAgentProviders().some(
            (provider) => provider.id === entry.providerId,
          ) &&
          entry.models.length > 0,
      ),
    [inventoryEntries],
  );

  const usableDefaultEntries = useMemo<UsableDefaultEntry[]>(
    () => [
      ...usableModelEntries.map((entry) => ({
        kind: "model" as const,
        entry,
      })),
      ...usableAgentEntries.map((entry) => ({
        kind: "agent" as const,
        entry,
      })),
    ],
    [usableAgentEntries, usableModelEntries],
  );

  async function selectModelProvider(entry: ProviderInventoryEntryDto) {
    const model = firstUsableModel(entry);
    if (!model) {
      setProviderError(t("onboarding:provider.noModels"));
      return;
    }

    setProviderError("");
    setSelectingProviderId(entry.providerId);
    try {
      await saveDefaults({ providerId: entry.providerId, modelId: model.id });
      const setup = {
        providerId: entry.providerId,
        modelId: model.id,
        modelName: model.name,
      };
      setStoredModelPreference("goose", setup);
      agentStore.setSelectedProvider("goose");
      onSelectedSetup(setup);
      onReady();
    } catch (error) {
      setProviderError(
        error instanceof Error
          ? error.message
          : t("onboarding:provider.selectFailed"),
      );
    } finally {
      setSelectingProviderId(null);
    }
  }

  function selectAgentProvider(entry: ProviderInventoryEntryDto) {
    const model = firstUsableModel(entry);
    agentStore.setSelectedProvider(entry.providerId);
    onSelectedSetup({
      providerId: entry.providerId,
      modelId: model?.id,
      modelName: model?.name,
    });
    onReady();
  }

  async function continueWithCurrentDefault() {
    if (!readiness.isUsable || !readiness.providerId) {
      return;
    }

    const setup = {
      providerId: readiness.providerId,
      modelId: readiness.modelId,
      modelName: readiness.modelName,
    };
    const isAgentProvider = getAgentProviders().some(
      (provider) => provider.id === readiness.providerId,
    );

    setProviderError("");
    setSelectingProviderId(readiness.providerId);
    try {
      if (isAgentProvider) {
        agentStore.setSelectedProvider(readiness.providerId);
      } else {
        if (!readiness.modelId || !readiness.modelName) {
          setProviderError(t("onboarding:provider.noModels"));
          return;
        }
        await saveDefaults({
          providerId: readiness.providerId,
          modelId: readiness.modelId,
        });
        setStoredModelPreference("goose", {
          providerId: readiness.providerId,
          modelId: readiness.modelId,
          modelName: readiness.modelName,
        });
        agentStore.setSelectedProvider("goose");
      }
      onSelectedSetup(setup);
      onReady();
    } catch (error) {
      setProviderError(
        error instanceof Error
          ? error.message
          : t("onboarding:provider.selectFailed"),
      );
    } finally {
      setSelectingProviderId(null);
    }
  }

  return {
    credentialLoading,
    modelProviders: visibleModelProviders,
    canBrowseAllProviders:
      !showAllProviders && visibleModelProviders.length < modelProviders.length,
    usableDefaultEntries,
    configuredIds,
    savingProviderIds,
    syncingProviderIds,
    inventoryWarnings,
    selectingProviderId,
    providerError,
    onGetConfig: getConfig,
    onSave: save,
    onRemove: remove,
    onCompleteNativeSetup: completeNativeSetup,
    onSelectModelProvider: (entry: ProviderInventoryEntryDto) =>
      void selectModelProvider(entry),
    onSelectAgentProvider: selectAgentProvider,
    onBrowseAllProviders: () => setShowAllProviders(true),
    onContinue: () => void continueWithCurrentDefault(),
  };
}
