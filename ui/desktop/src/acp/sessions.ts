import type {
  ListSessionsResponse,
  LoadSessionResponse,
  SessionInfo,
} from '@agentclientprotocol/sdk';
import { getAcpClient } from './acpConnection';
import { DEFAULT_CHAT_TITLE } from '../contexts/ChatContext';

export async function loadAcpSession(
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

export async function listAcpSessions(): Promise<ListSessionsResponse> {
  const client = await getAcpClient();
  return client.listSessions({});
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
