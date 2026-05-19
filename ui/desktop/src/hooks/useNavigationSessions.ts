import { useState, useEffect, useRef, useCallback } from 'react';
import { useNavigate, useLocation, useSearchParams } from 'react-router-dom';
import { getSession } from '../api';
import { useChatContext } from '../contexts/ChatContext';
import { useConfig } from '../components/ConfigContext';
import { useNavigation } from './useNavigation';
import { startNewSession, resumeSession, shouldShowNewChatTitle } from '../sessions';
import { getInitialWorkingDir } from '../utils/workingDir';
import { AppEvents } from '../constants/events';
import type { Session } from '../api';
import {
  listAcpSessions,
  sessionInfoToListItem,
  type SessionListItem,
} from '../acp/sessions';
import { DEFAULT_CHAT_TITLE } from '../contexts/ChatContext';

const MAX_RECENT_SESSIONS = 5;

interface UseNavigationSessionsOptions {
  onNavigate?: () => void;
  fetchOnMount?: boolean;
}

// Preserves locally-tracked empty sessions that the API hasn't returned yet
// (newly created sessions are absent from listSessions until they get a message).
function mergeWithEmptyLocals(
  prev: SessionListItem[],
  apiSessions: SessionListItem[]
): SessionListItem[] {
  const emptyLocals = prev.filter(
    (local) => local.messageCount === 0 && !apiSessions.some((api) => api.id === local.id)
  );
  return [...emptyLocals, ...apiSessions].slice(0, MAX_RECENT_SESSIONS);
}

function sortAndTrim(sessions: SessionListItem[]): SessionListItem[] {
  return [...sessions]
    .sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime())
    .slice(0, MAX_RECENT_SESSIONS);
}

function sessionToListItem(s: Session): SessionListItem {
  return {
    id: s.id,
    name: s.name || DEFAULT_CHAT_TITLE,
    workingDir: s.working_dir,
    updatedAt: s.updated_at,
    messageCount: s.message_count,
    createdAt: s.created_at,
    archivedAt: s.archived_at ?? undefined,
    projectId: s.project_id ?? undefined,
    providerId: s.provider_name ?? undefined,
    modelId: s.model_config?.model_name ?? undefined,
    userSetName: s.user_set_name ?? undefined,
    hasRecipe: !!s.recipe,
  };
}

