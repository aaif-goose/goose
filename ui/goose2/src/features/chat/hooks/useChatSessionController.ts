import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import type { ChatSendOptions, ChatSkillDraft, ModelOption } from "../types";
import { INITIAL_TOKEN_STATE } from "@/shared/types/chat";
import { useChat } from "./useChat";
import { useMessageQueue } from "./useMessageQueue";
import { useChatStore } from "../stores/chatStore";
import { useChatSessionStore } from "../stores/chatSessionStore";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { selectPersonas } from "@/features/agents/stores/agentSelectors";
import { useProviderSelection } from "@/features/agents/hooks/useProviderSelection";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { selectProjects } from "@/features/projects/stores/projectSelectors";
import { resolveAgentProviderCatalogIdStrictFromEntries } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import {
  composeSystemPrompt,
  resolveProjectDefaultArtifactRoot,
} from "@/features/projects/lib/chatProjectContext";
import { setStoredModelPreference } from "../lib/modelPreferences";
import { applyLatestSessionConfig } from "../lib/sessionConfigRequests";
import { supportsContextCompactionControls } from "../lib/autoCompact";
import { resolveSessionCwd } from "@/features/projects/lib/sessionCwdSelection";
import {
  useResolvedAgentModelPicker,
  type PreferredModelSelection,
} from "./useResolvedAgentModelPicker";
import { updateSessionProject } from "@/shared/api/acpApi";

interface UseChatSessionControllerOptions {
  sessionId: string | null;
  onMessageAccepted?: (sessionId: string) => void;
  onCreatePersonaRequested?: () => void;
}

const PENDING_HOME_SESSION_ID = "__home_pending__";
const EMPTY_SKILL_DRAFTS: ChatSkillDraft[] = [];

function movePendingHomeQueuedMessage(sessionId: string) {
  const chatState = useChatStore.getState();
  const pendingQueue =
    chatState.queuedMessageBySession[PENDING_HOME_SESSION_ID] ?? null;
  if (pendingQueue && !chatState.queuedMessageBySession[sessionId]) {
    chatState.enqueueMessage(sessionId, pendingQueue);
  }
}

