import { act, renderHook, waitFor } from "@testing-library/react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProviderInventoryStore } from "../stores/providerInventoryStore";
import { useCustomProviders } from "./useCustomProviders";

const mocks = vi.hoisted(() => ({
  createCustomProvider: vi.fn(),
  deleteCustomProvider: vi.fn(),
  getCustomProviderTemplate: vi.fn(),
  listCustomProviderCatalog: vi.fn(),
  readCustomProvider: vi.fn(),
  updateCustomProvider: vi.fn(),
  syncProviderInventory: vi.fn(),
}));

vi.mock("../api/customProviders", () => ({
  createCustomProvider: mocks.createCustomProvider,
  deleteCustomProvider: mocks.deleteCustomProvider,
  getCustomProviderTemplate: mocks.getCustomProviderTemplate,
  listCustomProviderCatalog: mocks.listCustomProviderCatalog,
  readCustomProvider: mocks.readCustomProvider,
  updateCustomProvider: mocks.updateCustomProvider,
}));

vi.mock("../api/inventorySync", () => ({
  syncProviderInventory: mocks.syncProviderInventory,
}));

function providerEntry(providerId: string): ProviderInventoryEntryDto {
  return {
    providerId,
    providerName: "Acme AI",
    description: "",
    defaultModel: "acme-large",
    configured: true,
    providerType: "Custom",
    category: "model",
    configKeys: [],
    setupSteps: [],
    supportsRefresh: true,
    refreshing: false,
    models: [],
    stale: false,
  };
}

describe("useCustomProviders", () => {
  const input = {
    engine: "openai_compatible" as const,
    displayName: "Acme AI",
    apiUrl: "https://api.acme.test/v1",
    apiKey: "secret",
    models: ["acme-large"],
    supportsStreaming: true,
    requiresAuth: true,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    useProviderInventoryStore.getState().setEntries([]);
    mocks.createCustomProvider.mockResolvedValue({
      providerId: "acme_ai",
      status: { providerId: "acme_ai", isConfigured: true },
      refresh: { started: ["acme_ai"], skipped: [] },
    });
    mocks.updateCustomProvider.mockResolvedValue({
      providerId: "acme_ai",
      status: { providerId: "acme_ai", isConfigured: true },
      refresh: { started: ["acme_ai"], skipped: [] },
    });
    mocks.deleteCustomProvider.mockResolvedValue({
      providerId: "acme_ai",
      refresh: { started: [], skipped: [] },
    });
    mocks.readCustomProvider.mockResolvedValue({
      provider: {
        providerId: "acme_ai",
        engine: "openai_compatible",
        displayName: "Acme AI",
        apiUrl: "https://api.acme.test/v1",
        models: ["acme-large"],
        supportsStreaming: true,
        headers: {},
        requiresAuth: true,
        apiKeySet: true,
      },
      editable: true,
      status: { providerId: "acme_ai", isConfigured: true },
    });
    mocks.listCustomProviderCatalog.mockResolvedValue([]);
    mocks.syncProviderInventory.mockImplementation(
      async (_providerIds, options) => {
        const entries = [providerEntry("acme_ai")];
        options?.onEntries?.(entries);
        return {
          entries,
          refresh: { started: ["acme_ai"], skipped: [] },
          settled: true,
          polledProviderIds: ["acme_ai"],
        };
      },
    );
  });

  it("loads catalog providers into hook state", async () => {
    const providers = [
      {
        providerId: "acme",
        name: "Acme AI",
        format: "openai",
        apiUrl: "https://api.acme.test/v1",
        modelCount: 1,
        docUrl: "https://acme.test/docs",
        envVar: "ACME_API_KEY",
      },
    ];
    mocks.listCustomProviderCatalog.mockResolvedValue(providers);
    const { result } = renderHook(() => useCustomProviders());

    await act(async () => {
      await result.current.loadCatalog("openai");
    });

    expect(result.current.catalog).toEqual(providers);
    expect(mocks.listCustomProviderCatalog).toHaveBeenCalledWith("openai");
  });

  it("creates a provider, tracks configured status, and merges inventory entries", async () => {
    const { result } = renderHook(() => useCustomProviders());

    await act(async () => {
      await result.current.create(input);
    });

    expect(mocks.createCustomProvider).toHaveBeenCalledWith(input);
    expect(result.current.configuredIds.has("acme_ai")).toBe(true);
    await waitFor(() =>
      expect(
        useProviderInventoryStore.getState().entries.get("acme_ai"),
      ).toEqual(providerEntry("acme_ai")),
    );
  });

  it("reads a provider and merges its status", async () => {
    const { result } = renderHook(() => useCustomProviders());

    await act(async () => {
      await result.current.read("acme_ai");
    });

    expect(mocks.readCustomProvider).toHaveBeenCalledWith("acme_ai");
    expect(result.current.configuredIds.has("acme_ai")).toBe(true);
  });

  it("updates from a validated draft", async () => {
    const { result } = renderHook(() => useCustomProviders());

    await act(async () => {
      await result.current.saveDraft({
        providerId: "acme_ai",
        editable: true,
        ...input,
        apiKeySet: false,
        basePath: "",
        modelsInput: "acme-large",
        headers: [],
        authInitiallyEnabled: true,
      });
    });

    expect(mocks.updateCustomProvider).toHaveBeenCalledWith("acme_ai", input);
  });

  it("removes stale inventory entries after deleting a custom provider", async () => {
    mocks.syncProviderInventory.mockResolvedValueOnce({
      entries: [],
      refresh: { started: [], skipped: [] },
      settled: true,
      polledProviderIds: ["acme_ai"],
    });
    useProviderInventoryStore.getState().setEntries([providerEntry("acme_ai")]);
    const { result } = renderHook(() => useCustomProviders());

    await act(async () => {
      await result.current.remove("acme_ai");
    });

    expect(mocks.deleteCustomProvider).toHaveBeenCalledWith("acme_ai");
    expect(useProviderInventoryStore.getState().entries.has("acme_ai")).toBe(
      false,
    );
  });
});
