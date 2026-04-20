import type {
  ContentBlock,
  NewSessionResponse,
  LoadSessionResponse,
  PromptResponse,
} from "@agentclientprotocol/sdk";
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
  messageCount: number;
}

const DEPRECATED_PROVIDER_IDS = new Set(["claude-code", "codex", "gemini-cli"]);

export async function listProviders(): Promise<AcpProvider[]> {
  const client = await getClient();
  const result = await client.goose.GooseProvidersList({});
  // biome-ignore lint/suspicious/noExplicitAny: ACP SDK types don't expose providers field
  return (result as any).providers
    .filter(
      (p: { id: string; label: string }) => !DEPRECATED_PROVIDER_IDS.has(p.id),
    )
    .map((p: { id: string; label: string }) => ({ id: p.id, label: p.label }));
}

export async function listSessions(): Promise<AcpSessionInfo[]> {
  const client = await getClient();
  // GooseClient.unstable_listSessions doesn't work with SDK 0.19 (renamed to listSessions).
  // Bypass GooseClient and call the connection directly. Fix when ui/acp is updated.
  // biome-ignore lint/suspicious/noExplicitAny: SDK doesn't expose conn property
  const conn = (client as any).conn;
  const response = await conn.listSessions({});
  return response.sessions.map(
    (info: {
      sessionId: string;
      title?: string;
      updatedAt?: string;
      _meta?: Record<string, unknown>;
    }) => ({
      sessionId: info.sessionId,
      title: info.title ?? null,
      updatedAt: info.updatedAt ?? null,
      messageCount: (info._meta?.messageCount as number) ?? 0,
    }),
  );
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
    cwd: "~/.goose/artifacts",
  });
  return {
    sessionId: response.sessionId,
    title: (response._meta?.title as string) ?? null,
    updatedAt: null,
    messageCount: (response._meta?.messageCount as number) ?? 0,
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
  await client.extMethod("goose/working_dir/update", { sessionId, workingDir });
}

export async function cancelSession(sessionId: string): Promise<void> {
  const client = await getClient();
  await client.cancel({ sessionId });
}

export async function newSession(
  workingDir: string,
  providerId?: string,
): Promise<NewSessionResponse> {
  const tClient = performance.now();
  const client = await getClient();
  const request: Parameters<typeof client.newSession>[0] & {
    meta?: Record<string, string>;
  } = {
    cwd: workingDir,
    mcpServers: [],
  };

  if (providerId) {
    request.meta = { provider: providerId };
  }

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
): Promise<PromptResponse> {
  const client = await getClient();
  return client.prompt({ sessionId, prompt: content });
}
