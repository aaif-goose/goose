import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import { MicrophoneSelector } from './MicrophoneSelector';

type FakeStream = MediaStream & {
  track: { stop: ReturnType<typeof vi.fn> };
};

type PendingStream = {
  stream: FakeStream;
  promise: Promise<MediaStream>;
  resolve: () => void;
};

class MockAudioContext {
  createMediaStreamSource = vi.fn(() => ({ connect: vi.fn() }));
  createAnalyser = vi.fn(() => ({
    fftSize: 0,
    frequencyBinCount: 128,
    getByteTimeDomainData: vi.fn(),
  }));
  close = vi.fn(() => Promise.resolve());
}

const createPendingStream = (): PendingStream => {
  const track = { stop: vi.fn() };
  const stream = {
    track,
    getTracks: () => [track],
  } as unknown as FakeStream;
  let resolvePromise: (stream: MediaStream) => void = () => {};
  const promise = new Promise<MediaStream>((resolve) => {
    resolvePromise = resolve;
  });

  return {
    stream,
    promise,
    resolve: () => resolvePromise(stream),
  };
};

const renderSelector = async () => {
  const result = render(<MicrophoneSelector selectedDeviceId={null} onDeviceChange={vi.fn()} />, {
    wrapper: IntlTestWrapper,
  });
  await screen.findByRole('button', { name: 'Test' });
  return result;
};

describe('MicrophoneSelector microphone lifecycle', () => {
  let pendingStreams: PendingStream[];

  beforeEach(() => {
    pendingStreams = [];
    vi.stubGlobal('AudioContext', MockAudioContext);
    vi.stubGlobal(
      'requestAnimationFrame',
      vi.fn(() => 1)
    );
    vi.stubGlobal('cancelAnimationFrame', vi.fn());

    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        enumerateDevices: vi
          .fn()
          .mockResolvedValue([
            { kind: 'audioinput', deviceId: 'mic1', label: 'Mic One', groupId: 'group1' },
          ]),
        getUserMedia: vi.fn(() => {
          const pendingStream = createPendingStream();
          pendingStreams.push(pendingStream);
          return pendingStream.promise;
        }),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('stops every stream acquired by overlapping test activations', async () => {
    await renderSelector();
    const button = screen.getByRole('button', { name: 'Test' });

    fireEvent.click(button);
    fireEvent.click(button);

    await act(async () => {
      pendingStreams.forEach(({ resolve }) => resolve());
      await Promise.all(pendingStreams.map(({ promise }) => promise));
    });

    const stopButton = screen.queryByRole('button', { name: 'Stop' });
    if (stopButton) fireEvent.click(stopButton);

    expect(pendingStreams.length).toBeGreaterThan(0);
    pendingStreams.forEach(({ stream }) => expect(stream.track.stop).toHaveBeenCalledOnce());
  });

  it('stops a stream that arrives after the user stops the test', async () => {
    await renderSelector();
    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));

    await act(async () => {
      pendingStreams[0].resolve();
      await pendingStreams[0].promise;
    });

    expect(pendingStreams[0].stream.track.stop).toHaveBeenCalledOnce();
    expect(screen.getByRole('button', { name: 'Test' })).toBeInTheDocument();
  });

  it('stops a stream that arrives after unmount', async () => {
    const { unmount } = await renderSelector();
    fireEvent.click(screen.getByRole('button', { name: 'Test' }));
    unmount();

    await act(async () => {
      pendingStreams[0].resolve();
      await pendingStreams[0].promise;
    });

    expect(pendingStreams[0].stream.track.stop).toHaveBeenCalledOnce();
  });
});
