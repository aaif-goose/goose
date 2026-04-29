import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";

export function ClockWidget() {
  const { t } = useTranslation("home");
  const [time, setTime] = useState(new Date());
  const { formatDate, getTimeParts } = useLocaleFormatting();

  useEffect(() => {
    const interval = setInterval(() => setTime(new Date()), 1000);
    return () => clearInterval(interval);
  }, []);

  const { hour, minute, dayPeriod } = getTimeParts(time, {
    hour: "numeric",
    minute: "2-digit",
  });

  return (
    <section className="flex h-full w-full flex-col justify-between rounded-lg border border-black/10 bg-white/75 p-5 text-[var(--text-default-alex)] backdrop-blur">
      <p className="text-[13px] text-[var(--text-muted-alex)]">
        {t("widgets.clock.current")}
      </p>
      <div>
        <div
          className="flex items-baseline gap-2"
          style={{ fontFamily: "var(--font-sans-alex)" }}
        >
          <span className="text-[52px] font-light leading-none tracking-normal">
            {hour}:{minute}
          </span>
          {dayPeriod ? (
            <span className="text-[20px] font-light leading-none text-[var(--text-muted-alex)]">
              {dayPeriod}
            </span>
          ) : null}
        </div>
        <p className="mt-2 text-sm text-[var(--text-muted-alex)]">
          {formatDate(time, {
            weekday: "long",
            month: "short",
            day: "numeric",
          })}
        </p>
      </div>
    </section>
  );
}
