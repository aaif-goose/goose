import { useTranslation } from "react-i18next";
import { SettingsPage } from "@/shared/ui/SettingsPage";

export function AboutSettings() {
  const { t } = useTranslation("settings");

  return (
    <SettingsPage
      title={t("about.title")}
      description={t("about.description")}
    />
  );
}
