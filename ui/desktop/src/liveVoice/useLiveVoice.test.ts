import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { isAcpRecovering, subscribeToAcpRecovery } from '../acp/acpConnection';
import { acpStartLiveVoice, acpStopLiveVoice } from '../acp/liveVoice';
import { publishLiveVoiceInteractionEnded } from '../acp/liveVoiceNotifications';
import { LiveVoiceMediaSession } from './LiveVoiceMediaSession';
import { useLiveVoice } from './useLiveVoice';

vi.mock('../acp/liveVoice', () => ({
  acpStartLiveVoice: vi.fn(),
  acpStopLiveVoice: vi.fn(),
}));

vi.mock('../acp/acpConnection', () => ({
  isAcpRecovering: vi.fn(() => false),
  subscribeToAcpRecovery: vi.fn(),
}));

vi.mock('./LiveVoiceMediaSession', () => ({
  LiveVoiceMediaSession: vi.fn(),
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe('useLiveVoice', () => {
  let mediaFailure: () => void;
  let recoveryChanged: (recovering: boolean) => void;
  const media = {
    createOffer: vi.fn(),
    applyAnswer: vi.fn(),
    setMuted: vi.fn(),
    teardown: vi.fn(),
  };
  beforeEach(() => {
    vi.clearAllMocks();
    media.createOffer.mockResolvedValue('offer');
    media.applyAnswer.mockResolvedValue(undefined);
    vi.mocked(isAcpRecovering).mockReturnValue(false);
    vi.mocked(subscribeToAcpRecovery).mockImplementation((listener) => {
      recoveryChanged = listener;
      return () => undefined;
    });
    vi.mocked(LiveVoiceMediaSession).mockImplementation(
      function LiveVoiceMediaSessionMock(onFailure) {
        mediaFailure = onFailure;
        return media as unknown as LiveVoiceMediaSession;
      }
    );
    vi.mocked(acpStartLiveVoice).mockResolvedValue({
      interactionId: 'live-opaque',
      answerSdp: 'answer',
    });
    vi.mocked(acpStopLiveVoice).mockResolvedValue(undefined);
  });

  it('starts media from the start response and stops the same connection', async () => {
    const { result } = renderHook(() => useLiveVoice());

    await act(async () => result.current.start('main-session'));
    expect(media.applyAnswer).toHaveBeenCalledWith('answer');
    expect(media.setMuted).toHaveBeenCalledWith(false);
    expect(result.current.phase).toBe('live');
    expect(result.current.activeSessionId).toBe('main-session');

    await act(async () => result.current.stop());
    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).toHaveBeenCalledWith('main-session', 'live-opaque');
    expect(result.current.phase).toBe('idle');
    expect(result.current.activeSessionId).toBeNull();
  });

  it('blocks retry until the server interaction stops after media setup fails', async () => {
    const cleanup = deferred<void>();
    media.applyAnswer.mockRejectedValueOnce(new Error('media failed'));
    vi.mocked(acpStopLiveVoice).mockReturnValueOnce(cleanup.promise);
    const { result } = renderHook(() => useLiveVoice());

    let startPromise!: Promise<void>;
    act(() => {
      startPromise = result.current.start('main-session');
    });
    await waitFor(() => expect(acpStopLiveVoice).toHaveBeenCalledOnce());

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).toHaveBeenCalledWith('main-session', 'live-opaque');
    expect(result.current.phase).toBe('stopping');

    await act(async () => result.current.start('main-session'));
    expect(acpStartLiveVoice).toHaveBeenCalledOnce();

    await act(async () => {
      cleanup.resolve();
      await startPromise;
    });
    expect(result.current.phase).toBe('error');
  });

  it('tears down and stops an active interaction when unmounted', async () => {
    const { result, unmount } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    unmount();

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).toHaveBeenCalledWith('main-session', 'live-opaque');
  });

  it('keeps one interaction when another session tries to start', async () => {
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    await act(async () => result.current.start('other-session'));

    expect(result.current.activeSessionId).toBe('main-session');
    expect(media.teardown).not.toHaveBeenCalled();
    expect(acpStopLiveVoice).not.toHaveBeenCalled();
    expect(LiveVoiceMediaSession).toHaveBeenCalledOnce();
    expect(acpStartLiveVoice).toHaveBeenCalledOnce();
  });

  it.each(['unmount', 'stop'] as const)(
    'closes an interaction that finishes after %s while connecting',
    async (action) => {
      const pending = deferred<{ interactionId: string; answerSdp: string }>();
      vi.mocked(acpStartLiveVoice).mockReturnValueOnce(pending.promise);
      const { result, unmount } = renderHook(() => useLiveVoice());
      act(() => {
        void result.current.start('main-session');
      });
      await waitFor(() => expect(acpStartLiveVoice).toHaveBeenCalledOnce());

      if (action === 'unmount') {
        unmount();
      } else {
        await act(async () => result.current.stop());
        expect(result.current.phase).toBe('stopping');
      }
      await act(async () => {
        pending.resolve({ interactionId: 'late-interaction', answerSdp: 'late-answer' });
      });

      expect(media.teardown).toHaveBeenCalledOnce();
      expect(media.applyAnswer).not.toHaveBeenCalled();
      expect(media.setMuted).not.toHaveBeenCalled();
      expect(acpStopLiveVoice).toHaveBeenCalledWith('main-session', 'late-interaction');
      if (action === 'stop') expect(result.current.phase).toBe('idle');
    }
  );

  it('does not become live after stopping during answer setup', async () => {
    const pending = deferred<void>();
    media.applyAnswer.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useLiveVoice());
    act(() => {
      void result.current.start('main-session');
    });
    await waitFor(() => expect(media.applyAnswer).toHaveBeenCalledOnce());

    await act(async () => result.current.stop());
    await act(async () => pending.resolve(undefined));

    expect(media.setMuted).not.toHaveBeenCalled();
    expect(result.current.phase).toBe('idle');
  });

  it('applies rapid mute changes to the current media session', async () => {
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    act(() => {
      result.current.toggleMute();
      result.current.toggleMute();
      result.current.toggleMute();
    });

    expect(media.setMuted.mock.calls).toEqual([[false], [true], [false], [true]]);
    expect(result.current.muted).toBe(true);
  });

  it('cleans up when the backend reports that the current interaction failed', async () => {
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    act(() => {
      publishLiveVoiceInteractionEnded({
        sessionId: 'main-session',
        update: {
          sessionUpdate: 'live_voice_interaction_ended',
          interactionId: 'live-opaque',
          outcome: 'failed',
        },
      });
      publishLiveVoiceInteractionEnded({
        sessionId: 'main-session',
        update: {
          sessionUpdate: 'live_voice_interaction_ended',
          interactionId: 'live-opaque',
          outcome: 'failed',
        },
      });
    });

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).not.toHaveBeenCalled();
    expect(result.current.phase).toBe('error');
  });

  it('ignores stale session and interaction endings', async () => {
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    act(() => {
      publishLiveVoiceInteractionEnded({
        sessionId: 'other-session',
        update: {
          sessionUpdate: 'live_voice_interaction_ended',
          interactionId: 'live-opaque',
          outcome: 'failed',
        },
      });
      publishLiveVoiceInteractionEnded({
        sessionId: 'main-session',
        update: {
          sessionUpdate: 'live_voice_interaction_ended',
          interactionId: 'old-interaction',
          outcome: 'failed',
        },
      });
    });

    expect(media.teardown).not.toHaveBeenCalled();
    expect(result.current.phase).toBe('live');
  });

  it('reconciles a terminal update that arrives before the start response', async () => {
    const pending = deferred<{ interactionId: string; answerSdp: string }>();
    vi.mocked(acpStartLiveVoice).mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useLiveVoice());
    act(() => {
      void result.current.start('main-session');
    });
    await waitFor(() => expect(acpStartLiveVoice).toHaveBeenCalledOnce());

    act(() => {
      publishLiveVoiceInteractionEnded({
        sessionId: 'main-session',
        update: {
          sessionUpdate: 'live_voice_interaction_ended',
          interactionId: 'early-interaction',
          outcome: 'failed',
        },
      });
    });
    await act(async () =>
      pending.resolve({ interactionId: 'early-interaction', answerSdp: 'answer' })
    );

    expect(media.applyAnswer).not.toHaveBeenCalled();
    expect(media.teardown).toHaveBeenCalledOnce();
    expect(result.current.phase).toBe('error');
  });

  it('cleans up locally without stopping through a recovering ACP connection', async () => {
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));

    act(() => recoveryChanged(true));

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).not.toHaveBeenCalled();
    expect(result.current.phase).toBe('idle');
  });

  it('does not stop a late interaction response through a recovered ACP connection', async () => {
    const pending = deferred<{ interactionId: string; answerSdp: string }>();
    vi.mocked(acpStartLiveVoice).mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useLiveVoice());
    act(() => {
      void result.current.start('main-session');
    });
    await waitFor(() => expect(acpStartLiveVoice).toHaveBeenCalledOnce());

    act(() => recoveryChanged(true));
    await act(async () =>
      pending.resolve({ interactionId: 'old-interaction', answerSdp: 'answer' })
    );

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).not.toHaveBeenCalled();
    expect(result.current.phase).toBe('idle');
  });

  it('blocks retry until the server interaction stops after active media fails', async () => {
    const cleanup = deferred<void>();
    const { result } = renderHook(() => useLiveVoice());
    await act(async () => result.current.start('main-session'));
    vi.mocked(acpStopLiveVoice).mockReturnValueOnce(cleanup.promise);

    act(() => mediaFailure());

    expect(media.teardown).toHaveBeenCalledOnce();
    expect(acpStopLiveVoice).toHaveBeenCalledWith('main-session', 'live-opaque');
    expect(result.current.phase).toBe('stopping');

    await act(async () => result.current.start('main-session'));
    expect(acpStartLiveVoice).toHaveBeenCalledOnce();

    await act(async () => cleanup.resolve());
    expect(result.current.phase).toBe('error');
  });
});
