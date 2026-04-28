import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCredentials } from "./useCredentials";

const mocks = vi.hoisted(() => ({
  checkAllProviderStatus: vi.fn(),
  deleteProviderConfig: vi.fn(),
  getProviderConfig: vi.fn(),
  saveProviderConfig: vi.fn(),
  syncProviderInventory: vi.fn(),
}));

vi.mock("@/features/providers/api/credentials", () => ({
  checkAllProviderStatus: mocks.checkAllProviderStatus,
  deleteProviderConfig: mocks.deleteProviderConfig,
  getProviderConfig: mocks.getProviderConfig,
  saveProviderConfig: mocks.saveProviderConfig,
}));

vi.mock("@/features/providers/api/inventorySync", () => ({
  syncProviderInventory: mocks.syncProviderInventory,
}));

describe("useCredentials", () => {
  const saveResponse = {
    status: {
      providerId: "anthropic",
      isConfigured: true,
    },
    refresh: {
      started: ["anthropic"],
      skipped: [],
    },
  };
  const deleteResponse = {
    status: {
      providerId: "anthropic",
      isConfigured: false,
    },
    refresh: {
      started: [],
      skipped: [
        {
          providerId: "anthropic",
          reason: "not_configured",
        },
      ],
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.checkAllProviderStatus.mockResolvedValue([
      {
        providerId: "anthropic",
        isConfigured: true,
      },
    ]);
    mocks.saveProviderConfig.mockResolvedValue(saveResponse);
    mocks.deleteProviderConfig.mockResolvedValue(deleteResponse);
    mocks.syncProviderInventory.mockResolvedValue({
      entries: [],
      refresh: {
        started: ["anthropic"],
        skipped: [],
      },
      settled: true,
      polledProviderIds: ["anthropic"],
    });
  });

  it("saves secret fields through the credential API and syncs inventory without requiring restart", async () => {
    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.save("anthropic", [
        {
          key: "ANTHROPIC_API_KEY",
          value: "sk-ant-test",
          isSecret: true,
        },
      ]);
    });

    const fields = [
      {
        key: "ANTHROPIC_API_KEY",
        value: "sk-ant-test",
      },
    ];

    expect(mocks.saveProviderConfig).toHaveBeenCalledWith("anthropic", fields);
    await waitFor(() =>
      expect(mocks.syncProviderInventory.mock.calls[0]?.[0]).toEqual([
        "anthropic",
      ]),
    );
    expect(mocks.syncProviderInventory.mock.calls[0]?.[1]).toEqual(
      expect.objectContaining({
        initialRefresh: saveResponse.refresh,
      }),
    );
    expect(result.current).not.toHaveProperty("needsRestart");
    expect(result.current).not.toHaveProperty("restart");
  });

  it("records refresh failure as a provider warning without rejecting the save", async () => {
    mocks.syncProviderInventory.mockRejectedValueOnce(
      new Error("model list failed"),
    );
    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.save("anthropic", [
        {
          key: "ANTHROPIC_API_KEY",
          value: "sk-ant-test",
          isSecret: true,
        },
      ]);
    });

    expect(mocks.saveProviderConfig).toHaveBeenCalled();
    await waitFor(() =>
      expect(result.current.inventoryWarnings.get("anthropic")).toContain(
        "model list failed",
      ),
    );
  });
});
