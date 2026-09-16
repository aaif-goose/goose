import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getAcpClient } from '../acpConnection';
import { acpGetLiveVoiceAvailability, acpStartLiveVoice, acpStopLiveVoice } from '../liveVoice';

vi.mock('../acpConnection', () => ({ getAcpClient: vi.fn() }));

describe('ACP Live voice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(window.electron.getSetting).mockResolvedValue(false);
  });

  it('uses the generated availability client for the displayed session', async () => {
    const sessionLiveVoiceAvailability = vi.fn().mockResolvedValue({
      status: 'ready',
      message: 'Start Live voice',
    });
    vi.mocked(getAcpClient).mockResolvedValue({
      goose: {
        sessionLiveVoiceAvailability_unstable: sessionLiveVoiceAvailability,
      },
    } as unknown as Awaited<ReturnType<typeof getAcpClient>>);

    await expect(acpGetLiveVoiceAvailability('main-session')).resolves.toEqual({
      status: 'ready',
      message: 'Start Live voice',
    });
    expect(sessionLiveVoiceAvailability).toHaveBeenCalledWith({
      sessionId: 'main-session',
      _meta: { goose: { unrolledAgentLoop: true } },
    });
  });

  it('checks availability without a session for a new chat', async () => {
    const sessionLiveVoiceAvailability = vi.fn().mockResolvedValue({
      status: 'unavailable',
      message: 'Live voice is disabled',
    });
    vi.mocked(getAcpClient).mockResolvedValue({
      goose: {
        sessionLiveVoiceAvailability_unstable: sessionLiveVoiceAvailability,
      },
    } as unknown as Awaited<ReturnType<typeof getAcpClient>>);

    await expect(acpGetLiveVoiceAvailability()).resolves.toEqual({
      status: 'unavailable',
      message: 'Live voice is disabled',
    });
    expect(sessionLiveVoiceAvailability).toHaveBeenCalledWith({
      _meta: { goose: { unrolledAgentLoop: true } },
    });
  });

  it('passes the legacy loop selection to availability', async () => {
    vi.mocked(window.electron.getSetting).mockResolvedValue(true);
    const sessionLiveVoiceAvailability = vi.fn().mockResolvedValue({
      status: 'unavailable',
      message: 'Live voice is unavailable while Use Legacy Agent Loop is enabled',
    });
    vi.mocked(getAcpClient).mockResolvedValue({
      goose: {
        sessionLiveVoiceAvailability_unstable: sessionLiveVoiceAvailability,
      },
    } as unknown as Awaited<ReturnType<typeof getAcpClient>>);

    await acpGetLiveVoiceAvailability('main-session');

    expect(sessionLiveVoiceAvailability).toHaveBeenCalledWith({
      sessionId: 'main-session',
      _meta: { goose: { unrolledAgentLoop: false } },
    });
  });

  it('uses generated start and stop clients with the interaction ID', async () => {
    const start = vi.fn().mockResolvedValue({
      interactionId: 'live-opaque',
      answerSdp: 'answer',
    });
    const stop = vi.fn().mockResolvedValue({});
    vi.mocked(getAcpClient).mockResolvedValue({
      goose: {
        sessionLiveVoiceStart_unstable: start,
        sessionLiveVoiceStop_unstable: stop,
      },
    } as unknown as Awaited<ReturnType<typeof getAcpClient>>);

    await expect(acpStartLiveVoice('main-session', 'offer')).resolves.toMatchObject({
      interactionId: 'live-opaque',
    });
    await acpStopLiveVoice('main-session', 'live-opaque');

    expect(start).toHaveBeenCalledWith({
      sessionId: 'main-session',
      offerSdp: 'offer',
      _meta: { goose: { unrolledAgentLoop: true } },
    });
    expect(stop).toHaveBeenCalledWith({
      sessionId: 'main-session',
      interactionId: 'live-opaque',
    });
  });
});
