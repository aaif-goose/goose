import { useTranslation } from "react-i18next";
import type { WidgetRenderProps } from "./types";

function getNoteText(state: Record<string, unknown> | undefined): string {
  return typeof state?.text === "string" ? state.text : "";
}

export function StickyNoteWidget({
  instance,
  onUpdateState,
}: WidgetRenderProps) {
  const { t } = useTranslation("home");
  const text = getNoteText(instance.state);

  return (
    <section className="h-full w-full rounded-lg border border-amber-900/10 bg-[#F7E7A6] p-4 text-amber-950 shadow-[0_8px_24px_rgba(0,0,0,0.08)]">
      <textarea
        value={text}
        onChange={(event) => onUpdateState({ text: event.target.value })}
        onPointerDown={(event) => event.stopPropagation()}
        placeholder={t("widgets.stickyNote.placeholder")}
        aria-label={t("widgets.stickyNote.ariaLabel")}
        className="h-full w-full resize-none bg-transparent text-[15px] leading-6 placeholder:text-amber-950/45 focus:outline-none"
        spellCheck={false}
      />
    </section>
  );
}
