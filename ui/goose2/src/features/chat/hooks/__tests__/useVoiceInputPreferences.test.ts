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
        GoosePreferencesRead: vi.fn().mockImplementation(() => {
          if (shouldFailProviderRead) {
            return Promise.reject(new Error("temporary acp failure"));
          }
          return Promise.resolve({
            values: [
              { key: "voiceAutoSubmitPhrases", value: null },
              { key: "voiceDictationProvider", value: "groq" },
              { key: "voiceDictationPreferredMic", value: null },
            ],
          });
        }),
        GoosePreferencesSave: vi.fn().mockResolvedValue({}),
        GoosePreferencesRemove: vi.fn().mockResolvedValue({}),
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
    const providerRead = deferred<{
      values: Array<{ key: string; value: unknown }>;
    }>();
    const pendingWrite = deferred<void>();

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: vi
          .fn()
          .mockResolvedValueOnce({
            values: [
              { key: "voiceAutoSubmitPhrases", value: null },
              { key: "voiceDictationProvider", value: null },
              { key: "voiceDictationPreferredMic", value: null },
            ],
          })
          .mockImplementation(() => providerRead.promise),
        GoosePreferencesSave: upsert.mockImplementation(
          () => pendingWrite.promise,
        ),
        GoosePreferencesRemove: vi.fn().mockResolvedValue({}),
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

    providerRead.resolve({
      values: [
        { key: "voiceAutoSubmitPhrases", value: null },
        { key: "voiceDictationProvider", value: "openai" },
        { key: "voiceDictationPreferredMic", value: null },
      ],
    });
    window.removeEventListener("goose:voice-input-preferences", eventListener);
  });

  it("does not broadcast failed preference writes and re-syncs stored state", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const save = vi.fn().mockRejectedValue(new Error("write failed"));
    const read = vi.fn().mockResolvedValue({
      values: [
        { key: "voiceAutoSubmitPhrases", value: null },
        { key: "voiceDictationProvider", value: "groq" },
        { key: "voiceDictationPreferredMic", value: null },
      ],
    });

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: read,
        GoosePreferencesSave: save,
        GoosePreferencesRemove: vi.fn().mockResolvedValue({}),
      },
    });

    const eventListener = vi.fn();
    window.addEventListener("goose:voice-input-preferences", eventListener);

    const { result } = renderHook(() => useVoiceInputPreferences());
    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    act(() => {
      result.current.setSelectedProvider("openai");
    });

    expect(result.current.selectedProvider).toBe("openai");
    await waitFor(() => expect(read).toHaveBeenCalledTimes(2));

    expect(eventListener).not.toHaveBeenCalled();
    expect(result.current.selectedProvider).toBe("groq");

    window.removeEventListener("goose:voice-input-preferences", eventListener);
    warn.mockRestore();
  });

  it("stores the disabled sentinel when provider is set to null", async () => {
    const save = vi.fn().mockResolvedValue({});

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: vi.fn().mockResolvedValue({
          values: [
            { key: "voiceAutoSubmitPhrases", value: null },
            { key: "voiceDictationProvider", value: "groq" },
            { key: "voiceDictationPreferredMic", value: null },
          ],
        }),
        GoosePreferencesSave: save,
        GoosePreferencesRemove: vi.fn().mockResolvedValue({}),
      },
    });

    const { result } = renderHook(() => useVoiceInputPreferences());
    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    act(() => {
      result.current.setSelectedProvider(null);
    });

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith({
        values: [{ key: "voiceDictationProvider", value: "__disabled__" }],
      });
    });
  });

  it("clearing selected provider removes only the provider preference", async () => {
    const remove = vi.fn().mockResolvedValue({});

    mockGetClient.mockResolvedValue({
      goose: {
        GoosePreferencesRead: vi.fn().mockResolvedValue({
          values: [
            { key: "voiceAutoSubmitPhrases", value: "submit" },
            { key: "voiceDictationProvider", value: "groq" },
            { key: "voiceDictationPreferredMic", value: "mic-1" },
          ],
        }),
        GoosePreferencesSave: vi.fn().mockResolvedValue({}),
        GoosePreferencesRemove: remove,
      },
    });

    const { result } = renderHook(() => useVoiceInputPreferences());
    await waitFor(() => expect(result.current.isHydrated).toBe(true));

    act(() => {
      result.current.clearSelectedProvider();
    });

    await waitFor(() => {
      expect(remove).toHaveBeenCalledWith({
        keys: ["voiceDictationProvider"],
      });
    });
  });
});
