import { useState } from "react";
import type { AppView } from "@/app/AppShell";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { ProjectIcon } from "@/features/projects/ui/ProjectIcon";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { SidebarProjectSection } from "./SidebarProjectSection";

interface TabInfo {
  id: string;
  title: string;
  projectId?: string;
  isRunning?: boolean;
  hasUnread?: boolean;
}

export function SidebarProjectList({
  projects,
  projectSessionsByProject,
  expandedProjects,
  toggleProject,
  collapsed,
  activeSessionId,
  onNavigate,
  onSelectSession,
  onNewChatInProject,
  onEditProject,
  onArchiveProject,
  onArchiveChat,
  onRenameChat,
  onMoveToProject,
  onReorderProject,
}: {
  projects: ProjectInfo[];
  projectSessionsByProject: Record<string, TabInfo[]>;
  expandedProjects: Record<string, boolean>;
  toggleProject: (projectId: string) => void;
  collapsed: boolean;
  activeSessionId?: string | null;
  onNavigate?: (view: AppView) => void;
  onSelectSession?: (sessionId: string) => void;
  onNewChatInProject?: (projectId: string) => void;
  onEditProject?: (projectId: string) => void;
  onArchiveProject?: (projectId: string) => void;
  onArchiveChat?: (sessionId: string) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onMoveToProject?: (sessionId: string, projectId: string | null) => void;
  onReorderProject?: (fromId: string, toId: string) => void;
}) {
  const [draggedProjectId, setDraggedProjectId] = useState<string | null>(null);
  const [dropTargetProjectId, setDropTargetProjectId] = useState<string | null>(
    null,
  );

  if (collapsed) {
    return (
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
    );
  }

  return (
    <div className="space-y-0.5">
      {projects.map((project) => (
        // biome-ignore lint/a11y/noStaticElementInteractions: drag-and-drop reorder target
        <div
          key={project.id}
          draggable
          onDragStart={(e) => {
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
          <SidebarProjectSection
            project={project}
            projectChats={projectSessionsByProject[project.id] ?? []}
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
  );
}
