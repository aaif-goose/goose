import { useTranslation } from "react-i18next";
import { IconArrowUpRight, IconSparkles } from "@tabler/icons-react";

export function WeeklyHighlightsTile() {
  const { t } = useTranslation("home");

  return (
    <section className="flex h-full w-full flex-col rounded-lg border border-black/10 bg-[#E7F4EA] p-5 text-[var(--text-default-alex)]">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted-alex)]">
          <IconSparkles className="size-4" />
          <span>{t("widgets.weeklyHighlights.kicker")}</span>
        </div>
        <IconArrowUpRight className="size-4 text-[var(--text-muted-alex)]" />
      </div>

      <h2 className="mt-4 text-xl font-normal leading-tight">
        {t("widgets.weeklyHighlights.title")}
      </h2>

      <div className="mt-auto space-y-2 pt-4">
        {["briefs", "prs", "docs"].map((key) => (
          <div key={key} className="flex items-center justify-between text-sm">
            <span className="text-[var(--text-muted-alex)]">
              {t(`widgets.weeklyHighlights.items.${key}.label`)}
            </span>
            <span>{t(`widgets.weeklyHighlights.items.${key}.value`)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
