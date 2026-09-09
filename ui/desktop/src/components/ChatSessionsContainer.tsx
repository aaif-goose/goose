import { useEffect, useRef } from 'react';
import { useSearchParams } from 'react-router';
import BaseChat from './BaseChat';
import { ChatType } from '../types/chat';
import { UserInput } from '../types/message';
import { subscribeToAcpRecovery } from '../acp/acpConnection';
import { acpChatSessionController } from '../acp/chatSessionController';

interface ChatSessionsContainerProps {
  setChat: (chat: ChatType) => void;
  activeSessions: Array<{
    sessionId: string;
    initialMessage?: UserInput;
    noAutoSubmit?: boolean;
  }>;
}

/**
 * Container that mounts ALL active chat sessions to keep them alive.
 * Uses CSS to show/hide sessions based on the current URL parameter.
 * This allows multiple sessions to stream simultaneously in the background.
 */
export default function ChatSessionsContainer({
  setChat,
  activeSessions,
}: ChatSessionsContainerProps) {
  const [searchParams] = useSearchParams();
  const currentSessionId = searchParams.get('resumeSessionId') ?? undefined;

  // Build the list of sessions to render
  let sessionsToRender = activeSessions;

  // If we have a currentSessionId that's not in activeSessions, add it (handles page refresh)
  if (currentSessionId && !activeSessions.some((s) => s.sessionId === currentSessionId)) {
    sessionsToRender = [...activeSessions, { sessionId: currentSessionId }];
  }

  const sessionIdsRef = useRef<string[]>([]);
  sessionIdsRef.current = sessionsToRender.map((session) => session.sessionId);

  useEffect(() => {
    return subscribeToAcpRecovery((recovering) => {
      if (recovering) {
        return;
      }
      for (const sessionId of sessionIdsRef.current) {
        void acpChatSessionController.restoreSession(sessionId);
      }
    });
  }, []);

  // Always render active sessions to keep SSE connections alive, even when not on /pair route
  if (!currentSessionId && activeSessions.length === 0) {
    return null;
  }

  return (
    <div className="relative w-full h-full">
      {sessionsToRender.map((session) => {
        const isVisible = session.sessionId === currentSessionId;

        return (
          <div
            key={session.sessionId}
            className={`absolute inset-0 ${isVisible ? 'block' : 'hidden'}`}
            data-session-id={session.sessionId}
          >
            <BaseChat
              setChat={setChat}
              sessionId={session.sessionId}
              initialMessage={session.initialMessage}
              noAutoSubmit={session.noAutoSubmit}
              suppressEmptyState={false}
              isActiveSession={isVisible}
            />
          </div>
        );
      })}
    </div>
  );
}
