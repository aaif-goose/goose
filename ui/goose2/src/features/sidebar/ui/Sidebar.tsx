import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  IconLayoutSidebar,
  IconLayoutSidebarFilled,
} from "@tabler/icons-react";
import { BookOpen, Bot, History, Home, Search } from "lucide-react";
import { getDisplaySessionTitle } from "@/features/chat/lib/sessionTitle";
import { GooseIcon } from "@/shared/ui/icons/GooseIcon";
import { cn } from "@/shared/lib/cn";
import type { AppView } from "@/app/AppShell";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  getVisibleSessions,
  useChatSessionStore,
} from "@/features/chat/stores/chatSessionStore";
import { isSessionRunning } from "@/features/chat/lib/sessionActivity";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { Button } from "@/shared/ui/button";
import { useSessionSearch } from "@/features/sessions/hooks/useSessionSearch";
import { SidebarProjectsSection } from "./SidebarProjectsSection";
import { SidebarSearchResults } from "./SidebarSearchResults";
import { useSidebarHighlight } from "./useSidebarHighlight";

interface SidebarProps {
  collapsed: boolean;
  width?: number;
  isResizing?: boolean;
  onCollapse: () => void;
  onNewChatInProject?: (projectId: string) => void;
  onNewChat?: () => void;
  onCreateProject?: () => void;
  onEditProject?: (projectId: string) => void;
  onArchiveProject?: (projectId: string) => void;
  onArchiveChat?: (sessionId: string) => void;
  onRenameChat?: (sessionId: string, nextTitle: string) => void;
  onMoveToProject?: (sessionId: string, projectId: string | null) => void;
  onReorderProject?: (fromId: string, toId: string) => void;
  onNavigate?: (view: AppView) => void;
  onSelectSession?: (sessionId: string) => void;
  onSelectSearchResult?: (
    sessionId: string,
    messageId?: string,
    query?: string,
  ) => void;
  activeView?: AppView;
  activeSessionId?: string | null;
  className?: string;
  projects: ProjectInfo[];
}

const EXPANDED_PROJECTS_STORAGE_KEY = "goose:sidebar:expanded-projects";

