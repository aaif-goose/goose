import { useTranslation } from "react-i18next";
import { IconCheck } from "@tabler/icons-react";
import { getProviderIcon } from "@/shared/ui/icons/ProviderIcons";
import { GooseAutoCompactSettings } from "./GooseAutoCompactSettings";

export function CompactionSettings() {
  const { t } = useTranslation("settings");
  const icon = getProviderIcon("goose", "size-6");

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold font-display tracking-tight">
          {t("compaction.title")}
        </h3>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("compaction.description")}
        </p>
      </div>

      <div className="rounded-lg border bg-background p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <div className="flex size-6 items-center justify-center [&>*]:size-6">
              {icon}
            </div>
            <span className="mt-2 block text-sm font-medium">
              {t("compaction.goose.label")}
            </span>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("compaction.goose.description")}
            </p>
          </div>

          <div className="inline-flex items-center gap-1 rounded-full bg-success/10 px-2 py-1 text-xxs font-medium text-success">
            <IconCheck className="size-3.5" />
            <span>{t("compaction.goose.builtIn")}</span>
          </div>
        </div>

        <div className="mt-4 border-t pt-4">
          <GooseAutoCompactSettings />
        </div>
      </div>
    </div>
  );
}
