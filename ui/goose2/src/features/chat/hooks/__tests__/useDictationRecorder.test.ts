import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockTranscribeDictation = vi.fn();

vi.mock("@/shared/api/dictation", () => ({
  transcribeDictation: (...args: unknown[]) => mockTranscribeDictation(...args),
}));

import { useDictationRecorder } from "../useDictationRecorder";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useDictationRecorder", () => {
  beforeEach(() => {
    mockTranscribeDictation.mockReset();

    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: {
        getUserMedia: vi.fn(),
      },
    });
  });

  it("lets a second toggle cancel a pending startup", async () => {
    const pendingStream = deferred<MediaStream>();
    const stopTrack = vi.fn();
    const stream = {
      getTracks: () => [{ stop: stopTrack }],
    } as unknown as MediaStream;

    vi.mocked(navigator.mediaDevices.getUserMedia).mockReturnValue(
      pendingStream.promise,
    );

    const { result } = renderHook(() =>
      useDictationRecorder({
        onError: vi.fn(),
        onTranscription: vi.fn(),
        preferredMicrophoneId: null,
        provider: "openai",
        providerConfigured: true,
      }),
    );

    act(() => {
      result.current.toggleRecording();
    });

    expect(result.current.isStarting()).toBe(true);

    act(() => {
      result.current.toggleRecording();
    });

    await act(async () => {
      pendingStream.resolve(stream);
      await pendingStream.promise;
    });

    await waitFor(() => expect(result.current.isStarting()).toBe(false));
    expect(result.current.isRecording).toBe(false);
    expect(stopTrack).toHaveBeenCalledTimes(1);
  });
});
