import { useEffect, useState } from "react";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { setStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import { getProviderInventory } from "@/features/providers/api/inventory";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import {
  applyOnboardingImports,
  scanOnboardingImports,
} from "../api/onboarding";
import {
  emptyImportCounts,
  hasImportableCounts,
  sumImportCounts,
} from "../lib/importCounts";
import type {
  OnboardingImportCandidate,
  OnboardingImportCounts,
  SelectedSetup,
  TFunctionLike,
} from "../types";

interface UseOnboardingImportStepParams {
  t: TFunctionLike;
  onProviderDefaults: (setup: SelectedSetup) => void;
  onContinue: () => void;
}

export function useOnboardingImportStep({
  t,
  onProviderDefaults,
  onContinue,
}: UseOnboardingImportStepParams) {
  const [candidates, setCandidates] = useState<OnboardingImportCandidate[]>([]);
  const [selectedCandidateIds, setSelectedCandidateIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [scanLoading, setScanLoading] = useState(true);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState("");
  const [setupCounts, setSetupCounts] =
    useState<OnboardingImportCounts>(emptyImportCounts);

  const setInventoryEntries = useProviderInventoryStore(
    (state) => state.setEntries,
  );
  const agentStore = useAgentStore();

  useEffect(() => {
    let cancelled = false;
    setScanLoading(true);
    scanOnboardingImports()
      .then((nextCandidates) => {
        if (cancelled) return;
        const importable = nextCandidates.filter(hasImportableCounts);
        setCandidates(importable);
        setSelectedCandidateIds(new Set(importable.map((item) => item.id)));
      })
      .catch((error) => {
        if (!cancelled) {
          setImportError(
            error instanceof Error
              ? error.message
              : t("onboarding:import.scanFailed"),
          );
        }
      })
      .finally(() => {
        if (!cancelled) {
          setScanLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [t]);

  async function refreshInventory() {
    const entries = await getProviderInventory();
    setInventoryEntries(entries);
    return entries;
  }

  function toggleCandidate(id: string) {
    setSelectedCandidateIds((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function skipImport() {
    setSetupCounts(emptyImportCounts());
    onContinue();
  }

  async function applyImports() {
    if (selectedCandidateIds.size === 0) {
      skipImport();
      return;
    }

    setImporting(true);
    setImportError("");
    try {
      const result = await applyOnboardingImports({
        candidateIds: [...selectedCandidateIds],
        enableImportedExtensions: false,
      });
      setSetupCounts(sumImportCounts(result.imported, result.skipped));
      await refreshInventory();
      if (
        result.providerDefaults?.providerId &&
        result.providerDefaults.modelId
      ) {
        const setup = {
          providerId: result.providerDefaults.providerId,
          modelId: result.providerDefaults.modelId,
          modelName: result.providerDefaults.modelId,
        };
        setStoredModelPreference("goose", setup);
        agentStore.setSelectedProvider("goose");
        onProviderDefaults(setup);
      }
      onContinue();
    } catch (error) {
      setImportError(
        error instanceof Error ? error.message : t("onboarding:import.failed"),
      );
    } finally {
      setImporting(false);
    }
  }

  return {
    setupCounts,
    props: {
      candidates,
      selectedCandidateIds,
      scanLoading,
      importing,
      importError,
      onToggleCandidate: toggleCandidate,
      onSkip: skipImport,
      onContinue: () => void applyImports(),
    },
  };
}
