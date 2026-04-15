import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { type LocalePreference, useLocale } from "@/shared/i18n";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import {
  Palette,
  Settings2,
  FolderKanban,
  Info,
  MessageSquare,
  Stethoscope,
  X,
} from "lucide-react";
import { IconPlug } from "@tabler/icons-react";
import { AppearanceSettings } from "./AppearanceSettings";
import { DoctorSettings } from "./DoctorSettings";
import { ProvidersSettings } from "./ProvidersSettings";
import {
  listArchivedProjects,
  restoreProject,
  deleteProject,
  type ProjectInfo,
} from "@/features/projects/api/projects";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";

import type { Session } from "@/shared/types/chat";

const NAV_ITEMS = [
  { id: "appearance", labelKey: "nav.appearance", icon: Palette },
  { id: "providers", labelKey: "nav.providers", icon: IconPlug },
  { id: "general", labelKey: "nav.general", icon: Settings2 },
  { id: "projects", labelKey: "nav.projects", icon: FolderKanban },
  { id: "chats", labelKey: "nav.chats", icon: MessageSquare },
  { id: "doctor", labelKey: "nav.doctor", icon: Stethoscope },
  { id: "about", labelKey: "nav.about", icon: Info },
] as const;

export type SectionId = (typeof NAV_ITEMS)[number]["id"];

interface SettingsModalProps {
  onClose: () => void;
  initialSection?: SectionId;
}

