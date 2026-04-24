import { useState, useEffect, useRef, useCallback } from 'react';
import { useNavigate, useLocation, useSearchParams } from 'react-router-dom';
import { getSession, listSessions } from '../api';
import { useChatContext } from '../contexts/ChatContext';
import { useConfig } from '../components/ConfigContext';
import { useNavigation } from './useNavigation';
import { startNewSession, resumeSession, shouldShowNewChatTitle } from '../sessions';
import { getInitialWorkingDir } from '../utils/workingDir';
import { AppEvents } from '../constants/events';
import type { Session } from '../api';

const MAX_RECENT_SESSIONS = 5;

interface UseNavigationSessionsOptions {
  onNavigate?: () => void;
  fetchOnMount?: boolean;
}

export function useNavigationSessions(options: UseNavigationSessionsOptions = {}) {
  const { onNavigate, fetchOnMount = false } = options;

  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const chatContext = useChatContext();
  const { extensionsList } = useConfig();
  const setView = useNavigation();

  const [recentSessions, setRecentSessions] = useState<Session[]>([]);
  const sessionsRef = useRef<Session[]>([]);
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
      const response = await listSessions({ throwOnError: false });
      if (response.data) {
        const sorted = [...response.data.sessions]
          .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
          .slice(0, MAX_RECENT_SESSIONS);
        setRecentSessions(sorted);
        sessionsRef.current = response.data.sessions;
      }
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
      setRecentSessions((prev) => {
        if (prev.some((s) => s.id === activeSessionId)) return prev;
        return [response.data as Session, ...prev].slice(0, MAX_RECENT_SESSIONS);
      });
    });
  }, [activeSessionId, recentSessions]);

  useEffect(() => {
    let pollingTimeouts: ReturnType<typeof setTimeout>[] = [];
    let isPolling = false;

    const handleSessionCreated = (event: Event) => {
      const { session } = (event as CustomEvent<{ session?: Session }>).detail || {};
      if (session) {
        setRecentSessions((prev) => {
          if (prev.some((s) => s.id === session.id)) return prev;
          return [session, ...prev].slice(0, MAX_RECENT_SESSIONS);
        });
        sessionsRef.current = [session, ...sessionsRef.current.filter((s) => s.id !== session.id)];
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
          const response = await listSessions({ throwOnError: false });
          if (response.data) {
            const apiSessions = response.data.sessions.slice(0, MAX_RECENT_SESSIONS);
            setRecentSessions((prev) => {
              const emptyLocalSessions = prev.filter(
                (local) =>
                  local.message_count === 0 && !apiSessions.some((api) => api.id === local.id)
              );
              return [...emptyLocalSessions, ...apiSessions].slice(0, MAX_RECENT_SESSIONS);
            });
            sessionsRef.current = response.data.sessions;
          }
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

    window.addEventListener(AppEvents.SESSION_CREATED, handleSessionCreated);
    return () => {
      window.removeEventListener(AppEvents.SESSION_CREATED, handleSessionCreated);
      pollingTimeouts.forEach(clearTimeout);
    };
  }, []);

  useEffect(() => {
    const handleSessionDeleted = (event: Event) => {
      const { sessionId } = (event as CustomEvent<{ sessionId: string }>).detail;

      setRecentSessions((prev) => prev.filter((session) => session.id !== sessionId));
      sessionsRef.current = sessionsRef.current.filter((session) => session.id !== sessionId);

      if (lastSessionIdRef.current === sessionId) {
        lastSessionIdRef.current = null;
      }
    };

    const handleSessionRenamed = (event: Event) => {
      const { sessionId, newName } =
        (event as CustomEvent<{ sessionId: string; newName: string }>).detail;

      setRecentSessions((prev) =>
        prev.map((session) => (session.id === sessionId ? { ...session, name: newName } : session))
      );
      sessionsRef.current = sessionsRef.current.map((session) =>
        session.id === sessionId ? { ...session, name: newName } : session
      );
    };

    window.addEventListener(AppEvents.SESSION_DELETED, handleSessionDeleted);
    window.addEventListener(AppEvents.SESSION_RENAMED, handleSessionRenamed);

    return () => {
      window.removeEventListener(AppEvents.SESSION_DELETED, handleSessionDeleted);
      window.removeEventListener(AppEvents.SESSION_RENAMED, handleSessionRenamed);
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

    // Only reuse the current window's own active session if it is empty.
    // Previously this grabbed the first empty session globally, which caused
    // multiple windows to claim the same empty session after a restart/upgrade.
    const currentActiveSession = activeSessionId
      ? sessionsRef.current.find((s) => s.id === activeSessionId)
      : undefined;
    const canReuseActive = currentActiveSession && shouldShowNewChatTitle(currentActiveSession);

    if (canReuseActive) {
      resumeSession(currentActiveSession, setView);
    } else {
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
