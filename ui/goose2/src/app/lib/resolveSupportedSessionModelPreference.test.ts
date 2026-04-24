import { beforeEach, describe, expect, it, vi } from "vitest";
import { resolveSupportedSessionModelPreference } from "./resolveSupportedSessionModelPreference";

const mockGetProviderInventory = vi.fn();

vi.mock("@/features/providers/api/inventory", () => ({
  getProviderInventory: (...args: unknown[]) =>
    mockGetProviderInventory(...args),
}));

describe("resolveSupportedSessionModelPreference", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
  });

  it("drops the model when provider inventory lookup fails", async () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "gpt-5.4",
          modelName: "GPT-5.4",
          providerId: "openai",
        },
      }),
    );
    mockGetProviderInventory.mockRejectedValue(
      new Error("inventory unavailable"),
    );

    await expect(
      resolveSupportedSessionModelPreference("goose", new Map()),
    ).resolves.toEqual({
      providerId: "openai",
    });
  });

  it("drops the model when provider inventory has no matching entry", async () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "gpt-5.4",
          modelName: "GPT-5.4",
          providerId: "openai",
        },
      }),
    );
    mockGetProviderInventory.mockResolvedValue([]);

    await expect(
      resolveSupportedSessionModelPreference("goose", new Map()),
    ).resolves.toEqual({
      providerId: "openai",
    });
  });
});
