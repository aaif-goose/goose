import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocalWhisperModels } from "../LocalWhisperModels";

const mockListDictationLocalModels = vi.fn();
const mockDownloadDictationLocalModel = vi.fn();
const mockGetDictationLocalModelDownloadProgress = vi.fn();
const mockCancelDictationLocalModelDownload = vi.fn();
const mockDeleteDictationLocalModel = vi.fn();

vi.mock("@/shared/api/dictation", () => ({
  listDictationLocalModels: (...args: unknown[]) =>
    mockListDictationLocalModels(...args),
  downloadDictationLocalModel: (...args: unknown[]) =>
    mockDownloadDictationLocalModel(...args),
  getDictationLocalModelDownloadProgress: (...args: unknown[]) =>
    mockGetDictationLocalModelDownloadProgress(...args),
  cancelDictationLocalModelDownload: (...args: unknown[]) =>
    mockCancelDictationLocalModelDownload(...args),
  deleteDictationLocalModel: (...args: unknown[]) =>
    mockDeleteDictationLocalModel(...args),
}));

describe("LocalWhisperModels", () => {
  beforeEach(() => {
    mockListDictationLocalModels.mockReset();
    mockDownloadDictationLocalModel.mockReset();
    mockGetDictationLocalModelDownloadProgress.mockReset();
    mockCancelDictationLocalModelDownload.mockReset();
    mockDeleteDictationLocalModel.mockReset();
  });

  it("clears cached progress when cancelling a download", async () => {
    const user = userEvent.setup();
    const onModelsChanged = vi.fn();

    mockListDictationLocalModels
      .mockResolvedValueOnce([
        {
          id: "tiny",
          description: "Tiny model",
          sizeMb: 75,
          downloaded: false,
          downloadInProgress: true,
        },
      ])
      .mockResolvedValueOnce([
        {
          id: "tiny",
          description: "Tiny model",
          sizeMb: 75,
          downloaded: false,
          downloadInProgress: false,
        },
      ]);
    mockGetDictationLocalModelDownloadProgress.mockResolvedValue({
      bytesDownloaded: 100,
      totalBytes: 1000,
      progressPercent: 10,
      status: "downloading",
      error: null,
    });
    mockCancelDictationLocalModelDownload.mockResolvedValue(undefined);

    render(
      <LocalWhisperModels
        selectedModelId=""
        onSelectModel={vi.fn()}
        onModelsChanged={onModelsChanged}
      />,
    );

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /cancel/i }),
      ).toBeInTheDocument(),
    );

    await waitFor(
      () =>
        expect(mockGetDictationLocalModelDownloadProgress).toHaveBeenCalledWith(
          "tiny",
        ),
      { timeout: 2000 },
    );

    await user.click(screen.getByRole("button", { name: /cancel/i }));

    await waitFor(() =>
      expect(mockCancelDictationLocalModelDownload).toHaveBeenCalledWith(
        "tiny",
      ),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /download/i }),
      ).toBeInTheDocument(),
    );

    expect(
      screen.queryByRole("button", { name: /cancel/i }),
    ).not.toBeInTheDocument();
    expect(onModelsChanged).not.toHaveBeenCalled();
  });
});
