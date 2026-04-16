import { describe, it, expect, vi, beforeEach } from "vitest";
import { getDictationConfig, transcribeDictation } from "../dictation";
import { getClient } from "../acpConnection";

vi.mock("../acpConnection", () => ({
  getClient: vi.fn(),
}));

describe("dictation SDK wiring", () => {
  let client: any;
  beforeEach(() => {
    client = {
      goose: {
        GooseDictationConfig: vi.fn().mockResolvedValue({
          providers: {
            openai: {
              configured: true,
              description: "OpenAI transcription",
              usesProviderConfig: true,
              availableModels: [],
            },
          },
        }),
        GooseDictationTranscribe: vi.fn().mockResolvedValue({ text: "hello" }),
      },
    };
    vi.mocked(getClient).mockResolvedValue(client);
  });

  it("getDictationConfig calls GooseDictationConfig and returns providers map", async () => {
    const result = await getDictationConfig();
    expect(client.goose.GooseDictationConfig).toHaveBeenCalledWith({});
    expect(result.openai.configured).toBe(true);
  });

  it("transcribeDictation forwards audio + mimeType + provider", async () => {
    const result = await transcribeDictation({
      audio: "base64==",
      mimeType: "audio/webm",
      provider: "openai" as any,
    });
    expect(client.goose.GooseDictationTranscribe).toHaveBeenCalledWith({
      audio: "base64==",
      mimeType: "audio/webm",
      provider: "openai",
    });
    expect(result.text).toBe("hello");
  });
});
