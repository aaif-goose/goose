import type {
  ContentBlock,
  NewSessionResponse,
  LoadSessionResponse,
  PromptResponse,
  SessionInfo,
} from "@agentclientprotocol/sdk";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { getClient } from "./acpConnection";
import { perfLog } from "@/shared/lib/perfLog";

export interface AcpProvider {
  id: string;
  label: string;
}

export interface AcpSessionInfo {
  sessionId: string;
  title: string | null;
  updatedAt: string | null;
  createdAt: string | null;
  archivedAt: string | null;
  userSetName: boolean;
  messageCount: number;
  workingDir: string | null;
  projectId?: string | null;
  providerId: string | null;
  modelId: string | null;
  personaId: string | null;
}

export const DEPRECATED_PROVIDER_IDS = new Set([
  "claude-code",
  "codex",
  "gemini-cli",
]);
export const DEFAULT_PROVIDER: AcpProvider = {
  id: "goose",
  label: "Goose (Default)",
};

/**
 * Build the ACP provider list from raw inventory entries.
 *
 * Shared by both `listProviders` (which fetches entries via RPC) and
 * `discoverAcpProvidersFromEntries` in acp.ts (which reuses
 * already-fetched entries at startup).
 */
export function buildProviderListFromEntries(
  entries: Array<
    Pick<ProviderInventoryEntryDto, "providerId" | "providerName" | "category">
  >,
): AcpProvider[] {
  return [
    DEFAULT_PROVIDER,
    ...entries
      .filter((entry) => !DEPRECATED_PROVIDER_IDS.has(entry.providerId))
      .filter((entry) => entry.category === "agent")
      .map((entry) => ({ id: entry.providerId, label: entry.providerName })),
  ];
}

export async function listProviders(): Promise<AcpProvider[]> {
  const client = await getClient();
  const result = await client.goose.GooseProvidersList({
    providerIds: [],
  });
  return buildProviderListFromEntries(result.entries);
}

export async function listSessions(): Promise<AcpSessionInfo[]> {
  const client = await getClient();
  const response = await client.listSessions({});
  return response.sessions.map((info: SessionInfo) => ({
    sessionId: info.sessionId,
    title: info.title ?? null,
    updatedAt: info.updatedAt ?? null,
    createdAt: (info._meta?.createdAt as string) ?? null,
    archivedAt: (info._meta?.archivedAt as string) ?? null,
    userSetName: info._meta?.userSetName === true,
    messageCount: (info._meta?.messageCount as number) ?? 0,
    workingDir: info.cwd ?? null,
    projectId: (info._meta?.projectId as string) ?? null,
    providerId: (info._meta?.providerId as string) ?? null,
    modelId: (info._meta?.modelId as string) ?? null,
    personaId: (info._meta?.personaId as string) ?? null,
  }));
}

export async function exportSession(sessionId: string): Promise<string> {
  const client = await getClient();
  const result = await client.goose.GooseSessionExport({ sessionId });
  // biome-ignore lint/suspicious/noExplicitAny: SDK doesn't expose data field on export result
  return (result as any).data;
}

export async function importSession(json: string): Promise<AcpSessionInfo> {
  const client = await getClient();
  const result = await client.goose.GooseSessionImport({ data: json });
  return result as unknown as AcpSessionInfo;
}

export async function forkSession(sessionId: string): Promise<AcpSessionInfo> {
  const client = await getClient();
  const response = await client.unstable_forkSession({
    sessionId,
    cwd: "~",
  });
  return {
    sessionId: response.sessionId,
    title: (response._meta?.title as string) ?? null,
    updatedAt: null,
    createdAt: (response._meta?.createdAt as string) ?? null,
    archivedAt: (response._meta?.archivedAt as string) ?? null,
    userSetName: response._meta?.userSetName === true,
    messageCount: (response._meta?.messageCount as number) ?? 0,
    workingDir: null,
    projectId: (response._meta?.projectId as string) ?? null,
    providerId: (response._meta?.providerId as string) ?? null,
    modelId: (response._meta?.modelId as string) ?? null,
    personaId: (response._meta?.personaId as string) ?? null,
  };
}

