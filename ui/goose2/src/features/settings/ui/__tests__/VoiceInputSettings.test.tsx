import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VoiceInputSettings } from "../VoiceInputSettings";

const mockGetDictationConfig = vi.fn();
const mockUseVoiceInputPreferences = vi.fn();

vi.mock("@/shared/api/dictation", () => ({
  getDictationConfig: () => mockGetDictationConfig(),
  saveDictationModelSelection: vi.fn(),
  saveDictationProviderSecret: vi.fn(),
  deleteDictationProviderSecret: vi.fn(),
}));

vi.mock("@/features/chat/hooks/useVoiceInputPreferences", () => ({
  useVoiceInputPreferences: () => mockUseVoiceInputPreferences(),
}));

vi.mock("@/shared/ui/ai-elements/mic-selector", () => ({
  useAudioDevices: () => ({
    devices: [],
    error: null,
    hasPermission: false,
    loadDevices: vi.fn(),
    loading: false,
  }),
}));

vi.mock("../LocalWhisperModels", () => ({
  LocalWhisperModels: () => <div />,
}));

describe("VoiceInputSettings", () => {
  beforeEach(() => {
    mockGetDictationConfig.mockReset();
    mockUseVoiceInputPreferences.mockReset();
    mockUseVoiceInputPreferences.mockReturnValue({
      clearSelectedProvider: vi.fn(),
      hasStoredProviderPreference: true,
      isHydrated: true,
      preferredMicrophoneId: null,
      rawAutoSubmitPhrases: "submit",
      selectedProvider: "openai",
      setPreferredMicrophoneId: vi.fn(),
      setRawAutoSubmitPhrases: vi.fn(),
      setSelectedProvider: vi.fn(),
    });
    mockGetDictationConfig.mockResolvedValue({
      openai: {
        configured: false,
        description: "Uses OpenAI Whisper API for high-quality transcription.",
        usesProviderConfig: true,
        settingsPath: "Settings > Models",
        configKey: null,
        modelConfigKey: "OPENAI_TRANSCRIPTION_MODEL",
        defaultModel: "whisper-1",
        selectedModel: null,
        availableModels: [
          {
            id: "whisper-1",
            label: "Whisper-1",
            description: "OpenAI's hosted Whisper transcription model.",
          },
        ],
      },
    });
  });

  it("points OpenAI Whisper setup to provider settings", async () => {
    const user = userEvent.setup();
    const openSettingsListener = vi.fn();
    window.addEventListener("goose:open-settings", openSettingsListener);

    render(<VoiceInputSettings />);

    await waitFor(() =>
      expect(screen.getByText("Provider credentials")).toBeInTheDocument(),
    );
    expect(
      screen.getByText(
        "This transcription provider uses the credentials from its model provider setup.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Add API key" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open Providers" }));

    expect(openSettingsListener).toHaveBeenCalledTimes(1);
    expect(openSettingsListener.mock.calls[0][0]).toMatchObject({
      detail: { section: "providers" },
    });

    window.removeEventListener("goose:open-settings", openSettingsListener);
  });
});
