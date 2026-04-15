import { create } from "zustand";
import { acpListSessions, type AcpSessionInfo } from "@/shared/api/acp";
import type { Session } from "@/shared/types/chat";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import {
  loadDraftSessionRecords,
  loadSessionMetadataOverlay,
  migrateSessionMetadataOverlayId,
  modelIdsMatch,
  persistSessionMetadataOverlay,
  persistDraftSessionRecord,
  persistDraftSessionRecords,
  removeDraftSessionRecord,
  upsertSessionMetadataOverlayRecord,
  type DraftSessionRecord,
  type SessionMetadataOverlayRecord,
} from "@/features/chat/lib/sessionMetadataOverlay";
import type { ModelOption } from "../types";

const EMPTY_MODELS: ModelOption[] = [];

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
  draft?: boolean;
  userSetName?: boolean;
}

export interface WorkingContext {
  path: string;
  branch: string | null;
}

interface ChatSessionStoreState {
  sessions: ChatSession[];
  activeSessionId: string | null;
  isLoading: boolean;
  contextPanelOpenBySession: Record<string, boolean>;
  activeWorkingContextBySession: Record<string, WorkingContext>;
  modelsBySession: Record<string, ModelOption[]>;
  modelCacheByProvider: Record<string, ModelOption[]>;
}

interface CreateSessionOpts {
  title?: string;
  projectId?: string;
  agentId?: string;
  providerId?: string;
  personaId?: string;
}

interface UpdateSessionOptions {
  localOnly?: boolean;
  persistOverlay?: boolean;
}

interface ChatSessionStoreActions {
  createSession: (opts?: CreateSessionOpts) => Promise<ChatSession>;
  createDraftSession: (opts?: CreateSessionOpts) => ChatSession;
  promoteDraft: (id: string) => void;
  removeDraft: (id: string) => void;
  loadSessions: () => Promise<void>;
  updateSession: (
    id: string,
    patch: Partial<ChatSession>,
    opts?: UpdateSessionOptions,
  ) => void;
  addSession: (session: ChatSession) => void;
  archiveSession: (id: string) => Promise<void>;
  unarchiveSession: (id: string) => Promise<void>;
  setSessionAcpId: (id: string, acpSessionId: string) => void;

  setActiveSession: (sessionId: string | null) => void;
  setContextPanelOpen: (sessionId: string, open: boolean) => void;
  setActiveWorkingContext: (sessionId: string, context: WorkingContext) => void;
  clearActiveWorkingContext: (sessionId: string) => void;
  setSessionModels: (sessionId: string, models: ModelOption[]) => void;
  switchSessionProvider: (
    sessionId: string,
    providerId: string,
    models: ModelOption[],
  ) => void;
  cacheModelsForProvider: (providerId: string, models: ModelOption[]) => void;
  getCachedModels: (providerId: string) => ModelOption[];

  getSession: (id: string) => ChatSession | undefined;
  getActiveSession: () => ChatSession | null;
  getArchivedSessions: () => ChatSession[];
  getSessionModels: (sessionId: string) => ModelOption[];
}

export type ChatSessionStore = ChatSessionStoreState & ChatSessionStoreActions;

const MODEL_CACHE_STORAGE_KEY = "goose:model-cache";

function loadModelCache(): Record<string, ModelOption[]> {
  if (typeof window === "undefined") return {};
  try {
    const stored = window.localStorage.getItem(MODEL_CACHE_STORAGE_KEY);
    if (!stored) return {};
    const parsed = JSON.parse(stored);
    return typeof parsed === "object" && parsed !== null
      ? (parsed as Record<string, ModelOption[]>)
      : {};
  } catch {
    return {};
  }
}

function persistModelCache(cache: Record<string, ModelOption[]>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(MODEL_CACHE_STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // localStorage may be unavailable
  }
}

function draftSessionToRecord(session: ChatSession): DraftSessionRecord {
  return {
    id: session.id,
    acpSessionId: session.acpSessionId,
    title: session.title,
    projectId: session.projectId,
    agentId: session.agentId,
    providerId: session.providerId,
    personaId: session.personaId,
    modelId: session.modelId,
    modelName: session.modelName,
    createdAt: session.createdAt,
    updatedAt: session.updatedAt,
    archivedAt: session.archivedAt,
    messageCount: session.messageCount,
    draft: true,
    userSetName: session.userSetName,
  };
}

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
    projectId: session.projectId ?? null,
    userSetTitle: session.userSetName ? session.title : null,
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
    projectId: overlay.projectId,
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
    projectId: overlay?.projectId,
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

