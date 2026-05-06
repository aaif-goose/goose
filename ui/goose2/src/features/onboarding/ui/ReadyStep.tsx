import { IconArrowRight, IconSparkles } from "@tabler/icons-react";
import { formatCount } from "../lib/importCounts";
import type {
  OnboardingImportCounts,
  SelectedSetup,
  TFunctionLike,
} from "../types";
import { Button } from "@/shared/ui/button";

interface ReadyStepProps {
  availableSkillCount: number | null;
  setupCounts: OnboardingImportCounts;
  selectedSetup: SelectedSetup | null;
  onBack: () => void;
  onFinish: () => void;
  t: TFunctionLike;
}

export function ReadyStep({
  availableSkillCount,
  setupCounts,
  selectedSetup,
  onBack,
  onFinish,
  t,
}: ReadyStepProps) {
  const extensionCount = setupCounts.extensions;
  const skillCount = availableSkillCount ?? setupCounts.skills;

  return (
    <section className="flex w-full flex-col items-center">
      <div className="flex size-14 items-center justify-center rounded-[14px] bg-muted text-muted-foreground">
        <IconSparkles className="size-6" strokeWidth={1.75} />
      </div>
      <h2 className="mt-6 text-[22px] font-semibold tracking-tight text-foreground">
        {t("tour.title")}
      </h2>
      <p className="mt-2 max-w-[420px] text-sm leading-6 text-muted-foreground">
        {selectedSetup?.modelName
          ? t("tour.readyWithModel", { model: selectedSetup.modelName })
          : t("tour.description")}
      </p>

      <div className="mt-8 flex w-full flex-col gap-2 text-left">
        <div className="rounded-[14px] border border-border px-4 py-3">
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            {t("tour.summary.defaultTitle")}
          </p>
          <p className="mt-1.5 text-sm font-medium text-foreground">
            {selectedSetup?.modelName ?? t("tour.summary.defaultFallback")}
          </p>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("tour.summary.defaultDescription")}
          </p>
        </div>
        <div className="rounded-[14px] border border-border px-4 py-3">
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            {t("tour.summary.extensionsTitle")}
          </p>
          <p className="mt-1.5 text-sm font-medium text-foreground">
            {formatCount("extensions", extensionCount, t)}
          </p>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("tour.summary.extensionsDescription")}
          </p>
        </div>
        <div className="rounded-[14px] border border-border px-4 py-3">
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
            {t("tour.summary.skillsTitle")}
          </p>
          <p className="mt-1.5 text-sm font-medium text-foreground">
            {formatCount("skills", skillCount, t)}
          </p>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("tour.summary.skillsDescription")}
          </p>
        </div>
      </div>

      <div className="mt-8 flex items-center justify-center gap-3">
        <Button type="button" variant="ghost" onClick={onBack}>
          {t("tour.back")}
        </Button>
        <Button type="button" onClick={onFinish} rightIcon={<IconArrowRight />}>
          {t("tour.finish")}
        </Button>
      </div>
    </section>
  );
}
