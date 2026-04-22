import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Sidebar } from "@/features/sidebar/ui/Sidebar";
import { StatusBar } from "@/features/status/ui/StatusBar";
import { CreateProjectDialog } from "@/features/projects/ui/CreateProjectDialog";
import { archiveProject } from "@/features/projects/api/projects";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { SettingsModal } from "@/features/settings/ui/SettingsModal";
import type { SectionId } from "@/features/settings/ui/SettingsModal";
import { OPEN_SETTINGS_EVENT } from "@/features/settings/lib/settingsEvents";
import { TopBar } from "./ui/TopBar";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  type ChatSession,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { findExistingDraft } from "@/features/chat/lib/newChat";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import { useAppStartup } from "./hooks/useAppStartup";
import { useHomeSessionStateSync } from "./hooks/useHomeSessionStateSync";
import { loadStoredHomeSessionId } from "./lib/homeSessionStorage";
import { resolveSupportedSessionModelPreference } from "./lib/resolveSupportedSessionModelPreference";
import { useCreatePersonaNavigation } from "./hooks/useCreatePersonaNavigation";
import { AppShellContent } from "./ui/AppShellContent";
import { acpPrepareSession, acpSetModel } from "@/shared/api/acp";
import {
  clearReplayBuffer,
  getAndDeleteReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { resolveSessionCwd } from "@/features/projects/lib/sessionCwdSelection";
import { perfLog } from "@/shared/lib/perfLog";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { sanitizeReplayMessages } from "@/features/chat/lib/replaySanitizer";

export type AppView =
  | "home"
  | "chat"
  | "skills"
  | "agents"
  | "projects"
  | "session-history";

const SIDEBAR_DEFAULT_WIDTH = 240;
const SIDEBAR_MIN_WIDTH = 180;
const SIDEBAR_MAX_WIDTH = 380;
const SIDEBAR_SNAP_COLLAPSE_THRESHOLD = 100;
const SIDEBAR_COLLAPSED_WIDTH = 48;
const SETTINGS_SECTIONS = new Set<SectionId>([
  "appearance",
  "providers",
  "compaction",
  "extensions",
  "voice",
  "general",
  "projects",
  "chats",
  "doctor",
  "about",
]);
export function AppShell({ children }: { children?: React.ReactNode }) {
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

  const chatStore = useChatStore();
  const sessionStore = useChatSessionStore();
  const agentStore = useAgentStore();
  const projectStore = useProjectStore();
  const providerInventoryEntries = useProviderInventoryStore((s) => s.entries);
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
      const gooseSessionId = session?.acpSessionId ?? sessionId;
      const project = session?.projectId
        ? (useProjectStore
            .getState()
            .projects.find((p) => p.id === session.projectId) ?? null)
        : null;
      const workingDir = await resolveSessionCwd(project);
      await acpLoadSession(sessionId, gooseSessionId, workingDir);
      const tFlush = performance.now();
      useChatStore.getState().setSessionLoading(sessionId, false);
      const buffer = getAndDeleteReplayBuffer(sessionId);
      const replayMessages = buffer
        ? sanitizeReplayMessages(buffer)
        : undefined;
      const replayStats = getReplayPerf(sessionId);
      clearReplayPerf(sessionId);
      if (replayMessages) {
        useChatStore.getState().setMessages(sessionId, replayMessages);
      }
      const t2 = performance.now();
      perfLog(
        `[perf:load] ${sid} replay: notifs=${replayStats?.count ?? 0} span=${replayStats?.spanMs.toFixed(1) ?? "0"}ms msgs=${replayMessages?.length ?? 0} flush=${(t2 - tFlush).toFixed(1)}ms total=${(t2 - t0).toFixed(1)}ms`,
      );
    } catch (err) {
      console.error("Failed to load session messages:", err);
      clearReplayBuffer(sessionId);
      useChatStore.getState().setSessionLoading(sessionId, false);
    }
  }, []);

  useAppStartup();

  useEffect(() => {
    projectStore.fetchProjects();
  }, [projectStore.fetchProjects]);

  const { activeSessionId } = sessionStore;

  useEffect(() => {
    if (activeView === "chat" && activeSessionId) {
      useChatStore.getState().markSessionRead(activeSessionId);
    }
  }, [activeSessionId, activeView]);

  const isHome = activeView === "home";

  const activeSession = activeSessionId
    ? sessionStore.getSession(activeSessionId)
    : undefined;
  const modelName =
    activeView === "chat" ? activeSession?.modelName : undefined;
  const tokenCount =
    activeView === "chat" && activeSessionId
      ? chatStore.getSessionRuntime(activeSessionId).tokenState.totalTokens
      : 0;
  const homeSession = homeSessionId
    ? sessionStore.getSession(homeSessionId)
    : undefined;

  useHomeSessionStateSync({
    homeSessionId,
    homeSession,
    messagesBySession: chatStore.messagesBySession,
    hasHydratedSessions: sessionStore.hasHydratedSessions,
    isLoading: sessionStore.isLoading,
    setHomeSessionId,
  });

  const ensureHomeSession = useCallback(async () => {
    if (!sessionStore.hasHydratedSessions || sessionStore.isLoading) {
      return null;
    }

    if (homeSessionRequestRef.current) {
      return homeSessionRequestRef.current;
    }

    const request = (async () => {
      if (
        homeSession &&
        !homeSession.archivedAt &&
        homeSession.messageCount === 0
      ) {
        const sessionModelPreference =
          await resolveSupportedSessionModelPreference(
            agentStore.selectedProvider ?? "goose",
            providerInventoryEntries,
          );
        const project = homeSession.projectId
          ? (projectStore.projects.find(
              (candidate) => candidate.id === homeSession.projectId,
            ) ?? null)
          : null;
        const workingDir = await resolveSessionCwd(project);
        await acpPrepareSession(
          homeSession.id,
          sessionModelPreference.providerId,
          workingDir,
          {
            personaId: homeSession.personaId,
          },
        );
        const shouldClearHomeModel =
          sessionModelPreference.providerId !== homeSession.providerId ||
          !sessionModelPreference.modelId;
        sessionStore.updateSession(homeSession.id, {
          providerId: sessionModelPreference.providerId,
          modelId: shouldClearHomeModel ? undefined : homeSession.modelId,
          modelName: shouldClearHomeModel ? undefined : homeSession.modelName,
        });
        if (sessionModelPreference.modelId) {
          await acpSetModel(homeSession.id, sessionModelPreference.modelId);
          sessionStore.updateSession(homeSession.id, {
            modelId: sessionModelPreference.modelId,
            modelName: sessionModelPreference.modelName,
          });
        }
        return homeSession;
      }

      const workingDir = await resolveSessionCwd(null);
      const sessionModelPreference =
        await resolveSupportedSessionModelPreference(
          agentStore.selectedProvider ?? "goose",
          providerInventoryEntries,
        );
      const session = await sessionStore.createSession({
        title: DEFAULT_CHAT_TITLE,
        providerId: sessionModelPreference.providerId,
        workingDir,
        modelId: sessionModelPreference.modelId,
        modelName: sessionModelPreference.modelName,
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
    agentStore.selectedProvider,
    homeSession,
    providerInventoryEntries,
    projectStore.projects,
    sessionStore.hasHydratedSessions,
    sessionStore,
    sessionStore.isLoading,
  ]);

  useEffect(() => {
    if (activeView !== "home") {
      return;
    }
    void ensureHomeSession().catch((error) => {
      console.error("Failed to ensure Home session:", error);
    });
  }, [activeView, ensureHomeSession]);

  const createNewTab = useCallback(
    async (title = DEFAULT_CHAT_TITLE, project?: ProjectInfo) => {
      const tStart = performance.now();
      perfLog(
        `[perf:newtab] createNewTab start (project=${project?.id ?? "none"})`,
      );
      const agentId = agentStore.activeAgentId ?? undefined;
      const providerId =
        project?.preferredProvider ?? agentStore.selectedProvider ?? "goose";
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
        sessionStore.setActiveSession(existingDraft.id);
        setActiveView("chat");
        chatStore.setActiveSession(existingDraft.id);
        perfLog(
          `[perf:newtab] ${existingDraft.id.slice(0, 8)} reused draft in ${(performance.now() - tStart).toFixed(1)}ms`,
        );
        return existingDraft;
      }

      const workingDir = await resolveSessionCwd(project);
      const session = await sessionStore.createSession({
        title,
        projectId: project?.id,
        agentId,
        providerId: sessionModelPreference.providerId,
        workingDir,
        modelId: sessionModelPreference.modelId,
        modelName: sessionModelPreference.modelName,
      });
      sessionStore.setActiveSession(session.id);
      setActiveView("chat");
      chatStore.setActiveSession(session.id);
      perfLog(
        `[perf:newtab] ${session.id.slice(0, 8)} created session in ${(performance.now() - tStart).toFixed(1)}ms`,
      );
      return session;
    },
    [
      agentStore.activeAgentId,
      agentStore.selectedProvider,
      chatStore,
      providerInventoryEntries,
      sessionStore,
    ],
  );

  const handleStartChatFromProject = useCallback(
    (project: ProjectInfo) => {
      void createNewTab(DEFAULT_CHAT_TITLE, project);
    },
    [createNewTab],
  );

  const handleNewChatInProject = useCallback(
    (projectId: string) => {
      const project = projectStore.projects.find((p) => p.id === projectId);
      if (project) {
        void createNewTab(DEFAULT_CHAT_TITLE, project);
      }
    },
    [createNewTab, projectStore.projects],
  );

  const handleArchiveProject = useCallback(
    async (projectId: string) => {
      try {
        await archiveProject(projectId);
        projectStore.fetchProjects();
      } catch {
        // best-effort
      }
    },
    [projectStore.fetchProjects],
  );

  const clearActiveSession = useCallback(
    (sessionId: string) => {
      chatStore.cleanupSession(sessionId);
      sessionStore.setActiveSession(null);
      setActiveView("home");
    },
    [chatStore, sessionStore],
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
        await sessionStore.archiveSession(sessionId);
        chatStore.cleanupSession(sessionId);

        if (!wasActiveSession) {
          return;
        }

        sessionStore.setActiveSession(null);
        setActiveView("home");
      } catch {
        // best-effort
      }
    },
    [chatStore, sessionStore],
  );

  const handleEditProject = useCallback(
    (projectId: string) => {
      const project = projectStore.projects.find((p) => p.id === projectId);
      if (project) {
        setEditingProject(project);
        setCreateProjectOpen(true);
      }
    },
    [projectStore.projects],
  );

  const handleMoveToProject = useCallback(
    (sessionId: string, projectId: string | null) => {
      sessionStore.updateSession(sessionId, { projectId });

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
        await acpPrepareSession(
          sessionId,
          session.providerId ?? agentStore.selectedProvider ?? "goose",
          workingDir,
          {
            personaId: session.personaId,
          },
        );
      })().catch((error) => {
        console.error(
          "Failed to update ACP session project working directory:",
          error,
        );
      });
    },
    [agentStore.selectedProvider, sessionStore],
  );

  const handleRenameChat = useCallback(
    (sessionId: string, nextTitle: string) => {
      sessionStore.updateSession(sessionId, {
        title: nextTitle,
        userSetName: true,
      });
    },
    [sessionStore],
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
      sessionStore.setActiveSession(sessionId);
      setActiveView("chat");
      chatStore.setActiveSession(sessionId);
      useChatStore.getState().markSessionRead(sessionId);
    },
    [chatStore, homeSessionId, sessionStore],
  );

  const handleSelectSession = useCallback(
    (id: string) => {
      sessionStore.setActiveSession(id);
      setActiveView("chat");
      chatStore.setActiveSession(id);
      useChatStore.getState().markSessionRead(id);
      loadSessionMessages(id);
    },
    [sessionStore, chatStore, loadSessionMessages],
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
        sessionStore.setActiveSession(null);
      }
      setActiveView(view);
    },
    [sessionStore],
  );

  const handleCreatePersona = useCreatePersonaNavigation(() =>
    handleNavigate("agents"),
  );

  const toggleSidebar = () => setSidebarCollapsed((prev) => !prev);

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
    setSidebarCollapsed(false);
    setSidebarWidth(SIDEBAR_DEFAULT_WIDTH);
  }, []);

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
        setSidebarCollapsed((prev) => !prev);
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
        sessionStore.setActiveSession(null);
        setActiveView("home");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [clearActiveSession, sessionStore]);

  const editingProjectProp = useMemo(
    () =>
      editingProject
        ? {
            id: editingProject.id,
            name: editingProject.name,
            description: editingProject.description,
            prompt: editingProject.prompt,
            icon: editingProject.icon,
            color: editingProject.color,
            preferredProvider: editingProject.preferredProvider,
            preferredModel: editingProject.preferredModel,
            workingDirs: editingProject.workingDirs,
            useWorktrees: editingProject.useWorktrees,
          }
        : undefined,
    [editingProject],
  );

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <TopBar onSettingsClick={() => openSettings()} />

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
            onNavigate={handleNavigate}
            onNewChatInProject={handleNewChatInProject}
            onNewChat={() => {
              sessionStore.setActiveSession(null);
              setActiveView("home");
            }}
            onCreateProject={() => openCreateProjectDialog()}
            onEditProject={handleEditProject}
            onArchiveProject={handleArchiveProject}
            onArchiveChat={handleArchiveChat}
            onRenameChat={handleRenameChat}
            onMoveToProject={handleMoveToProject}
            onReorderProject={projectStore.reorderProjects}
            onSelectSession={handleSelectSession}
            onSelectSearchResult={handleSelectSearchResult}
            activeView={activeView}
            activeSessionId={activeSessionId}
            projects={projectStore.projects}
            className="h-full rounded-xl"
          />
        </div>

        {/* biome-ignore lint/a11y/noStaticElementInteractions: drag handle for sidebar resize */}
        <div
          onMouseDown={handleResizeStart}
          onDoubleClick={handleResizeDoubleClick}
          className="flex-shrink-0 w-2 h-full cursor-col-resize group flex items-center justify-center"
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
            />
          )}
        </main>
      </div>

      <div
        className={`overflow-hidden transition-all duration-300 ease-in-out ${
          isHome ? "max-h-0 opacity-0" : "max-h-8 opacity-100"
        }`}
      >
        <StatusBar
          modelName={modelName}
          sessionId={activeSessionId ?? undefined}
          tokenCount={tokenCount}
        />
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
          projectStore.fetchProjects();
          pendingProjectCreatedRef.current?.(project.id);
          pendingProjectCreatedRef.current = null;
          setCreateProjectInitialWorkingDir(null);
        }}
        initialWorkingDir={createProjectInitialWorkingDir}
        editingProject={editingProjectProp}
      />
    </div>
  );
}
