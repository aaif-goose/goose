import type { ModelOption } from "../types";

const ACP_SESSION_METADATA_STORAGE_KEY = "goose:acp-session-metadata";
const DRAFT_SESSION_STORAGE_KEY = "goose:chat-draft-sessions";
const LEGACY_SESSION_CACHE_STORAGE_KEY = "goose:chat-sessions";
const DRAFT_TEXT_STORAGE_KEY = "goose:chat-drafts";

export interface SessionMetadataOverlayRecord {
  sessionId: string;
  projectId?: string | null;
  userSetTitle?: string | null;
  providerId?: string | null;
  personaId?: string | null;
  modelId?: string | null;
  modelName?: string | null;
  archivedAt?: string | null;
  createdAt?: string | null;
  agentId?: string | null;
  lastKnownTitle?: string | null;
  lastKnownUpdatedAt?: string | null;
  lastKnownMessageCount?: number | null;
  updatedAt: string;
}

export interface DraftSessionRecord {
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
  draft?: true;
  userSetName?: boolean;
}

function parseStorageArray<T>(storageKey: string): T[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = window.localStorage.getItem(storageKey);
    if (!stored) return [];
    const parsed = JSON.parse(stored);
    return Array.isArray(parsed) ? (parsed as T[]) : [];
  } catch {
    return [];
  }
}

function persistStorageArray<T>(storageKey: string, records: T[]): void {
  if (typeof window === "undefined") return;
  try {
    if (records.length === 0) {
      window.localStorage.removeItem(storageKey);
      return;
    }
    window.localStorage.setItem(storageKey, JSON.stringify(records));
  } catch {
    // localStorage may be unavailable
  }
}

function draftsWithText(): Set<string> {
  if (typeof window === "undefined") return new Set();
  try {
    const stored = window.localStorage.getItem(DRAFT_TEXT_STORAGE_KEY);
    if (!stored) return new Set();
    const parsed = JSON.parse(stored);
    return new Set(
      Object.entries(parsed)
        .filter(([, value]) => typeof value === "string" && value.length > 0)
        .map(([sessionId]) => sessionId),
    );
  } catch {
    return new Set();
  }
}

function loadLegacySessions(): Array<
  DraftSessionRecord & {
    userSetName?: boolean;
  }
> {
  return parseStorageArray(LEGACY_SESSION_CACHE_STORAGE_KEY);
}

function recordFromLegacySession(
  session: DraftSessionRecord & { userSetName?: boolean },
): SessionMetadataOverlayRecord {
  return {
    sessionId: session.acpSessionId ?? session.id,
    projectId: session.projectId,
    userSetTitle: session.userSetName ? session.title : null,
    providerId: session.providerId,
    personaId: session.personaId,
    modelId: session.modelId,
    modelName: session.modelName,
    archivedAt: session.archivedAt ?? null,
    createdAt: session.createdAt,
    agentId: session.agentId,
    lastKnownTitle: session.title,
    lastKnownUpdatedAt: session.updatedAt,
    lastKnownMessageCount: session.messageCount,
    updatedAt: session.updatedAt,
  };
}

export function loadSessionMetadataOverlay(): Map<
  string,
  SessionMetadataOverlayRecord
> {
  const records = parseStorageArray<SessionMetadataOverlayRecord>(
    ACP_SESSION_METADATA_STORAGE_KEY,
  );
  const overlays = new Map(records.map((record) => [record.sessionId, record]));

  for (const session of loadLegacySessions()) {
    if (session.draft) continue;
    const key = session.acpSessionId ?? session.id;
    if (overlays.has(key)) continue;
    overlays.set(key, recordFromLegacySession(session));
  }

  return overlays;
}

export function persistSessionMetadataOverlay(
  records: Iterable<SessionMetadataOverlayRecord>,
): void {
  persistStorageArray(
    ACP_SESSION_METADATA_STORAGE_KEY,
    [...records].sort((a, b) => a.sessionId.localeCompare(b.sessionId)),
  );
}

export function loadDraftSessionRecords(): DraftSessionRecord[] {
  const records = parseStorageArray<DraftSessionRecord>(
    DRAFT_SESSION_STORAGE_KEY,
  );
  const drafts = new Map(records.map((record) => [record.id, record]));

  for (const session of loadLegacySessions()) {
    if (!session.draft) continue;
    if (drafts.has(session.id)) continue;
    drafts.set(session.id, session);
  }

  return [...drafts.values()];
}

export function persistDraftSessionRecords(
  records: DraftSessionRecord[],
): void {
  const withText = draftsWithText();
  persistStorageArray(
    DRAFT_SESSION_STORAGE_KEY,
    records.filter((record) => withText.has(record.id)),
  );
}

export function migrateSessionMetadataOverlayId(
  previousId: string,
  nextId: string,
): void {
  if (!previousId || !nextId || previousId === nextId) return;
  const overlays = loadSessionMetadataOverlay();
  const previous = overlays.get(previousId);
  if (!previous) return;
  const existing = overlays.get(nextId);
  overlays.set(nextId, {
    ...previous,
    ...existing,
    sessionId: nextId,
  });
  overlays.delete(previousId);
  persistSessionMetadataOverlay(overlays.values());
}

export function upsertSessionMetadataOverlayRecord(
  record: SessionMetadataOverlayRecord,
): void {
  const overlays = loadSessionMetadataOverlay();
  overlays.set(record.sessionId, record);
  persistSessionMetadataOverlay(overlays.values());
}

export function removeSessionMetadataOverlayRecord(sessionId: string): void {
  const overlays = loadSessionMetadataOverlay();
  if (!overlays.delete(sessionId)) return;
  persistSessionMetadataOverlay(overlays.values());
}

export function persistDraftSessionRecord(record: DraftSessionRecord): void {
  const drafts = loadDraftSessionRecords();
  const nextDrafts = [
    ...drafts.filter((draft) => draft.id !== record.id),
    record,
  ];
  persistDraftSessionRecords(nextDrafts);
}

export function removeDraftSessionRecord(sessionId: string): void {
  persistDraftSessionRecords(
    loadDraftSessionRecords().filter((record) => record.id !== sessionId),
  );
}

export function modelIdsMatch(
  cached: ModelOption[] | undefined,
  next: ModelOption[],
): boolean {
  return Boolean(
    cached &&
      cached.length === next.length &&
      cached.every((model, index) => model.id === next[index]?.id),
  );
}
