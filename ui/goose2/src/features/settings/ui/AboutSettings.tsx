import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsPage } from "@/shared/ui/SettingsPage";

interface AboutAppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  identifier: string;
}

function AboutInfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate text-right text-sm font-medium">
        {value}
      </span>
    </div>
  );
}

export function AboutSettings() {
  const { t } = useTranslation("settings");
  const [appInfo, setAppInfo] = useState<AboutAppInfo | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadAppInfo() {
      if (!window.__TAURI_INTERNALS__) {
        return;
      }

      try {
        const { getIdentifier, getName, getTauriVersion, getVersion } =
          await import("@tauri-apps/api/app");
        const [name, version, tauriVersion, identifier] = await Promise.all([
          getName(),
          getVersion(),
          getTauriVersion(),
          getIdentifier(),
        ]);

        if (!cancelled) {
          setAppInfo({ name, version, tauriVersion, identifier });
        }
      } catch {
        if (!cancelled) {
          setAppInfo(null);
        }
      }
    }

    void loadAppInfo();

    return () => {
      cancelled = true;
    };
  }, []);

  const fallback = t("about.unavailable");

  return (
    <SettingsPage title={t("about.title")}>
      <div className="space-y-1">
        <div className="divide-y divide-border">
          <AboutInfoRow
            label={t("about.fields.name")}
            value={appInfo?.name ?? "Goose"}
          />
          <AboutInfoRow
            label={t("about.fields.version")}
            value={appInfo?.version ?? fallback}
          />
          <AboutInfoRow
            label={t("about.fields.buildMode")}
            value={
              import.meta.env.DEV
                ? t("about.buildModes.development")
                : t("about.buildModes.production")
            }
          />
          <AboutInfoRow
            label={t("about.fields.tauriVersion")}
            value={appInfo?.tauriVersion ?? fallback}
          />
          <AboutInfoRow
            label={t("about.fields.identifier")}
            value={appInfo?.identifier ?? fallback}
          />
          <AboutInfoRow label={t("about.fields.license")} value="Apache-2.0" />
        </div>
      </div>
    </SettingsPage>
  );
}