export function Sidebar({
  collapsed,
  width = 240,
  isResizing = false,
  onCollapse,
  onNewChatInProject,
  onNewChat,
  onCreateProject,
  onEditProject,
  onArchiveProject,
  onArchiveChat,
  onRenameChat,
  onMoveToProject,
  onReorderProject,
  onNavigate,
  onSelectSession,
  onSelectSearchResult,
  activeView,
  activeSessionId,
  className,
  projects,
}: SidebarProps) {
  const { t, i18n } = useTranslation(["sidebar", "common"]);
  const [expanded, setExpanded] = useState(!collapsed);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const prevCollapsed = useRef(collapsed);
  const [expandedProjects, setExpandedProjects] = useState<
    Record<string, boolean>
  >(() => {
    if (typeof window === "undefined") return {};
    try {
      const stored = window.localStorage.getItem(EXPANDED_PROJECTS_STORAGE_KEY);
      if (!stored) return {};
      const parsed = JSON.parse(stored);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  });

  const chatStore = useChatStore();
  const { sessions } = useChatSessionStore();
  const visibleSessions = getVisibleSessions(
    sessions,
    chatStore.messagesBySession,
  );
  const activeSessions = visibleSessions.filter(
    (session) => !session.archivedAt,
  );

  useEffect(() => {
    if (collapsed) {
      setExpanded(false);
    } else if (prevCollapsed.current && !collapsed) {
      const timer = setTimeout(() => setExpanded(true), 60);
      return () => clearTimeout(timer);
    } else {
      setExpanded(true);
    }
    prevCollapsed.current = collapsed;
  }, [collapsed]);

  const labelTransition = "transition-[opacity,width] duration-300 ease-out";
  const labelVisible = expanded && !collapsed;
  const defaultTitle = t("common:session.defaultTitle");
  const navItems: readonly { id: AppView; label: string; icon: typeof Bot }[] =
    [
      { id: "agents", label: t("navigation.agents"), icon: Bot },
      { id: "skills", label: t("navigation.skills"), icon: BookOpen },
      {
        id: "session-history",
        label: t("navigation.sessionHistory"),
        icon: History,
      },
    ];

  const MAX_RECENTS = 20;
  const validProjectIds = new Set(projects.map((project) => project.id));

  const projectSessions = (() => {
    type SessionItem = {
      id: string;
      title: string;
      sessionId: string;
      projectId?: string;
      updatedAt: string;
      isRunning: boolean;
      hasUnread: boolean;
    };
    const byProject: Record<string, SessionItem[]> = {};
    const standalone: SessionItem[] = [];
    for (const session of visibleSessions) {
      if (session.archivedAt) continue;
      const runtime = chatStore.getSessionRuntime(session.id);
      const item: SessionItem = {
        id: session.id,
        title: session.title,
        sessionId: session.id,
        projectId: session.projectId ?? undefined,
        updatedAt: session.updatedAt,
        isRunning: isSessionRunning(runtime.chatState),
        hasUnread: runtime.hasUnread,
      };
      if (session.projectId && validProjectIds.has(session.projectId)) {
        if (!byProject[session.projectId]) byProject[session.projectId] = [];
        byProject[session.projectId].push(item);
      } else {
        standalone.push(item);
      }
    }
    for (const chats of Object.values(byProject)) {
      chats.sort(
        (a, b) =>
          new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
      );
    }

    standalone.sort(
      (a, b) =>
        new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
    );
    const limitedStandalone = standalone.slice(0, MAX_RECENTS);
    return { byProject, standalone: limitedStandalone };
  })();

  const agentStoreState = useAgentStore();
  const projectStoreState = useProjectStore();

  const sidebarResolvers = {
    getPersonaName: (personaId: string) =>
      agentStoreState.getPersonaById(personaId)?.displayName,
    getProjectName: (projectId: string) =>
      projectStoreState.projects.find((p: { id: string }) => p.id === projectId)
        ?.name,
  };
  const sidebarSearch = useSessionSearch({
    sessions: activeSessions,
    resolvers: sidebarResolvers,
    locale: i18n.resolvedLanguage,
    getDisplayTitle: (session) =>
      getDisplaySessionTitle(session.title, defaultTitle),
  });

  useEffect(() => {
    if (!activeSessionId) return;
    const activeSession = visibleSessions.find((s) => s.id === activeSessionId);
    const projectId = activeSession?.projectId;
    if (projectId) {
      setExpandedProjects((prev) => {
        if (prev[projectId]) return prev;
        return { ...prev, [projectId]: true };
      });
    }
  }, [activeSessionId, visibleSessions]);

  useEffect(() => {
    try {
      window.localStorage.setItem(
        EXPANDED_PROJECTS_STORAGE_KEY,
        JSON.stringify(expandedProjects),
      );
    } catch {
      // localStorage may be unavailable
    }
  }, [expandedProjects]);

  useEffect(() => {
    if (projects.length === 0) return;
    const validProjectIds = new Set(projects.map((project) => project.id));
    setExpandedProjects((prev) => {
      const next = Object.fromEntries(
        Object.entries(prev).filter(([projectId]) =>
          validProjectIds.has(projectId),
        ),
      );
      return Object.keys(next).length === Object.keys(prev).length
        ? prev
        : next;
    });
  }, [projects]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "k" && e.metaKey) {
        e.preventDefault();
        searchInputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const toggleProject = (projectId: string) =>
    setExpandedProjects((prev) => ({ ...prev, [projectId]: !prev[projectId] }));

  const navRef = useRef<HTMLElement>(null);
  const homeRef = useRef<HTMLButtonElement>(null);
  const navItemRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const {
    currentRect,
    isHovering,
    isResizing: isHighlightResizing,
    onItemMouseEnter,
    onNavMouseLeave,
    updateActiveRect,
  } = useSidebarHighlight(navRef);

  const activeProjectId =
    activeSessionId && activeView === "chat"
      ? (sessions.find((s) => s.id === activeSessionId)?.projectId ?? null)
      : null;

  useEffect(() => {
    if (activeSessionId && activeView === "chat") return;
    if (activeView === "home") {
      updateActiveRect(homeRef.current);
    } else if (activeView && navItemRefs.current[activeView]) {
      updateActiveRect(navItemRefs.current[activeView]);
    } else {
      updateActiveRect(null);
    }
  }, [activeSessionId, activeView, updateActiveRect]);

  const activeSessionRefCallback = useCallback(
    (el: HTMLElement | null) => {
      if (activeSessionId && el) updateActiveRect(el);
    },
    [activeSessionId, updateActiveRect],
  );
  const activeProjectRefCallback = useCallback(
    (el: HTMLElement | null) => {
      if (activeProjectId && el) updateActiveRect(el);
    },
    [activeProjectId, updateActiveRect],
  );

  return (
    <div
      className={cn(
        "relative h-full",
        !isResizing && "transition-[width] duration-300 ease-in-out",
        className,
      )}
      style={{ width: collapsed ? 54 : width }}
    >
      <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border bg-background [--muted-foreground:var(--text-subtle)]">
        <div
          className={cn(
            "flex-shrink-0 pt-3",
            collapsed ? "px-1.5 pb-1.5" : "px-3 pb-1",
          )}
        >
          <div
            className={cn(
              "flex items-center",
              collapsed ? "justify-center" : "justify-between",
            )}
          >
            <GooseIcon className="text-foreground" />
            {!collapsed && (
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                onClick={onCollapse}
                className="text-muted-foreground hover:text-foreground"
                aria-label={t("actions.collapse")}
                title={t("actions.collapse")}
              >
                <IconLayoutSidebarFilled className="size-4" />
              </Button>
            )}
          </div>
        </div>

        <nav
          ref={navRef}
          className="relative flex-1 min-h-0 overflow-y-auto overflow-x-hidden px-1.5 py-1 pt-1.5 scrollbar-none"
          onMouseLeave={onNavMouseLeave}
        >
          {currentRect && !collapsed && (
            <div
              className="absolute left-1.5 right-1.5 rounded-lg bg-accent/50 pointer-events-none z-0"
              style={{
                top: currentRect.top,
                height: currentRect.height,
                transition:
                  isHovering || isHighlightResizing
                    ? "top 0ms, height 0ms"
                    : "top 200ms ease, height 200ms ease, opacity 200ms ease",
              }}
            />
          )}

          <div className="relative z-10 space-y-0.5">
            {collapsed && (
              <button
                type="button"
                onClick={onCollapse}
                title={t("actions.expand")}
                className="flex w-full items-center gap-2.5 rounded-md p-3 text-[13px] text-muted-foreground transition-colors duration-200 hover:text-foreground"
                aria-label={t("actions.expand")}
              >
                <IconLayoutSidebar className="size-4 flex-shrink-0" />
                <span className="sr-only">{t("actions.expand")}</span>
              </button>
            )}

            <div
              className={cn(
                "mb-4 flex items-center w-full rounded-md transition-all duration-300 ease-out",
                collapsed
                  ? "justify-center p-3 text-muted-foreground"
                  : "gap-2 border border-border px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-transparent",
              )}
            >
              <Search className="size-3.5 flex-shrink-0 text-placeholder" />
              {!collapsed && (
                <input
                  ref={searchInputRef}
                  type="text"
                  enterKeyHint="search"
                  value={sidebarSearch.query}
                  onChange={(e) => sidebarSearch.setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void sidebarSearch.search();
                    }
                  }}
                  placeholder={t("search.placeholder")}
                  className={cn(
                    "focus-override appearance-none bg-transparent border-none text-xs flex-1 min-w-0 placeholder:text-placeholder outline-none focus-visible:ring-0 focus-visible:ring-offset-0",
                    labelTransition,
                    labelVisible
                      ? "opacity-100 w-auto"
                      : "opacity-0 w-0 overflow-hidden",
                  )}
                  onClick={(e) => e.stopPropagation()}
                />
              )}
            </div>

            <button
              ref={homeRef}
              type="button"
              data-testid="nav-home"
              onClick={() => onNavigate?.("home")}
              onMouseEnter={onItemMouseEnter}
              title={collapsed ? t("navigation.home") : undefined}
              aria-label={t("navigation.home")}
              className={cn(
                "flex items-center w-full text-[13px] transition-colors duration-200 rounded-md",
                "gap-2.5 p-3",
                activeView === "home"
                  ? "font-medium text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              <Home className="size-4 flex-shrink-0" />
              <span
                className={cn(
                  "whitespace-nowrap",
                  labelTransition,
                  labelVisible
                    ? "opacity-100 w-auto"
                    : "opacity-0 w-0 overflow-hidden",
                )}
              >
                {t("navigation.home")}
              </span>
            </button>

            {navItems.map((item, index) => {
              const Icon = item.icon;
              const isActive = activeView === item.id;
              return (
                <button
                  key={item.id}
                  ref={(el) => {
                    navItemRefs.current[item.id] = el;
                  }}
                  type="button"
                  onClick={() => onNavigate?.(item.id)}
                  onMouseEnter={onItemMouseEnter}
                  title={collapsed ? item.label : undefined}
                  className={cn(
                    "flex items-center w-full text-[13px] transition-colors duration-200 rounded-md",
                    "gap-2.5 p-3",
                    isActive
                      ? "font-medium text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  aria-current={isActive ? "page" : undefined}
                  style={{
                    transitionDelay:
                      !collapsed && expanded ? `${index * 30}ms` : "0ms",
                  }}
                >
                  <Icon className="size-4 flex-shrink-0" />
                  <span
                    className={cn(
                      "whitespace-nowrap",
                      labelTransition,
                      labelVisible
                        ? "opacity-100 w-auto"
                        : "opacity-0 w-0 overflow-hidden",
                    )}
                    style={{
                      transitionDelay: labelVisible
                        ? `${index * 30 + 60}ms`
                        : "0ms",
                    }}
                  >
                    {item.label}
                  </span>
                </button>
              );
            })}
          </div>

          {!collapsed &&
            (sidebarSearch.submittedQuery ? (
              <div className="relative z-10 space-y-2">
                {sidebarSearch.error && (
                  <p className="px-1 text-xs text-danger">
                    {t("search.error")}
                  </p>
                )}

                {sidebarSearch.isSearching &&
                  sidebarSearch.results.length === 0 && (
                    <div className="rounded-lg border border-dashed border-border px-3 py-6 text-center text-xs text-muted-foreground">
                      {t("search.searching")}
                    </div>
                  )}

                {(!sidebarSearch.isSearching ||
                  sidebarSearch.results.length > 0) && (
                  <SidebarSearchResults
                    results={sidebarSearch.results}
                    activeSessionId={activeSessionId}
                    onSelectResult={(sessionId, messageId) => {
                      if (messageId) {
                        onSelectSearchResult?.(
                          sessionId,
                          messageId,
                          sidebarSearch.submittedQuery,
                        );
                        return;
                      }
                      onSelectSession?.(sessionId);
                    }}
                    getPersonaName={sidebarResolvers.getPersonaName}
                    getProjectName={sidebarResolvers.getProjectName}
                  />
                )}
              </div>
            ) : (
              <SidebarProjectsSection
                projects={projects}
                projectSessions={projectSessions}
                expandedProjects={expandedProjects}
                toggleProject={toggleProject}
                collapsed={collapsed}
                labelTransition={labelTransition}
                labelVisible={labelVisible}
                activeSessionId={activeSessionId}
                activeProjectId={activeProjectId}
                onNavigate={onNavigate}
                onSelectSession={onSelectSession}
                onNewChatInProject={onNewChatInProject}
                onNewChat={onNewChat}
                onCreateProject={onCreateProject}
                onEditProject={onEditProject}
                onArchiveProject={onArchiveProject}
                onArchiveChat={onArchiveChat}
                onRenameChat={onRenameChat}
                onMoveToProject={onMoveToProject}
                onReorderProject={onReorderProject}
                onItemMouseEnter={onItemMouseEnter}
                activeSessionRefCallback={activeSessionRefCallback}
                activeProjectRefCallback={activeProjectRefCallback}
              />
            ))}
        </nav>
      </div>
    </div>
  );
}
