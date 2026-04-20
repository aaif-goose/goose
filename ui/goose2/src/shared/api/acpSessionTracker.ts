import * as acpApi from "./acpApi";
import { perfLog } from "@/shared/lib/perfLog";

interface PreparedSession {
  gooseSessionId: string;
  providerId: string;
  workingDir: string;
}

type SessionRegistrationListener = (
  localSessionId: string,
  gooseSessionId: string,
) => void;

const prepared = new Map<string, PreparedSession>();
const gooseToLocal = new Map<string, string>();
const registrationListeners = new Set<SessionRegistrationListener>();

function restoreGooseRegistration(
  gooseSessionId: string,
  localSessionId: string | undefined,
): void {
  if (localSessionId === undefined) {
    gooseToLocal.delete(gooseSessionId);
    return;
  }

  gooseToLocal.set(gooseSessionId, localSessionId);
}

function makeKey(sessionId: string, personaId?: string): string {
  if (personaId && personaId.length > 0) {
    return `${sessionId}__${personaId}`;
  }
  return sessionId;
}

function notifySessionRegistered(
  localSessionId: string,
  gooseSessionId: string,
): void {
  for (const listener of registrationListeners) {
    listener(localSessionId, gooseSessionId);
  }
}

export function subscribeToSessionRegistration(
  listener: SessionRegistrationListener,
): () => void {
  registrationListeners.add(listener);
  return () => registrationListeners.delete(listener);
}

export async function prepareSession(
  sessionId: string,
  providerId: string,
  workingDir: string,
  personaId?: string,
): Promise<string> {
  const sid = sessionId.slice(0, 8);
  const key = makeKey(sessionId, personaId);

  const existing = prepared.get(key) ?? prepared.get(sessionId);
  if (existing) {
    const tReuse = performance.now();
    let changed = false;
    if (existing.workingDir !== workingDir) {
      await acpApi.updateWorkingDir(existing.gooseSessionId, workingDir);
      existing.workingDir = workingDir;
      changed = true;
    }
    if (existing.providerId !== providerId) {
      const tProv = performance.now();
      await acpApi.setProvider(existing.gooseSessionId, providerId);
      perfLog(
        `[perf:prepare] ${sid} reuse setProvider(${providerId}) in ${(performance.now() - tProv).toFixed(1)}ms (goose_sid=${existing.gooseSessionId.slice(0, 8)})`,
      );
      existing.providerId = providerId;
      changed = true;
    }
    perfLog(
      `[perf:prepare] ${sid} reuse existing session (updates=${changed}) in ${(performance.now() - tReuse).toFixed(1)}ms`,
    );
    return existing.gooseSessionId;
  }

  let gooseSessionId: string | null = null;

  const tLoad = performance.now();
  try {
    await acpApi.loadSession(sessionId, workingDir);
    gooseSessionId = sessionId;
    perfLog(
      `[perf:prepare] ${sid} tracker loadSession ok in ${(performance.now() - tLoad).toFixed(1)}ms`,
    );
  } catch {
    perfLog(
      `[perf:prepare] ${sid} tracker loadSession failed in ${(performance.now() - tLoad).toFixed(1)}ms → newSession`,
    );
  }

  if (!gooseSessionId) {
    const tNew = performance.now();
    const response = await acpApi.newSession(workingDir, providerId);
    gooseSessionId = response.sessionId;
    perfLog(
      `[perf:prepare] ${sid} tracker newSession done in ${(performance.now() - tNew).toFixed(1)}ms (goose_sid=${gooseSessionId.slice(0, 8)})`,
    );
  }

  const gooseSid = gooseSessionId.slice(0, 8);
  const tProv = performance.now();
  await acpApi.setProvider(gooseSessionId, providerId);
  perfLog(
    `[perf:prepare] ${sid} tracker setProvider(${providerId}) in ${(performance.now() - tProv).toFixed(1)}ms (goose_sid=${gooseSid})`,
  );

  prepared.set(key, { gooseSessionId, providerId, workingDir });
  prepared.set(sessionId, { gooseSessionId, providerId, workingDir });
  gooseToLocal.set(gooseSessionId, sessionId);
  notifySessionRegistered(sessionId, gooseSessionId);

  return gooseSessionId;
}

export function getGooseSessionId(
  sessionId: string,
  personaId?: string,
): string | null {
  const key = makeKey(sessionId, personaId);
  return (
    prepared.get(key)?.gooseSessionId ??
    prepared.get(sessionId)?.gooseSessionId ??
    null
  );
}

export function getLocalSessionId(gooseSessionId: string): string | null {
  return gooseToLocal.get(gooseSessionId) ?? null;
}

export function registerSession(
  sessionId: string,
  gooseSessionId: string,
  providerId: string,
  workingDir: string,
): () => void {
  const previousEntry = prepared.get(sessionId);
  const previousGooseSessionLocal = gooseToLocal.get(gooseSessionId);
  const previousSessionGooseLocal = previousEntry
    ? gooseToLocal.get(previousEntry.gooseSessionId)
    : undefined;
  const entry = { gooseSessionId, providerId, workingDir };

  if (
    previousEntry &&
    previousEntry.gooseSessionId !== gooseSessionId &&
    gooseToLocal.get(previousEntry.gooseSessionId) === sessionId
  ) {
    gooseToLocal.delete(previousEntry.gooseSessionId);
  }

  prepared.set(sessionId, entry);
  gooseToLocal.set(gooseSessionId, sessionId);
  notifySessionRegistered(sessionId, gooseSessionId);

  return () => {
    prepared.delete(sessionId);
    if (previousEntry) {
      prepared.set(sessionId, previousEntry);
    }

    restoreGooseRegistration(gooseSessionId, previousGooseSessionLocal);
    if (previousEntry && previousEntry.gooseSessionId !== gooseSessionId) {
      restoreGooseRegistration(
        previousEntry.gooseSessionId,
        previousSessionGooseLocal,
      );
    }
  };
}

export function unregisterSession(
  sessionId: string,
  gooseSessionId?: string,
): void {
  const entry = prepared.get(sessionId);
  prepared.delete(sessionId);

  const resolvedGooseSessionId = gooseSessionId ?? entry?.gooseSessionId;
  if (
    resolvedGooseSessionId &&
    gooseToLocal.get(resolvedGooseSessionId) === sessionId
  ) {
    gooseToLocal.delete(resolvedGooseSessionId);
  }
}
