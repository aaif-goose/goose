import { create } from "zustand";
import {
  acpCreateSession,
  acpListSessions,
  type AcpSessionInfo,
} from "@/shared/api/acp";
import type { Session } from "@/shared/types/chat";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import {
  loadSessionMetadataOverlay,
  persistSessionMetadataOverlay,
  upsertSessionMetadataOverlayRecord,
  type SessionMetadataOverlayRecord,
} from "@/features/chat/lib/sessionMetadataOverlay";
import { updateSessionProject } from "@/shared/api/acpApi";

export interface ChatSession {
  id: string;
  acpSessionId?: string;
  title: string;
  projectId?: string | null;
  agentId?: string;
  providerId?: string;
  personaId?: string;
  modelId?: string;
  modelName?: string;
  createdAt: string;
  updatedAt: string;
  archivedAt?: string;
  messageCount: number;
  userSetName?: boolean;
}

export interface ActiveWorkspace {
  path: string;
  branch: string | null;
}

export function hasSessionStarted(
  session: Pick<ChatSession, "messageCount">,
  localMessages?: ArrayLike<unknown>,
): boolean {
  return session.messageCount > 0 || (localMessages?.length ?? 0) > 0;
}

export function getVisibleSessions<
  T extends Pick<ChatSession, "id" | "messageCount">,
>(
  sessions: T[],
  messagesBySession: Record<string, ArrayLike<unknown> | undefined>,
): T[] {
  return sessions.filter((session) =>
    hasSessionStarted(session, messagesBySession[session.id]),
  );
}

interface ChatSessionStoreState {
  sessions: ChatSession[];
  activeSessionId: string | null;
  isLoading: boolean;
  hasHydratedSessions: boolean;
  contextPanelOpenBySession: Record<string, boolean>;
  activeWorkspaceBySession: Record<string, ActiveWorkspace>;
}

interface CreateSessionOpts {
  title?: string;
  projectId?: string;
  agentId?: string;
  providerId?: string;
  personaId?: string;
  workingDir?: string;
  modelId?: string;
  modelName?: string;
}

interface UpdateSessionOptions {
  persistOverlay?: boolean;
}

interface ChatSessionStoreActions {
  createSession: (opts?: CreateSessionOpts) => Promise<ChatSession>;
  loadSessions: () => Promise<void>;
  updateSession: (
    id: string,
    patch: Partial<ChatSession>,
    opts?: UpdateSessionOptions,
  ) => void;
  addSession: (session: ChatSession) => void;
  archiveSession: (id: string) => Promise<void>;
  unarchiveSession: (id: string) => Promise<void>;

  setActiveSession: (sessionId: string | null) => void;
  setContextPanelOpen: (sessionId: string, open: boolean) => void;
  setActiveWorkspace: (sessionId: string, context: ActiveWorkspace) => void;
  clearActiveWorkspace: (sessionId: string) => void;
  switchSessionProvider: (sessionId: string, providerId: string) => void;

  getSession: (id: string) => ChatSession | undefined;
  getActiveSession: () => ChatSession | null;
  getArchivedSessions: () => ChatSession[];
}

export type ChatSessionStore = ChatSessionStoreState & ChatSessionStoreActions;

function overlayKeyForSession(
  session: Pick<ChatSession, "id" | "acpSessionId">,
) {
  return session.acpSessionId ?? session.id;
}

function buildOverlayRecord(
  session: ChatSession,
  existing?: SessionMetadataOverlayRecord,
): SessionMetadataOverlayRecord {
  return {
    sessionId: overlayKeyForSession(session),
    userSetTitle: session.userSetName ? session.title : null,
    projectId: session.projectId ?? null,
    providerId: session.providerId ?? null,
    personaId: session.personaId ?? null,
    modelId: session.modelId ?? null,
    modelName: session.modelName ?? null,
    archivedAt: session.archivedAt ?? null,
    createdAt: session.createdAt ?? existing?.createdAt ?? null,
    agentId: session.agentId ?? existing?.agentId ?? null,
    lastKnownTitle: session.title,
    lastKnownUpdatedAt: session.updatedAt,
    lastKnownMessageCount: session.messageCount,
    updatedAt: new Date().toISOString(),
  };
}

