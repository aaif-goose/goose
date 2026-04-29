import { useTranslation } from "react-i18next";
import { IconCalendarEvent, IconChecklist } from "@tabler/icons-react";

export function MondayBriefTile() {
  const { t } = useTranslation("home");

  return (
    <section className="flex h-full w-full flex-col rounded-lg border border-black/10 bg-[#F4F0FF] p-5 text-[var(--text-default-alex)]">
      <div className="flex items-center gap-2 text-[13px] text-[var(--text-muted-alex)]">
        <IconCalendarEvent className="size-4" />
        <span>{t("widgets.mondayBrief.kicker")}</span>
      </div>
      <h2 className="mt-4 text-xl font-normal leading-tight">
        {t("widgets.mondayBrief.title")}
      </h2>
      <div className="mt-auto grid grid-cols-3 gap-2 pt-4">
        <div className="rounded-md bg-white/60 p-2">
          <p className="text-lg leading-none">3</p>
          <p className="mt-1 text-[11px] text-[var(--text-muted-alex)]">
            {t("widgets.mondayBrief.priorities")}
          </p>
        </div>
        <div className="rounded-md bg-white/60 p-2">
          <p className="text-lg leading-none">2</p>
          <p className="mt-1 text-[11px] text-[var(--text-muted-alex)]">
            {t("widgets.mondayBrief.meetings")}
          </p>
        </div>
        <div className="rounded-md bg-white/60 p-2">
          <IconChecklist className="size-5" />
          <p className="mt-1 text-[11px] text-[var(--text-muted-alex)]">
            {t("widgets.mondayBrief.focus")}
          </p>
        </div>
      </div>
    </section>
  );
}
