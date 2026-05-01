import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsPage } from "@/shared/ui/SettingsPage";
import { Button, buttonVariants } from "@/shared/ui/button";
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
import {
  deleteProject,
  listArchivedProjects,
  restoreProject,
  type ProjectInfo,
} from "@/features/projects/api/projects";
import { ProjectIcon } from "@/features/projects/ui/ProjectIcon";
import { useProjectStore } from "@/features/projects/stores/projectStore";

export function ProjectsSettings() {
  const { t } = useTranslation(["settings", "common"]);
  const [archivedProjects, setArchivedProjects] = useState<ProjectInfo[]>([]);
  const [loadingArchived, setLoadingArchived] = useState(true);
  const [deletingProject, setDeletingProject] = useState<ProjectInfo | null>(
    null,
  );

  useEffect(() => {
    listArchivedProjects()
      .then(setArchivedProjects)
      .catch(() => setArchivedProjects([]))
      .finally(() => setLoadingArchived(false));
  }, []);

  async function handleRestoreProject(id: string) {
    try {
      await restoreProject(id);
      await useProjectStore.getState().fetchProjects();
      setArchivedProjects((prev) => prev.filter((p) => p.id !== id));
    } catch {
      // best-effort
    }
  }

  async function handleDelete(id: string) {
    try {
      await deleteProject(id);
      setArchivedProjects((prev) => prev.filter((p) => p.id !== id));
    } catch {
      // best-effort
    }
  }

  return (
    <>
      <SettingsPage title={t("projects.title")}>
        <div className="space-y-3">
          <h4 className="text-sm font-semibold">
            {t("projects.sectionTitle")}
          </h4>
          {!loadingArchived && archivedProjects.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              {t("projects.empty")}
            </p>
          ) : null}
          {archivedProjects.map((project) => (
            <div
              key={project.id}
              className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"
            >
              <div className="flex min-w-0 items-center gap-2">
                <ProjectIcon
                  icon={project.icon}
                  className="size-4 shrink-0 text-foreground"
                  imageClassName="size-4 shrink-0 rounded-[4px]"
                />
                <span className="truncate text-sm">{project.name}</span>
              </div>
              <div className="flex flex-shrink-0 items-center gap-1.5">
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  onClick={() => handleRestoreProject(project.id)}
                >
                  {t("common:actions.restore")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() => setDeletingProject(project)}
                  className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                >
                  {t("common:actions.delete")}
                </Button>
              </div>
            </div>
          ))}
        </div>
      </SettingsPage>

      <AlertDialog
        open={!!deletingProject}
        onOpenChange={(open) => !open && setDeletingProject(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("deleteProject.title", {
                name: deletingProject?.name ?? "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("deleteProject.description", {
                name: deletingProject?.name ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={() => {
                if (deletingProject) {
                  void handleDelete(deletingProject.id);
                  setDeletingProject(null);
                }
              }}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
