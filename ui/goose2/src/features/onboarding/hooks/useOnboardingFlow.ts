import { useState } from "react";
import { setStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import { getAgentProviders } from "@/features/providers/providerCatalog";
import { useOnboardingImportStep } from "./useOnboardingImportStep";
import { useOnboardingProviderStep } from "./useOnboardingProviderStep";
import { useOnboardingReadyStep } from "./useOnboardingReadyStep";
import type {
  OnboardingReadiness,
  OnboardingStep,
  SelectedSetup,
  TFunctionLike,
} from "../types";

interface UseOnboardingFlowParams {
  readiness: OnboardingReadiness;
  onComplete: (setup: SelectedSetup) => void;
  t: TFunctionLike;
}

export function useOnboardingFlow({
  readiness,
  onComplete,
  t,
}: UseOnboardingFlowParams) {
  const [step, setStep] = useState<OnboardingStep>("import");
  const [selectedSetup, setSelectedSetup] = useState<SelectedSetup | null>(
    () =>
      readiness.isUsable && readiness.providerId
        ? {
            providerId: readiness.providerId,
            modelId: readiness.modelId,
            modelName: readiness.modelName,
          }
        : null,
  );

  const importStep = useOnboardingImportStep({
    t,
    onProviderDefaults: setSelectedSetup,
    onContinue: () => setStep("provider"),
  });

  const providerStep = useOnboardingProviderStep({
    readiness,
    t,
    onSelectedSetup: setSelectedSetup,
    onReady: () => setStep("tour"),
  });

  const readyStep = useOnboardingReadyStep(step);

  function finish() {
    const setup =
      selectedSetup ??
      (readiness.providerId
        ? {
            providerId: readiness.providerId,
            modelId: readiness.modelId,
            modelName: readiness.modelName,
          }
        : null);
    if (!setup) {
      setStep("provider");
      return;
    }
    if (
      setup.modelId &&
      !getAgentProviders().some((provider) => provider.id === setup.providerId)
    ) {
      setStoredModelPreference("goose", {
        providerId: setup.providerId,
        modelId: setup.modelId,
        modelName: setup.modelName ?? setup.modelId,
      });
    }
    onComplete(setup);
  }

  return {
    step,
    stepOrder: ["import", "provider", "tour"] as const,
    importStep: importStep.props,
    providerStep,
    readyStep: {
      availableSkillCount: readyStep.availableSkillCount,
      setupCounts: importStep.setupCounts,
      selectedSetup,
      onBack: () => setStep("provider"),
      onFinish: finish,
    },
  };
}
