import { useCallback, useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { IconEdit, IconMessage } from "@tabler/icons-react";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { SessionActivityIndicator } from "@/shared/ui/SessionActivityIndicator";
import { SidebarChatRow } from "./SidebarChatRow";

interface TabInfo {
  id: string;
  title: string;
  projectId?: string;
  isRunning?: boolean;
  hasUnread?: boolean;
}

export function SidebarRecentsSection({
  sessions,
  collapsed,
  labelTransition,
  labelVisible,
  activeSessionId,
  onNewChat,
  onSelectSession,
  onArchiveChat,
  onRenameChat,
  onMoveToProject,
}: {
  sessions: TabInfo[];
  collapsed: boolean;
  labelTransition: string;
  labelVisible: boolean;
  activeSessionId?: string | null;
  onNewChat?: () => void;
  onSelectSession?: (sessionId: string) => void;
  onArchiveChat?: (sessionId: string) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onMoveToProject?: (sessionId: string, projectId: string | null) => void;
}) {
  const { t } = useTranslation(["sidebar", "common"]);
  const [recentsDragOver, setRecentsDragOver] = useState(false);

  const handleRecentsDragOver = useCallback((e: DragEvent<HTMLDivElement>) => {
    const hasSession = e.dataTransfer.types.includes("text/x-session-id");
    if (hasSession) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setRecentsDragOver(true);
    }
  }, []);

  const handleRecentsDragLeave = useCallback((e: DragEvent<HTMLDivElement>) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setRecentsDragOver(false);
    }
  }, []);

  const handleRecentsDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setRecentsDragOver(false);
      const sessionId = e.dataTransfer.getData("text/x-session-id");
      if (sessionId) {
        onMoveToProject?.(sessionId, null);
      }
    },
    [onMoveToProject],
  );

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drop target for drag-and-drop
    <div
      onDragOver={handleRecentsDragOver}
      onDragLeave={handleRecentsDragLeave}
      onDrop={handleRecentsDrop}
    >
      <div
        className={cn(
          "relative group/chats-header flex items-center transition-all duration-300",
          collapsed ? "px-0 pt-0 pb-1 justify-center" : "pt-5 pb-1.5",
        )}
      >
        <span
          className={cn(
            "text-[12px] font-normal text-muted-foreground/80 flex-1 pl-3",
            labelTransition,
            labelVisible
              ? "opacity-100 w-auto"
              : "opacity-0 w-0 overflow-hidden",
          )}
        >
          {t("sections.recents")}
        </span>
        {!collapsed && onNewChat && (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            onClick={onNewChat}
            aria-label={t("actions.newChat")}
            title={t("actions.newChat")}
            className={cn(
              "mr-1 size-6 flex-shrink-0 rounded-md",
              "opacity-0 pointer-events-none group-hover/chats-header:opacity-100 group-hover/chats-header:pointer-events-auto group-focus-within/chats-header:opacity-100 group-focus-within/chats-header:pointer-events-auto focus-visible:opacity-100 focus-visible:pointer-events-auto",
            )}
          >
            <IconEdit className="size-4" />
          </Button>
        )}

        {recentsDragOver && (
          <div className="absolute bottom-0 left-3 right-3 h-px bg-foreground" />
        )}
      </div>

      {sessions.length > 0 &&
        (collapsed ? (
          <div className="flex flex-col items-center gap-1">
            {sessions.map((session) => (
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                key={session.id}
                title={getDisplaySessionTitle(
                  session.title,
                  t("common:session.defaultTitle"),
                )}
                onClick={() => onSelectSession?.(session.id)}
                className={cn(
                  "relative rounded-lg",
                  activeSessionId === session.id
                    ? "bg-transparent text-foreground hover:bg-transparent"
                    : "text-foreground hover:text-foreground",
                )}
              >
                <IconMessage className="size-4" />
                <SessionActivityIndicator
                  isRunning={session.isRunning}
                  hasUnread={session.hasUnread}
                  variant="overlay"
                />
              </Button>
            ))}
          </div>
        ) : (
          <div className="space-y-0.5">
            {sessions.map((session) => {
              const isActive = activeSessionId === session.id;
              return (
                <SidebarChatRow
                  key={session.id}
                  id={session.id}
                  title={session.title}
                  isActive={isActive}
                  isRunning={session.isRunning ?? false}
                  hasUnread={session.hasUnread ?? false}
                  onSelect={onSelectSession}
                  onRename={onRenameChat}
                  onArchive={onArchiveChat}
                />
              );
            })}
          </div>
        ))}
    </div>
  );
}
