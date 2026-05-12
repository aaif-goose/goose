import { useTranslation } from "react-i18next";
import type { AppView } from "@/app/AppShell";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { SidebarProjectList } from "./SidebarProjectList";
import { SidebarRecentsSection } from "./SidebarRecentsSection";

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

      <SidebarProjectList
        projects={projects}
        projectSessionsByProject={projectSessions.byProject}
        expandedProjects={expandedProjects}
        toggleProject={toggleProject}
        collapsed={collapsed}
        activeSessionId={activeSessionId}
        onNavigate={onNavigate}
        onSelectSession={onSelectSession}
        onNewChatInProject={onNewChatInProject}
        onEditProject={onEditProject}
        onArchiveProject={onArchiveProject}
        onArchiveChat={onArchiveChat}
        onRenameChat={onRenameChat}
        onMoveToProject={onMoveToProject}
        onReorderProject={onReorderProject}
      />

      <SidebarRecentsSection
        sessions={projectSessions.standalone}
        collapsed={collapsed}
        labelTransition={labelTransition}
        labelVisible={labelVisible}
        activeSessionId={activeSessionId}
        onNewChat={onNewChat}
        onSelectSession={onSelectSession}
        onArchiveChat={onArchiveChat}
        onRenameChat={onRenameChat}
        onMoveToProject={onMoveToProject}
      />
    </div>
  );
}
