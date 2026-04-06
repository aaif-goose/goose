import { AppEvents } from '../constants/events';
/**
 * Hub Component
 *
 * The Hub is the main landing page and entry point for the Goose Desktop application.
 * It serves as the welcome screen where users can start new conversations.
 *
 * Key Responsibilities:
 * - Displays SessionInsights to show session statistics and recent chats
 * - Provides a ChatInput for users to start new conversations
 * - Creates a new session and navigates to Pair with the session ID
 * - Shows loading state while session is being created
 *
 * Navigation Flow:
 * Hub (input submission) → Create Session → Pair (with session ID and initial message)
 */

import { useEffect, useRef, useState } from 'react';
import { SessionInsights } from './sessions/SessionsInsights';
import ChatInput from './ChatInput';
import { ChatState } from '../types/chatState';
import 'react-toastify/dist/ReactToastify.css';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import {
  getExtensionConfigsWithOverrides,
  clearExtensionOverrides,
} from '../store/extensionOverrides';
import { getInitialWorkingDir } from '../utils/workingDir';
import { createSession } from '../sessions';
import LoadingGoose from './LoadingGoose';
import { UserInput } from '../types/message';

export default function Hub({
  setView,
}: {
  setView: (view: View, viewOptions?: ViewOptions) => void;
}) {
  const { extensionsList } = useConfig();
  const [workingDir, setWorkingDir] = useState(getInitialWorkingDir());
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // rAF is more reliable than autoFocus across async render boundaries (Suspense, OnboardingGuard, etc.)
  useEffect(() => {
    const frameId = requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => cancelAnimationFrame(frameId);
  }, []);

  const handleSubmit = async (input: UserInput) => {
    const { msg: userMessage, images } = input;
    if ((images.length > 0 || userMessage.trim()) && !isCreatingSession) {
      const extensionConfigs = getExtensionConfigsWithOverrides(extensionsList);
      clearExtensionOverrides();
      setIsCreatingSession(true);

      try {
        const session = await createSession(workingDir, {
          extensionConfigs,
          allExtensions: extensionConfigs.length > 0 ? undefined : extensionsList,
        });

        window.dispatchEvent(new CustomEvent(AppEvents.SESSION_CREATED));
        window.dispatchEvent(
          new CustomEvent(AppEvents.ADD_ACTIVE_SESSION, {
            detail: { sessionId: session.id, initialMessage: { msg: userMessage, images } },
          })
        );

        setView('pair', {
          disableAnimation: true,
          resumeSessionId: session.id,
          initialMessage: { msg: userMessage, images },
        });
      } catch (error) {
        console.error('Failed to create session:', error);
        setIsCreatingSession(false);
      }
    }
  };

  return (
    <div className="flex flex-col h-full min-h-0 bg-background-secondary">
      <div className="flex-1 flex flex-col min-h-[45vh] overflow-hidden mb-0.5 relative">
        <SessionInsights />
        {isCreatingSession && (
          <div className="absolute bottom-1 left-4 z-20 pointer-events-none">
            <LoadingGoose chatState={ChatState.LoadingConversation} />
          </div>
        )}
      </div>

      <div className="flex-shrink-0 max-h-[50vh] min-h-0 overflow-hidden flex flex-col">
        <ChatInput
          sessionId={null}
          handleSubmit={handleSubmit}
          chatState={isCreatingSession ? ChatState.LoadingConversation : ChatState.Idle}
          onStop={() => {}}
          initialValue=""
          setView={setView}
          totalTokens={0}
          accumulatedInputTokens={0}
          accumulatedOutputTokens={0}
          droppedFiles={[]}
          onFilesProcessed={() => {}}
          messages={[]}
          disableAnimation={false}
          sessionCosts={undefined}
          toolCount={0}
          onWorkingDirChange={setWorkingDir}
          inputRef={inputRef}
        />
      </div>
    </div>
  );
}
