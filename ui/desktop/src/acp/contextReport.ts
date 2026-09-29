import type { ContextReportResponse_unstable } from '@aaif/goose-acp-client';
import { getAcpClient } from './acpConnection';

export type ContextReport = ContextReportResponse_unstable;
export type ContextSegment = ContextReport['segments'][number];
export type ContextPart = NonNullable<ContextSegment['parts']>[number];
export type ContextCategory = ContextSegment['category'];

export async function getContextReport(sessionId: string): Promise<ContextReport> {
  const client = await getAcpClient();
  const useLegacyAgentLoop = await window.electron.getSetting('useLegacyAgentLoop');
  return client.goose.contextReport_unstable({
    sessionId,
    unrolledAgentLoop: !useLegacyAgentLoop,
  });
}
