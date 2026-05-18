import type { LoadSessionResponse } from '@agentclientprotocol/sdk';
import { getAcpClient } from './acpConnection';

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
