import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  getStoredProvider,
  useAgentStore,
} from "@/features/agents/stores/agentStore";
import { useProviderSelection } from "@/features/agents/hooks/useProviderSelection";
import { ChatInput } from "@/features/chat/ui/ChatInput";
import { useChatStore } from "@/features/chat/stores/chatStore";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { useLocaleFormatting } from "@/shared/i18n";

const HOME_DRAFT_KEY = "home";

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
  onStartChat?: (
    initialMessage?: string,
    providerId?: string,
    personaId?: string,
    projectId?: string | null,
    attachments?: ChatAttachmentDraft[],
  ) => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
}

export function HomeScreen({ onStartChat, onCreateProject }: HomeScreenProps) {
  const { t } = useTranslation("home");
  const [hour] = useState(() => new Date().getHours());
  const greeting = t(`greeting.${getGreetingKey(hour)}`);

  const personas = useAgentStore((s) => s.personas);
  const {
    providers,
    providersLoading,
    selectedProvider,
    setSelectedProvider,
    setSelectedProviderWithoutPersist,
  } = useProviderSelection();
  const projects = useProjectStore((s) => s.projects);
  const [selectedPersonaId, setSelectedPersonaId] = useState<string | null>(
    null,
  );
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(
    null,
  );

  const handlePersonaChange = useCallback(
    (personaId: string | null) => {
      setSelectedPersonaId(personaId);
      const persona = personaId
        ? personas.find((candidate) => candidate.id === personaId)
        : null;
      const nextProvider = persona?.provider ?? getStoredProvider(providers);

      setSelectedProviderWithoutPersist(nextProvider);
    },
    [personas, providers, setSelectedProviderWithoutPersist],
  );

  const handleCreatePersona = useCallback(() => {
    useAgentStore.getState().openPersonaEditor();
  }, []);

  const homeDraft = useChatStore(
    (s) => s.draftsBySession[HOME_DRAFT_KEY] ?? "",
  );
  const handleDraftChange = useCallback((text: string) => {
    useChatStore.getState().setDraft(HOME_DRAFT_KEY, text);
  }, []);

  const handleSend = useCallback(
    (
      message: string,
      personaId?: string,
      attachments?: ChatAttachmentDraft[],
    ) => {
      const effectivePersonaId = personaId ?? selectedPersonaId ?? undefined;

      useChatStore.getState().clearDraft(HOME_DRAFT_KEY);
      onStartChat?.(
        message,
        selectedProvider,
        effectivePersonaId,
        selectedProjectId,
        attachments,
      );
    },
    [onStartChat, selectedPersonaId, selectedProjectId, selectedProvider],
  );

  return (
    <div className="h-full w-full overflow-y-auto">
      <div className="relative flex min-h-full flex-col items-center justify-center px-6 pb-4">
        <div className="flex w-full max-w-[600px] flex-col antialiased">
          {/* Clock */}
          <HomeClock />

          {/* Greeting */}
          <p className="mb-6 pl-4 text-xl font-normal font-display text-muted-foreground">
            {greeting}
          </p>

          {/* Chat input */}
          <ChatInput
            onSend={handleSend}
            initialValue={homeDraft}
            onDraftChange={handleDraftChange}
            personas={personas}
            selectedPersonaId={selectedPersonaId}
            onPersonaChange={handlePersonaChange}
            onCreatePersona={handleCreatePersona}
            providers={providers}
            providersLoading={providersLoading}
            selectedProvider={selectedProvider}
            onProviderChange={setSelectedProvider}
            selectedProjectId={selectedProjectId}
            availableProjects={projects.map((project) => ({
              id: project.id,
              name: project.name,
              workingDirs: project.workingDirs,
              color: project.color,
            }))}
            onProjectChange={setSelectedProjectId}
            onCreateProject={(options) =>
              onCreateProject?.({
                onCreated: (projectId) => {
                  setSelectedProjectId(projectId);
                  options?.onCreated?.(projectId);
                },
              })
            }
          />
        </div>
      </div>
    </div>
  );
}
