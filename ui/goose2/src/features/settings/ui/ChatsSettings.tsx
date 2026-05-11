import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsPage } from "@/shared/ui/SettingsPage";
import { Button } from "@/shared/ui/button";
import {
  type ChatSession,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";

export function ChatsSettings() {
  const { t } = useTranslation(["settings", "common"]);
  const [archivedChats, setArchivedChats] = useState<ChatSession[]>([]);
  const [loadingArchivedChats, setLoadingArchivedChats] = useState(true);

  useEffect(() => {
    setArchivedChats(useChatSessionStore.getState().getArchivedSessions());
    setLoadingArchivedChats(false);
  }, []);

  async function handleRestoreChat(id: string) {
    await useChatSessionStore.getState().unarchiveSession(id);
    setArchivedChats((prev) => prev.filter((session) => session.id !== id));
  }

  return (
    <SettingsPage title={t("chats.title")}>
      <div className="space-y-3">
        <h4 className="text-sm font-semibold">{t("chats.sectionTitle")}</h4>
        {!loadingArchivedChats && archivedChats.length === 0 ? (
          <p className="text-xs text-muted-foreground">{t("chats.empty")}</p>
        ) : null}
        {archivedChats.map((session) => (
          <div
            key={session.id}
            className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"
          >
            <div className="min-w-0">
              <div className="truncate text-sm">
                {getDisplaySessionTitle(
                  session.title,
                  t("common:session.defaultTitle"),
                )}
              </div>
              <p className="truncate text-xs text-muted-foreground">
                {session.projectId
                  ? t("chats.types.project")
                  : t("chats.types.standalone")}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={() => handleRestoreChat(session.id)}
              className="flex-shrink-0"
            >
              {t("common:actions.restore")}
            </Button>
          </div>
        ))}
      </div>
    </SettingsPage>
  );
}