export function SettingsModal({
  onClose,
  initialSection = "appearance",
}: SettingsModalProps) {
  const { t } = useTranslation(["settings", "common"]);
  const { preference, setLocalePreference, systemLocaleLabel } = useLocale();
  const [activeSection, setActiveSection] = useState<SectionId>(initialSection);
  const [isLoaded, setIsLoaded] = useState(false);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [archivedProjects, setArchivedProjects] = useState<ProjectInfo[]>([]);
  const [archivedChats, setArchivedChats] = useState<Session[]>([]);
  const [loadingArchived, setLoadingArchived] = useState(true);
  const [loadingArchivedChats, setLoadingArchivedChats] = useState(true);
  const [deletingProject, setDeletingProject] = useState<ProjectInfo | null>(
    null,
  );

  // Trigger entrance animations after mount
  useEffect(() => {
    const timer = setTimeout(() => setIsLoaded(true), 50);
    return () => clearTimeout(timer);
  }, []);

  // Load archived projects on mount
  useEffect(() => {
    listArchivedProjects()
      .then(setArchivedProjects)
      .catch(() => setArchivedProjects([]))
      .finally(() => setLoadingArchived(false));
  }, []);

  // Load archived chats from the session store (persisted in localStorage)
  useEffect(() => {
    const archived = useChatSessionStore.getState().getArchivedSessions();
    setArchivedChats(archived as unknown as Session[]);
    setLoadingArchivedChats(false);
  }, []);

  const handleRestoreProject = async (id: string) => {
    try {
      await restoreProject(id);
      await useProjectStore.getState().fetchProjects();
      setArchivedProjects((prev) => prev.filter((p) => p.id !== id));
    } catch {
      // best-effort
    }
  };

  const handleRestoreChat = async (id: string) => {
    await useChatSessionStore.getState().unarchiveSession(id);
    setArchivedChats((prev) => prev.filter((session) => session.id !== id));
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteProject(id);
      setArchivedProjects((prev) => prev.filter((p) => p.id !== id));
    } catch {
      // best-effort
    }
  };

  // Content transition on section change
  // biome-ignore lint/correctness/useExhaustiveDependencies: activeSection triggers the transition effect intentionally
  useEffect(() => {
    setIsTransitioning(true);
    const timer = setTimeout(() => setIsTransitioning(false), 150);
    return () => clearTimeout(timer);
  }, [activeSection]);

  const navItems = NAV_ITEMS.map((item) => ({
    ...item,
    label: t(item.labelKey),
  }));

  return (
    <div
      role="dialog"
      className={cn(
        "fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm transition-opacity duration-300",
        isLoaded ? "opacity-100" : "opacity-0",
      )}
      onClick={onClose}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: stopPropagation on inner container is not a meaningful interaction */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: click handler only prevents backdrop dismiss propagation */}
      <div
        className={cn(
          "flex h-[600px] w-full max-w-3xl overflow-hidden rounded-xl border bg-background shadow-modal transition-all duration-500 ease-out",
          isLoaded ? "opacity-100 scale-100" : "opacity-0 scale-95",
          isTransitioning ? "scale-[0.98]" : "scale-100",
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Sidebar */}
        <div
          className={cn(
            "flex w-44 flex-col border-r bg-muted/50 transition-all duration-700 ease-out",
            isLoaded ? "opacity-100 translate-x-0" : "opacity-0 -translate-x-2",
          )}
        >
          <div
            className={cn(
              "px-4 py-4 transition-all duration-500 ease-out",
              isLoaded
                ? "opacity-100 translate-x-0"
                : "opacity-0 -translate-x-2",
            )}
          >
            <h2 className="text-sm font-semibold">{t("title")}</h2>
          </div>
          <nav className="flex flex-col gap-1 px-2">
            {navItems.map((item, index) => (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                key={item.id}
                onClick={() => setActiveSection(item.id)}
                className={cn(
                  "w-full justify-start rounded-lg px-3 py-2 transition-all duration-600 ease-out",
                  activeSection === item.id
                    ? "bg-background text-foreground shadow-sm hover:bg-background"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground duration-300",
                  isLoaded
                    ? "opacity-100 translate-x-0"
                    : "opacity-0 translate-x-4",
                )}
                style={{
                  transitionDelay: isLoaded ? "0ms" : `${index * 40 + 300}ms`,
                }}
              >
                <item.icon className="size-4" />
                {item.label}
              </Button>
            ))}
          </nav>
        </div>

        {/* Content */}
        <div className="relative flex-1 overflow-y-auto">
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            onClick={onClose}
            aria-label={t("common:actions.close")}
            className="absolute right-4 top-4 z-10 rounded-md text-muted-foreground hover:text-foreground"
          >
            <X className="size-4" />
          </Button>

          <div
            className={cn(
              "px-6 py-4 transition-all duration-400 ease-out",
              isTransitioning
                ? "opacity-0 translate-y-2"
                : "opacity-100 translate-y-0",
            )}
          >
            <div
              className={cn(
                "transition-all duration-600 ease-out",
                isLoaded
                  ? "opacity-100 translate-y-0"
                  : "opacity-0 translate-y-4",
              )}
              style={{
                transitionDelay: isLoaded ? "400ms" : "0ms",
              }}
            >
              {activeSection === "appearance" && <AppearanceSettings />}
              {activeSection === "providers" && <ProvidersSettings />}
              {activeSection === "doctor" && <DoctorSettings />}
              {activeSection === "general" && (
                <div className="space-y-6">
                  <div>
                    <h3 className="text-lg font-semibold font-display tracking-tight">
                      {t("general.title")}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {t("general.description")}
                    </p>
                  </div>

                  <div className="space-y-3">
                    <div>
                      <h4 className="text-sm font-semibold">
                        {t("general.language.label")}
                      </h4>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {t("general.language.description")}
                      </p>
                    </div>
                    <Select
                      value={preference}
                      onValueChange={(value) =>
                        void setLocalePreference(value as LocalePreference)
                      }
                    >
                      <SelectTrigger className="w-full max-w-sm">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="system">
                          {t("general.language.system", {
                            language: systemLocaleLabel,
                          })}
                        </SelectItem>
                        <SelectItem value="en">
                          {t("general.language.english")}
                        </SelectItem>
                        <SelectItem value="es">
                          {t("general.language.spanish")}
                        </SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              )}
              {activeSection === "projects" && (
                <div className="space-y-6">
                  <div>
                    <h3 className="text-lg font-semibold font-display tracking-tight">
                      {t("projects.title")}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {t("projects.description")}
                    </p>
                  </div>

                  {/* Archived Projects */}
                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold">
                      {t("projects.sectionTitle")}
                    </h3>
                    {!loadingArchived && archivedProjects.length === 0 && (
                      <p className="text-xs text-muted-foreground">
                        {t("projects.empty")}
                      </p>
                    )}
                    {archivedProjects.map((project) => (
                      <div
                        key={project.id}
                        className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <span
                            className="inline-block w-2 h-2 rounded-full flex-shrink-0"
                            style={{ backgroundColor: project.color }}
                          />
                          <span className="text-sm truncate">
                            {project.name}
                          </span>
                        </div>
                        <div className="flex items-center gap-1.5 flex-shrink-0">
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
                            className="text-destructive hover:text-destructive hover:bg-destructive/10"
                          >
                            {t("common:actions.delete")}
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {activeSection === "chats" && (
                <div className="space-y-6">
                  <div>
                    <h3 className="text-lg font-semibold font-display tracking-tight">
                      {t("chats.title")}
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {t("chats.description")}
                    </p>
                  </div>

                  <div className="space-y-3">
                    <h3 className="text-sm font-semibold">
                      {t("chats.sectionTitle")}
                    </h3>
                    {!loadingArchivedChats && archivedChats.length === 0 && (
                      <p className="text-xs text-muted-foreground">
                        {t("chats.empty")}
                      </p>
                    )}
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
                </div>
              )}
              {activeSection === "about" && (
                <div>
                  <h3 className="text-lg font-semibold font-display tracking-tight">
                    {t("about.title")}
                  </h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {t("about.description")}
                  </p>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      <AlertDialog
        open={!!deletingProject}
        onOpenChange={(open) => !open && setDeletingProject(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteProject.title")}</AlertDialogTitle>
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
                  handleDelete(deletingProject.id);
                  setDeletingProject(null);
                }
              }}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
