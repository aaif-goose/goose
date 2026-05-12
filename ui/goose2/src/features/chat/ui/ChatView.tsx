import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AnimatePresence } from "motion/react";
import { MessageTimeline } from "./MessageTimeline";
import { ChatInput } from "./ChatInput";
import { LoadingGoose } from "./LoadingGoose";
import { ChatLoadingSkeleton } from "./ChatLoadingSkeleton";
import { useChatSessionStore } from "../stores/chatSessionStore";
import { ArtifactPolicyProvider } from "../hooks/ArtifactPolicyContext";
import { ChatContextPanel } from "./ChatContextPanel";
import { perfLog } from "@/shared/lib/perfLog";
import { useChatSessionController } from "../hooks/useChatSessionController";
import type { Message } from "@/shared/types/messages";

interface ChatViewProps {
  sessionId: string;
  onCreatePersona?: () => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
}

function shouldOverlapComposerWithLatestMcpApp(messages: Message[]): boolean {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (
      message.metadata?.userVisible === false ||
      (message.role === "assistant" &&
        message.content.length === 0 &&
        message.metadata?.completionStatus === "inProgress")
    ) {
      continue;
    }

    return (
      message.role === "assistant" && message.content.at(-1)?.type === "mcpApp"
    );
  }

  return false;
}

export function ChatView({
  sessionId,
  onCreatePersona,
  onCreateProject,
}: ChatViewProps) {
  const { t } = useTranslation("chat");
  const mountStart = useRef(performance.now());
  const isContextPanelOpen = useChatSessionStore(
    (s) => s.contextPanelOpenBySession[sessionId] ?? false,
  );
  const setContextPanelOpen = useChatSessionStore((s) => s.setContextPanelOpen);
  const [isLoadingIndicatorMounted, setIsLoadingIndicatorMounted] =
    useState(false);
  const controller = useChatSessionController({
    sessionId,
    onCreatePersonaRequested: onCreatePersona,
  });
  const contextPanelLabel = isContextPanelOpen
    ? t("context.closePanel")
    : t("context.openPanel");

  useEffect(() => {
    const ms = (performance.now() - mountStart.current).toFixed(1);
    perfLog(`[perf:chatview] ${sessionId.slice(0, 8)} mounted in ${ms}ms`);
  }, [sessionId]);

  const showIndicator =
    controller.chatState === "thinking" ||
    controller.chatState === "streaming" ||
    controller.chatState === "waiting" ||
    controller.chatState === "compacting";
  const shouldShowLoadingIndicator =
    showIndicator && !controller.isLoadingHistory;
  const shouldReserveComposerGap =
    shouldShowLoadingIndicator || isLoadingIndicatorMounted;
  const shouldOverlapComposer =
    !shouldReserveComposerGap &&
    shouldOverlapComposerWithLatestMcpApp(controller.messages);

  useEffect(() => {
    if (shouldShowLoadingIndicator) {
      setIsLoadingIndicatorMounted(true);
    }
  }, [shouldShowLoadingIndicator]);

  return (
    <ArtifactPolicyProvider
      messages={controller.messages}
      sessionCwd={controller.sessionArtifactCwd}
    >
      <div className="relative flex h-full min-w-0">
        <div className="flex min-w-0 flex-1 flex-col pr-1">
          {controller.isLoadingHistory ? (
            <ChatLoadingSkeleton />
          ) : (
            <MessageTimeline
              messages={controller.messages}
              streamingMessageId={controller.streamingMessageId}
              scrollTargetMessageId={controller.scrollTarget?.messageId ?? null}
              scrollTargetQuery={controller.scrollTarget?.query ?? null}
              onScrollTargetHandled={controller.handleScrollTargetHandled}
              onSendMcpAppMessage={controller.handleSend}
            />
          )}

          <AnimatePresence
            initial={false}
            onExitComplete={() => setIsLoadingIndicatorMounted(false)}
          >
            {shouldShowLoadingIndicator ? (
              <LoadingGoose
                key="loading-indicator"
                chatState={
                  controller.chatState as
                    | "thinking"
                    | "streaming"
                    | "waiting"
                    | "compacting"
                }
              />
            ) : null}
          </AnimatePresence>

          <ChatInput
            className={shouldOverlapComposer ? "-mt-4" : undefined}
            composerActions={{
              onSend: controller.handleSend,
              disabled:
                controller.projectMetadataPending ||
                controller.isCompactingContext,
              queuedMessage: controller.queue.queuedMessage,
              onDismissQueue: controller.queue.dismiss,
              onStop: controller.stopStreaming,
              isStreaming:
                controller.chatState === "streaming" ||
                controller.chatState === "thinking",
            }}
            initialValue={controller.draftValue}
            onDraftChange={controller.handleDraftChange}
            selectedSkills={controller.selectedSkills}
            onSkillsChange={controller.handleSkillsChange}
            personaPicker={{
              personas: controller.personas,
              selectedPersonaId: controller.selectedPersonaId,
              onPersonaChange: controller.handlePersonaChange,
            }}
            agentModelPicker={{
              providers: controller.pickerAgents,
              providersLoading: controller.providersLoading,
              selectedProvider: controller.selectedProvider,
              onProviderChange: controller.handleProviderChange,
              currentModelId: controller.currentModelId,
              currentModelProviderId: controller.currentModelProviderId,
              currentModel: controller.currentModelName ?? undefined,
              availableModels: controller.availableModels,
              modelsLoading: controller.modelsLoading,
              modelStatusMessage: controller.modelStatusMessage,
              onModelChange: controller.handleModelChange,
              onPickerOpen: controller.handlePickerOpen,
            }}
            projectPicker={{
              selectedProjectId: controller.selectedProjectId,
              availableProjects: controller.availableProjects,
              onProjectChange: controller.handleProjectChange,
              onCreateProject: (options) =>
                onCreateProject?.({
                  onCreated: (projectId) => {
                    controller.handleProjectChange(projectId);
                    options?.onCreated?.(projectId);
                  },
                }),
            }}
            contextUsage={{
              contextTokens: controller.tokenState.accumulatedTotal,
              contextLimit: controller.tokenState.contextLimit,
              isContextUsageReady: controller.isContextUsageReady,
              onCompactContext: controller.compactConversation,
              canCompactContext: controller.canCompactContext,
              isCompactingContext: controller.isCompactingContext,
              supportsCompactionControls: controller.supportsCompactionControls,
            }}
          />
        </div>

        <ChatContextPanel
          activeSessionId={sessionId}
          isOpen={isContextPanelOpen}
          label={contextPanelLabel}
          project={controller.project}
          setOpen={setContextPanelOpen}
        />
      </div>
    </ArtifactPolicyProvider>
  );
}
