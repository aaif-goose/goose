import type {
  LiveVoiceAvailabilityResponse_unstable,
  LiveVoiceStartResponse_unstable,
} from '@aaif/goose-acp-client';
import { getAcpClient } from './acpConnection';

export async function acpGetLiveVoiceAvailability(
  sessionId?: string
): Promise<LiveVoiceAvailabilityResponse_unstable> {
  const { goose } = await getAcpClient();
  const useLegacyAgentLoop = await window.electron.getSetting('useLegacyAgentLoop');
  return goose.sessionLiveVoiceAvailability_unstable({
    ...(sessionId ? { sessionId } : {}),
    _meta: { goose: { unrolledAgentLoop: !useLegacyAgentLoop } },
  });
}

export async function acpStartLiveVoice(
  sessionId: string,
  offerSdp: string
): Promise<LiveVoiceStartResponse_unstable> {
  const { goose } = await getAcpClient();
  const useLegacyAgentLoop = await window.electron.getSetting('useLegacyAgentLoop');
  return goose.sessionLiveVoiceStart_unstable({
    sessionId,
    offerSdp,
    _meta: { goose: { unrolledAgentLoop: !useLegacyAgentLoop } },
  });
}

export async function acpStopLiveVoice(sessionId: string, interactionId: string): Promise<void> {
  const { goose } = await getAcpClient();
  await goose.sessionLiveVoiceStop_unstable({ sessionId, interactionId });
}