function overlayToFallbackSession(
  overlay: SessionMetadataOverlayRecord,
): ChatSession {
  const updatedAt =
    overlay.lastKnownUpdatedAt ?? overlay.createdAt ?? overlay.updatedAt;
  return {
    id: overlay.sessionId,
    acpSessionId: overlay.sessionId,
    title: overlay.userSetTitle ?? overlay.lastKnownTitle ?? "Untitled",
    projectId: overlay.projectId ?? undefined,
    agentId: overlay.agentId ?? undefined,
    providerId: overlay.providerId ?? undefined,
    personaId: overlay.personaId ?? undefined,
    modelId: overlay.modelId ?? undefined,
    modelName: overlay.modelName ?? undefined,
    createdAt: overlay.createdAt ?? updatedAt,
    updatedAt,
    archivedAt: overlay.archivedAt ?? undefined,
    messageCount: overlay.lastKnownMessageCount ?? 0,
    userSetName: Boolean(overlay.userSetTitle),
  };
}

function mergeAcpSessionWithOverlay(
  session: AcpSessionInfo,
  overlay?: SessionMetadataOverlayRecord,
): ChatSession {
  const updatedAt = session.updatedAt ?? overlay?.lastKnownUpdatedAt;
  return {
    id: session.sessionId,
    acpSessionId: session.sessionId,
    title:
      overlay?.userSetTitle ??
      session.title ??
      overlay?.lastKnownTitle ??
      "Untitled",
    projectId: session.projectId ?? undefined,
    agentId: overlay?.agentId ?? undefined,
    providerId: overlay?.providerId ?? undefined,
    personaId: overlay?.personaId ?? undefined,
    modelId: overlay?.modelId ?? undefined,
    modelName: overlay?.modelName ?? undefined,
    createdAt: overlay?.createdAt ?? updatedAt ?? new Date().toISOString(),
    updatedAt: updatedAt ?? new Date().toISOString(),
    archivedAt: overlay?.archivedAt ?? undefined,
    messageCount: session.messageCount,
    userSetName: Boolean(overlay?.userSetTitle),
  };
}

function syncOverlaySnapshots(
  sessions: ChatSession[],
  existingOverlays = loadSessionMetadataOverlay(),
): void {
  const overlays = new Map(existingOverlays);
  for (const session of sessions) {
    overlays.set(
      overlayKeyForSession(session),
      buildOverlayRecord(
        session,
        existingOverlays.get(overlayKeyForSession(session)),
      ),
    );
  }
  persistSessionMetadataOverlay(overlays.values());
}

