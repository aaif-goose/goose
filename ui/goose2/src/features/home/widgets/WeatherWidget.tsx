import { useTranslation } from "react-i18next";
import { IconCloud, IconCloudRain, IconSun } from "@tabler/icons-react";

const FORECAST = [
  { key: "today", temp: "72", icon: IconSun },
  { key: "tomorrow", temp: "68", icon: IconCloud },
  { key: "friday", temp: "63", icon: IconCloudRain },
] as const;

export function WeatherWidget() {
  const { t } = useTranslation("home");

  return (
    <section className="flex h-full w-full flex-col rounded-lg border border-black/10 bg-white/75 p-5 text-[var(--text-default-alex)] backdrop-blur">
      <div className="flex items-start justify-between">
        <div>
          <p className="text-[13px] text-[var(--text-muted-alex)]">
            {t("widgets.weather.location")}
          </p>
          <p className="mt-1 text-4xl font-light leading-none">72°</p>
        </div>
        <div className="rounded-full bg-yellow-200/70 p-3 text-yellow-700">
          <IconSun className="size-6" />
        </div>
      </div>

      <p className="mt-4 text-sm leading-5 text-[var(--text-muted-alex)]">
        {t("widgets.weather.summary")}
      </p>

      <div className="mt-auto grid grid-cols-3 gap-2 pt-4">
        {FORECAST.map(({ key, temp, icon: Icon }) => (
          <div
            key={key}
            className="flex flex-col items-center gap-2 rounded-md bg-black/[0.04] px-2 py-3 text-center"
          >
            <Icon className="size-4 text-[var(--text-muted-alex)]" />
            <span className="text-[11px] text-[var(--text-muted-alex)]">
              {t(`widgets.weather.forecast.${key}`)}
            </span>
            <span className="text-sm">{temp}°</span>
          </div>
        ))}
      </div>
    </section>
  );
}
