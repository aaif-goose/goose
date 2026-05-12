import { useCallback, useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  IconChevronDown,
  IconChevronRight,
  IconEdit,
} from "@tabler/icons-react";
import type { AppView } from "@/app/AppShell";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { ProjectIcon } from "@/features/projects/ui/ProjectIcon";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { SidebarChatRow } from "./SidebarChatRow";
import { SidebarItemMenu } from "./SidebarItemMenu";

const MAX_VISIBLE_CHATS = 5;
const PROJECT_ROW_TEXT_CLASS =
  "text-foreground hover:bg-transparent hover:text-foreground";

interface TabInfo {
  id: string;
  title: string;
  projectId?: string;
  isRunning?: boolean;
  hasUnread?: boolean;
}

export function SidebarProjectSection({
  project,
  projectChats,
  isExpanded,
  toggleProject,
  activeSessionId,
  onSelectSession,
  onNewChatInProject,
  onNavigate,
  onEditProject,
  onArchiveProject,
  onArchiveChat,
  onRenameChat,
  onMoveToProject,
}: {
  project: ProjectInfo;
  projectChats: TabInfo[];
  isExpanded: boolean;
  toggleProject: (projectId: string) => void;
  activeSessionId?: string | null;
  onSelectSession?: (sessionId: string) => void;
  onNewChatInProject?: (projectId: string) => void;
  onNavigate?: (view: AppView) => void;
  onEditProject?: (projectId: string) => void;
  onArchiveProject?: (projectId: string) => void;
  onArchiveChat?: (sessionId: string) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onMoveToProject?: (sessionId: string, projectId: string | null) => void;
}) {
  const { t } = useTranslation(["sidebar", "common"]);
  const [showAll, setShowAll] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);

  const handleDragOver = useCallback((e: DragEvent<HTMLDivElement>) => {
    if (e.dataTransfer.types.includes("text/x-session-id")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: DragEvent<HTMLDivElement>) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setDragOver(false);
    }
  }, []);

  const handleDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setDragOver(false);
      const sessionId = e.dataTransfer.getData("text/x-session-id");
      if (sessionId) {
        onMoveToProject?.(sessionId, project.id);
        if (!isExpanded) toggleProject(project.id);
      }
    },
    [onMoveToProject, project.id, isExpanded, toggleProject],
  );
  const visibleChats = projectChats.slice(
    0,
    showAll ? undefined : MAX_VISIBLE_CHATS,
  );

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drop target for drag-and-drop
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div
        className={cn(
          "relative flex items-center group rounded-md transition-colors duration-200 hover:bg-background-alt focus-within:bg-background-alt",
          menuOpen && "bg-background-alt",
        )}
      >
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => toggleProject(project.id)}
          className={cn(
            "flex-1 min-w-0 justify-start gap-2 rounded-md px-3 py-2 text-sm font-normal",
            PROJECT_ROW_TEXT_CLASS,
          )}
        >
          <span className="relative flex h-4 w-4 flex-shrink-0 items-center justify-center text-foreground">
            <span className="absolute group-hover:opacity-0">
              <ProjectIcon
                icon={project.icon}
                className="size-3.5"
                imageClassName="size-3.5 rounded-[3px]"
              />
            </span>
            {isExpanded ? (
              <IconChevronDown className="absolute size-3 opacity-0 group-hover:opacity-100" />
            ) : (
              <IconChevronRight className="absolute size-3 opacity-0 group-hover:opacity-100" />
            )}
          </span>
          <span className="flex-1 min-w-0 truncate text-left">
            {project.name}
          </span>
        </Button>
        <SidebarItemMenu
          label={project.name}
          onOpenChange={setMenuOpen}
          onEdit={() => onEditProject?.(project.id)}
          onArchive={() => onArchiveProject?.(project.id)}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={(e) => {
            e.stopPropagation();
            onNewChatInProject?.(project.id);
          }}
          title={t("actions.newChatInProject")}
          className={cn(
            "mr-1 size-6 flex-shrink-0 rounded-md",
            menuOpen
              ? "visible opacity-100"
              : "invisible opacity-0 group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100",
          )}
        >
          <IconEdit className="size-4" />
        </Button>

        {dragOver && (
          <div className="absolute bottom-0 left-3 right-3 h-px bg-foreground" />
        )}
      </div>

      {isExpanded && (
        <div className="mt-0.5 space-y-0.5">
          {visibleChats.map((session) => {
            const isActive = activeSessionId === session.id;
            return (
              <SidebarChatRow
                key={session.id}
                id={session.id}
                title={session.title}
                isActive={isActive}
                isRunning={session.isRunning ?? false}
                hasUnread={session.hasUnread ?? false}
                nested
                onSelect={onSelectSession}
                onRename={onRenameChat}
                onArchive={onArchiveChat}
              />
            );
          })}
          {projectChats.length > MAX_VISIBLE_CHATS && (
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => {
                if (showAll) {
                  setShowAll(false);
                } else {
                  if (projectChats.length > 8) {
                    onNavigate?.("projects");
                  } else {
                    setShowAll(true);
                  }
                }
              }}
              className="h-auto w-full justify-start gap-1.5 rounded-md py-1 pl-8 pr-3 text-[11px] text-foreground hover:text-foreground"
            >
              {showAll ? (
                <>
                  <IconChevronDown className="size-3" />
                  {t("showLess")}
                </>
              ) : (
                <>
                  <IconChevronRight className="size-3" />
                  {projectChats.length > 8
                    ? t("viewAllChats", {
                        count: projectChats.length,
                        displayCount: projectChats.length,
                      })
                    : t("moreChats", {
                        count: projectChats.length - MAX_VISIBLE_CHATS,
                        displayCount: projectChats.length - MAX_VISIBLE_CHATS,
                      })}
                </>
              )}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
