import { useTranslation } from "react-i18next";
import { type LocalePreference, useLocale } from "@/shared/i18n";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import { Separator } from "@/shared/ui/separator";

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-8 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{label}</p>
        {description ? (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

export function GeneralSettings() {
  const { t } = useTranslation("settings");
  const { preference, setLocalePreference, systemLocaleLabel } = useLocale();

  return (
    <div>
      <h3 className="text-lg font-semibold font-display tracking-tight">
        {t("general.title")}
      </h3>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("general.description")}
      </p>

      <Separator className="my-4" />

      <SettingRow
        label={t("general.language.label")}
        description={t("general.language.description")}
      >
        <Select
          value={preference}
          onValueChange={(value) =>
            void setLocalePreference(value as LocalePreference)
          }
        >
          <SelectTrigger className="w-full min-w-64">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="system">
              {t("general.language.system", {
                language: systemLocaleLabel,
              })}
            </SelectItem>
            <SelectItem value="en">{t("general.language.english")}</SelectItem>
            <SelectItem value="es">{t("general.language.spanish")}</SelectItem>
          </SelectContent>
        </Select>
      </SettingRow>
    </div>
  );
}
