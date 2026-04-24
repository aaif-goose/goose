import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockGetDictationConfig = vi.fn();
const mockUseDictationRecorder = vi.fn();
const mockUseVoiceInputPreferences = vi.fn();

vi.mock("@/shared/api/dictation", () => ({
  getDictationConfig: () => mockGetDictationConfig(),
}));

vi.mock("../useDictationRecorder", () => ({
  useDictationRecorder: (options: unknown) => mockUseDictationRecorder(options),
}));

vi.mock("../useVoiceInputPreferences", () => ({
  useVoiceInputPreferences: () => mockUseVoiceInputPreferences(),
}));

import { useVoiceDictation } from "../useVoiceDictation";

describe("useVoiceDictation", () => {
  beforeEach(() => {
    mockGetDictationConfig.mockReset();
    mockUseDictationRecorder.mockReset();
    mockUseVoiceInputPreferences.mockReset();

    mockUseDictationRecorder.mockReturnValue({
      isEnabled: false,
      isRecording: false,
      isStarting: () => false,
      isTranscribing: false,
      startRecording: vi.fn(),
      stopRecording: vi.fn(),
      toggleRecording: vi.fn(),
    });
  });

  it("defers default provider fallback until preferences hydrate", async () => {
    const voicePrefs = {
      autoSubmitPhrases: [],
      clearSelectedProvider: vi.fn(),
      hasStoredProviderPreference: false,
      isHydrated: false,
      preferredMicrophoneId: null,
      rawAutoSubmitPhrases: "submit",
      selectedProvider: null,
      setPreferredMicrophoneId: vi.fn(),
      setRawAutoSubmitPhrases: vi.fn(),
      setSelectedProvider: vi.fn(),
    };

    mockUseVoiceInputPreferences.mockImplementation(() => voicePrefs);
    mockGetDictationConfig.mockResolvedValue({
      openai: {
        availableModels: [],
        configured: true,
        description: "OpenAI",
        usesProviderConfig: true,
      },
    });

    const { rerender } = renderHook(() =>
      useVoiceDictation({
        attachments: [],
        clearAttachments: vi.fn(),
        onSend: vi.fn(),
        resetTextarea: vi.fn(),
        selectedPersonaId: null,
        setText: vi.fn(),
        text: "",
      }),
    );

    await waitFor(() =>
      expect(mockGetDictationConfig).toHaveBeenCalledTimes(1),
    );
    await waitFor(() =>
      expect(mockUseDictationRecorder).toHaveBeenLastCalledWith(
        expect.objectContaining({
          provider: null,
          providerConfigured: false,
        }),
      ),
    );

    voicePrefs.isHydrated = true;
    rerender();

    await waitFor(() =>
      expect(mockUseDictationRecorder).toHaveBeenLastCalledWith(
        expect.objectContaining({
          provider: "openai",
          providerConfigured: true,
        }),
      ),
    );
  });
});