export function useNavigationSessions(options: UseNavigationSessionsOptions = {}) {
  const { onNavigate, fetchOnMount = false } = options;

  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
  const { extensionsList } = useConfig();
  const setView = useNavigation();

  const [recentSessions, setRecentSessions] = useState<SessionListItem[]>([]);
  const sessionsRef = useRef<SessionListItem[]>([]);
  const lastSessionIdRef = useRef<string | null>(null);
  const isCreatingSessionRef = useRef(false);

  const activeSessionId = searchParams.get('resumeSessionId') ?? undefined;
  const currentSessionId =
    location.pathname === '/pair' ? searchParams.get('resumeSessionId') : null;

  useEffect(() => {
    sessionsRef.current = recentSessions;
  }, [recentSessions]);

  useEffect(() => {
    if (currentSessionId) {
      lastSessionIdRef.current = currentSessionId;
    }
  }, [currentSessionId]);

  const fetchSessions = useCallback(async () => {
    try {
      const response = await listAcpSessions();
      setRecentSessions(sortAndTrim(response.sessions.map(sessionInfoToListItem)));
    } catch (error) {
      console.error('Failed to fetch sessions:', error);
    }
  }, []);

  useEffect(() => {
    if (fetchOnMount) {
      fetchSessions();
    }
  }, [fetchOnMount, fetchSessions]);

  useEffect(() => {
    if (!activeSessionId) return;
    if (recentSessions.some((s) => s.id === activeSessionId)) return;

    getSession({ path: { session_id: activeSessionId }, throwOnError: false }).then((response) => {
      if (!response.data) return;
      const item = sessionToListItem(response.data as Session);
      setRecentSessions((prev) => {
        if (prev.some((s) => s.id === activeSessionId)) return prev;
        return [item, ...prev].slice(0, MAX_RECENT_SESSIONS);
      });
    });
  }, [activeSessionId, recentSessions]);

  useEffect(() => {
    let pollingTimeouts: ReturnType<typeof setTimeout>[] = [];
    let isPolling = false;

    const handleSessionCreated = (event: Event) => {
      const { session } = (event as CustomEvent<{ session?: Session }>).detail || {};
      if (session) {
        const item = sessionToListItem(session);
        setRecentSessions((prev) => {
          if (prev.some((s) => s.id === item.id)) return prev;
          return [item, ...prev].slice(0, MAX_RECENT_SESSIONS);
        });
      }

      if (isPolling) return;
      isPolling = true;

      const pollIntervalMs = 300;
      const maxPollDurationMs = 10000;
      const maxPolls = maxPollDurationMs / pollIntervalMs;
      let pollCount = 0;

      const pollForUpdates = async () => {
        pollCount++;
        try {
          const response = await listAcpSessions();
          const apiSessions = response.sessions
            .slice(0, MAX_RECENT_SESSIONS)
            .map(sessionInfoToListItem);
          setRecentSessions((prev) => mergeWithEmptyLocals(prev, apiSessions));
        } catch (error) {
          console.error('Failed to poll sessions:', error);
        }

        if (pollCount < maxPolls) {
          const timeout = setTimeout(pollForUpdates, pollIntervalMs);
          pollingTimeouts.push(timeout);
        } else {
          isPolling = false;
        }
      };

      pollForUpdates();
    };

    const handleSessionDeleted = (event: Event) => {
      const { sessionId } = (event as CustomEvent<{ sessionId: string }>).detail;
      setRecentSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (lastSessionIdRef.current === sessionId) {
        lastSessionIdRef.current = null;
      }
    };

    const handleSessionRenamed = (event: Event) => {
      const { sessionId, newName } = (event as CustomEvent<{ sessionId: string; newName: string }>)
        .detail;
      setRecentSessions((prev) =>
        prev.map((session) => (session.id === sessionId ? { ...session, name: newName } : session))
      );
    };

    window.addEventListener(AppEvents.SESSION_CREATED, handleSessionCreated);
    window.addEventListener(AppEvents.SESSION_DELETED, handleSessionDeleted);
    window.addEventListener(AppEvents.SESSION_RENAMED, handleSessionRenamed);

    return () => {
      window.removeEventListener(AppEvents.SESSION_CREATED, handleSessionCreated);
      window.removeEventListener(AppEvents.SESSION_DELETED, handleSessionDeleted);
      window.removeEventListener(AppEvents.SESSION_RENAMED, handleSessionRenamed);
      pollingTimeouts.forEach(clearTimeout);
    };
  }, []);

  const handleNavClick = useCallback(
    (path: string) => {
      if (path === '/pair') {
        const sessionId =
          currentSessionId || lastSessionIdRef.current || chatContext?.chat?.sessionId;
        if (sessionId && sessionId.length > 0) {
          navigate(`/pair?resumeSessionId=${sessionId}`);
        } else {
          navigate('/');
        }
      } else {
        navigate(path);
      }
      onNavigate?.();
    },
    [navigate, currentSessionId, chatContext?.chat?.sessionId, onNavigate]
  );

  const handleNewChat = useCallback(async () => {
    if (isCreatingSessionRef.current) return;

    // Empty placeholder sessions are filtered out of listAcpSessions, so the
    // active one isn't in sessionsRef. Fetch it directly to check reusability.
    if (activeSessionId) {
      const resp = await getSession({
        path: { session_id: activeSessionId },
        throwOnError: false,
      });
      const active = resp.data;
      if (active && shouldShowNewChatTitle(active)) {
        resumeSession(active, setView);
        onNavigate?.();
        return;
      }
    }

    isCreatingSessionRef.current = true;
    try {
      await startNewSession('', setView, getInitialWorkingDir(), {
        allExtensions: extensionsList,
      });
    } finally {
      setTimeout(() => {
        isCreatingSessionRef.current = false;
      }, 1000);
    }
    onNavigate?.();
  }, [setView, onNavigate, extensionsList, activeSessionId]);

  const handleSessionClick = useCallback(
    (sessionId: string) => {
      navigate(`/pair?resumeSessionId=${sessionId}`);
      onNavigate?.();
    },
    [navigate, onNavigate]
  );

  return {
    recentSessions,
    activeSessionId,
    currentSessionId,
    fetchSessions,
    handleNavClick,
    handleNewChat,
    handleSessionClick,
  };
}

export function getSessionDisplayName(session: Session): string {
  if (session.user_set_name) {
    return session.name;
  }
  if (session.recipe?.title) {
    return session.recipe.title;
  }
  if (shouldShowNewChatTitle(session)) {
    return 'New Chat';
  }
  return session.name;
}

export function truncateMessage(msg?: string, maxLen = 20): string {
  if (!msg) return 'New Chat';
  return msg.length > maxLen ? msg.substring(0, maxLen) + '...' : msg;
}
