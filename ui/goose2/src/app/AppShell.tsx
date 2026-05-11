import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Sidebar } from "@/features/sidebar/ui/Sidebar";
import { CreateProjectDialog } from "@/features/projects/ui/CreateProjectDialog";
import { archiveProject } from "@/features/projects/api/projects";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { SettingsModal } from "@/features/settings/ui/SettingsModal";
import type { SectionId } from "@/features/settings/ui/SettingsModal";
import { OPEN_SETTINGS_EVENT } from "@/features/settings/lib/settingsEvents";
import { TopBar } from "./ui/TopBar";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { selectMessagesBySession } from "@/features/chat/stores/chatSelectors";
import {
  type ChatSession,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import {
  selectActiveSessionId,
  selectHasHydratedSessions,
  selectSessions,
  selectSessionsLoading,
} from "@/features/chat/stores/chatSessionSelectors";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { selectSelectedProvider } from "@/features/agents/stores/agentSelectors";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { selectProjects } from "@/features/projects/stores/projectSelectors";
import { findExistingDraft } from "@/features/chat/lib/newChat";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import { useAppStartup } from "./hooks/useAppStartup";
import { useHomeSessionStateSync } from "./hooks/useHomeSessionStateSync";
import { loadStoredHomeSessionId } from "./lib/homeSessionStorage";
import { resolveSupportedSessionModelPreference } from "./lib/resolveSupportedSessionModelPreference";
import { useCreatePersonaNavigation } from "./hooks/useCreatePersonaNavigation";
import { AppShellContent } from "./ui/AppShellContent";
import { applyLatestSessionConfig } from "@/features/chat/lib/sessionConfigRequests";
import { updateSessionTitle } from "@/features/chat/stores/chatSessionOperations";
import {
  clearReplayBuffer,
  getAndDeleteReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { resolveSessionCwd } from "@/features/projects/lib/sessionCwdSelection";
import { perfLog } from "@/shared/lib/perfLog";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import type { SkillInfo } from "@/features/skills/api/skills";
import { toChatSkillDraft } from "@/features/skills/lib/skillChatPrompt";
import { OnboardingFlow } from "@/features/onboarding/ui/OnboardingFlow";
import { useOnboardingGate } from "@/features/onboarding/hooks/useOnboardingGate";
import { Spinner } from "@/shared/ui/spinner";

export type AppView =
  | "home"
  | "chat"
  | "skills"
  | "extensions"
  | "agents"
  | "projects"
  | "session-history";

const SIDEBAR_DEFAULT_WIDTH = 240;
const SIDEBAR_MIN_WIDTH = 180;
const SIDEBAR_MAX_WIDTH = 380;
const SIDEBAR_SNAP_COLLAPSE_THRESHOLD = 100;
const SIDEBAR_COLLAPSED_WIDTH = 48;
const APP_SHELL_HORIZONTAL_CHROME_WIDTH = 28;
const MIN_MAIN_CONTENT_WIDTH = 532;
const MIN_WINDOW_HEIGHT = 600;
const COLLAPSED_WINDOW_MIN_WIDTH =
  SIDEBAR_COLLAPSED_WIDTH +
  APP_SHELL_HORIZONTAL_CHROME_WIDTH +
  MIN_MAIN_CONTENT_WIDTH;
const SETTINGS_SECTIONS = new Set<SectionId>([
  "appearance",
  "providers",
  "compaction",
  "voice",
  "general",
  "projects",
  "chats",
  "doctor",
  "about",
]);

function getExpandedSidebarFitWidth(sidebarWidth: number) {
  return (
    sidebarWidth + APP_SHELL_HORIZONTAL_CHROME_WIDTH + MIN_MAIN_CONTENT_WIDTH
  );
}

async function ensureWindowWidth(minWidth: number) {
  if (!window.__TAURI_INTERNALS__ || window.innerWidth >= minWidth) {
    return;
  }

  const { getCurrentWindow, LogicalSize } = await import(
    "@tauri-apps/api/window"
  );
  await getCurrentWindow().setSize(
    new LogicalSize(minWidth, window.innerHeight),
  );
}

async function syncWindowMinimumSize() {
  if (!window.__TAURI_INTERNALS__) {
    return;
  }

  const { getCurrentWindow, LogicalSize } = await import(
    "@tauri-apps/api/window"
  );
  await getCurrentWindow().setMinSize(
    new LogicalSize(COLLAPSED_WINDOW_MIN_WIDTH, MIN_WINDOW_HEIGHT),
  );
}

export function AppShell({ children }: { children?: React.ReactNode }) {
  const { t } = useTranslation("chat");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(SIDEBAR_DEFAULT_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsInitialSection, setSettingsInitialSection] =
    useState<SectionId>("appearance");
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [createProjectInitialWorkingDir, setCreateProjectInitialWorkingDir] =
    useState<string | null>(null);
  const [editingProject, setEditingProject] = useState<ProjectInfo | null>(
    null,
  );
  const [activeView, setActiveView] = useState<AppView>("home");
  const [homeSessionId, setHomeSessionId] = useState<string | null>(() =>
    loadStoredHomeSessionId(),
  );

  const messagesBySession = useChatStore(selectMessagesBySession);
  const setChatActiveSession = useChatStore((s) => s.setActiveSession);
  const cleanupChatSession = useChatStore((s) => s.cleanupSession);
  const sessions = useChatSessionStore(selectSessions);
  const activeSessionId = useChatSessionStore(selectActiveSessionId);
  const hasHydratedSessions = useChatSessionStore(selectHasHydratedSessions);
  const sessionsLoading = useChatSessionStore(selectSessionsLoading);
  const createSession = useChatSessionStore((s) => s.createSession);
  const patchSession = useChatSessionStore((s) => s.patchSession);
  const setActiveSession = useChatSessionStore((s) => s.setActiveSession);
  const archiveSession = useChatSessionStore((s) => s.archiveSession);
  const selectedProvider = useAgentStore(selectSelectedProvider);
  const projects = useProjectStore(selectProjects);
  const fetchProjects = useProjectStore((s) => s.fetchProjects);
  const reorderProjects = useProjectStore((s) => s.reorderProjects);
  const providerInventoryEntries = useProviderInventoryStore((s) => s.entries);
  const startup = useAppStartup();
  const onboardingGate = useOnboardingGate(startup.ready);
  const pendingProjectCreatedRef = useRef<((projectId: string) => void) | null>(
    null,
  );
  const homeSessionRequestRef = useRef<Promise<ChatSession | null> | null>(
    null,
  );
  const loadSessionMessages = useCallback(async (sessionId: string) => {
    const sid = sessionId.slice(0, 8);
    const existingMsgs = useChatStore.getState().messagesBySession[sessionId];
    if ((existingMsgs?.length ?? 0) > 0) {
      perfLog(`[perf:load] ${sid} skip — has messages`);
      return;
    }
    const t0 = performance.now();
    perfLog(`[perf:load] ${sid} start`);
    useChatStore.getState().setSessionLoading(sessionId, true);
    try {
      const [{ acpLoadSession }, { getReplayPerf, clearReplayPerf }] =
        await Promise.all([
          import("@/shared/api/acp"),
          import("@/shared/api/acpNotificationHandler"),
        ]);
      const t1 = performance.now();
      perfLog(`[perf:load] ${sid} import in ${(t1 - t0).toFixed(1)}ms`);
      const session = useChatSessionStore.getState().getSession(sessionId);
      const project = session?.projectId
        ? (useProjectStore
            .getState()
            .projects.find((p) => p.id === session.projectId) ?? null)
        : null;
      const workingDir = await resolveSessionCwd(project);
      await acpLoadSession(sessionId, workingDir);
      const tFlush = performance.now();
      useChatStore.getState().setSessionLoading(sessionId, false);
      const buffer = getAndDeleteReplayBuffer(sessionId);
      const replayStats = getReplayPerf(sessionId);
      clearReplayPerf(sessionId);
      if (buffer && buffer.length > 0) {
        useChatStore.getState().setMessages(sessionId, buffer);
      }
      const t2 = performance.now();
      perfLog(
        `[perf:load] ${sid} replay: notifs=${replayStats?.count ?? 0} span=${replayStats?.spanMs.toFixed(1) ?? "0"}ms msgs=${buffer?.length ?? 0} flush=${(t2 - tFlush).toFixed(1)}ms total=${(t2 - t0).toFixed(1)}ms`,
      );
    } catch (err) {
      console.error("Failed to load session messages:", err);
      clearReplayBuffer(sessionId);
      useChatStore.getState().setSessionLoading(sessionId, false);
    }
  }, []);

  useEffect(() => {
    fetchProjects();
  }, [fetchProjects]);

  useEffect(() => {
    if (activeView === "chat" && activeSessionId) {
      useChatStore.getState().markSessionRead(activeSessionId);
    }
  }, [activeSessionId, activeView]);

  const activeSession = activeSessionId
    ? sessions.find((session) => session.id === activeSessionId)
    : undefined;
  const homeSession = homeSessionId
    ? sessions.find((session) => session.id === homeSessionId)
    : undefined;

  useHomeSessionStateSync({
    homeSessionId,
    homeSession,
    messagesBySession,
    hasHydratedSessions,
    isLoading: sessionsLoading,
    setHomeSessionId,
  });

  const ensureHomeSession = useCallback(async () => {
    if (!hasHydratedSessions || sessionsLoading) {
      return null;
    }

    if (homeSessionRequestRef.current) {
      return homeSessionRequestRef.current;
    }

    const request = (async () => {
      const currentProvider = () =>
        useAgentStore.getState().selectedProvider ?? "goose";

      // Resolve the provider to use after an async gap. If the user changed
      // their selection while we were awaiting (liveProvider differs from what
      // it was before the await), prefer the live value; otherwise use the
      // model-preference resolution result.
      const resolveProviderAfterAwait = (
        providerAtStart: string,
        sessionModelPreference: { providerId: string },
      ): string => {
        const liveProvider = currentProvider();
        return liveProvider !== providerAtStart
          ? liveProvider
          : sessionModelPreference.providerId;
      };

      if (
        homeSession &&
        !homeSession.archivedAt &&
        homeSession.messageCount === 0
      ) {
        const providerAtStart = currentProvider();
        const sessionModelPreference =
          await resolveSupportedSessionModelPreference(
            providerAtStart,
            providerInventoryEntries,
          );
        const project = homeSession.projectId
          ? (projects.find(
              (candidate) => candidate.id === homeSession.projectId,
            ) ?? null)
          : null;
        const workingDir = await resolveSessionCwd(project);
        const resolvedProviderId = resolveProviderAfterAwait(
          providerAtStart,
          sessionModelPreference,
        );
        const modelIdToApply =
          resolvedProviderId === sessionModelPreference.providerId
            ? sessionModelPreference.modelId
            : undefined;
        const result = await applyLatestSessionConfig({
          sessionId: homeSession.id,
          providerId: resolvedProviderId,
          workingDir,
          modelId: modelIdToApply,
        });
        if (!result.applied) {
          return homeSession;
        }

        const shouldClearHomeModel =
          resolvedProviderId !== homeSession.providerId || !modelIdToApply;
        patchSession(homeSession.id, {
          providerId: resolvedProviderId,
          modelId:
            modelIdToApply ??
            (shouldClearHomeModel ? undefined : homeSession.modelId),
          modelName:
            modelIdToApply != null
              ? sessionModelPreference.modelName
              : shouldClearHomeModel
                ? undefined
                : homeSession.modelName,
        });
        return (
          useChatSessionStore.getState().getSession(homeSession.id) ??
          homeSession
        );
      }

      const providerAtStart = currentProvider();
      const workingDir = await resolveSessionCwd(null);
      const sessionModelPreference =
        await resolveSupportedSessionModelPreference(
          providerAtStart,
          providerInventoryEntries,
        );
      const resolvedProviderId = resolveProviderAfterAwait(
        providerAtStart,
        sessionModelPreference,
      );
      const session = await createSession({
        title: DEFAULT_CHAT_TITLE,
        providerId: resolvedProviderId,
        workingDir,
        modelId:
          resolvedProviderId === sessionModelPreference.providerId
            ? sessionModelPreference.modelId
            : undefined,
        modelName:
          resolvedProviderId === sessionModelPreference.providerId
            ? sessionModelPreference.modelName
            : undefined,
      });
      setHomeSessionId(session.id);
      return session;
    })();

    homeSessionRequestRef.current = request;
    try {
      return await request;
    } finally {
      if (homeSessionRequestRef.current === request) {
        homeSessionRequestRef.current = null;
      }
    }
  }, [
    createSession,
    hasHydratedSessions,
    homeSession,
    providerInventoryEntries,
    projects,
    sessionsLoading,
    patchSession,
  ]);

  useEffect(() => {
    if (activeView !== "home" || onboardingGate.shouldShowOnboarding) {
      return;
    }
    void ensureHomeSession().catch((error) => {
      console.error("Failed to ensure Home session:", error);
    });
  }, [activeView, ensureHomeSession, onboardingGate.shouldShowOnboarding]);

  const createNewTab = useCallback(
    async (title = DEFAULT_CHAT_TITLE, project?: ProjectInfo) => {
      const tStart = performance.now();
      perfLog(
        `[perf:newtab] createNewTab start (project=${project?.id ?? "none"})`,
      );
      const providerId =
        project?.preferredProvider ?? selectedProvider ?? "goose";
      const sessionModelPreference =
        await resolveSupportedSessionModelPreference(
          providerId,
          providerInventoryEntries,
          project?.preferredModel ?? undefined,
        );
      const sessionState = useChatSessionStore.getState();
      const chatState = useChatStore.getState();
      const existingDraft = findExistingDraft({
        sessions: sessionState.sessions,
        activeSessionId: sessionState.activeSessionId,
        draftsBySession: chatState.draftsBySession,
        messagesBySession: chatState.messagesBySession,
        request: {
          title,
          projectId: project?.id,
        },
      });

      if (existingDraft) {
        setActiveSession(existingDraft.id);
        setActiveView("chat");
        setChatActiveSession(existingDraft.id);
        perfLog(
          `[perf:newtab] ${existingDraft.id.slice(0, 8)} reused draft in ${(performance.now() - tStart).toFixed(1)}ms`,
        );
        return existingDraft;
      }

      const workingDir = await resolveSessionCwd(project);
      const session = await createSession({
        title,
        projectId: project?.id,
        providerId: sessionModelPreference.providerId,
        workingDir,
        modelId: sessionModelPreference.modelId,
        modelName: sessionModelPreference.modelName,
      });
      setActiveSession(session.id);
      setActiveView("chat");
      setChatActiveSession(session.id);
      perfLog(
        `[perf:newtab] ${session.id.slice(0, 8)} created session in ${(performance.now() - tStart).toFixed(1)}ms`,
      );
      return session;
    },
    [
      selectedProvider,
      createSession,
      providerInventoryEntries,
      setActiveSession,
      setChatActiveSession,
    ],
  );

  const handleStartChatFromProject = useCallback(
    (project: ProjectInfo) => {
      void createNewTab(DEFAULT_CHAT_TITLE, project);
    },
    [createNewTab],
  );

  const handleStartChatWithSkill = useCallback(
    (skill: SkillInfo, projectId?: string | null) => {
      const project = projectId
        ? projects.find((candidate) => candidate.id === projectId)
        : undefined;

      void createNewTab(DEFAULT_CHAT_TITLE, project)
        .then((session) => {
          useChatStore
            .getState()
            .setSkillDrafts(session.id, [toChatSkillDraft(skill)]);
        })
        .catch((error) => {
          console.error("Failed to start chat with skill:", error);
        });
    },
    [createNewTab, projects],
  );

  const handleNewChatInProject = useCallback(
    (projectId: string) => {
      const project = projects.find((p) => p.id === projectId);
      if (project) {
        void createNewTab(DEFAULT_CHAT_TITLE, project);
      }
    },
    [createNewTab, projects],
  );

  const handleArchiveProject = useCallback(
    async (projectId: string) => {
      try {
        await archiveProject(projectId);
        fetchProjects();
      } catch {
        // best-effort
      }
    },
    [fetchProjects],
  );

  const clearActiveSession = useCallback(
    (sessionId: string) => {
      cleanupChatSession(sessionId);
      setActiveSession(null);
      setActiveView("home");
    },
    [cleanupChatSession, setActiveSession],
  );
  const openSettings = useCallback((section: SectionId = "appearance") => {
    setSettingsInitialSection(section);
    setSettingsOpen(true);
  }, []);

  useEffect(() => {
    const handleOpenSettingsEvent = (event: Event) => {
      const section = (event as CustomEvent<{ section?: string }>).detail
        ?.section;
      if (section && SETTINGS_SECTIONS.has(section as SectionId)) {
        openSettings(section as SectionId);
        return;
      }

      openSettings();
    };

    window.addEventListener(
      OPEN_SETTINGS_EVENT,
      handleOpenSettingsEvent as EventListener,
    );
    return () => {
      window.removeEventListener(
        OPEN_SETTINGS_EVENT,
        handleOpenSettingsEvent as EventListener,
      );
    };
  }, [openSettings]);

  const handleArchiveChat = useCallback(
    async (sessionId: string) => {
      const { activeSessionId: currentActiveSessionId } =
        useChatSessionStore.getState();
      const wasActiveSession = currentActiveSessionId === sessionId;

      try {
        await archiveSession(sessionId);
        cleanupChatSession(sessionId);

        if (!wasActiveSession) {
          return;
        }

        setActiveSession(null);
        setActiveView("home");
      } catch {
        // best-effort
      }
    },
    [archiveSession, cleanupChatSession, setActiveSession],
  );

  const handleEditProject = useCallback(
    (projectId: string) => {
      const project = projects.find((p) => p.id === projectId);
      if (project) {
        setEditingProject(project);
        setCreateProjectOpen(true);
      }
    },
    [projects],
  );

  const handleMoveToProject = useCallback(
    (sessionId: string, projectId: string | null) => {
      useChatSessionStore.getState().patchSession(sessionId, { projectId });

      const session = useChatSessionStore.getState().getSession(sessionId);
      if (!session) {
        return;
      }

      void (async () => {
        const nextProject =
          projectId == null
            ? null
            : (useProjectStore
                .getState()
                .projects.find((project) => project.id === projectId) ?? null);
        const workingDir = await resolveSessionCwd(nextProject);
        if (!workingDir) {
          return;
        }
        await applyLatestSessionConfig({
          sessionId,
          providerId: session.providerId ?? selectedProvider ?? "goose",
          workingDir,
          modelId: session.modelId,
        });
      })().catch((error) => {
        console.error(
          "Failed to update ACP session project working directory:",
          error,
        );
      });
    },
    [selectedProvider],
  );

  const handleRenameChat = useCallback(
    (sessionId: string, nextTitle: string) => {
      void updateSessionTitle(sessionId, nextTitle).catch((error) => {
        console.error("Failed to rename session:", error);
        toast.error(t("notifications.renameError"));
      });
    },
    [t],
  );

  const openCreateProjectDialog = useCallback(
    (options?: {
      initialWorkingDir?: string | null;
      onCreated?: (projectId: string) => void;
    }) => {
      setEditingProject(null);
      setCreateProjectInitialWorkingDir(options?.initialWorkingDir ?? null);
      pendingProjectCreatedRef.current = options?.onCreated ?? null;
      setCreateProjectOpen(true);
    },
    [],
  );

  const activateHomeSession = useCallback(
    (sessionId: string) => {
      if (homeSessionId === sessionId) {
        setHomeSessionId(null);
      }
      setActiveSession(sessionId);
      setActiveView("chat");
      setChatActiveSession(sessionId);
      useChatStore.getState().markSessionRead(sessionId);
    },
    [homeSessionId, setActiveSession, setChatActiveSession],
  );

  const handleSelectSession = useCallback(
    (id: string) => {
      setActiveSession(id);
      setActiveView("chat");
      setChatActiveSession(id);
      useChatStore.getState().markSessionRead(id);
      loadSessionMessages(id);
    },
    [setActiveSession, setChatActiveSession, loadSessionMessages],
  );

  const handleSelectSearchResult = useCallback(
    (sessionId: string, messageId?: string, query?: string) => {
      if (messageId) {
        useChatStore
          .getState()
          .setScrollTargetMessage(sessionId, messageId, query);
      }
      handleSelectSession(sessionId);
    },
    [handleSelectSession],
  );

  const handleNavigate = useCallback(
    (view: AppView) => {
      if (view !== "chat") {
        setActiveSession(null);
      }
      setActiveView(view);
    },
    [setActiveSession],
  );

  const handleCreatePersona = useCreatePersonaNavigation(() =>
    handleNavigate("agents"),
  );

  const collapseSidebar = useCallback(() => {
    setSidebarCollapsed(true);
  }, []);

  const expandSidebar = useCallback(async () => {
    const expandedFitWidth = getExpandedSidebarFitWidth(sidebarWidth);

    try {
      await ensureWindowWidth(expandedFitWidth);
    } catch (error) {
      console.warn("Failed to resize window before expanding sidebar:", error);
    }

    setSidebarCollapsed(false);
  }, [sidebarWidth]);

  const toggleSidebar = useCallback(() => {
    if (sidebarCollapsed) {
      void expandSidebar();
      return;
    }

    collapseSidebar();
  }, [collapseSidebar, expandSidebar, sidebarCollapsed]);

  const handleResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsResizing(true);
      const startX = e.clientX;
      const startWidth = sidebarCollapsed
        ? SIDEBAR_COLLAPSED_WIDTH
        : sidebarWidth;
      let shouldCollapse = false;

      const onMouseMove = (moveEvent: MouseEvent) => {
        const delta = moveEvent.clientX - startX;
        const newWidth = startWidth + delta;

        if (newWidth < SIDEBAR_SNAP_COLLAPSE_THRESHOLD) {
          shouldCollapse = true;
          setSidebarWidth(SIDEBAR_MIN_WIDTH);
        } else {
          shouldCollapse = false;
          setSidebarCollapsed(false);
          setSidebarWidth(
            Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, newWidth)),
          );
        }
      };

      const cleanup = () => {
        setIsResizing(false);
        if (shouldCollapse) setSidebarCollapsed(true);
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", cleanup);
        window.removeEventListener("blur", cleanup);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", cleanup);
      window.addEventListener("blur", cleanup);
    },
    [sidebarCollapsed, sidebarWidth],
  );

  const handleResizeDoubleClick = useCallback(() => {
    setSidebarWidth(SIDEBAR_DEFAULT_WIDTH);
    void ensureWindowWidth(getExpandedSidebarFitWidth(SIDEBAR_DEFAULT_WIDTH))
      .catch((error) => {
        console.warn(
          "Failed to resize window before resetting sidebar:",
          error,
        );
      })
      .finally(() => setSidebarCollapsed(false));
  }, []);

  useEffect(() => {
    void syncWindowMinimumSize().catch((error) => {
      console.warn("Failed to update window minimum size:", error);
    });
  }, []);

  useEffect(() => {
    if (sidebarCollapsed) {
      return;
    }

    const handleWindowResize = () => {
      if (window.innerWidth < getExpandedSidebarFitWidth(sidebarWidth)) {
        setSidebarCollapsed(true);
      }
    };

    handleWindowResize();
    window.addEventListener("resize", handleWindowResize);
    return () => window.removeEventListener("resize", handleWindowResize);
  }, [sidebarCollapsed, sidebarWidth]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Cmd+, for settings
      if (e.key === "," && e.metaKey) {
        e.preventDefault();
        setSettingsOpen((prev) => !prev);
      }
      // Cmd+B for sidebar toggle
      if (e.key === "b" && e.metaKey) {
        e.preventDefault();
        toggleSidebar();
      }
      // Cmd+W returns to home instead of closing the window
      if (e.key === "w" && e.metaKey) {
        e.preventDefault();
        const { activeSessionId } = useChatSessionStore.getState();
        if (activeSessionId) {
          clearActiveSession(activeSessionId);
        }
      }
      // Cmd+N opens new conversation screen
      if (e.key === "n" && e.metaKey) {
        e.preventDefault();
        setActiveSession(null);
        setActiveView("home");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [clearActiveSession, setActiveSession, toggleSidebar]);

  if (!startup.ready) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-background text-foreground">
        <Spinner className="size-5 text-brand" />
      </div>
    );
  }

  if (onboardingGate.shouldShowOnboarding) {
    return (
      <OnboardingFlow
        readiness={onboardingGate.readiness}
        onComplete={(setup) => {
          onboardingGate.completeOnboarding(setup);
          setActiveView("home");
        }}
      />
    );
  }

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <TopBar />

      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div
          className="flex-shrink-0 h-full py-3 pl-3"
          style={{
            width: sidebarCollapsed
              ? SIDEBAR_COLLAPSED_WIDTH + 12
              : sidebarWidth + 12,
            transition: isResizing ? "none" : "width 200ms ease-out",
          }}
        >
          <Sidebar
            collapsed={sidebarCollapsed}
            width={sidebarWidth}
            isResizing={isResizing}
            onCollapse={toggleSidebar}
            onSettingsClick={() => openSettings()}
            onNavigate={handleNavigate}
            onNewChatInProject={handleNewChatInProject}
            onNewChat={() => {
              setActiveSession(null);
              setActiveView("home");
            }}
            onCreateProject={() => openCreateProjectDialog()}
            onEditProject={handleEditProject}
            onArchiveProject={handleArchiveProject}
            onArchiveChat={handleArchiveChat}
            onRenameChat={handleRenameChat}
            onMoveToProject={handleMoveToProject}
            onReorderProject={reorderProjects}
            onSelectSession={handleSelectSession}
            onSelectSearchResult={handleSelectSearchResult}
            activeView={activeView}
            activeSessionId={activeSessionId}
            projects={projects}
            className="h-full rounded-xl"
          />
        </div>

        {/* biome-ignore lint/a11y/noStaticElementInteractions: drag handle for sidebar resize */}
        <div
          onMouseDown={handleResizeStart}
          onDoubleClick={handleResizeDoubleClick}
          className="flex-shrink-0 w-4 h-full cursor-col-resize group flex items-center justify-center"
        >
          <div className="w-px h-8 rounded-full bg-transparent group-hover:bg-border transition-colors" />
        </div>

        <main className="min-h-0 min-w-0 flex-1">
          {children ?? (
            <AppShellContent
              activeView={activeView}
              activeSession={activeSession}
              homeSessionId={homeSessionId}
              onCreatePersona={handleCreatePersona}
              onArchiveChat={handleArchiveChat}
              onCreateProject={openCreateProjectDialog}
              onActivateHomeSession={activateHomeSession}
              onRenameChat={handleRenameChat}
              onSelectSession={handleSelectSession}
              onSelectSearchResult={handleSelectSearchResult}
              onStartChatFromProject={handleStartChatFromProject}
              onStartChatWithSkill={handleStartChatWithSkill}
            />
          )}
        </main>
      </div>

      {settingsOpen && (
        <SettingsModal
          initialSection={settingsInitialSection}
          onClose={() => setSettingsOpen(false)}
        />
      )}

      <CreateProjectDialog
        isOpen={createProjectOpen}
        onClose={() => {
          setCreateProjectOpen(false);
          setEditingProject(null);
          setCreateProjectInitialWorkingDir(null);
          pendingProjectCreatedRef.current = null;
        }}
        onCreated={(project) => {
          fetchProjects();
          pendingProjectCreatedRef.current?.(project.id);
          pendingProjectCreatedRef.current = null;
          setCreateProjectInitialWorkingDir(null);
        }}
        initialWorkingDir={createProjectInitialWorkingDir}
        editingProject={editingProject ?? undefined}
      />
    </div>
  );
}
