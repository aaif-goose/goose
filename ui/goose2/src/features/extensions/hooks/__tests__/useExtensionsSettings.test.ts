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

  it("preserves an edited extension's enabled flag", async () => {
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
      true,
    );
  });

  it("saves new extensions as disabled catalog entries", async () => {
    const { result } = renderHook(() => useExtensionsSettings());
    const newExtension: ExtensionEntry = {
      ...enabledExtension,
      name: "linear",
      config_key: "linear",
    };

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    await act(async () => {
      await result.current.handleSubmit("linear", newExtension);
    });

    expect(mocks.addExtension).toHaveBeenCalledWith(
      "linear",
      newExtension,
      false,
    );
  });

  it("does not delete the new extension when renamed old-key removal fails", async () => {
    mocks.removeExtension.mockRejectedValueOnce(new Error("remove failed"));
    const { result } = renderHook(() => useExtensionsSettings());
    const renamedExtension: ExtensionEntry = {
      ...enabledExtension,
      name: "linear",
      config_key: "linear",
    };

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    act(() => {
      result.current.handleConfigure(enabledExtension);
    });
    await act(async () => {
      await result.current.handleSubmit("linear", renamedExtension);
    });

    expect(mocks.addExtension).toHaveBeenCalledWith(
      "linear",
      renamedExtension,
      true,
    );
    expect(mocks.removeExtension).toHaveBeenCalledTimes(1);
    expect(mocks.removeExtension).toHaveBeenCalledWith("github");
    expect(mocks.removeExtension).not.toHaveBeenCalledWith("linear");
    expect(mocks.toastError).toHaveBeenCalledWith(
      "extensions.errors.saveFailed",
    );
  });
});
