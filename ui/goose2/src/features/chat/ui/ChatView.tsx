import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { AnimatePresence } from "motion/react";
import { MessageTimeline } from "./MessageTimeline";
import { ChatInput } from "./ChatInput";
import { LoadingGoose } from "./LoadingGoose";
import { ChatLoadingSkeleton } from "./ChatLoadingSkeleton";
import { useChatSessionStore } from "../stores/chatSessionStore";
import { defaultGlobalArtifactRoot } from "@/features/projects/lib/chatProjectContext";
import { ArtifactPolicyProvider } from "../hooks/ArtifactPolicyContext";
import { ChatContextPanel } from "./ChatContextPanel";
import { perfLog } from "@/shared/lib/perfLog";
import { useChatSessionController } from "../hooks/useChatSessionController";

interface ChatViewProps {
  sessionId: string;
  onCreatePersona?: () => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
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
  const [globalArtifactRoot, setGlobalArtifactRoot] = useState<string | null>(
    null,
  );
  const controller = useChatSessionController({
    sessionId,
    onCreatePersonaRequested: onCreatePersona,
  });
  const contextPanelLabel = isContextPanelOpen
    ? t("context.closePanel")
    : t("context.openPanel");
  const allowedArtifactRoots = [
    ...controller.allowedArtifactRoots,
    ...(globalArtifactRoot ? [globalArtifactRoot] : []),
  ];

  useEffect(() => {
    const ms = (performance.now() - mountStart.current).toFixed(1);
    perfLog(`[perf:chatview] ${sessionId.slice(0, 8)} mounted in ${ms}ms`);
  }, [sessionId]);

  useEffect(() => {
    let cancelled = false;
    defaultGlobalArtifactRoot()
      .then((artifactRoot) => {
        if (!cancelled) {
          setGlobalArtifactRoot(artifactRoot);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setGlobalArtifactRoot(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const showIndicator =
    controller.chatState === "thinking" ||
    controller.chatState === "streaming" ||
    controller.chatState === "waiting" ||
    controller.chatState === "compacting";

  return (
    <ArtifactPolicyProvider
      messages={controller.messages}
      allowedRoots={allowedArtifactRoots}
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
            />
          )}

          <AnimatePresence initial={false}>
            {showIndicator && !controller.isLoadingHistory ? (
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
            onSend={controller.handleSend}
            disabled={
              controller.projectMetadataPending ||
              controller.isCompactingContext
            }
            queuedMessage={controller.queue.queuedMessage}
            onDismissQueue={controller.queue.dismiss}
            initialValue={controller.draftValue}
            onDraftChange={controller.handleDraftChange}
            onStop={controller.stopStreaming}
            isStreaming={
              controller.chatState === "streaming" ||
              controller.chatState === "thinking"
            }
            personas={controller.personas}
            selectedPersonaId={controller.selectedPersonaId}
            onPersonaChange={controller.handlePersonaChange}
            onCreatePersona={controller.handleCreatePersona}
            providers={controller.pickerAgents}
            providersLoading={controller.providersLoading}
            selectedProvider={controller.selectedProvider}
            onProviderChange={controller.handleProviderChange}
            currentModelId={controller.currentModelId}
            currentModel={controller.currentModelName ?? undefined}
            availableModels={controller.availableModels}
            modelsLoading={controller.modelsLoading}
            modelStatusMessage={controller.modelStatusMessage}
            onModelChange={controller.handleModelChange}
            selectedProjectId={controller.selectedProjectId}
            availableProjects={controller.availableProjects}
            onProjectChange={controller.handleProjectChange}
            onCreateProject={(options) =>
              onCreateProject?.({
                onCreated: (projectId) => {
                  controller.handleProjectChange(projectId);
                  options?.onCreated?.(projectId);
                },
              })
            }
            contextTokens={controller.tokenState.accumulatedTotal}
            contextLimit={controller.tokenState.contextLimit}
            isContextUsageReady={controller.isContextUsageReady}
            onCompactContext={controller.compactConversation}
            canCompactContext={controller.canCompactContext}
            isCompactingContext={controller.isCompactingContext}
            supportsCompactionControls={controller.supportsCompactionControls}
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
