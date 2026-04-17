import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockGetClient = vi.fn();

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mockGetClient(),
}));

import { useVoiceInputPreferences } from "../useVoiceInputPreferences";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useVoiceInputPreferences", () => {
  beforeEach(() => {
    mockGetClient.mockReset();
  });

  it("does not hydrate until provider config can be read successfully", async () => {
    let shouldFailProviderRead = true;

    mockGetClient.mockResolvedValue({
      goose: {
        GooseConfigRead: vi.fn().mockImplementation(({ key }) => {
          if (key === "VOICE_DICTATION_PROVIDER") {
            if (shouldFailProviderRead) {
              return Promise.reject(new Error("temporary acp failure"));
            }
            return Promise.resolve({ value: "groq" });
          }
          return Promise.resolve({ value: null });
        }),
        GooseConfigUpsert: vi.fn().mockResolvedValue({}),
        GooseConfigRemove: vi.fn().mockResolvedValue({}),
      },
    });

    const { result } = renderHook(() => useVoiceInputPreferences());

    await act(async () => {});

    expect(result.current.isHydrated).toBe(false);
    expect(result.current.selectedProvider).toBeNull();

    shouldFailProviderRead = false;

    await act(async () => {
      window.dispatchEvent(new Event("goose:voice-input-preferences"));
    });

    await waitFor(() => expect(result.current.isHydrated).toBe(true));
    expect(result.current.selectedProvider).toBe("groq");
    expect(result.current.hasStoredProviderPreference).toBe(true);
  });

  it("broadcasts preference changes only after config persistence settles", async () => {
    const upsert = vi.fn();
    const providerRead = deferred<{ value?: unknown }>();
    const pendingWrite = deferred<void>();

    mockGetClient.mockResolvedValue({
      goose: {
        GooseConfigRead: vi
          .fn()
          .mockResolvedValueOnce({ value: null })
          .mockResolvedValueOnce({ value: null })
          .mockResolvedValueOnce({ value: null })
          .mockImplementation(() => providerRead.promise),
        GooseConfigUpsert: upsert.mockImplementation(
          () => pendingWrite.promise,
        ),
        GooseConfigRemove: vi.fn().mockResolvedValue({}),
      },
    });

    const eventListener = vi.fn();
    window.addEventListener("goose:voice-input-preferences", eventListener);

    const { result } = renderHook(() => useVoiceInputPreferences());

    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    act(() => {
      result.current.setSelectedProvider("openai");
    });

    expect(eventListener).not.toHaveBeenCalled();
    expect(result.current.selectedProvider).toBe("openai");

    await act(async () => {
      pendingWrite.resolve();
      await pendingWrite.promise;
    });

    await waitFor(() => expect(eventListener).toHaveBeenCalledTimes(1));

    providerRead.resolve({ value: "openai" });
    window.removeEventListener("goose:voice-input-preferences", eventListener);
  });
});
