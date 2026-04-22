import type { ContentBlock } from "@agentclientprotocol/sdk";
import * as directAcp from "./acpApi";
import type { AcpSessionInfo } from "./acpApi";
import * as sessionTracker from "./acpSessionTracker";
import {
  getCatalogEntry,
  resolveAgentProviderCatalogId,
} from "@/features/providers/providerCatalog";
import {
  setActiveMessageId,
  clearActiveMessageId,
} from "./acpNotificationHandler";
import { searchSessionsViaExports } from "./sessionSearch";
import { perfLog } from "@/shared/lib/perfLog";

export interface AcpProvider {
  id: string;
  label: string;
}

export interface AcpSendMessageOptions {
  systemPrompt?: string;
  personaId?: string;
  personaName?: string;
  /** Image attachments as [base64Data, mimeType] pairs. */
  images?: [string, string][];
}

export interface AcpPrepareSessionOptions {
  personaId?: string;
  projectId?: string;
}

export interface AcpCreateSessionOptions extends AcpPrepareSessionOptions {
  modelId?: string | null;
}

/** Discover ACP providers installed on the system. */
export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  const providers = await directAcp.listProviders();
  const seen = new Set<string>();

  return providers
    .map((provider) => {
      const catalogId = resolveAgentProviderCatalogId(
        provider.id,
        provider.label,
      );
      if (!catalogId || seen.has(catalogId)) {
        return null;
      }
      seen.add(catalogId);
      return {
        id: catalogId,
        label: getCatalogEntry(catalogId)?.displayName ?? provider.label,
      };
    })
    .filter((provider): provider is AcpProvider => provider !== null);
}

/** Send a message to an ACP agent. Response streams via Tauri events. */
export async function acpSendMessage(
  sessionId: string,
  prompt: string,
  options: AcpSendMessageOptions = {},
): Promise<void> {
  const { systemPrompt, personaId, images } = options;
  const sid = sessionId.slice(0, 8);
  const tStart = performance.now();

  const gooseSessionId = sessionTracker.getGooseSessionId(sessionId, personaId);
  if (!gooseSessionId) {
    throw new Error("Session not prepared. Call acpPrepareSession first.");
  }

  const hasSystem = systemPrompt && systemPrompt.trim().length > 0;
  const effectivePrompt = hasSystem
    ? `<persona-instructions>\n${systemPrompt}\n</persona-instructions>\n\n<user-message>\n${prompt}\n</user-message>`
    : prompt;

  const content: ContentBlock[] = [{ type: "text", text: effectivePrompt }];
  if (images) {
    for (const [data, mimeType] of images) {
      content.push({ type: "image", data, mimeType } as ContentBlock);
    }
  }

  const messageId = crypto.randomUUID();
  setActiveMessageId(gooseSessionId, messageId);

  perfLog(
    `[perf:send] ${sid} acpSendMessage → prompt(len=${prompt.length}, imgs=${images?.length ?? 0})`,
  );
  const tPrompt = performance.now();
  await directAcp.prompt(gooseSessionId, content);
  const tDone = performance.now();
  perfLog(
    `[perf:send] ${sid} prompt() resolved in ${(tDone - tPrompt).toFixed(1)}ms (total acpSendMessage ${(tDone - tStart).toFixed(1)}ms)`,
  );

  clearActiveMessageId(gooseSessionId);
}

/** Prepare or warm an ACP session ahead of the first prompt. */
export async function acpPrepareSession(
  sessionId: string,
  providerId: string,
  workingDir: string,
  options: AcpPrepareSessionOptions = {},
): Promise<string> {
  const sid = sessionId.slice(0, 8);
  const t0 = performance.now();
  perfLog(
    `[perf:prepare] ${sid} acpPrepareSession start (provider=${providerId})`,
  );
  const gooseSessionId = await sessionTracker.prepareSession(
    sessionId,
    providerId,
    workingDir,
    options.personaId,
    options.projectId,
  );
  perfLog(
    `[perf:prepare] ${sid} acpPrepareSession done in ${(performance.now() - t0).toFixed(1)}ms`,
  );
  return gooseSessionId;
}

export async function acpCreateSession(
  providerId: string,
  workingDir: string,
  options: AcpCreateSessionOptions = {},
): Promise<{ sessionId: string }> {
  const localSessionId = crypto.randomUUID();
  const gooseSessionId = await acpPrepareSession(
    localSessionId,
    providerId,
    workingDir,
    options,
  );
  sessionTracker.registerSession(
    gooseSessionId,
    gooseSessionId,
    providerId,
    workingDir,
  );
  if (options.modelId) {
    await directAcp.setModel(gooseSessionId, options.modelId);
  }
  return { sessionId: gooseSessionId };
}

export async function acpSetModel(
  sessionId: string,
  modelId: string,
): Promise<void> {
  const gooseSessionId = sessionTracker.getGooseSessionId(sessionId);
  return directAcp.setModel(gooseSessionId ?? sessionId, modelId);
}

export type { AcpSessionInfo };

export interface AcpSessionSearchResult {
  sessionId: string;
  snippet: string;
  messageId: string;
  messageRole?: "user" | "assistant" | "system";
  matchCount: number;
}

/** List all sessions known to the goose binary. */
export async function acpListSessions(): Promise<AcpSessionInfo[]> {
  return directAcp.listSessions();
}

export async function acpSearchSessions(
  query: string,
  sessionIds: string[],
): Promise<AcpSessionSearchResult[]> {
  return searchSessionsViaExports(query, sessionIds);
}

/**
 * Load an existing session from the goose binary.
 *
 * This triggers message replay via SessionNotification events that the
 * notification handler picks up automatically.
 */
export async function acpLoadSession(
  sessionId: string,
  gooseSessionId: string,
  workingDir?: string,
): Promise<void> {
  const effectiveWorkingDir = workingDir ?? "~/.goose/artifacts";
  const sid = sessionId.slice(0, 8);
  const t0 = performance.now();
  const rollbackSessionRegistration = sessionTracker.registerSession(
    sessionId,
    gooseSessionId,
    "goose",
    effectiveWorkingDir,
  );
  try {
    perfLog(`[perf:load] ${sid} acpLoadSession → client.loadSession`);
    await directAcp.loadSession(gooseSessionId, effectiveWorkingDir);
    perfLog(
      `[perf:load] ${sid} client.loadSession resolved in ${(performance.now() - t0).toFixed(1)}ms`,
    );
  } catch (error) {
    rollbackSessionRegistration();
    throw error;
  }
}

/** Export a session as JSON via the goose binary. */
export async function acpExportSession(sessionId: string): Promise<string> {
  return directAcp.exportSession(sessionId);
}

/** Import a session from JSON via the goose binary. Returns new session metadata. */
export async function acpImportSession(json: string): Promise<AcpSessionInfo> {
  return directAcp.importSession(json);
}

/** Duplicate (fork) a session via the goose binary. Returns new session metadata. */
export async function acpDuplicateSession(
  sessionId: string,
): Promise<AcpSessionInfo> {
  const gooseSessionId =
    sessionTracker.getGooseSessionId(sessionId) ?? sessionId;
  return directAcp.forkSession(gooseSessionId);
}

/** Cancel an in-progress ACP session so the backend stops streaming. */
export async function acpCancelSession(
  sessionId: string,
  personaId?: string,
): Promise<boolean> {
  const gooseSessionId = sessionTracker.getGooseSessionId(sessionId, personaId);
  await directAcp.cancelSession(gooseSessionId ?? sessionId);
  return true;
}
