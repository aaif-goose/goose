import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { IconMessageCircle } from "@tabler/icons-react";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  getVisibleSessions,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { useLocaleFormatting } from "@/shared/i18n";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import { useWidgetActivationGuard } from "./useWidgetActivationGuard";
import type { WidgetRenderProps } from "./types";

function getSessionId(
  state: Record<string, unknown> | undefined,
): string | null {
  return typeof state?.sessionId === "string" ? state.sessionId : null;
}

function resolveSession(
  sessions: ChatSession[],
  id: string | null,
): ChatSession | undefined {
  return sessions.find((session) => session.id === id) ?? sessions[0];
}

export function ChatPinWidget({
  instance,
  shouldIgnoreActivation,
  onSelectSession,
}: WidgetRenderProps) {
  const { t } = useTranslation("home");
  const { formatRelativeTimeToNow } = useLocaleFormatting();
  const sessions = useChatSessionStore((state) => state.sessions);
  const messagesBySession = useChatStore((state) => state.messagesBySession);
  const visibleSessions = useMemo(
    () =>
      getVisibleSessions(sessions, messagesBySession).filter(
        (session) => !session.archivedAt,
      ),
    [messagesBySession, sessions],
  );
  const session = resolveSession(visibleSessions, getSessionId(instance.state));
  const title = session?.title ?? t("widgets.chatPin.emptyTitle");
  const activationGuard = useWidgetActivationGuard(shouldIgnoreActivation);

  return (
    <button
      type="button"
      {...activationGuard.pointerHandlers}
      onClick={(event) => {
        if (activationGuard.shouldIgnoreActivation()) {
          event.preventDefault();
          activationGuard.clearIgnoredActivation();
          return;
        }
        if (session) {
          onSelectSession?.(session.id);
        }
      }}
      aria-label={t("widgets.chatPin.openAria", { title })}
      className="flex h-full w-full flex-col rounded-lg border border-black/10 bg-white/80 p-4 text-left text-[var(--text-default-alex)] backdrop-blur transition-colors hover:bg-white"
    >
      <span className="flex items-center gap-2 text-[13px] text-[var(--text-muted-alex)]">
        <IconMessageCircle className="size-4" />
        {t("widgets.chatPin.kicker")}
      </span>
      <span className="mt-3 line-clamp-2 text-base leading-5">{title}</span>
      <span className="mt-auto text-sm text-[var(--text-muted-alex)]">
        {session
          ? formatRelativeTimeToNow(session.updatedAt)
          : t("widgets.chatPin.emptyDescription")}
      </span>
    </button>
  );
}
