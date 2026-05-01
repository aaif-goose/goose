import { act, renderHook } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readProjectIcon, scanProjectIcons } from "../../api/projects";
import { DEFAULT_PROJECT_ICON } from "../../lib/projectIcons";
import { useProjectIconSelection } from "../useProjectIconSelection";

vi.mock("../../api/projects", () => ({
  scanProjectIcons: vi.fn().mockResolvedValue([]),
  readProjectIcon: vi.fn().mockResolvedValue({
    icon: "data:image/png;base64,aWNvbg==",
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("useProjectIconSelection", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.mocked(scanProjectIcons).mockResolvedValue([]);
    vi.mocked(readProjectIcon).mockResolvedValue({
      icon: "data:image/png;base64,aWNvbg==",
    });
    vi.mocked(open).mockResolvedValue(null);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("scans project working dirs after a short debounce", async () => {
    vi.mocked(scanProjectIcons).mockResolvedValueOnce([
      {
        id: "/repo/public/logo.svg",
        label: "public/logo.svg",
        icon: "data:image/svg+xml;base64,bG9nbw==",
        sourceDir: "repo",
      },
    ]);

    const { result } = renderHook(() =>
      useProjectIconSelection({
        isOpen: true,
        prompt: "include: /repo",
      }),
    );

    expect(result.current.iconScanPending).toBe(true);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(250);
    });

    expect(scanProjectIcons).toHaveBeenCalledWith(["/repo"]);
    expect(result.current.iconCandidates).toHaveLength(1);
    expect(result.current.iconScanPending).toBe(false);
  });

  it("clears scanned candidates when the dialog is closed", () => {
    const { result } = renderHook(() =>
      useProjectIconSelection({
        isOpen: false,
        prompt: "include: /repo",
      }),
    );

    expect(result.current.iconCandidates).toEqual([]);
    expect(result.current.iconScanPending).toBe(false);
    expect(scanProjectIcons).not.toHaveBeenCalled();
  });

  it("resets and chooses icons while clearing icon errors", async () => {
    vi.mocked(open).mockResolvedValueOnce("/tmp/logo.png");

    const { result } = renderHook(() =>
      useProjectIconSelection({
        isOpen: true,
        prompt: "",
      }),
    );

    act(() => {
      result.current.chooseIcon("tabler:code");
    });
    expect(result.current.icon).toBe("tabler:code");

    await act(async () => {
      await result.current.chooseCustomIcon({
        title: "Custom icon",
        filterName: "Images",
      });
    });

    expect(open).toHaveBeenCalledWith({
      directory: false,
      multiple: false,
      title: "Custom icon",
      filters: [
        {
          name: "Images",
          extensions: ["svg", "png", "ico", "jpg", "jpeg", "webp"],
        },
      ],
    });
    expect(readProjectIcon).toHaveBeenCalledWith("/tmp/logo.png");
    expect(result.current.icon).toBe("data:image/png;base64,aWNvbg==");

    act(() => {
      result.current.resetIcon(null);
    });
    expect(result.current.icon).toBe(DEFAULT_PROJECT_ICON);
  });

  it("surfaces custom icon upload errors", async () => {
    vi.mocked(open).mockResolvedValueOnce("/tmp/large-icon.png");
    vi.mocked(readProjectIcon).mockRejectedValueOnce("Icon file is too large");

    const { result } = renderHook(() =>
      useProjectIconSelection({
        isOpen: true,
        prompt: "",
      }),
    );

    await act(async () => {
      await result.current.chooseCustomIcon({
        title: "Custom icon",
        filterName: "Images",
      });
    });

    expect(result.current.iconError).toBe("Icon file is too large");
  });
});
