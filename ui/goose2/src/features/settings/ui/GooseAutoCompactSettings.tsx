import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";
import {
  DEFAULT_AUTO_COMPACT_THRESHOLD,
  autoCompactPercentToThreshold,
  autoCompactThresholdToPercent,
  clampAutoCompactThresholdPercent,
} from "@/features/chat/lib/autoCompact";
import { useAutoCompactPreferences } from "@/features/chat/hooks/useAutoCompactPreferences";
import { Slider } from "@/shared/ui/slider";
import { Switch } from "@/shared/ui/switch";

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
  const defaultEnabledThresholdPercent = Math.round(
    DEFAULT_AUTO_COMPACT_THRESHOLD * 100,
  );
  const [draftThresholdPercent, setDraftThresholdPercent] = useState(
    autoCompactThresholdPercent,
  );
  const [isAutoCompactEnabled, setIsAutoCompactEnabled] = useState(
    autoCompactThresholdPercent < 100,
  );
  const [lastEnabledThresholdPercent, setLastEnabledThresholdPercent] =
    useState(
      autoCompactThresholdPercent < 100
        ? autoCompactThresholdPercent
        : defaultEnabledThresholdPercent,
    );
  const [isSavingThreshold, setIsSavingThreshold] = useState(false);
  const [thresholdError, setThresholdError] = useState<string | null>(null);
  const translationKeyPrefix = "compaction.goose.autoCompact";
  const autoCompactValueLabel = !isAutoCompactThresholdHydrated
    ? t(`${translationKeyPrefix}.loading`)
    : !isAutoCompactEnabled
      ? t(`${translationKeyPrefix}.off`)
      : formatNumber(draftThresholdPercent / 100, {
          style: "percent",
          minimumFractionDigits: 0,
          maximumFractionDigits: 0,
        });

  useEffect(() => {
    setDraftThresholdPercent(autoCompactThresholdPercent);
    setIsAutoCompactEnabled(autoCompactThresholdPercent < 100);
    if (autoCompactThresholdPercent < 100) {
      setLastEnabledThresholdPercent(autoCompactThresholdPercent);
    }
  }, [autoCompactThresholdPercent]);

  const normalizeThresholdPercent = (value: number | undefined) =>
    clampAutoCompactThresholdPercent(value ?? autoCompactThresholdPercent);

  const handleThresholdSliderChange = (values: number[]) => {
    const nextPercent = normalizeThresholdPercent(values[0]);
    setThresholdError(null);
    setDraftThresholdPercent(nextPercent);
    if (nextPercent < 100) {
      setLastEnabledThresholdPercent(nextPercent);
    }
  };

  const saveThresholdPercent = async (
    nextPercent: number,
    options?: {
      enabled?: boolean;
      restorePercentOnError?: number;
      restoreEnabledOnError?: boolean;
    },
  ) => {
    if (isSavingThreshold) {
      return;
    }

    const nextEnabled = options?.enabled ?? nextPercent < 100;
    const restorePercentOnError =
      options?.restorePercentOnError ?? autoCompactThresholdPercent;
    const restoreEnabledOnError =
      options?.restoreEnabledOnError ?? autoCompactThresholdPercent < 100;

    setThresholdError(null);
    setDraftThresholdPercent(nextPercent);
    setIsAutoCompactEnabled(nextEnabled);

    if (nextPercent < 100) {
      setLastEnabledThresholdPercent(nextPercent);
    }

    if (
      nextPercent === autoCompactThresholdPercent &&
      nextEnabled === autoCompactThresholdPercent < 100
    ) {
      return;
    }

    setIsSavingThreshold(true);
    try {
      await setAutoCompactThreshold(autoCompactPercentToThreshold(nextPercent));
    } catch {
      setThresholdError(t(`${translationKeyPrefix}.saveError`));
      setDraftThresholdPercent(restorePercentOnError);
      setIsAutoCompactEnabled(restoreEnabledOnError);
    } finally {
      setIsSavingThreshold(false);
    }
  };

  const handleThresholdSliderCommit = async (values: number[]) => {
    const nextPercent = normalizeThresholdPercent(values[0]);
    await saveThresholdPercent(nextPercent);
  };

  const handleToggleChange = async (checked: boolean) => {
    const restorePercentOnError = draftThresholdPercent;
    const restoreEnabledOnError = isAutoCompactEnabled;
    const nextPercent = checked
      ? lastEnabledThresholdPercent < 100
        ? lastEnabledThresholdPercent
        : defaultEnabledThresholdPercent
      : 100;

    await saveThresholdPercent(nextPercent, {
      enabled: checked,
      restorePercentOnError,
      restoreEnabledOnError,
    });
  };

  return (
    <div className="space-y-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">
            {t(`${translationKeyPrefix}.label`)}
          </p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t(`${translationKeyPrefix}.description`)}
          </p>
        </div>

        <div className="flex items-center gap-2 pt-0.5">
          <span className="text-xs text-muted-foreground">
            {isAutoCompactEnabled
              ? t(`${translationKeyPrefix}.on`)
              : t(`${translationKeyPrefix}.off`)}
          </span>
          <Switch
            checked={isAutoCompactEnabled}
            onCheckedChange={(checked) => {
              void handleToggleChange(checked);
            }}
            disabled={isSavingThreshold || !isAutoCompactThresholdHydrated}
            aria-label={t(`${translationKeyPrefix}.toggleLabel`)}
          />
        </div>
      </div>

      <div className="w-full max-w-sm space-y-2">
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
          disabled={
            isSavingThreshold ||
            !isAutoCompactThresholdHydrated ||
            !isAutoCompactEnabled
          }
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
