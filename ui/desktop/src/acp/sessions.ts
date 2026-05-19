import type {
  ListSessionsResponse,
  LoadSessionResponse,
  SessionInfo,
} from '@agentclientprotocol/sdk';
import { getAcpClient } from './acpConnection';
import { DEFAULT_CHAT_TITLE } from '../contexts/ChatContext';

export async function acpLoadSession(
  sessionId: string,
  workingDir: string
): Promise<LoadSessionResponse> {
  const client = await getAcpClient();
  return client.loadSession({
    sessionId,
    cwd: workingDir,
    mcpServers: [],
  });
}

export async function acpListSessions(): Promise<ListSessionsResponse> {
  const client = await getAcpClient();
  return client.listSessions({});
}

export async function acpRenameSession(sessionId: string, title: string): Promise<void> {
  const client = await getAcpClient();
  await client.goose.GooseSessionRename({ sessionId, title });
}

export async function acpDeleteSession(sessionId: string): Promise<void> {
  const client = await getAcpClient();
  await client.goose.sessionDelete({ sessionId });
}

export async function acpForkSession(sessionId: string, cwd: string): Promise<void> {
  const client = await getAcpClient();
  await client.unstable_forkSession({ sessionId, cwd, mcpServers: [] });
}

export async function acpExportSession(sessionId: string): Promise<string> {
  const client = await getAcpClient();
  const result = await client.goose.GooseSessionExport({ sessionId });
  return result.data;
}

export async function acpImportSession(data: string): Promise<void> {
  const client = await getAcpClient();
  await client.goose.GooseSessionImport({ data });
}

interface GooseSessionInfoMeta {
  messageCount?: number;
  createdAt?: string;
  archivedAt?: string;
  projectId?: string;
  providerId?: string;
  modelId?: string;
  userSetName?: boolean;
  hasRecipe?: boolean;
}

export interface SessionListItem {
  id: string;
  name: string;
  workingDir: string;
  updatedAt: string;
  messageCount: number;
  createdAt: string;
  archivedAt?: string;
  projectId?: string;
  providerId?: string;
  modelId?: string;
  userSetName?: boolean;
  hasRecipe?: boolean;
}

export function sessionInfoToListItem(s: SessionInfo): SessionListItem {
  const meta = (s._meta ?? {}) as GooseSessionInfoMeta;
  return {
    id: String(s.sessionId),
    name: s.title ?? DEFAULT_CHAT_TITLE,
    workingDir: s.cwd,
    updatedAt: s.updatedAt ?? '',
    messageCount: meta.messageCount ?? 0,
    createdAt: meta.createdAt ?? s.updatedAt ?? '',
    archivedAt: meta.archivedAt,
    projectId: meta.projectId,
    providerId: meta.providerId,
    modelId: meta.modelId,
    userSetName: meta.userSetName,
    hasRecipe: meta.hasRecipe,
  };
}
