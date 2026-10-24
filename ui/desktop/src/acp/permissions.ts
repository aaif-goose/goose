import type { ToolListItem, ToolPermissionEntry, ToolPermissionLevel } from '@aaif/goose-acp-client';
import { getAcpClient } from './acpConnection';

export type { ToolListItem, ToolPermissionEntry, ToolPermissionLevel };

export async function listTools(sessionId: string, extensionName?: string): Promise<ToolListItem[]> {
  const client = await getAcpClient();
  const response = await client.goose.toolsList_unstable({
    sessionId,
    extensionName: extensionName ?? null,
  });
  return response.tools ?? [];
}

export interface ToolCallResult {
  content: unknown[];
  structuredContent: unknown;
  isError: boolean;
}

export async function callTool(
  sessionId: string,
  extensionName: string,
  name: string,
  args: Record<string, unknown> = {}
): Promise<ToolCallResult> {
  const client = await getAcpClient();
  const response = await client.goose.toolsCall_unstable({
    sessionId,
    extensionName,
    name,
    arguments: args,
  });
  return {
    content: response.content ?? [],
    structuredContent: response.structuredContent ?? null,
    isError: response.isError,
  };
}

export async function setToolPermissions(toolPermissions: ToolPermissionEntry[]): Promise<void> {
  const client = await getAcpClient();
  await client.goose.toolsPermissionsSet_unstable({ toolPermissions });
}
