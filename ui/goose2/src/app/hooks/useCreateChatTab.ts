import { useCallback } from "react";
import type { ProjectInfo } from "@/features/projects/api/projects";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { findExistingDraft } from "@/features/chat/lib/newChat";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { resolveSessionCwd } from "@/features/projects/lib/sessionCwdSelection";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { perfLog } from "@/shared/lib/perfLog";
import type { GlobalComposeOptions } from "@/shared/ui/GlobalComposerPill";
import type { AppView } from "../types";
import { resolveSupportedSessionModelPreference } from "../lib/resolveSupportedSessionModelPreference";

interface CreateTabOptions extends GlobalComposeOptions {
  personaId?: string;
}

export function useCreateChatTab(setActiveView: (view: AppView) => void) {
  const agentStore = useAgentStore();
  const chatStore = useChatStore();
  const sessionStore = useChatSessionStore();
  const providerInventoryEntries = useProviderInventoryStore(
    (state) => state.entries,
  );

  return useCallback(
    async (
      title = DEFAULT_CHAT_TITLE,
      project?: ProjectInfo | null,
      composeOptions?: CreateTabOptions,
    ) => {
      const tStart = performance.now();
      perfLog(
        `[perf:newtab] createNewTab start (project=${project?.id ?? "none"})`,
      );
      const effectiveProject = project ?? null;
      const agentId = agentStore.activeAgentId ?? undefined;
      const personaId = composeOptions?.personaId;
      const providerId =
        composeOptions?.providerId ??
        effectiveProject?.preferredProvider ??
        agentStore.selectedProvider ??
        "goose";
      const preferredModel =
        composeOptions?.modelId ??
        (composeOptions?.providerId &&
        effectiveProject?.preferredProvider &&
        composeOptions.providerId !== effectiveProject.preferredProvider
          ? undefined
          : (effectiveProject?.preferredModel ?? undefined));
      const sessionModelPreference =
        await resolveSupportedSessionModelPreference(
          providerId,
          providerInventoryEntries,
          preferredModel,
        );
      const sessionState = useChatSessionStore.getState();
      const chatState = useChatStore.getState();
      const existingDraft = findExistingDraft({
        sessions: sessionState.sessions,
        activeSessionId: sessionState.activeSessionId,
        draftsBySession: chatState.draftsBySession,
        messagesBySession: chatState.messagesBySession,
        request: {
          title,
          projectId: effectiveProject?.id,
          personaId,
        },
      });

      const draftMatchesSelection =
        existingDraft &&
        existingDraft.providerId === sessionModelPreference.providerId &&
        (existingDraft.modelId ?? null) ===
          (sessionModelPreference.modelId ?? null);

      if (draftMatchesSelection) {
        sessionStore.setActiveSession(existingDraft.id);
        setActiveView("chat");
        chatStore.setActiveSession(existingDraft.id);
        perfLog(
          `[perf:newtab] ${existingDraft.id.slice(0, 8)} reused draft in ${(performance.now() - tStart).toFixed(1)}ms`,
        );
        return existingDraft;
      }

      const workingDir = await resolveSessionCwd(effectiveProject);
      const session = await sessionStore.createSession({
        title,
        projectId: effectiveProject?.id,
        agentId,
        personaId,
        providerId: sessionModelPreference.providerId,
        workingDir,
        modelId: sessionModelPreference.modelId,
        modelName: sessionModelPreference.modelId
          ? (composeOptions?.modelName ?? sessionModelPreference.modelName)
          : undefined,
      });
      sessionStore.setActiveSession(session.id);
      setActiveView("chat");
      chatStore.setActiveSession(session.id);
      perfLog(
        `[perf:newtab] ${session.id.slice(0, 8)} created session in ${(performance.now() - tStart).toFixed(1)}ms`,
      );
      return session;
    },
    [
      agentStore.activeAgentId,
      agentStore.selectedProvider,
      chatStore,
      providerInventoryEntries,
      sessionStore,
      setActiveView,
    ],
  );
}
