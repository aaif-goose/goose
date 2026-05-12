import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ChatInput } from "@/features/chat/ui/ChatInput";
import { useLocaleFormatting } from "@/shared/i18n";
import { useChatSessionController } from "@/features/chat/hooks/useChatSessionController";

function HomeClock() {
  const [time, setTime] = useState(new Date());
  const { getTimeParts } = useLocaleFormatting();

  useEffect(() => {
    const interval = setInterval(() => setTime(new Date()), 1000);
    return () => clearInterval(interval);
  }, []);

  const { hour, minute, dayPeriod } = getTimeParts(time, {
    hour: "numeric",
    minute: "2-digit",
  });

  return (
    <div className="mb-1 flex items-baseline gap-1.5 pl-4">
      <span className="text-6xl font-normal font-display tracking-tight text-foreground">
        {hour}:{minute}
      </span>
      {dayPeriod ? (
        <span className="text-lg font-normal text-muted-foreground">
          {dayPeriod}
        </span>
      ) : null}
    </div>
  );
}

function getGreetingKey(hour: number): "morning" | "afternoon" | "evening" {
  if (hour < 12) return "morning";
  if (hour < 17) return "afternoon";
  return "evening";
}

interface HomeScreenProps {
  sessionId: string | null;
  onActivateSession: (sessionId: string) => void;
  onCreatePersona?: () => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
}

function HomeComposer({
  sessionId,
  onActivateSession,
  onCreatePersona,
  onCreateProject,
}: {
  sessionId: string | null;
  onActivateSession: (sessionId: string) => void;
  onCreatePersona?: () => void;
  onCreateProject?: HomeScreenProps["onCreateProject"];
}) {
  const controller = useChatSessionController({
    sessionId,
    onMessageAccepted: onActivateSession,
    onCreatePersonaRequested: onCreatePersona,
  });

  return (
    <ChatInput
      composerActions={{
        onSend: controller.handleSend,
        disabled: controller.projectMetadataPending,
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
      }}
    />
  );
}

export function HomeScreen({
  sessionId,
  onActivateSession,
  onCreatePersona,
  onCreateProject,
}: HomeScreenProps) {
  const { t } = useTranslation("home");
  const [hour] = useState(() => new Date().getHours());
  const greeting = t(`greeting.${getGreetingKey(hour)}`);

  return (
    <div className="h-full w-full overflow-y-auto">
      <div className="relative flex min-h-full flex-col items-center justify-center px-6 pb-4">
        <div className="flex w-full max-w-[600px] flex-col antialiased">
          <HomeClock />

          <p className="mb-6 pl-4 text-xl font-normal font-display text-muted-foreground">
            {greeting}
          </p>

          <HomeComposer
            sessionId={sessionId}
            onActivateSession={onActivateSession}
            onCreatePersona={onCreatePersona}
            onCreateProject={onCreateProject}
          />
        </div>
      </div>
    </div>
  );
}
