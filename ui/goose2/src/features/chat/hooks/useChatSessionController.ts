import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import { useChat } from "./useChat";
import { useMessageQueue } from "./useMessageQueue";
import { useChatStore } from "../stores/chatStore";
import { useChatSessionStore } from "../stores/chatSessionStore";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProviderSelection } from "@/features/agents/hooks/useProviderSelection";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { resolveAgentProviderCatalogIdStrict } from "@/features/providers/providerCatalog";
import {
  buildProjectSystemPrompt,
  composeSystemPrompt,
  getProjectArtifactRoots,
  resolveProjectDefaultArtifactRoot,
} from "@/features/projects/lib/chatProjectContext";
import { setStoredModelPreference } from "../lib/modelPreferences";
import { resolveSessionCwd } from "@/features/projects/lib/sessionCwdSelection";
import { acpPrepareSession, acpSetModel } from "@/shared/api/acp";
import {
  useResolvedAgentModelPicker,
  type PreferredModelSelection,
} from "./useResolvedAgentModelPicker";

interface UseChatSessionControllerOptions {
  sessionId: string | null;
  onMessageAccepted?: (sessionId: string) => void;
}

const PENDING_HOME_SESSION_ID = "__home_pending__";

export function useChatSessionController({
  sessionId,
  onMessageAccepted,
}: UseChatSessionControllerOptions) {
  const stateSessionId = sessionId ?? PENDING_HOME_SESSION_ID;
  const {
    providers,
    providersLoading,
    selectedProvider: globalSelectedProvider,
    setSelectedProvider: setGlobalSelectedProvider,
  } = useProviderSelection();
  const personas = useAgentStore((s) => s.personas);
  const session = useChatSessionStore((s) =>
    sessionId
      ? s.sessions.find((candidate) => candidate.id === sessionId)
      : undefined,
  );
  const activeWorkspace = useChatSessionStore((s) =>
    sessionId ? s.activeWorkspaceBySession[sessionId] : undefined,
  );
  const clearActiveWorkspace = useChatSessionStore(
    (s) => s.clearActiveWorkspace,
  );
  const projects = useProjectStore((s) => s.projects);
  const projectsLoading = useProjectStore((s) => s.loading);
  const [pendingPersonaId, setPendingPersonaId] = useState<string | null>();
  const [pendingProjectId, setPendingProjectId] = useState<string | null>();
  const [pendingProviderId, setPendingProviderId] = useState<string>();
  const [pendingModelSelection, setPendingModelSelection] =
    useState<PreferredModelSelection | null>();
  const pendingDraftValue = useChatStore(
    (s) => s.draftsBySession[PENDING_HOME_SESSION_ID] ?? "",
  );
  const pendingQueuedMessage = useChatStore(
    (s) => s.queuedMessageBySession[PENDING_HOME_SESSION_ID] ?? null,
  );
  const effectiveProjectId =
    pendingProjectId !== undefined
      ? pendingProjectId
      : (session?.projectId ?? null);
  const storedProject = useProjectStore((s) =>
    effectiveProjectId
      ? s.projects.find((candidate) => candidate.id === effectiveProjectId)
      : undefined,
  );
  const project = storedProject ?? null;
  const selectedProvider =
    pendingProviderId ??
    session?.providerId ??
    project?.preferredProvider ??
    globalSelectedProvider;
  const selectedPersonaId =
    pendingPersonaId !== undefined
      ? pendingPersonaId
      : (session?.personaId ?? null);
  const selectedPersona = personas.find(
    (persona) => persona.id === selectedPersonaId,
  );
  const projectArtifactRoots = useMemo(
    () => getProjectArtifactRoots(project),
    [project],
  );
  const projectDefaultArtifactRoot = useMemo(
    () => resolveProjectDefaultArtifactRoot(project),
    [project],
  );
  const projectMetadataPending = Boolean(
    effectiveProjectId && !projectDefaultArtifactRoot && projectsLoading,
  );
  const allowedArtifactRoots = useMemo(
    () => [
      ...new Set(
        projectArtifactRoots.map((path) => path.trim()).filter(Boolean),
      ),
    ],
    [projectArtifactRoots],
  );
  const availableProjects = useMemo(
    () =>
      [...projects]
        .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name))
        .map((projectInfo) => ({
          id: projectInfo.id,
          name: projectInfo.name,
          workingDirs: projectInfo.workingDirs,
          color: projectInfo.color,
        })),
    [projects],
  );
  const projectSystemPrompt = useMemo(
    () => buildProjectSystemPrompt(project),
    [project],
  );
  const workingContextPrompt = useMemo(() => {
    if (!activeWorkspace?.branch) return undefined;
    return `<active-working-context>\nActive branch: ${activeWorkspace.branch}\nWorking directory: ${activeWorkspace.path}\n</active-working-context>`;
  }, [activeWorkspace?.branch, activeWorkspace?.path]);
  const effectiveSystemPrompt = useMemo(
    () =>
      composeSystemPrompt(
        selectedPersona?.systemPrompt,
        projectSystemPrompt,
        workingContextPrompt,
      ),
    [projectSystemPrompt, selectedPersona?.systemPrompt, workingContextPrompt],
  );

  const prepareCurrentSession = useCallback(
    async (
      providerId: string,
      nextProject = project,
      nextWorkspacePath = activeWorkspace?.path,
      personaId = selectedPersonaId ?? undefined,
      modelSelection?: PreferredModelSelection | null,
    ) => {
      if (!sessionId) {
        return;
      }
      const workingDir = await resolveSessionCwd(
        nextProject,
        nextWorkspacePath,
      );
      await acpPrepareSession(sessionId, providerId, workingDir, { personaId });
      if (!modelSelection?.id) {
        return;
      }

      const sessionStore = useChatSessionStore.getState();
      const liveSession = sessionStore.getSession(sessionId);
      const modelAlreadyApplied =
        liveSession?.modelId === modelSelection.id &&
        liveSession?.modelName === modelSelection.name;

      if (modelAlreadyApplied) {
        return;
      }

      await acpSetModel(sessionId, modelSelection.id);
      sessionStore.updateSession(sessionId, {
        modelId: modelSelection.id,
        modelName: modelSelection.name,
      });
    },
    [activeWorkspace?.path, project, selectedPersonaId, sessionId],
  );
  const prepareSelectedProvider = useCallback(
    (providerId: string, modelSelection?: PreferredModelSelection | null) =>
      prepareCurrentSession(
        providerId,
        project,
        activeWorkspace?.path,
        selectedPersonaId ?? undefined,
        modelSelection,
      ),
    [activeWorkspace?.path, prepareCurrentSession, project, selectedPersonaId],
  );

  const prevProjectIdRef = useRef(session?.projectId);
  useEffect(() => {
    if (!sessionId) {
      return;
    }
    const previousProjectId = prevProjectIdRef.current;
    prevProjectIdRef.current = session?.projectId;
    if (
      previousProjectId !== undefined &&
      previousProjectId !== session?.projectId
    ) {
      clearActiveWorkspace(sessionId);
    }
  }, [clearActiveWorkspace, session?.projectId, sessionId]);

  const {
    selectedAgentId,
    pickerAgents,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleProviderChange,
    handleModelChange,
    effectiveModelSelection,
  } = useResolvedAgentModelPicker({
    providers,
    selectedProvider,
    sessionId,
    session,
    pendingModelSelection,
    setPendingProviderId,
    setPendingModelSelection,
    setGlobalSelectedProvider,
    prepareSelectedProvider,
  });

  const prevWorkspaceRef = useRef(activeWorkspace);
  useEffect(() => {
    const previousWorkspace = prevWorkspaceRef.current;
    if (
      !sessionId ||
      !activeWorkspace ||
      !selectedProvider ||
      activeWorkspace === previousWorkspace
    ) {
      return;
    }
    prevWorkspaceRef.current = activeWorkspace;
    if (previousWorkspace?.path === activeWorkspace.path) {
      return;
    }
    void prepareSelectedProvider(
      selectedProvider,
      effectiveModelSelection,
    ).catch((error) => {
      console.error("Failed to prepare ACP session:", error);
    });
  }, [
    activeWorkspace,
    effectiveModelSelection,
    prepareSelectedProvider,
    selectedProvider,
    sessionId,
  ]);

  const handleProjectChange = useCallback(
    (projectId: string | null) => {
      if (!sessionId) {
        setPendingProjectId(projectId);
        return;
      }
      const nextProject =
        projectId == null
          ? null
          : (useProjectStore
              .getState()
              .projects.find((candidate) => candidate.id === projectId) ??
            null);

      useChatSessionStore.getState().updateSession(sessionId, { projectId });
      if (!selectedProvider) {
        return;
      }
      void prepareCurrentSession(
        selectedProvider,
        nextProject,
        activeWorkspace?.path,
        selectedPersonaId ?? undefined,
        effectiveModelSelection,
      ).catch((error) => {
        console.error("Failed to update ACP session working directory:", error);
      });
    },
    [
      activeWorkspace?.path,
      effectiveModelSelection,
      prepareCurrentSession,
      selectedPersonaId,
      selectedProvider,
      sessionId,
    ],
  );

  const handlePersonaChange = useCallback(
    (personaId: string | null) => {
      const persona = personas.find((candidate) => candidate.id === personaId);
      if (persona?.provider) {
        const matchingProvider = providers.find(
          (provider) =>
            provider.id === persona.provider ||
            provider.label.toLowerCase().includes(persona.provider ?? ""),
        );
        if (matchingProvider) {
          if (!sessionId) {
            setPendingProviderId(matchingProvider.id);
            setPendingModelSelection(undefined);
            setGlobalSelectedProvider(matchingProvider.id);
          } else {
            handleProviderChange(matchingProvider.id);
          }
        }
      }
      const agentStore = useAgentStore.getState();
      const matchingAgent = agentStore.agents.find(
        (agent) => agent.personaId === personaId,
      );
      if (matchingAgent) {
        agentStore.setActiveAgent(matchingAgent.id);
      }
      if (!sessionId) {
        setPendingPersonaId(personaId);
        return;
      }
      useChatSessionStore
        .getState()
        .updateSession(sessionId, { personaId: personaId ?? undefined });
    },
    [
      handleProviderChange,
      personas,
      providers,
      sessionId,
      setGlobalSelectedProvider,
    ],
  );

  useEffect(() => {
    if (
      selectedPersonaId !== null &&
      personas.length > 0 &&
      !personas.find((persona) => persona.id === selectedPersonaId)
    ) {
      if (sessionId) {
        useChatSessionStore
          .getState()
          .updateSession(sessionId, { personaId: undefined });
      } else {
        setPendingPersonaId(undefined);
      }
    }
  }, [personas, selectedPersonaId, sessionId]);

  const personaInfo = selectedPersona
    ? { id: selectedPersona.id, name: selectedPersona.displayName }
    : undefined;
  const {
    messages,
    chatState,
    tokenState,
    sendMessage,
    stopStreaming,
    streamingMessageId,
  } = useChat(
    stateSessionId,
    selectedProvider,
    effectiveSystemPrompt,
    personaInfo,
    {
      onMessageAccepted: sessionId ? onMessageAccepted : undefined,
      ensurePrepared: selectedProvider
        ? () =>
            prepareSelectedProvider(selectedProvider, effectiveModelSelection)
        : undefined,
    },
  );
  const isLoadingHistory = useChatStore((s) =>
    sessionId
      ? s.loadingSessionIds.has(sessionId) &&
        (s.messagesBySession[sessionId]?.length ?? 0) === 0
      : false,
  );
  const deferredSend = useRef<{
    text: string;
    attachments?: ChatAttachmentDraft[];
  } | null>(null);
  const queue = useMessageQueue(
    stateSessionId,
    sessionId ? chatState : "thinking",
    sendMessage,
  );
  const chatStore = useChatStore();

  const handleSend = useCallback(
    (text: string, personaId?: string, attachments?: ChatAttachmentDraft[]) => {
      if (!sessionId) {
        if (!queue.queuedMessage) {
          queue.enqueue(text, personaId, attachments);
        }
        return;
      }

      if (personaId && personaId !== selectedPersonaId) {
        const nextPersona = personas.find(
          (persona) => persona.id === personaId,
        );
        if (nextPersona) {
          chatStore.addMessage(sessionId, {
            id: crypto.randomUUID(),
            role: "system",
            created: Date.now(),
            content: [
              {
                type: "systemNotification",
                notificationType: "info",
                text: `Switched to ${nextPersona.displayName}`,
              },
            ],
            metadata: { userVisible: true, agentVisible: false },
          });
        }
        handlePersonaChange(personaId);
        deferredSend.current = { text, attachments };
        return;
      }

      if (chatState !== "idle" && !queue.queuedMessage) {
        queue.enqueue(text, personaId, attachments);
        return;
      }

      sendMessage(text, undefined, attachments);
    },
    [
      chatState,
      chatStore,
      handlePersonaChange,
      personas,
      queue,
      sessionId,
      selectedPersonaId,
      sendMessage,
    ],
  );

  useEffect(() => {
    if (deferredSend.current && selectedPersona) {
      const { text, attachments } = deferredSend.current;
      deferredSend.current = null;
      sendMessage(text, undefined, attachments);
    }
  }, [selectedPersona, sendMessage]);

  const handleCreatePersona = useCallback(() => {
    useAgentStore.getState().openPersonaEditor();
  }, []);

  const sessionDraftValue = useChatStore((s) =>
    sessionId ? (s.draftsBySession[sessionId] ?? "") : "",
  );
  const draftValue = sessionId ? sessionDraftValue : pendingDraftValue;
  const handleDraftChange = useCallback(
    (text: string) => {
      useChatStore.getState().setDraft(stateSessionId, text);
    },
    [stateSessionId],
  );
  const scrollTarget = useChatStore((s) =>
    sessionId ? (s.scrollTargetMessageBySession[sessionId] ?? null) : null,
  );
  const handleScrollTargetHandled = useCallback(() => {
    if (!sessionId) {
      return;
    }
    useChatStore.getState().clearScrollTargetMessage(sessionId);
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    let cancelled = false;
    void pendingDraftValue;
    void pendingQueuedMessage;

    const syncPendingHomeState = async () => {
      const chatState = useChatStore.getState();
      const pendingDraft =
        chatState.draftsBySession[PENDING_HOME_SESSION_ID] ?? "";

      if (pendingDraft && !chatState.draftsBySession[sessionId]) {
        chatState.setDraft(sessionId, pendingDraft);
      }

      const hasPendingProvider = pendingProviderId !== undefined;
      const hasPendingPersona = pendingPersonaId !== undefined;
      const hasPendingProject = pendingProjectId !== undefined;
      const hasPendingModel = pendingModelSelection !== undefined;

      if (
        hasPendingProvider ||
        hasPendingPersona ||
        hasPendingProject ||
        hasPendingModel
      ) {
        const nextProviderId = pendingProviderId ?? selectedProvider;
        const nextPersonaId =
          pendingPersonaId !== undefined
            ? (pendingPersonaId ?? undefined)
            : session?.personaId;
        const nextProjectId =
          pendingProjectId !== undefined
            ? pendingProjectId
            : session?.projectId;
        const nextProject =
          nextProjectId == null
            ? null
            : (useProjectStore
                .getState()
                .projects.find((candidate) => candidate.id === nextProjectId) ??
              null);

        const patch: {
          providerId?: string;
          personaId?: string | undefined;
          projectId?: string | null;
          modelId?: string | undefined;
          modelName?: string | undefined;
        } = {};

        if (hasPendingProvider) {
          patch.providerId = nextProviderId;
          patch.modelId = undefined;
          patch.modelName = undefined;
        }
        if (hasPendingPersona) {
          patch.personaId = nextPersonaId;
        }
        if (hasPendingProject) {
          patch.projectId = nextProjectId ?? null;
        }

        useChatSessionStore.getState().updateSession(sessionId, patch);

        try {
          await prepareCurrentSession(
            nextProviderId,
            nextProject,
            activeWorkspace?.path,
            nextPersonaId,
            pendingModelSelection,
          );
          if (cancelled) {
            return;
          }
          if (pendingModelSelection?.source === "explicit") {
            const agentId =
              resolveAgentProviderCatalogIdStrict(
                pendingModelSelection.providerId ?? nextProviderId,
              ) ?? "goose";
            setStoredModelPreference(agentId, {
              modelId: pendingModelSelection.id,
              modelName: pendingModelSelection.name,
              providerId: pendingModelSelection.providerId ?? nextProviderId,
            });
          }
        } catch (error) {
          console.error("Failed to sync pending Home state:", error);
          return;
        }

        setPendingProviderId(undefined);
        setPendingPersonaId(undefined);
        setPendingProjectId(undefined);
        setPendingModelSelection(undefined);
      }

      const latestChatState = useChatStore.getState();
      const latestPendingQueue =
        latestChatState.queuedMessageBySession[PENDING_HOME_SESSION_ID] ?? null;
      if (
        latestPendingQueue &&
        !latestChatState.queuedMessageBySession[sessionId]
      ) {
        latestChatState.enqueueMessage(sessionId, latestPendingQueue);
      }

      useChatStore.getState().clearDraft(PENDING_HOME_SESSION_ID);
      useChatStore.getState().dismissQueuedMessage(PENDING_HOME_SESSION_ID);
      useChatStore.getState().cleanupSession(PENDING_HOME_SESSION_ID);
    };

    void syncPendingHomeState();

    return () => {
      cancelled = true;
    };
  }, [
    activeWorkspace?.path,
    pendingDraftValue,
    pendingModelSelection,
    pendingPersonaId,
    pendingProjectId,
    pendingProviderId,
    pendingQueuedMessage,
    prepareCurrentSession,
    selectedProvider,
    session?.personaId,
    session?.projectId,
    sessionId,
  ]);

  return {
    session,
    project,
    allowedArtifactRoots,
    messages,
    chatState,
    tokenState,
    stopStreaming,
    streamingMessageId,
    isLoadingHistory,
    queue,
    handleSend,
    draftValue,
    handleDraftChange,
    scrollTarget,
    handleScrollTargetHandled,
    projectMetadataPending,
    personas,
    selectedPersonaId,
    handlePersonaChange,
    handleCreatePersona,
    pickerAgents,
    providersLoading,
    selectedProvider: selectedAgentId,
    handleProviderChange,
    currentModelId: effectiveModelSelection?.id ?? null,
    currentModelName: effectiveModelSelection?.name ?? null,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleModelChange,
    selectedProjectId: effectiveProjectId,
    availableProjects,
    handleProjectChange,
  };
}
