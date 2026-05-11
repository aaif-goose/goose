import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCredentials } from "./useCredentials";

const mocks = vi.hoisted(() => ({
  checkAllProviderStatus: vi.fn(),
  deleteProviderConfig: vi.fn(),
  getProviderConfig: vi.fn(),
  refreshProviderInventory: vi.fn(),
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

vi.mock("@/features/providers/api/inventory", () => ({
  refreshProviderInventory: mocks.refreshProviderInventory,
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
    mocks.refreshProviderInventory.mockResolvedValue({
      started: ["anthropic"],
      skipped: [],
    });
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
    const voiceConfigListener = vi.fn();
    window.addEventListener(
      "goose:voice-dictation-config",
      voiceConfigListener,
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
    expect(voiceConfigListener).toHaveBeenCalledTimes(1);
    expect(result.current).not.toHaveProperty("needsRestart");
    expect(result.current).not.toHaveProperty("restart");

    window.removeEventListener(
      "goose:voice-dictation-config",
      voiceConfigListener,
    );
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

  it("suppresses stale refresh errors after deleting provider config", async () => {
    const voiceConfigListener = vi.fn();
    window.addEventListener(
      "goose:voice-dictation-config",
      voiceConfigListener,
    );

    mocks.syncProviderInventory.mockResolvedValueOnce({
      entries: [
        {
          providerId: "anthropic",
          lastRefreshError: "old refresh failure",
          refreshing: false,
        },
      ],
      refresh: deleteResponse.refresh,
      settled: true,
      polledProviderIds: ["anthropic"],
    });
    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.remove("anthropic");
    });

    await waitFor(() =>
      expect(result.current.syncingProviderIds.has("anthropic")).toBe(false),
    );
    expect(voiceConfigListener).toHaveBeenCalledTimes(1);
    expect(result.current.inventoryWarnings.has("anthropic")).toBe(false);

    window.removeEventListener(
      "goose:voice-dictation-config",
      voiceConfigListener,
    );
  });

  it("invalidates native OAuth secrets before refreshing provider status", async () => {
    const refreshResponse = {
      started: ["chatgpt_codex"],
      skipped: [],
    };
    mocks.checkAllProviderStatus
      .mockResolvedValueOnce([
        {
          providerId: "chatgpt_codex",
          isConfigured: false,
        },
      ])
      .mockResolvedValueOnce([
        {
          providerId: "chatgpt_codex",
          isConfigured: true,
        },
      ]);
    mocks.refreshProviderInventory.mockResolvedValueOnce(refreshResponse);

    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.configuredIds.has("chatgpt_codex")).toBe(false);

    await act(async () => {
      await result.current.completeNativeSetup("chatgpt_codex");
    });

    expect(mocks.refreshProviderInventory).toHaveBeenCalledWith([
      "chatgpt_codex",
    ]);
    expect(
      mocks.refreshProviderInventory.mock.invocationCallOrder[0],
    ).toBeLessThan(mocks.checkAllProviderStatus.mock.invocationCallOrder[1]);
    expect(mocks.syncProviderInventory).toHaveBeenCalledWith(
      ["chatgpt_codex"],
      expect.objectContaining({
        initialRefresh: refreshResponse,
      }),
    );
    expect(result.current.configuredIds.has("chatgpt_codex")).toBe(true);
  });

  it("uses native OAuth ACP result without an extra status refresh", async () => {
    const refreshResponse = {
      started: ["chatgpt_codex"],
      skipped: [],
    };
    mocks.checkAllProviderStatus.mockResolvedValueOnce([
      {
        providerId: "chatgpt_codex",
        isConfigured: false,
      },
    ]);

    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.completeNativeSetup("chatgpt_codex", {
        status: {
          providerId: "chatgpt_codex",
          isConfigured: true,
        },
        refresh: refreshResponse,
      });
    });

    expect(mocks.refreshProviderInventory).not.toHaveBeenCalled();
    expect(mocks.checkAllProviderStatus).toHaveBeenCalledTimes(1);
    expect(mocks.syncProviderInventory).toHaveBeenCalledWith(
      ["chatgpt_codex"],
      expect.objectContaining({
        initialRefresh: refreshResponse,
      }),
    );
    expect(result.current.configuredIds.has("chatgpt_codex")).toBe(true);
  });

  it("refreshes native OAuth status when initial inventory refresh fails", async () => {
    mocks.checkAllProviderStatus
      .mockResolvedValueOnce([
        {
          providerId: "chatgpt_codex",
          isConfigured: false,
        },
      ])
      .mockResolvedValueOnce([
        {
          providerId: "chatgpt_codex",
          isConfigured: true,
        },
      ]);
    mocks.refreshProviderInventory.mockRejectedValueOnce(
      new Error("refresh unavailable"),
    );

    const { result } = renderHook(() => useCredentials());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.configuredIds.has("chatgpt_codex")).toBe(false);

    await act(async () => {
      await result.current.completeNativeSetup("chatgpt_codex");
    });

    expect(mocks.refreshProviderInventory).toHaveBeenCalledWith([
      "chatgpt_codex",
    ]);
    expect(result.current.configuredIds.has("chatgpt_codex")).toBe(true);
    expect(mocks.syncProviderInventory).toHaveBeenCalledWith(
      ["chatgpt_codex"],
      expect.objectContaining({
        initialRefresh: undefined,
      }),
    );
  });
});
