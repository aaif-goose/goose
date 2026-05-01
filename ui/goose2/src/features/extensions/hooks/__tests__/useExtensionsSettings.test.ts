import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExtensionEntry } from "../../types";
import { useExtensionsSettings } from "../useExtensionsSettings";

const mocks = vi.hoisted(() => ({
  addExtension: vi.fn(),
  listExtensions: vi.fn(),
  removeExtension: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("../../api/extensions", () => ({
  addExtension: mocks.addExtension,
  listExtensions: mocks.listExtensions,
  removeExtension: mocks.removeExtension,
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("sonner", () => ({
  toast: { error: mocks.toastError },
}));

const enabledExtension: ExtensionEntry = {
  type: "stdio",
  name: "github",
  description: "Issue tracker",
  cmd: "npx",
  args: [],
  config_key: "github",
  enabled: true,
};

describe("useExtensionsSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listExtensions.mockResolvedValue([enabledExtension]);
    mocks.addExtension.mockResolvedValue(undefined);
    mocks.removeExtension.mockResolvedValue(undefined);
  });

  it("saves edited non-manager extensions as disabled catalog entries", async () => {
    const { result } = renderHook(() => useExtensionsSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    act(() => {
      result.current.handleConfigure(enabledExtension);
    });
    await act(async () => {
      await result.current.handleSubmit("github", enabledExtension);
    });

    expect(mocks.addExtension).toHaveBeenCalledWith(
      "github",
      enabledExtension,
      false,
    );
  });
});
