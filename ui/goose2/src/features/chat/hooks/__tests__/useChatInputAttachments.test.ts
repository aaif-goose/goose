import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mockResizeImage = vi.fn();

vi.mock("../../lib/resizeImage", () => ({
  resizeImage: (...args: unknown[]) => mockResizeImage(...args),
}));

vi.mock("@/shared/lib/platform", () => ({
  getPlatform: () => "mac",
}));

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://${path}`,
}));

vi.mock("@/shared/api/system", () => ({
  inspectAttachmentPaths: vi.fn(),
  readImageAttachment: vi.fn(),
}));

import { useChatInputAttachments } from "../useChatInputAttachments";

describe("useChatInputAttachments", () => {
  beforeEach(() => {
    mockResizeImage.mockReset();
  });

  it("keeps valid browser file attachments when an image read fails", async () => {
    mockResizeImage.mockRejectedValue(new Error("resize failed"));

    const fileReaderSpy = vi.spyOn(globalThis, "FileReader").mockImplementation(
      () =>
        ({
          onload: null,
          onerror: null,
          readAsDataURL() {
            this.onerror?.(new ProgressEvent("error"));
          },
        }) as unknown as FileReader,
    );

    const { result } = renderHook(() => useChatInputAttachments());

    await act(async () => {
      await result.current.addBrowserFiles([
        new File(["bad image"], "broken.png", { type: "image/png" }),
        new File(["report"], "report.txt", { type: "text/plain" }),
      ]);
    });

    expect(result.current.attachments).toEqual([
      expect.objectContaining({
        kind: "file",
        name: "report.txt",
        mimeType: "text/plain",
      }),
    ]);

    fileReaderSpy.mockRestore();
  });
});
