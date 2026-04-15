import * as acpApi from "./acpApi";

interface PreparedSession {
  gooseSessionId: string;
  providerId: string;
  workingDir: string;
}

const prepared = new Map<string, PreparedSession>();

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
    return existing.gooseSessionId;
  }

  let gooseSessionId: string | null = null;

  try {
    await acpApi.loadSession(sessionId, workingDir);
    gooseSessionId = sessionId;
  } catch {
    // session doesn't exist — create new
  }

  if (!gooseSessionId) {
    const response = await acpApi.newSession(workingDir);
    gooseSessionId = response.sessionId;
  }

  prepared.set(key, { gooseSessionId, providerId, workingDir });
  prepared.set(sessionId, { gooseSessionId, providerId, workingDir });

  return gooseSessionId;
}

export function getGooseSessionId(sessionId: string, personaId?: string): string | null {
  const key = makeKey(sessionId, personaId);
  return prepared.get(key)?.gooseSessionId ?? prepared.get(sessionId)?.gooseSessionId ?? null;
}

