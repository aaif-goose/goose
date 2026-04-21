import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";
import {
  autoCompactPercentToThreshold,
  autoCompactThresholdToPercent,
  clampAutoCompactThresholdPercent,
} from "@/features/chat/lib/autoCompact";
import { useAutoCompactPreferences } from "@/features/chat/hooks/useAutoCompactPreferences";
import { Slider } from "@/shared/ui/slider";

export function GooseAutoCompactSettings() {
  const { t } = useTranslation("settings");
  const { formatNumber } = useLocaleFormatting();
  const {
    autoCompactThreshold,
    isHydrated: isAutoCompactThresholdHydrated,
    setAutoCompactThreshold,
  } = useAutoCompactPreferences();
  const autoCompactThresholdPercent =
    autoCompactThresholdToPercent(autoCompactThreshold);
  const [draftThresholdPercent, setDraftThresholdPercent] = useState(
    autoCompactThresholdPercent,
  );
  const [isSavingThreshold, setIsSavingThreshold] = useState(false);
  const [thresholdError, setThresholdError] = useState<string | null>(null);
  const translationKeyPrefix = "compaction.goose.autoCompact";
  const autoCompactValueLabel = !isAutoCompactThresholdHydrated
    ? t(`${translationKeyPrefix}.loading`)
    : draftThresholdPercent >= 100
      ? t(`${translationKeyPrefix}.off`)
      : formatNumber(draftThresholdPercent / 100, {
          style: "percent",
          minimumFractionDigits: 0,
          maximumFractionDigits: 0,
        });

  useEffect(() => {
    setDraftThresholdPercent(autoCompactThresholdPercent);
  }, [autoCompactThresholdPercent]);

  const normalizeThresholdPercent = (value: number | undefined) =>
    clampAutoCompactThresholdPercent(value ?? autoCompactThresholdPercent);

  const handleThresholdSliderChange = (values: number[]) => {
    const nextPercent = normalizeThresholdPercent(values[0]);
    setThresholdError(null);
    setDraftThresholdPercent(nextPercent);
  };

  const saveThresholdPercent = async (nextPercent: number) => {
    if (isSavingThreshold) {
      return;
    }

    setThresholdError(null);
    setDraftThresholdPercent(nextPercent);
    if (nextPercent === autoCompactThresholdPercent) {
      return;
    }

    setIsSavingThreshold(true);
    try {
      await setAutoCompactThreshold(autoCompactPercentToThreshold(nextPercent));
    } catch {
      setThresholdError(t(`${translationKeyPrefix}.saveError`));
      setDraftThresholdPercent(autoCompactThresholdPercent);
    } finally {
      setIsSavingThreshold(false);
    }
  };

  const handleThresholdSliderCommit = async (values: number[]) => {
    const nextPercent = normalizeThresholdPercent(values[0]);
    await saveThresholdPercent(nextPercent);
  };

  return (
    <div className="space-y-3">
      <div className="min-w-0">
        <p className="text-sm font-medium">
          {t(`${translationKeyPrefix}.label`)}
        </p>
        <p className="mt-0.5 text-xs text-muted-foreground">
          {t(`${translationKeyPrefix}.description`)}
        </p>
      </div>

      <div className="w-full space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs text-muted-foreground">
            {t(`${translationKeyPrefix}.current`)}
          </span>
          <div className="flex items-center gap-1.5 text-xs text-foreground">
            {isSavingThreshold ? (
              <Loader2 className="size-3 animate-spin text-muted-foreground" />
            ) : null}
            <span className="shrink-0 font-medium">
              {autoCompactValueLabel}
            </span>
          </div>
        </div>

        <Slider
          value={[draftThresholdPercent]}
          min={1}
          max={100}
          step={1}
          onValueChange={handleThresholdSliderChange}
          onValueCommit={(values) => {
            void handleThresholdSliderCommit(values);
          }}
          disabled={isSavingThreshold || !isAutoCompactThresholdHydrated}
          aria-label={t(`${translationKeyPrefix}.label`)}
        />

        <p className="text-[11px] text-muted-foreground">
          {t(`${translationKeyPrefix}.helper`)}
        </p>

        {thresholdError ? (
          <p className="text-[11px] text-destructive">{thresholdError}</p>
        ) : null}
      </div>
    </div>
  );
}
