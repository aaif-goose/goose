import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  IconChevronDown,
  IconChevronRight,
  IconEdit,
  IconMessage,
} from "@tabler/icons-react";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import type { AppView } from "@/app/AppShell";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { ProjectIcon } from "@/features/projects/ui/ProjectIcon";
import { SessionActivityIndicator } from "@/shared/ui/SessionActivityIndicator";
import { SidebarItemMenu } from "./SidebarItemMenu";
import { SidebarChatRow } from "./SidebarChatRow";

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
interface SidebarProjectsSectionProps {
  projects: ProjectInfo[];
  projectSessions: {
    byProject: Record<string, TabInfo[]>;
    standalone: TabInfo[];
  };
  expandedProjects: Record<string, boolean>;
  toggleProject: (projectId: string) => void;
  collapsed: boolean;
  labelTransition: string;
  labelVisible: boolean;
  activeSessionId?: string | null;
  onNavigate?: (view: AppView) => void;
  onSelectSession?: (sessionId: string) => void;
  onNewChatInProject?: (projectId: string) => void;
  onNewChat?: () => void;
  onCreateProject?: () => void;
  onEditProject?: (projectId: string) => void;
  onArchiveProject?: (projectId: string) => void;
  onArchiveChat?: (sessionId: string) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onMoveToProject?: (sessionId: string, projectId: string | null) => void;
  onReorderProject?: (fromId: string, toId: string) => void;
}

