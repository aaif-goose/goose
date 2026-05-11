import { useCallback, useMemo, useState } from "react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import {
  getModelProviders,
  resolveAgentProviderCatalogIdStrict,
} from "@/features/providers/providerCatalog";
import { useProviderInventory } from "@/features/providers/hooks/useProviderInventory";
import { useDistroStore } from "@/features/settings/stores/distroStore";
import { filterModelProvidersForDistro } from "@/features/providers/distroProviderConstraints";
import { getStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import {
  ONBOARDING_STORAGE_KEY,
  type OnboardingCompletion,
  type OnboardingReadiness,
} from "../types";

function readCompletion(): OnboardingCompletion | null {
  try {
    const raw = localStorage.getItem(ONBOARDING_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<OnboardingCompletion>;
    if (!parsed.completedAt || !parsed.providerId) return null;
    return {
      completedAt: parsed.completedAt,
      providerId: parsed.providerId,
      modelId: parsed.modelId,
    };
  } catch {
    return null;
  }
}

function writeCompletion(completion: OnboardingCompletion) {
  localStorage.setItem(ONBOARDING_STORAGE_KEY, JSON.stringify(completion));
}

export function resetOnboardingCompletion() {
  localStorage.removeItem(ONBOARDING_STORAGE_KEY);
}

function firstUsableModel(entry: ProviderInventoryEntryDto) {
  return (
    entry.models.find((model) => model.recommended) ??
    entry.models.find((model) => model.id === entry.defaultModel) ??
    entry.models[0]
  );
}

export function useOnboardingGate(startupReady: boolean) {
  const selectedProvider = useAgentStore((state) => state.selectedProvider);
  const { entries, configuredModelProviderEntries, getModelsForAgent } =
    useProviderInventory();
  const distro = useDistroStore((state) => state.manifest);
  const [completion, setCompletion] = useState<OnboardingCompletion | null>(
    readCompletion,
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

  const readiness = useMemo<OnboardingReadiness>(() => {
    const selectedAgentId =
      resolveAgentProviderCatalogIdStrict(selectedProvider) ?? "goose";

    if (selectedAgentId !== "goose") {
      const models = getModelsForAgent(selectedAgentId);
      const entry = entries.get(selectedAgentId);
      const isReady = !!entry?.configured && models.length > 0;
      if (isReady) {
        return {
          hasCompletedOnboarding: !!completion,
          isUsable: true,
          providerId: selectedAgentId,
          modelId: models[0]?.id,
          modelName: models[0]?.name,
          reason: "ready",
        };
      }
    }

    const storedGooseModel = getStoredModelPreference("goose");
    if (storedGooseModel) {
      const entry = entries.get(storedGooseModel.providerId ?? "");
      const modelStillExists = entry?.models.some(
        (model) => model.id === storedGooseModel.modelId,
      );
      if (entry?.configured && modelStillExists) {
        return {
          hasCompletedOnboarding: !!completion,
          isUsable: true,
          providerId: storedGooseModel.providerId ?? "goose",
          modelId: storedGooseModel.modelId,
          modelName: storedGooseModel.modelName,
          reason: "ready",
        };
      }
    }

    const configuredEntry =
      configuredModelProviderEntries.find(
        (entry) =>
          (entry.providerType === "Custom" ||
            modelProviderIds.has(entry.providerId)) &&
          firstUsableModel(entry),
      ) ?? null;
    const model = configuredEntry ? firstUsableModel(configuredEntry) : null;

    if (configuredEntry && model) {
      return {
        hasCompletedOnboarding: !!completion,
        isUsable: true,
        providerId: configuredEntry.providerId,
        modelId: model.id,
        modelName: model.name,
        reason: "ready",
      };
    }

    return {
      hasCompletedOnboarding: !!completion,
      isUsable: false,
      providerId: null,
      reason: completion ? "missing_provider" : "not_completed",
    };
  }, [
    completion,
    configuredModelProviderEntries,
    entries,
    getModelsForAgent,
    modelProviderIds,
    selectedProvider,
  ]);

  const completeOnboarding = useCallback(
    (next: Omit<OnboardingCompletion, "completedAt">) => {
      const completionValue = {
        ...next,
        completedAt: new Date().toISOString(),
      };
      writeCompletion(completionValue);
      setCompletion(completionValue);
    },
    [],
  );

  const resetOnboarding = useCallback(() => {
    resetOnboardingCompletion();
    setCompletion(null);
  }, []);

  return {
    completion,
    readiness,
    shouldShowOnboarding:
      startupReady &&
      (!readiness.hasCompletedOnboarding || !readiness.isUsable),
    completeOnboarding,
    resetOnboarding,
  };
}