export async function setModel(
  sessionId: string,
  modelId: string,
): Promise<void> {
  const sid = sessionId.slice(0, 8);
  const tClient = performance.now();
  const client = await getClient();
  const tCall = performance.now();
  await client.setSessionConfigOption({
    sessionId,
    configId: "model",
    value: modelId,
  });
  perfLog(
    `[perf:api] ${sid} setModel(${modelId}) getClient=${(tCall - tClient).toFixed(1)}ms wire=${(performance.now() - tCall).toFixed(1)}ms`,
  );
}

export async function setProvider(
  sessionId: string,
  providerId: string,
): Promise<void> {
  const sid = sessionId.slice(0, 8);
  const tClient = performance.now();
  const client = await getClient();
  const tCall = performance.now();
  await client.setSessionConfigOption({
    sessionId,
    configId: "provider",
    value: providerId,
  });
  perfLog(
    `[perf:api] ${sid} setProvider(${providerId}) getClient=${(tCall - tClient).toFixed(1)}ms wire=${(performance.now() - tCall).toFixed(1)}ms`,
  );
}

export async function updateWorkingDir(
  sessionId: string,
  workingDir: string,
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/working_dir/update", {
    sessionId,
    workingDir,
  });
}

export async function updateSessionProject(
  sessionId: string,
  projectId: string | null,
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/session/update_project", {
    sessionId,
    projectId,
  });
}

export async function archiveSession(sessionId: string): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/session/archive", { sessionId });
}

export async function unarchiveSession(sessionId: string): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/session/unarchive", { sessionId });
}

export async function renameSession(
  sessionId: string,
  title: string,
): Promise<void> {
  const client = await getClient();
  await client.extMethod("_goose/session/rename", { sessionId, title });
}

export async function cancelSession(sessionId: string): Promise<void> {
  const client = await getClient();
  await client.cancel({ sessionId });
}

export async function newSession(
  workingDir: string,
  providerId?: string,
  projectId?: string,
  personaId?: string,
): Promise<NewSessionResponse> {
  const tClient = performance.now();
  const client = await getClient();
  const request: Parameters<typeof client.newSession>[0] = {
    cwd: workingDir,
    mcpServers: [],
  };

  const meta: Record<string, string> = {};
  if (providerId) meta.provider = providerId;
  if (projectId) meta.projectId = projectId;
  if (personaId) meta.personaId = personaId;
  if (Object.keys(meta).length > 0) request._meta = meta;

  const tCall = performance.now();
  const response = await client.newSession(request);
  const sid = response.sessionId.slice(0, 8);
  perfLog(
    `[perf:api] ${sid} newSession getClient=${(tCall - tClient).toFixed(1)}ms wire=${(performance.now() - tCall).toFixed(1)}ms`,
  );
  return response;
}

export async function loadSession(
  sessionId: string,
  workingDir: string,
): Promise<LoadSessionResponse> {
  const sid = sessionId.slice(0, 8);
  const tClient = performance.now();
  const client = await getClient();
  const tCall = performance.now();
  const response = await client.loadSession({
    sessionId,
    cwd: workingDir,
    mcpServers: [],
  });
  perfLog(
    `[perf:api] ${sid} loadSession getClient=${(tCall - tClient).toFixed(1)}ms wire=${(performance.now() - tCall).toFixed(1)}ms`,
  );
  return response;
}

export async function prompt(
  sessionId: string,
  content: ContentBlock[],
  meta?: Record<string, unknown>,
): Promise<PromptResponse> {
  const client = await getClient();
  return client.prompt({ sessionId, prompt: content, _meta: meta });
}
