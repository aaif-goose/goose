import * as acpApi from "./acpApi";
import { perfLog } from "@/shared/lib/perfLog";

interface PreparedSession {
  providerId: string;
  workingDir: string;
}

const prepared = new Map<string, PreparedSession>();

export async function prepareSession(
  sessionId: string,
  providerId: string,
  workingDir: string,
  _personaId?: string,
  _projectId?: string,
): Promise<string> {
  const sid = sessionId.slice(0, 8);

  const existing = prepared.get(sessionId);
  if (existing) {
    const tReuse = performance.now();
    let changed = false;
    if (existing.workingDir !== workingDir) {
      await acpApi.updateWorkingDir(sessionId, workingDir);
      existing.workingDir = workingDir;
      changed = true;
    }
    if (existing.providerId !== providerId) {
      const tProv = performance.now();
      await acpApi.setProvider(sessionId, providerId);
      perfLog(
        `[perf:prepare] ${sid} reuse setProvider(${providerId}) in ${(performance.now() - tProv).toFixed(1)}ms`,
      );
      existing.providerId = providerId;
      changed = true;
    }
    perfLog(
      `[perf:prepare] ${sid} reuse existing session (updates=${changed}) in ${(performance.now() - tReuse).toFixed(1)}ms`,
    );
    return sessionId;
  }

  const tLoad = performance.now();
  await acpApi.loadSession(sessionId, workingDir);
  perfLog(
    `[perf:prepare] ${sid} tracker loadSession ok in ${(performance.now() - tLoad).toFixed(1)}ms`,
  );

  const gooseSid = sessionId.slice(0, 8);
  const tProv = performance.now();
  await acpApi.setProvider(sessionId, providerId);
  perfLog(
    `[perf:prepare] ${sid} tracker setProvider(${providerId}) in ${(performance.now() - tProv).toFixed(1)}ms (goose_sid=${gooseSid})`,
  );

  const entry = { providerId, workingDir };
  prepared.set(sessionId, entry);

  return sessionId;
}

export function getGooseSessionId(sessionId: string): string | null {
  return prepared.has(sessionId) ? sessionId : null;
}

export function registerSession(
  sessionId: string,
  providerId: string,
  workingDir: string,
): () => void {
  const previousEntry = prepared.get(sessionId);
  const entry = { providerId, workingDir };

  prepared.set(sessionId, entry);

  return () => {
    prepared.delete(sessionId);
    if (previousEntry) {
      prepared.set(sessionId, previousEntry);
    }
  };
}

export function unregisterSession(sessionId: string): void {
  prepared.delete(sessionId);
}
