import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { useOnboardingFlow } from "../hooks/useOnboardingFlow";
import type { OnboardingReadiness, SelectedSetup } from "../types";
import { ImportStep } from "./ImportStep";
import { ProviderStep } from "./ProviderStep";
import { ReadyStep } from "./ReadyStep";

interface OnboardingFlowProps {
  readiness: OnboardingReadiness;
  onComplete: (setup: SelectedSetup) => void;
}

export function OnboardingFlow({ readiness, onComplete }: OnboardingFlowProps) {
  const { t } = useTranslation(["onboarding", "common", "settings"]);
  const flow = useOnboardingFlow({ readiness, onComplete, t });

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <div
        className="flex h-11 shrink-0 items-center justify-center"
        data-tauri-drag-region
      >
        <div className="text-xs font-medium text-foreground">
          {t("onboarding:windowTitle")}
        </div>
      </div>

      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="flex min-h-full w-full flex-col items-center justify-center px-6 py-10">
          <div className="flex w-full max-w-[520px] flex-col items-center text-center">
            {flow.step === "import" ? (
              <ImportStep {...flow.importStep} t={t} />
            ) : null}

            {flow.step === "provider" ? (
              <ProviderStep {...flow.providerStep} t={t} />
            ) : null}

            {flow.step === "tour" ? (
              <ReadyStep {...flow.readyStep} t={t} />
            ) : null}
          </div>
        </div>
      </main>

      <div className="flex h-14 shrink-0 items-center justify-center gap-1.5">
        {flow.stepOrder.map((item) => (
          <span
            key={item}
            aria-hidden="true"
            className={cn(
              "size-1.5 rounded-full transition-colors",
              item === flow.step ? "bg-foreground" : "bg-border",
            )}
          />
        ))}
      </div>
    </div>
  );
}
