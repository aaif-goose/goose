import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  getVisibleSessions,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverAnchor, PopoverContent } from "@/shared/ui/popover";
import {
  HOME_WIDGET_CATALOG,
  HOME_WIDGET_CATEGORIES,
} from "../widgets/catalog";
import type { WidgetCategory } from "../widgets/types";

interface WidgetPickerProps {
  open: boolean;
  x: number;
  y: number;
  onClose: () => void;
  onSelect: (type: string, state?: Record<string, unknown>) => void;
}

const SECTION_CLASS_BY_CATEGORY: Record<WidgetCategory, string> = {
  tile: "bg-[#F4F0FF]",
  app: "bg-[#EEF4F8]",
  pin: "bg-[#E7F4EA]",
};

export function WidgetPicker({
  open,
  x,
  y,
  onClose,
  onSelect,
}: WidgetPickerProps) {
  const { t } = useTranslation("home");
  const personas = useAgentStore((state) => state.personas);
  const sessions = useChatSessionStore((state) => state.sessions);
  const messagesBySession = useChatStore((state) => state.messagesBySession);
  const visibleSessions = useMemo(
    () =>
      getVisibleSessions(sessions, messagesBySession).filter(
        (session) => !session.archivedAt,
      ),
    [messagesBySession, sessions],
  );

  if (!open) {
    return null;
  }

  const getDefaultState = (type: string) => {
    if (type === "agentPin") {
      const persona =
        personas.find((candidate) => candidate.isBuiltin) ?? personas[0];
      return persona ? { agentId: persona.id } : undefined;
    }

    if (type === "chatPin") {
      const session = visibleSessions[0];
      return session ? { sessionId: session.id } : undefined;
    }

    if (type === "stickyNote") {
      return { text: t("widgets.stickyNote.defaultText") };
    }

    return undefined;
  };

  return (
    <Popover open={open} onOpenChange={(nextOpen) => !nextOpen && onClose()}>
      <PopoverAnchor asChild>
        <span
          aria-hidden="true"
          className="pointer-events-none absolute size-0"
          style={{ left: x, top: y }}
        />
      </PopoverAnchor>
      <PopoverContent
        align="start"
        side="right"
        sideOffset={10}
        onOpenAutoFocus={(event) => event.preventDefault()}
        className="w-80 rounded-lg border-black/10 bg-white/95 p-3 text-[var(--text-default-alex)] backdrop-blur"
      >
        <div className="space-y-4">
          {HOME_WIDGET_CATEGORIES.map((category) => {
            const entries = HOME_WIDGET_CATALOG.filter(
              (entry) => entry.category === category,
            );

            return (
              <section key={category}>
                <h2 className="px-1 text-[11px] font-medium uppercase tracking-normal text-[var(--text-muted-alex)]">
                  {t(`widgets.picker.sections.${category}`)}
                </h2>
                <div className="mt-2 space-y-1">
                  {entries.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      onClick={() =>
                        onSelect(entry.id, getDefaultState(entry.id))
                      }
                      className={cn(
                        "flex w-full items-start gap-3 rounded-md px-3 py-2.5 text-left transition-colors hover:bg-black/[0.04]",
                        SECTION_CLASS_BY_CATEGORY[category],
                      )}
                    >
                      <span className="mt-1 size-2 shrink-0 rounded-full bg-black/25" />
                      <span className="min-w-0">
                        <span className="block text-sm">
                          {t(entry.labelKey)}
                        </span>
                        {entry.descriptionKey ? (
                          <span className="mt-0.5 block text-xs leading-4 text-[var(--text-muted-alex)]">
                            {t(entry.descriptionKey)}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      </PopoverContent>
    </Popover>
  );
}
