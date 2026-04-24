import { useEffect } from "react";
import {
  hasSessionStarted,
  type ChatSession,
} from "@/features/chat/stores/chatSessionStore";
import { persistHomeSessionId } from "../lib/homeSessionStorage";

interface UseHomeSessionStateSyncOptions {
  homeSessionId: string | null;
  homeSession?: ChatSession;
  messagesBySession: Record<string, ArrayLike<unknown> | undefined>;
  hasHydratedSessions: boolean;
  isLoading: boolean;
  setHomeSessionId: (sessionId: string | null) => void;
}

export function useHomeSessionStateSync({
  homeSessionId,
  homeSession,
  messagesBySession,
  hasHydratedSessions,
  isLoading,
  setHomeSessionId,
}: UseHomeSessionStateSyncOptions): void {
  useEffect(() => {
    if (!homeSessionId || !hasHydratedSessions || isLoading) {
      return;
    }

    if (
      !homeSession ||
      homeSession.archivedAt ||
      hasSessionStarted(homeSession, messagesBySession[homeSession.id])
    ) {
      setHomeSessionId(null);
    }
  }, [
    hasHydratedSessions,
    homeSession,
    homeSession?.archivedAt,
    homeSession?.messageCount,
    homeSessionId,
    isLoading,
    messagesBySession,
    setHomeSessionId,
  ]);

  useEffect(() => {
    persistHomeSessionId(homeSessionId);
  }, [homeSessionId]);
}
