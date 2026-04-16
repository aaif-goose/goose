import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  cancelDictationLocalModelDownload,
  deleteDictationLocalModel,
  downloadDictationLocalModel,
  getDictationConfig,
  getDictationLocalModelDownloadProgress,
  listDictationLocalModels,
  saveDictationModelSelection,
  transcribeDictation,
} from "../dictation";
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

  it("saveDictationModelSelection calls GooseDictationModelSelect", async () => {
    client.goose.GooseDictationModelSelect = vi.fn().mockResolvedValue({});
    await saveDictationModelSelection("local" as any, "tiny");
    expect(client.goose.GooseDictationModelSelect).toHaveBeenCalledWith({
      provider: "local",
      modelId: "tiny",
    });
  });

  it("listDictationLocalModels returns the models array", async () => {
    client.goose.GooseDictationModelsList = vi.fn().mockResolvedValue({
      models: [
        {
          id: "tiny",
          description: "Tiny",
          sizeMb: 75,
          downloaded: true,
          downloadInProgress: false,
        },
      ],
    });
    const result = await listDictationLocalModels();
    expect(client.goose.GooseDictationModelsList).toHaveBeenCalledWith({});
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("tiny");
  });

  it("downloadDictationLocalModel forwards modelId", async () => {
    client.goose.GooseDictationModelsDownload = vi.fn().mockResolvedValue({});
    await downloadDictationLocalModel("tiny");
    expect(client.goose.GooseDictationModelsDownload).toHaveBeenCalledWith({
      modelId: "tiny",
    });
  });

  it("getDictationLocalModelDownloadProgress returns progress or null", async () => {
    client.goose.GooseDictationModelsDownloadProgress = vi
      .fn()
      .mockResolvedValue({
        progress: {
          bytesDownloaded: 100,
          totalBytes: 1000,
          progressPercent: 10,
          status: "downloading",
          error: null,
        },
      });
    const result = await getDictationLocalModelDownloadProgress("tiny");
    expect(result?.bytesDownloaded).toBe(100);
    expect(
      client.goose.GooseDictationModelsDownloadProgress,
    ).toHaveBeenCalledWith({
      modelId: "tiny",
    });
  });

  it("getDictationLocalModelDownloadProgress returns null when no download", async () => {
    client.goose.GooseDictationModelsDownloadProgress = vi
      .fn()
      .mockResolvedValue({
        progress: undefined,
      });
    const result = await getDictationLocalModelDownloadProgress("tiny");
    expect(result).toBeNull();
  });

  it("cancelDictationLocalModelDownload forwards modelId", async () => {
    client.goose.GooseDictationModelsCancel = vi.fn().mockResolvedValue({});
    await cancelDictationLocalModelDownload("tiny");
    expect(client.goose.GooseDictationModelsCancel).toHaveBeenCalledWith({
      modelId: "tiny",
    });
  });

  it("deleteDictationLocalModel forwards modelId", async () => {
    client.goose.GooseDictationModelsDelete = vi.fn().mockResolvedValue({});
    await deleteDictationLocalModel("tiny");
    expect(client.goose.GooseDictationModelsDelete).toHaveBeenCalledWith({
      modelId: "tiny",
    });
  });
});
