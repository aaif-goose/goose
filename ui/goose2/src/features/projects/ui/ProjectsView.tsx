import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  MessageSquare,
  Plus,
  Trash2,
  MoreHorizontal,
  Pencil,
  FolderKanban,
} from "lucide-react";
import { SearchBar } from "@/shared/ui/SearchBar";
import { Button, buttonVariants } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { CreateProjectDialog } from "./CreateProjectDialog";
import { ProjectIcon } from "./ProjectIcon";
import { deleteProject, type ProjectInfo } from "../api/projects";
import { useProjectStore } from "../stores/projectStore";

function ProjectCardMenu({
  project,
  onStartChat,
  onEdit,
  onDelete,
}: {
  project: ProjectInfo;
  onStartChat?: (project: ProjectInfo) => void;
  onEdit: (project: ProjectInfo) => void;
  onDelete: (project: ProjectInfo) => void;
}) {
  const { t } = useTranslation(["projects", "common"]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={t("view.optionsAria", { name: project.name })}
          className="size-6 rounded-md text-muted-foreground hover:text-foreground"
        >
          <MoreHorizontal className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={4}>
        {onStartChat && (
          <DropdownMenuItem onSelect={() => onStartChat(project)}>
            <MessageSquare className="size-3.5" />
            {t("view.startChat")}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onSelect={() => onEdit(project)}>
          <Pencil className="size-3.5" />
          {t("common:actions.edit")}
        </DropdownMenuItem>
        <DropdownMenuItem
          variant="destructive"
          onSelect={() => onDelete(project)}
        >
          <Trash2 className="size-3.5" />
          {t("common:actions.delete")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

interface ProjectsViewProps {
  onStartChat?: (project: ProjectInfo) => void;
}

export function ProjectsView({ onStartChat }: ProjectsViewProps) {
  const { t } = useTranslation(["projects", "common"]);
  const fetchProjects = useProjectStore((s) => s.fetchProjects);
  const [search, setSearch] = useState("");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editingProject, setEditingProject] = useState<ProjectInfo | undefined>(
    undefined,
  );
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [deletingProject, setDeletingProject] = useState<ProjectInfo | null>(
    null,
  );

  const loadProjects = useCallback(async () => {
    try {
      await fetchProjects();
      setProjects(useProjectStore.getState().projects);
    } catch {
      // projects may not exist yet
      setProjects([]);
    } finally {
      setLoading(false);
    }
  }, [fetchProjects]);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  const handleDelete = (project: ProjectInfo) => {
    setDeletingProject(project);
  };

  const handleConfirmDeleteProject = async () => {
    if (!deletingProject) return;
    try {
      await deleteProject(deletingProject.id);
      await loadProjects();
    } catch {
      // best-effort
    }
    setDeletingProject(null);
  };

  const handleEdit = (project: ProjectInfo) => {
    setEditingProject(project);
    setDialogOpen(true);
  };

  const handleDialogClose = () => {
    setDialogOpen(false);
    setEditingProject(undefined);
  };

  const handleNewProject = () => {
    setEditingProject(undefined);
    setDialogOpen(true);
  };

  const filtered = projects.filter(
    (p) =>
      p.name.toLowerCase().includes(search.toLowerCase()) ||
      p.description.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div className="flex flex-1 flex-col h-full min-h-0">
      <div className="flex-1 overflow-y-auto min-h-0">
        <div className="max-w-5xl mx-auto w-full px-6 py-8 space-y-5 page-transition">
          {/* Header */}
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h1 className="text-lg font-semibold font-display tracking-tight">
                {t("view.title")}
              </h1>
              <p className="text-xs text-muted-foreground">
                {t("view.description")}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="outline"
                size="xs"
                onClick={handleNewProject}
              >
                <Plus className="size-3.5" />
                {t("view.newProject")}
              </Button>
            </div>
          </div>

          {/* Search */}
          <SearchBar
            value={search}
            onChange={setSearch}
            placeholder={t("view.searchPlaceholder")}
          />

          {/* Projects list */}
          {!loading && filtered.length > 0 && (
            <div className="space-y-2">
              {filtered.map((project) => (
                <div
                  key={project.id}
                  className="flex items-start justify-between gap-3 rounded-lg border border-border px-4 py-3"
                >
                  <div className="min-w-0 flex-1 flex items-start gap-3">
                    <ProjectIcon
                      icon={project.icon}
                      className="mt-0.5 size-4 shrink-0 text-foreground"
                      imageClassName="mt-0.5 size-4 shrink-0 rounded-[4px]"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium">{project.name}</p>
                      {project.prompt && (
                        <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">
                          {project.prompt}
                        </p>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <ProjectCardMenu
                      project={project}
                      onStartChat={onStartChat}
                      onEdit={handleEdit}
                      onDelete={handleDelete}
                    />
                  </div>
                </div>
              ))}

              {/* New Project card */}
              <Button
                type="button"
                variant="ghost"
                onClick={handleNewProject}
                className="h-auto w-full rounded-lg border border-dashed border-border px-4 py-3 text-muted-foreground hover:border-border hover:bg-accent/50"
              >
                <Plus className="size-4" />
                <span className="text-sm">{t("view.newProject")}</span>
              </Button>
            </div>
          )}

          {/* Empty state */}
          {!loading && filtered.length === 0 && (
            <div className="flex flex-col items-center justify-center gap-3 py-16 text-muted-foreground rounded-lg border border-dashed border-transparent">
              <FolderKanban className="h-10 w-10 opacity-30" />
              <div className="text-center">
                <p className="text-sm font-medium">
                  {projects.length === 0
                    ? t("view.emptyTitle")
                    : t("view.noMatchesTitle")}
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  {projects.length === 0
                    ? t("view.emptyDescription")
                    : t("view.noMatchesDescription")}
                </p>
              </div>
              {projects.length === 0 && (
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  onClick={handleNewProject}
                  className="mt-2"
                >
                  <Plus className="size-3.5" />
                  {t("view.newProject")}
                </Button>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Create / Edit dialog */}
      <CreateProjectDialog
        isOpen={dialogOpen}
        onClose={handleDialogClose}
        onCreated={loadProjects}
        editingProject={editingProject}
      />

      {/* Delete confirmation dialog */}
      <AlertDialog
        open={!!deletingProject}
        onOpenChange={(open) => !open && setDeletingProject(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("view.deleteTitle", {
                name: deletingProject?.name ?? "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("view.deleteDescription", {
                name: deletingProject?.name ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={handleConfirmDeleteProject}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
