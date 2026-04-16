import * as acpApi from "./acpApi";

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
  const key = makeKey(sessionId, personaId);

  const existing = prepared.get(key) ?? prepared.get(sessionId);
  if (existing) {
    if (existing.workingDir !== workingDir) {
      await acpApi.updateWorkingDir(existing.gooseSessionId, workingDir);
      existing.workingDir = workingDir;
    }
    if (existing.providerId !== providerId) {
      await acpApi.setProvider(existing.gooseSessionId, providerId);
      existing.providerId = providerId;
    }
    return existing.gooseSessionId;
  }

  let gooseSessionId: string | null = null;

  try {
    await acpApi.loadSession(sessionId, workingDir);
    gooseSessionId = sessionId;
  } catch {}

  if (!gooseSessionId) {
    const response = await acpApi.newSession(workingDir);
    gooseSessionId = response.sessionId;
  }

  await acpApi.setProvider(gooseSessionId, providerId);

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