function ProjectSection({
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

  const handleDragOver = useCallback((e: React.DragEvent) => {
    if (e.dataTransfer.types.includes("text/x-session-id")) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setDragOver(false);
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
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

export function SidebarProjectsSection({
  projects,
  projectSessions,
  expandedProjects,
  toggleProject,
  collapsed,
  labelTransition,
  labelVisible,
  activeSessionId,
  onNavigate,
  onSelectSession,
  onNewChatInProject,
  onNewChat,
  onCreateProject,
  onEditProject,
  onArchiveProject,
  onArchiveChat,
  onRenameChat,
  onMoveToProject,
  onReorderProject,
}: SidebarProjectsSectionProps) {
  const { t } = useTranslation(["sidebar", "common"]);
  const [recentsDragOver, setRecentsDragOver] = useState(false);
  const [draggedProjectId, setDraggedProjectId] = useState<string | null>(null);
  const [dropTargetProjectId, setDropTargetProjectId] = useState<string | null>(
    null,
  );

  const handleRecentsDragOver = useCallback((e: React.DragEvent) => {
    const hasSession = e.dataTransfer.types.includes("text/x-session-id");
    if (hasSession) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setRecentsDragOver(true);
    }
  }, []);

  const handleRecentsDragLeave = useCallback((e: React.DragEvent) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setRecentsDragOver(false);
    }
  }, []);

  const handleRecentsDrop = useCallback(
    (e: React.DragEvent) => {
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
    <div
      className={cn(
        "relative z-10",
        labelTransition,
        labelVisible
          ? "opacity-100 max-h-[2000px]"
          : collapsed
            ? "opacity-100 max-h-[2000px]"
            : "opacity-0 max-h-0 overflow-hidden",
      )}
    >
      <div
        className={cn(
          "group/projects-header flex items-center transition-all duration-300",
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
          {t("sections.projects")}
        </span>
        {!collapsed && (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={onCreateProject}
            title={t("actions.newProject")}
            className={cn(
              "mr-1 h-6 flex-shrink-0 rounded-full bg-muted px-2 text-[11px] text-foreground opacity-0 transition-opacity duration-150 ease-out hover:bg-muted/80 hover:text-foreground",
              "pointer-events-none group-hover/projects-header:pointer-events-auto group-hover/projects-header:opacity-100 focus-visible:pointer-events-auto focus-visible:opacity-100",
            )}
          >
            {t("actions.newProject")}
          </Button>
        )}
      </div>

      {collapsed ? (
        <div className="flex flex-col items-center gap-1">
          {projects.map((project) => (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              key={project.id}
              title={project.name}
              onClick={() => onNavigate?.("projects")}
              className="rounded-lg text-foreground hover:bg-transparent hover:text-foreground"
            >
              <ProjectIcon
                icon={project.icon}
                className="size-4"
                imageClassName="size-4 rounded-[4px]"
              />
            </Button>
          ))}
        </div>
      ) : (
        <div className="space-y-0.5">
          {projects.map((project) => (
            // biome-ignore lint/a11y/noStaticElementInteractions: drag-and-drop reorder target
            <div
              key={project.id}
              draggable
              onDragStart={(e) => {
                // Skip if a child chat row already initiated a session drag
                if (e.dataTransfer.types.includes("text/x-session-id")) return;
                e.dataTransfer.setData("text/x-project-id", project.id);
                e.dataTransfer.effectAllowed = "move";
                setDraggedProjectId(project.id);
              }}
              onDragOver={(e) => {
                if (e.dataTransfer.types.includes("text/x-project-id")) {
                  e.preventDefault();
                  e.dataTransfer.dropEffect = "move";
                  if (project.id !== draggedProjectId) {
                    setDropTargetProjectId(project.id);
                  }
                }
              }}
              onDragLeave={(e) => {
                if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                  setDropTargetProjectId((prev) =>
                    prev === project.id ? null : prev,
                  );
                }
              }}
              onDrop={(e) => {
                const fromId = e.dataTransfer.getData("text/x-project-id");
                if (fromId && fromId !== project.id) {
                  e.preventDefault();
                  e.stopPropagation();
                  onReorderProject?.(fromId, project.id);
                }
                setDraggedProjectId(null);
                setDropTargetProjectId(null);
              }}
              onDragEnd={() => {
                setDraggedProjectId(null);
                setDropTargetProjectId(null);
              }}
              className={cn(
                "relative",
                draggedProjectId === project.id && "opacity-40",
              )}
            >
              {dropTargetProjectId === project.id &&
                draggedProjectId !== project.id && (
                  <div className="absolute top-0 left-3 right-3 h-0.5 rounded-full bg-foreground" />
                )}
              <ProjectSection
                project={project}
                projectChats={projectSessions.byProject[project.id] ?? []}
                isExpanded={expandedProjects[project.id] ?? false}
                toggleProject={toggleProject}
                activeSessionId={activeSessionId}
                onSelectSession={onSelectSession}
                onNewChatInProject={onNewChatInProject}
                onNavigate={onNavigate}
                onEditProject={onEditProject}
                onArchiveProject={onArchiveProject}
                onArchiveChat={onArchiveChat}
                onRenameChat={onRenameChat}
                onMoveToProject={onMoveToProject}
              />
            </div>
          ))}
        </div>
      )}

      {/* --- RECENTS — always rendered as a drop target so chats can be unassigned from projects --- */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: drop target for drag-and-drop */}
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
              size="xs"
              onClick={onNewChat}
              aria-label={t("actions.newChat")}
              title={t("actions.newChat")}
              className={cn(
                "mr-1 h-6 flex-shrink-0 rounded-full bg-muted px-2 text-[11px] text-foreground opacity-0 transition-opacity duration-150 ease-out hover:bg-muted/80 hover:text-foreground",
                "pointer-events-none group-hover/chats-header:pointer-events-auto group-hover/chats-header:opacity-100 focus-visible:pointer-events-auto focus-visible:opacity-100",
              )}
            >
              {t("actions.newChat")}
            </Button>
          )}

          {recentsDragOver && (
            <div className="absolute bottom-0 left-3 right-3 h-px bg-foreground" />
          )}
        </div>

        {projectSessions.standalone.length > 0 &&
          (collapsed ? (
            <div className="flex flex-col items-center gap-1">
              {projectSessions.standalone.map((session) => (
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
              {projectSessions.standalone.map((session) => {
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
    </div>
  );
}
