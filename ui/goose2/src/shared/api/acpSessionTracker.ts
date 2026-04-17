import * as acpApi from "./acpApi";
import { perfLog } from "@/shared/lib/perfLog";

interface PreparedSession {
  gooseSessionId: string;
  providerId: string;
  workingDir: string;
}

const prepared = new Map<string, PreparedSession>();
const gooseToLocal = new Map<string, string>();

function makeKey(sessionId: string, personaId?: string): string {
  if (personaId && personaId.length > 0) {
    return `${sessionId}__${personaId}`;
  }
  return sessionId;
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
    const response = await acpApi.newSession(workingDir);
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
): void {
  const entry = { gooseSessionId, providerId, workingDir };
  prepared.set(sessionId, entry);
  gooseToLocal.set(gooseSessionId, sessionId);
}
