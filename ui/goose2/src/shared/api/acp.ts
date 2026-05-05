import type { ContentBlock } from "@agentclientprotocol/sdk";
import * as directAcp from "./acpApi";
import type { AcpSessionInfo } from "./acpApi";
import * as sessionRegistry from "./acpSessionRegistry";
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
  assistantPrompt?: string;
  personaId?: string;
  personaName?: string;
  /** Image attachments as [base64Data, mimeType] pairs. */
  images?: [string, string][];
}

export interface AcpCreateSessionOptions {
  personaId?: string;
  projectId?: string;
  modelId?: string | null;
}

/** Discover ACP providers installed on the system. */
export async function discoverAcpProviders(): Promise<AcpProvider[]> {
  const providers = await directAcp.listProviders();
  return resolveProvidersCatalog(providers);
}

/**
 * Derive ACP providers from already-fetched inventory entries,
 * avoiding a duplicate `_goose/providers/list` RPC.
 */
export function discoverAcpProvidersFromEntries(
  entries: Parameters<typeof directAcp.buildProviderListFromEntries>[0],
): AcpProvider[] {
  return resolveProvidersCatalog(
    directAcp.buildProviderListFromEntries(entries),
  );
}

function resolveProvidersCatalog(providers: AcpProvider[]): AcpProvider[] {
  const seen = new Set<string>();

  return providers
    .map((provider) => {
      const catalogId = resolveAgentProviderCatalogId(
        provider.id,
        provider.label,
      );
      const resolvedId = catalogId ?? provider.id;
      if (seen.has(resolvedId)) {
        return null;
      }
      seen.add(resolvedId);
      return {
        id: resolvedId,
        label: getCatalogEntry(resolvedId)?.displayName ?? provider.label,
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
  const { systemPrompt, assistantPrompt, personaId, images } = options;
  const sid = sessionId.slice(0, 8);
  const tStart = performance.now();

  if (!sessionRegistry.isSessionPrepared(sessionId)) {
    throw new Error("Session not prepared. Call acpPrepareSession first.");
  }

  const content: ContentBlock[] = [];
  if (systemPrompt?.trim()) {
    content.push({
      type: "text",
      text: systemPrompt,
      annotations: { audience: ["assistant"] },
    });
  }
  if (assistantPrompt?.trim()) {
    content.push({
      type: "text",
      text: assistantPrompt,
      annotations: { audience: ["assistant"] },
    });
  }
  content.push({ type: "text", text: prompt });
  if (images) {
    for (const [data, mimeType] of images) {
      content.push({ type: "image", data, mimeType } as ContentBlock);
    }
  }

  const messageId = crypto.randomUUID();
  setActiveMessageId(sessionId, messageId);

  perfLog(
    `[perf:send] ${sid} acpSendMessage → prompt(len=${prompt.length}, imgs=${images?.length ?? 0})`,
  );
  const tPrompt = performance.now();
  const meta: Record<string, unknown> = {};
  if (personaId) meta.personaId = personaId;
  try {
    await directAcp.prompt(
      sessionId,
      content,
      Object.keys(meta).length > 0 ? meta : undefined,
    );
    const tDone = performance.now();
    perfLog(
      `[perf:send] ${sid} prompt() resolved in ${(tDone - tPrompt).toFixed(1)}ms (total acpSendMessage ${(tDone - tStart).toFixed(1)}ms)`,
    );
  } finally {
    clearActiveMessageId(sessionId);
  }
}

/** Prepare or warm an ACP session ahead of the first prompt. */
export async function acpPrepareSession(
  sessionId: string,
  providerId: string,
  workingDir: string,
): Promise<void> {
  const sid = sessionId.slice(0, 8);
  const t0 = performance.now();
  perfLog(
    `[perf:prepare] ${sid} acpPrepareSession start (provider=${providerId})`,
  );
  await sessionRegistry.prepareSession(sessionId, providerId, workingDir);
  perfLog(
    `[perf:prepare] ${sid} acpPrepareSession done in ${(performance.now() - t0).toFixed(1)}ms`,
  );
}

export async function acpCreateSession(
  providerId: string,
  workingDir: string,
  options: AcpCreateSessionOptions = {},
): Promise<{ sessionId: string }> {
  const response = await directAcp.newSession(
    workingDir,
    providerId,
    options.projectId,
    options.personaId,
  );
  const sessionId = response.sessionId;
  await directAcp.setProvider(sessionId, providerId);
  sessionRegistry.registerPreparedSession(sessionId, providerId, workingDir);
  if (options.modelId) {
    await directAcp.setModel(sessionId, options.modelId);
  }
  return { sessionId };
}

export async function acpSetModel(
  sessionId: string,
  modelId: string,
): Promise<void> {
  return directAcp.setModel(sessionId, modelId);
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
  workingDir?: string,
): Promise<void> {
  const effectiveWorkingDir = workingDir ?? "~";
  const sid = sessionId.slice(0, 8);
  const t0 = performance.now();
  const rollbackSessionRegistration = sessionRegistry.registerPreparedSession(
    sessionId,
    "goose",
    effectiveWorkingDir,
  );
  try {
    perfLog(`[perf:load] ${sid} acpLoadSession → client.loadSession`);
    await directAcp.loadSession(sessionId, effectiveWorkingDir);
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
  return directAcp.forkSession(sessionId);
}

/** Cancel an in-progress ACP session so the backend stops streaming. */
export async function acpCancelSession(sessionId: string): Promise<boolean> {
  await directAcp.cancelSession(sessionId);
  return true;
}
