import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  INVENTORY_POLL_DELAYS_MS,
  syncProviderInventory,
} from "./inventorySync";

function inventoryEntry(
  providerId: string,
  refreshing: boolean,
  lastRefreshError: string | null = null,
): ProviderInventoryEntryDto {
  return {
    providerId,
    providerName: providerId,
    description: "",
    defaultModel: "default-model",
    configured: true,
    providerType: "remote",
    category: "model",
    configKeys: [],
    setupSteps: [],
    supportsRefresh: true,
    refreshing,
    models: [],
    lastUpdatedAt: null,
    lastRefreshAttemptAt: null,
    lastRefreshError,
    stale: false,
    modelSelectionHint: null,
  };
}

describe("syncProviderInventory", () => {
  const getInventory = vi.fn();
  const refreshInventory = vi.fn();
  const onEntries = vi.fn();
  const sleep = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    sleep.mockResolvedValue(undefined);
  });

  it("refreshes and polls with the startup delay schedule until entries settle", async () => {
    refreshInventory.mockResolvedValue({
      started: ["anthropic"],
      skipped: [],
    });
    getInventory
      .mockResolvedValueOnce([inventoryEntry("anthropic", true)])
      .mockResolvedValueOnce([inventoryEntry("anthropic", false)]);

    const result = await syncProviderInventory(["anthropic"], {
      getInventory,
      refreshInventory,
      onEntries,
      sleep,
    });

    expect(refreshInventory).toHaveBeenCalledWith(["anthropic"]);
    expect(getInventory).toHaveBeenNthCalledWith(1, ["anthropic"]);
    expect(getInventory).toHaveBeenNthCalledWith(2, ["anthropic"]);
    expect(sleep).toHaveBeenCalledTimes(1);
    expect(sleep).toHaveBeenCalledWith(INVENTORY_POLL_DELAYS_MS[0]);
    expect(onEntries).toHaveBeenCalledTimes(2);
    expect(result.settled).toBe(true);
    expect(result.entries).toEqual([inventoryEntry("anthropic", false)]);
  });

  it("uses an initial refresh acknowledgement without starting another refresh", async () => {
    const initialRefresh = {
      started: ["anthropic"],
      skipped: [],
    };
    getInventory.mockResolvedValueOnce([inventoryEntry("anthropic", false)]);

    const result = await syncProviderInventory(["anthropic"], {
      getInventory,
      refreshInventory,
      initialRefresh,
      sleep,
    });

    expect(refreshInventory).not.toHaveBeenCalled();
    expect(getInventory).toHaveBeenCalledWith(["anthropic"]);
    expect(result.refresh).toBe(initialRefresh);
    expect(result.settled).toBe(true);
  });

  it("polls providers skipped because they were already refreshing", async () => {
    refreshInventory.mockResolvedValue({
      started: [],
      skipped: [
        {
          providerId: "anthropic",
          reason: "already_refreshing",
        },
      ],
    });
    getInventory
      .mockResolvedValueOnce([inventoryEntry("anthropic", true)])
      .mockResolvedValueOnce([inventoryEntry("anthropic", false)]);

    const result = await syncProviderInventory(["anthropic"], {
      getInventory,
      refreshInventory,
      sleep,
    });

    expect(result.polledProviderIds).toEqual(["anthropic"]);
    expect(getInventory).toHaveBeenNthCalledWith(2, ["anthropic"]);
    expect(result.settled).toBe(true);
  });

  it("merges skipped not-configured entries without forcing a warning state", async () => {
    refreshInventory.mockResolvedValue({
      started: [],
      skipped: [
        {
          providerId: "anthropic",
          reason: "not_configured",
        },
      ],
    });
    getInventory.mockResolvedValueOnce([
      {
        ...inventoryEntry("anthropic", false),
        configured: false,
      },
    ]);

    const result = await syncProviderInventory(["anthropic"], {
      getInventory,
      refreshInventory,
      sleep,
    });

    expect(result.settled).toBe(true);
    expect(result.entries[0]?.configured).toBe(false);
    expect(result.entries[0]?.lastRefreshError).toBeNull();
    expect(sleep).not.toHaveBeenCalled();
  });
});