export function useChatSessionController({
  sessionId,
  onMessageAccepted,
  onCreatePersonaRequested,
}: UseChatSessionControllerOptions) {
  const stateSessionId = sessionId ?? PENDING_HOME_SESSION_ID;
  const {
    providers,
    providersLoading,
    selectedProvider: globalSelectedProvider,
    setSelectedProvider: setGlobalSelectedProvider,
  } = useProviderSelection();
  const personas = useAgentStore(selectPersonas);
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
  const projects = useProjectStore(selectProjects);
  const projectsLoading = useProjectStore((s) => s.loading);
  const catalogEntries = useProviderCatalogStore((s) => s.entries);
  const [pendingPersonaId, setPendingPersonaId] = useState<string | null>();
  const [pendingProjectId, setPendingProjectId] = useState<string | null>();
  const [pendingProviderId, setPendingProviderId] = useState<string>();
  const [pendingModelSelection, setPendingModelSelection] =
    useState<PreferredModelSelection | null>();
  const pendingDraftValue = useChatStore(
    (s) => s.draftsBySession[PENDING_HOME_SESSION_ID] ?? "",
  );
  const pendingSkillDrafts = useChatStore(
    (s) =>
      s.skillDraftsBySession[PENDING_HOME_SESSION_ID] ?? EMPTY_SKILL_DRAFTS,
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
  const hasContextUsageSnapshot = useChatStore(
    (s) => s.sessionStateById[stateSessionId]?.hasUsageSnapshot ?? false,
  );
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
  const sessionCwd =
    activeWorkspace?.path ??
    session?.workingDir ??
    resolveProjectDefaultArtifactRoot(project);
  const projectDefaultArtifactRoot = useMemo(
    () => resolveProjectDefaultArtifactRoot(project),
    [project],
  );
  const projectMetadataPending = Boolean(
    effectiveProjectId && !projectDefaultArtifactRoot && projectsLoading,
  );
  const sessionArtifactCwd = useMemo(
    () => sessionCwd?.trim() || null,
    [sessionCwd],
  );
  const availableProjects = useMemo(
    () =>
      [...projects]
        .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name))
        .map((projectInfo) => ({
          id: projectInfo.id,
          name: projectInfo.name,
          workingDirs: projectInfo.workingDirs,
          icon: projectInfo.icon,
          color: projectInfo.color,
        })),
    [projects],
  );
  const workingContextPrompt = useMemo(() => {
    if (!activeWorkspace?.branch) return undefined;
    return `<active-working-context>\nActive branch: ${activeWorkspace.branch}\nWorking directory: ${activeWorkspace.path}\n</active-working-context>`;
  }, [activeWorkspace?.branch, activeWorkspace?.path]);
  const effectiveSystemPrompt = useMemo(
    () =>
      composeSystemPrompt(selectedPersona?.systemPrompt, workingContextPrompt),
    [selectedPersona?.systemPrompt, workingContextPrompt],
  );

  const prepareCurrentSession = useCallback(
    async (
      providerId: string,
      nextProject = project,
      nextWorkspacePath = activeWorkspace?.path,
      modelSelection?: PreferredModelSelection | null,
    ) => {
      if (!sessionId) {
        return false;
      }
      const workingDir = await resolveSessionCwd(
        nextProject,
        nextWorkspacePath,
      );
      const result = await applyLatestSessionConfig({
        sessionId,
        providerId,
        workingDir,
        modelId: modelSelection?.id,
      });
      if (!result.applied || !modelSelection?.id) {
        return result.applied;
      }

      const sessionStore = useChatSessionStore.getState();
      const liveSession = sessionStore.getSession(sessionId);
      const modelAlreadyApplied =
        liveSession?.modelId === modelSelection.id &&
        liveSession?.modelName === modelSelection.name;

      if (modelAlreadyApplied) {
        return true;
      }

      sessionStore.patchSession(sessionId, {
        modelId: modelSelection.id,
        modelName: modelSelection.name,
      });
      return true;
    },
    [activeWorkspace?.path, project, sessionId],
  );
  const prepareSelectedProvider = useCallback(
    (providerId: string, modelSelection?: PreferredModelSelection | null) =>
      prepareCurrentSession(
        providerId,
        project,
        activeWorkspace?.path,
        modelSelection,
      ),
    [activeWorkspace?.path, prepareCurrentSession, project],
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
    handlePickerOpen,
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

  const handleProviderChangeWithContextReset = useCallback(
    (providerId: string) => {
      if (providerId === selectedProvider) {
        return;
      }

      useChatStore.getState().resetTokenState(stateSessionId);
      handleProviderChange(providerId);
    },
    [handleProviderChange, selectedProvider, stateSessionId],
  );

  const handleModelChangeWithContextReset = useCallback(
    (modelId: string, model?: ModelOption) => {
      const nextProviderId = model?.providerId;
      if (
        modelId === effectiveModelSelection?.id &&
        (!nextProviderId ||
          nextProviderId === effectiveModelSelection?.providerId)
      ) {
        return;
      }
      useChatStore.getState().resetTokenState(stateSessionId);
      handleModelChange(modelId, model);
    },
    [
      effectiveModelSelection?.id,
      effectiveModelSelection?.providerId,
      handleModelChange,
      stateSessionId,
    ],
  );

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

      useChatSessionStore.getState().patchSession(sessionId, { projectId });

      void updateSessionProject(sessionId, projectId).catch(console.error);

      if (!selectedProvider) {
        return;
      }
      void prepareCurrentSession(
        selectedProvider,
        nextProject,
        activeWorkspace?.path,
        effectiveModelSelection,
      ).catch((error) => {
        console.error("Failed to update ACP session working directory:", error);
      });
    },
    [
      activeWorkspace?.path,
      effectiveModelSelection,
      prepareCurrentSession,
      selectedProvider,
      sessionId,
    ],
  );

  const handlePersonaChange = useCallback(
    (personaId: string | null) => {
      if (personaId === selectedPersonaId) {
        return;
      }

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
        .patchSession(sessionId, { personaId: personaId ?? undefined });
    },
    [
      handleProviderChange,
      personas,
      providers,
      sessionId,
      selectedPersonaId,
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
          .patchSession(sessionId, { personaId: undefined });
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
    compactConversation,
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
            prepareCurrentSession(
              selectedProvider,
              project,
              activeWorkspace?.path,
              effectiveModelSelection,
            )
        : undefined,
    },
  );
  const resolvedTokenState = tokenState ?? INITIAL_TOKEN_STATE;
  const supportsCompactionControls =
    supportsContextCompactionControls(selectedAgentId);
  const isCompactingContext = chatState === "compacting";
  const isLoadingHistory = useChatStore((s) =>
    sessionId
      ? s.loadingSessionIds.has(sessionId) &&
        (s.messagesBySession[sessionId]?.length ?? 0) === 0
      : false,
  );
  const deferredSend = useRef<{
    text: string;
    attachments?: ChatAttachmentDraft[];
    sendOptions?: ChatSendOptions;
    resolve?: (accepted: boolean) => void;
  } | null>(null);
  const queue = useMessageQueue(
    stateSessionId,
    sessionId ? chatState : "thinking",
    (...args) => {
      void sendMessage(...args);
    },
  );

  const handleSend = useCallback(
    (
      text: string,
      personaId?: string,
      attachments?: ChatAttachmentDraft[],
      sendOptions?: ChatSendOptions,
    ) => {
      if (!sessionId) {
        if (!queue.queuedMessage) {
          queue.enqueue(text, personaId, attachments, sendOptions);
        }
        return true;
      }

      if (personaId && personaId !== selectedPersonaId) {
        handlePersonaChange(personaId);
        return new Promise<boolean>((resolve) => {
          deferredSend.current = { text, attachments, sendOptions, resolve };
        });
      }

      if (chatState !== "idle" && !queue.queuedMessage) {
        queue.enqueue(text, personaId, attachments, sendOptions);
        return true;
      }

      if (sendOptions) {
        void sendMessage(text, undefined, attachments, sendOptions);
      } else {
        void sendMessage(text, undefined, attachments);
      }
      return true;
    },
    [
      chatState,
      handlePersonaChange,
      queue,
      sessionId,
      selectedPersonaId,
      sendMessage,
    ],
  );

  useEffect(() => {
    if (deferredSend.current && selectedPersona) {
      const { text, attachments, sendOptions, resolve } = deferredSend.current;
      deferredSend.current = null;
      const sendResult = sendOptions
        ? sendMessage(text, undefined, attachments, sendOptions)
        : sendMessage(text, undefined, attachments);
      void sendResult.then(() => resolve?.(true));
    }
  }, [selectedPersona, sendMessage]);

  const handleCreatePersona = useCallback(() => {
    if (onCreatePersonaRequested) {
      onCreatePersonaRequested();
      return;
    }
    useAgentStore.getState().openPersonaEditor();
  }, [onCreatePersonaRequested]);

  const sessionDraftValue = useChatStore((s) =>
    sessionId ? (s.draftsBySession[sessionId] ?? "") : "",
  );
  const sessionSkillDrafts = useChatStore((s) =>
    sessionId
      ? (s.skillDraftsBySession[sessionId] ?? EMPTY_SKILL_DRAFTS)
      : EMPTY_SKILL_DRAFTS,
  );
  const draftValue = sessionId ? sessionDraftValue : pendingDraftValue;
  const selectedSkills = sessionId ? sessionSkillDrafts : pendingSkillDrafts;
  const handleDraftChange = useCallback(
    (text: string) => {
      useChatStore.getState().setDraft(stateSessionId, text);
    },
    [stateSessionId],
  );
  const handleSkillsChange = useCallback(
    (skills: typeof selectedSkills) => {
      useChatStore.getState().setSkillDrafts(stateSessionId, skills);
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
    void pendingSkillDrafts;
    void pendingQueuedMessage;

    const syncPendingHomeState = async () => {
      const chatState = useChatStore.getState();
      const pendingDraft =
        chatState.draftsBySession[PENDING_HOME_SESSION_ID] ?? "";
      const pendingSkills =
        chatState.skillDraftsBySession[PENDING_HOME_SESSION_ID] ?? [];

      if (pendingDraft && !chatState.draftsBySession[sessionId]) {
        chatState.setDraft(sessionId, pendingDraft);
      }
      if (
        pendingSkills.length > 0 &&
        !chatState.skillDraftsBySession[sessionId]?.length
      ) {
        chatState.setSkillDrafts(sessionId, pendingSkills);
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
          modelId?: string | undefined;
          modelName?: string | undefined;
          projectId?: string | null;
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
          void updateSessionProject(sessionId, nextProjectId ?? null).catch(
            console.error,
          );
        }

        useChatSessionStore.getState().patchSession(sessionId, patch);

        try {
          const applied = await prepareCurrentSession(
            nextProviderId,
            nextProject,
            activeWorkspace?.path,
            pendingModelSelection,
          );
          if (cancelled) {
            return;
          }
          if (applied && pendingModelSelection?.source === "explicit") {
            const agentId =
              resolveAgentProviderCatalogIdStrictFromEntries(
                catalogEntries,
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

      movePendingHomeQueuedMessage(sessionId);
      useChatStore.getState().clearDraft(PENDING_HOME_SESSION_ID);
      useChatStore.getState().clearSkillDrafts(PENDING_HOME_SESSION_ID);
      useChatStore.getState().dismissQueuedMessage(PENDING_HOME_SESSION_ID);
      useChatStore.getState().cleanupSession(PENDING_HOME_SESSION_ID);
    };

    void syncPendingHomeState();

    return () => {
      cancelled = true;
    };
  }, [
    activeWorkspace?.path,
    catalogEntries,
    pendingDraftValue,
    pendingSkillDrafts,
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
    sessionArtifactCwd,
    messages,
    chatState,
    tokenState: resolvedTokenState,
    stopStreaming,
    streamingMessageId,
    compactConversation,
    canCompactContext:
      supportsCompactionControls && messages.length > 0 && chatState === "idle",
    isCompactingContext,
    supportsCompactionControls,
    isContextUsageReady:
      hasContextUsageSnapshot && resolvedTokenState.contextLimit > 0,
    isLoadingHistory,
    queue,
    handleSend,
    draftValue,
    handleDraftChange,
    selectedSkills,
    handleSkillsChange,
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
    handleProviderChange: handleProviderChangeWithContextReset,
    currentModelId: effectiveModelSelection?.id ?? null,
    currentModelProviderId: effectiveModelSelection?.providerId ?? null,
    currentModelName: effectiveModelSelection?.name ?? null,
    availableModels,
    modelsLoading,
    modelStatusMessage,
    handleModelChange: handleModelChangeWithContextReset,
    handlePickerOpen,
    selectedProjectId: effectiveProjectId,
    availableProjects,
    handleProjectChange,
  };
}