function mergeDraftSessions(
  currentDrafts: ChatSession[],
  persistedDrafts: DraftSessionRecord[],
): ChatSession[] {
  const draftsById = new Map<string, ChatSession>(
    persistedDrafts.map((record) => [
      record.id,
      {
        ...record,
        draft: true,
      } satisfies ChatSession,
    ]),
  );

  for (const draft of currentDrafts) {
    draftsById.set(draft.id, draft);
  }

  return [...draftsById.values()];
}

function persistDraftsFromSessions(sessions: ChatSession[]): void {
  persistDraftSessionRecords(
    sessions.filter((session) => session.draft).map(draftSessionToRecord),
  );
}

function syncOverlaySnapshots(
  sessions: ChatSession[],
  existingOverlays = loadSessionMetadataOverlay(),
): void {
  const overlays = new Map(existingOverlays);
  for (const session of sessions) {
    if (session.draft) continue;
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
  contextPanelOpenBySession: {},
  activeWorkingContextBySession: {},
  modelsBySession: {},
  modelCacheByProvider: loadModelCache(),

  createSession: async (_opts) => {
    throw new Error(
      "createSession not yet wired to ACP — use createDraftSession",
    );
  },

  createDraftSession: (opts) => {
    const now = new Date().toISOString();
    const chatSession: ChatSession = {
      id: crypto.randomUUID(),
      title: opts?.title ?? DEFAULT_CHAT_TITLE,
      projectId: opts?.projectId,
      agentId: opts?.agentId,
      providerId: opts?.providerId,
      personaId: opts?.personaId,
      createdAt: now,
      updatedAt: now,
      messageCount: 0,
      draft: true,
    };
    set((state) => ({ sessions: [...state.sessions, chatSession] }));
    persistDraftSessionRecord(draftSessionToRecord(chatSession));
    return chatSession;
  },

  promoteDraft: (id) => {
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (!session?.draft) return;
    set((state) => ({
      sessions: state.sessions.map((candidate) =>
        candidate.id === id ? { ...candidate, draft: undefined } : candidate,
      ),
    }));
    persistDraftsFromSessions(get().sessions);
  },

  removeDraft: (id) => {
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (!session?.draft) return;
    const { [id]: _ignoredPanelState, ...remainingPanelState } =
      get().contextPanelOpenBySession;
    const { [id]: _ignoredContext, ...remainingContextState } =
      get().activeWorkingContextBySession;
    const remainingModels = { ...get().modelsBySession };
    delete remainingModels[id];
    set((state) => ({
      sessions: state.sessions.filter((candidate) => candidate.id !== id),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
      contextPanelOpenBySession: remainingPanelState,
      activeWorkingContextBySession: remainingContextState,
      modelsBySession: remainingModels,
    }));
    removeDraftSessionRecord(id);
  },

  loadSessions: async () => {
    set({ isLoading: true });
    try {
      const overlays = loadSessionMetadataOverlay();
      const acpSessions = await acpListSessions();
      const persistedDrafts = loadDraftSessionRecords();
      const currentDrafts = get().sessions.filter((session) => session.draft);
      const drafts = mergeDraftSessions(currentDrafts, persistedDrafts);
      const mergedAcpSessions = sortByUpdatedAtDesc(
        acpSessions.map((session) =>
          mergeAcpSessionWithOverlay(session, overlays.get(session.sessionId)),
        ),
      );
      const merged = [...mergedAcpSessions, ...drafts];
      const activeSessionId = get().activeSessionId;
      const activeSessionStillExists =
        activeSessionId == null ||
        merged.some((session) => session.id === activeSessionId);
      set({
        sessions: merged,
        activeSessionId: activeSessionStillExists ? activeSessionId : null,
      });
      persistDraftsFromSessions(merged);
      syncOverlaySnapshots(mergedAcpSessions, overlays);
    } catch (error) {
      console.error("Failed to load sessions from ACP:", error);
      const overlays = loadSessionMetadataOverlay();
      const persistedDrafts = loadDraftSessionRecords();
      const currentDrafts = get().sessions.filter((session) => session.draft);
      const drafts = mergeDraftSessions(currentDrafts, persistedDrafts);
      const fallbackSessions = sortByUpdatedAtDesc([
        ...[...overlays.values()].map(overlayToFallbackSession),
        ...drafts,
      ]);
      set({ sessions: fallbackSessions });
      persistDraftsFromSessions(fallbackSessions);
    } finally {
      set({ isLoading: false });
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
    persistDraftsFromSessions(get().sessions);

    if (
      updatedSession &&
      !updatedSession.draft &&
      opts?.persistOverlay !== false
    ) {
      const key = overlayKeyForSession(updatedSession);
      const existing = loadSessionMetadataOverlay().get(key);
      upsertSessionMetadataOverlayRecord(
        buildOverlayRecord(updatedSession, existing),
      );
    }

    if (opts?.localOnly) return;
    if (updatedSession?.draft) return;
    // TODO: wire non-draft updates to ACP when supported
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
    persistDraftsFromSessions(get().sessions);
    if (!normalizedSession.draft) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(normalizedSession),
      );
      upsertSessionMetadataOverlayRecord(
        buildOverlayRecord(normalizedSession, existing),
      );
    }
  },

  archiveSession: async (id) => {
    const remainingModels = { ...get().modelsBySession };
    delete remainingModels[id];
    set((state) => ({
      sessions: state.sessions.map((session) =>
        session.id === id
          ? { ...session, archivedAt: new Date().toISOString() }
          : session,
      ),
      activeSessionId:
        state.activeSessionId === id ? null : state.activeSessionId,
      modelsBySession: remainingModels,
    }));
    persistDraftsFromSessions(get().sessions);
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (session && !session.draft) {
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
    persistDraftsFromSessions(get().sessions);
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (session && !session.draft) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(session),
      );
      upsertSessionMetadataOverlayRecord(buildOverlayRecord(session, existing));
    }
  },

  setSessionAcpId: (id, acpSessionId) => {
    if (!acpSessionId) return;
    const session = get().sessions.find((candidate) => candidate.id === id);
    if (!session || session.acpSessionId === acpSessionId) return;

    set((state) => ({
      sessions: state.sessions.map((candidate) =>
        candidate.id === id ? { ...candidate, acpSessionId } : candidate,
      ),
    }));

    if (!session.draft) {
      migrateSessionMetadataOverlayId(
        overlayKeyForSession(session),
        acpSessionId,
      );
      const updatedSession = get().sessions.find(
        (candidate) => candidate.id === id,
      );
      if (updatedSession) {
        const existing = loadSessionMetadataOverlay().get(acpSessionId);
        upsertSessionMetadataOverlayRecord(
          buildOverlayRecord(updatedSession, existing),
        );
      }
    } else {
      persistDraftsFromSessions(get().sessions);
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

  setActiveWorkingContext: (sessionId, context) => {
    set((state) => ({
      activeWorkingContextBySession: {
        ...state.activeWorkingContextBySession,
        [sessionId]: context,
      },
    }));
  },

  clearActiveWorkingContext: (sessionId) => {
    set((state) => {
      const { [sessionId]: _, ...rest } = state.activeWorkingContextBySession;
      return { activeWorkingContextBySession: rest };
    });
  },

  setSessionModels: (sessionId, models) => {
    set((state) => ({
      modelsBySession: {
        ...state.modelsBySession,
        [sessionId]: models,
      },
    }));
  },

  switchSessionProvider: (sessionId, providerId, models) => {
    set((state) => ({
      modelsBySession: {
        ...state.modelsBySession,
        [sessionId]: models,
      },
      sessions: state.sessions.map((session) =>
        session.id === sessionId
          ? {
              ...session,
              providerId,
              modelId: models.length > 0 ? models[0].id : undefined,
              modelName:
                models.length > 0
                  ? (models[0].displayName ?? models[0].name)
                  : undefined,
              updatedAt: session.updatedAt,
            }
          : session,
      ),
    }));
    persistDraftsFromSessions(get().sessions);
    const session = get().sessions.find(
      (candidate) => candidate.id === sessionId,
    );
    if (session && !session.draft) {
      const existing = loadSessionMetadataOverlay().get(
        overlayKeyForSession(session),
      );
      upsertSessionMetadataOverlayRecord(buildOverlayRecord(session, existing));
    }
  },

  cacheModelsForProvider: (providerId, models) => {
    if (models.length === 0) return;
    const existing = get().modelCacheByProvider[providerId];
    if (modelIdsMatch(existing, models)) {
      return;
    }
    set((state) => {
      const updated = {
        ...state.modelCacheByProvider,
        [providerId]: models,
      };
      persistModelCache(updated);
      return { modelCacheByProvider: updated };
    });
  },

  getCachedModels: (providerId) =>
    get().modelCacheByProvider[providerId] ?? EMPTY_MODELS,

  getSession: (id) => get().sessions.find((session) => session.id === id),

  getActiveSession: () => {
    const { activeSessionId, sessions } = get();
    if (!activeSessionId) return null;
    return sessions.find((session) => session.id === activeSessionId) ?? null;
  },

  getArchivedSessions: () =>
    get().sessions.filter((session) => !!session.archivedAt),

  getSessionModels: (sessionId) =>
    get().modelsBySession[sessionId] ?? EMPTY_MODELS,
}));