function sortByUpdatedAtDesc(sessions: ChatSession[]): ChatSession[] {
  return [...sessions].sort(
    (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
  );
}

export function sessionToChatSession(session: Session): ChatSession {
  return {
    id: session.id,
    acpSessionId: session.id,
    title: session.title,
    agentId: session.agentId,
    projectId: session.projectId,
    providerId: session.providerId,
    personaId: session.personaId,
    modelId: session.modelId,
    modelName: session.modelName,
    createdAt: session.createdAt,
    updatedAt: session.updatedAt,
    archivedAt: session.archivedAt,
    messageCount: session.messageCount,
    userSetName: session.userSetName,
  };
}

export const useChatSessionStore = create<ChatSessionStore>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  isLoading: false,
  hasHydratedSessions: false,
  contextPanelOpenBySession: {},
  activeWorkspaceBySession: {},

  createSession: async (opts) => {
    if (!opts?.workingDir) {
      throw new Error("createSession requires a working directory");
    }
    const now = new Date().toISOString();
    const providerId = opts.providerId ?? "goose";
    const { sessionId } = await acpCreateSession(providerId, opts.workingDir, {
      personaId: opts.personaId,
      modelId: opts.modelId,
      projectId: opts.projectId,
    });
    const chatSession: ChatSession = {
      id: sessionId,
      acpSessionId: sessionId,
      title: opts.title ?? DEFAULT_CHAT_TITLE,
      projectId: opts.projectId,
      agentId: opts.agentId,
      providerId,
      personaId: opts.personaId,
      modelId: opts.modelId,
      modelName: opts.modelName,
      createdAt: now,
      updatedAt: now,
      messageCount: 0,
    };
    set((state) => ({ sessions: [chatSession, ...state.sessions] }));
    const existing = loadSessionMetadataOverlay().get(
      overlayKeyForSession(chatSession),
    );
    upsertSessionMetadataOverlayRecord(
      buildOverlayRecord(chatSession, existing),
    );
    return chatSession;
  },

  loadSessions: async () => {
    set({ isLoading: true });
    try {
      const overlays = loadSessionMetadataOverlay();
      const acpSessions = await acpListSessions();
      const mergedAcpSessions = sortByUpdatedAtDesc(
        acpSessions.map((session) =>
          mergeAcpSessionWithOverlay(session, overlays.get(session.sessionId)),
        ),
      );
      const merged = mergedAcpSessions;
      const activeSessionId = get().activeSessionId;
      const activeSessionStillExists =
        activeSessionId == null ||
        merged.some((session) => session.id === activeSessionId);
      set({
        sessions: merged,
        activeSessionId: activeSessionStillExists ? activeSessionId : null,
      });
      syncOverlaySnapshots(mergedAcpSessions, overlays);
    } catch (error) {
      console.error("Failed to load sessions from ACP:", error);
      const overlays = loadSessionMetadataOverlay();
      const fallbackSessions = sortByUpdatedAtDesc(
        [...overlays.values()].map(overlayToFallbackSession),
      );
      set({ sessions: fallbackSessions });
    } finally {
      set({ isLoading: false, hasHydratedSessions: true });
    }
  },

  updateSession: (id, patch, opts) => {
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id
          ? {
              ...session,
              ...patch,
              updatedAt: patch.updatedAt ?? session.updatedAt,
            }
          : session,
      ),
    }));

    const updatedSession = get().sessions.find((session) => session.id === id);

    if (updatedSession && opts?.persistOverlay !== false) {
      const key = overlayKeyForSession(updatedSession);
      const existing = loadSessionMetadataOverlay().get(key);
      upsertSessionMetadataOverlayRecord(
        buildOverlayRecord(updatedSession, existing),
      );
    }

    // Persist projectId change to ACP backend
    const acpSessionId = updatedSession?.acpSessionId;
    if ("projectId" in patch && acpSessionId) {
      updateSessionProject(acpSessionId, patch.projectId ?? null).catch(
        (err: unknown) =>
          console.error("Failed to update session project in backend:", err),
      );
    }
  },

  addSession: (session) => {
    const normalizedSession = {
      ...session,
      acpSessionId: session.acpSessionId ?? session.id,
    };
    set((state) => {
      const existing = state.sessions.findIndex(
        (candidate) => candidate.id === normalizedSession.id,
      );
      if (existing >= 0) {
        const updated = [...state.sessions];
        updated[existing] = { ...updated[existing], ...normalizedSession };
        return { sessions: updated };
      }
      return { sessions: [normalizedSession, ...state.sessions] };
    });
    const existing = loadSessionMetadataOverlay().get(
      overlayKeyForSession(normalizedSession),
    );
    upsertSessionMetadataOverlayRecord(
      buildOverlayRecord(normalizedSession, existing),
    );
  },

  archiveSession: async (id) => {
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id
          ? { ...session, archivedAt: new Date().toISOString() }
          : session,
      ),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
    }));
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (session) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(session),
      );
      upsertSessionMetadataOverlayRecord(buildOverlayRecord(session, existing));
    }
  },

  unarchiveSession: async (id) => {
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id ? { ...session, archivedAt: undefined } : session,
      ),
    }));
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (session) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(session),
      );
      upsertSessionMetadataOverlayRecord(buildOverlayRecord(session, existing));
    }
  },

  setActiveSession: (sessionId) => {
    if (get().activeSessionId === sessionId) return;
    set({ activeSessionId: sessionId });
  },

  setContextPanelOpen: (sessionId, open) => {
    set((state) => ({
      contextPanelOpenBySession: {
        ...state.contextPanelOpenBySession,
        [sessionId]: open,
      },
    }));
  },

  setActiveWorkspace: (sessionId, context) => {
    set((state) => ({
      activeWorkspaceBySession: {
        ...state.activeWorkspaceBySession,
        [sessionId]: context,
      },
    }));
  },

  clearActiveWorkspace: (sessionId) => {
    set((state) => {
      const { [sessionId]: _, ...rest } = state.activeWorkspaceBySession;
      return { activeWorkspaceBySession: rest };
    });
  },

  switchSessionProvider: (sessionId, providerId) => {
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === sessionId
          ? {
              ...session,
              providerId,
              modelId: undefined,
              modelName: undefined,
              updatedAt: session.updatedAt,
            }
          : session,
      ),
    }));
    const session = get().sessions.find(
      (candidate) => candidate.id === sessionId,
    );
    if (session) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(session),
      );
      upsertSessionMetadataOverlayRecord(buildOverlayRecord(session, existing));
    }
  },

  getSession: (id) => get().sessions.find((session) => session.id === id),

  getActiveSession: () => {
    const { activeSessionId, sessions } = get();
    if (!activeSessionId) return null;
    return sessions.find((session) => session.id === activeSessionId) ?? null;
  },

  getArchivedSessions: () =>
    get().sessions.filter((session) => !!session.archivedAt),
}));
