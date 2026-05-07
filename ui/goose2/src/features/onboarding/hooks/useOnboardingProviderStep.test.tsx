import { act, renderHook, waitFor } from "@testing-library/react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { getStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import type { OnboardingReadiness } from "../types";
import { useOnboardingProviderStep } from "./useOnboardingProviderStep";

const mocks = vi.hoisted(() => ({
  saveDefaults: vi.fn(),
}));

vi.mock("../api/onboarding", () => ({
  saveDefaults: mocks.saveDefaults,
}));

vi.mock("@/features/providers/hooks/useCredentials", () => ({
  useCredentials: () => ({
    configuredIds: new Set<string>(),
    loading: false,
    savingProviderIds: new Set<string>(),
    syncingProviderIds: new Set<string>(),
    inventoryWarnings: new Map<string, string[]>(),
    getConfig: vi.fn(),
    save: vi.fn(),
    remove: vi.fn(),
    completeNativeSetup: vi.fn(),
  }),
}));

function providerEntry(
  overrides: Partial<ProviderInventoryEntryDto>,
): ProviderInventoryEntryDto {
  const providerId = overrides.providerId ?? "anthropic";

  return {
    providerId,
    providerName: overrides.providerName ?? providerId,
    description: "",
    defaultModel: overrides.defaultModel ?? "claude-sonnet-4-5",
    configured: true,
    providerType: "Preferred",
    category: "model",
    configKeys: [],
    setupSteps: [],
    supportsRefresh: true,
    refreshing: false,
    models: [
      {
        id: "claude-sonnet-4-5",
        name: "Claude Sonnet 4.5",
        family: "claude",
        contextLimit: 200000,
        recommended: true,
      },
    ],
    stale: false,
    ...overrides,
  };
}

function readyReadiness(
  overrides: Partial<OnboardingReadiness> = {},
): OnboardingReadiness {
  return {
    hasCompletedOnboarding: true,
    isUsable: true,
    providerId: "anthropic",
    modelId: "claude-sonnet-4-5",
    modelName: "Claude Sonnet 4.5",
    reason: "ready",
    ...overrides,
  };
}

function renderProviderStep(readiness: OnboardingReadiness) {
  const onSelectedSetup = vi.fn();
  const onReady = vi.fn();

  const result = renderHook(() =>
    useOnboardingProviderStep({
      readiness,
      t: (key) => key,
      onSelectedSetup,
      onReady,
    }),
  );

  return {
    ...result,
    onReady,
    onSelectedSetup,
  };
}

describe("useOnboardingProviderStep", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    mocks.saveDefaults.mockResolvedValue({
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
    });
    useAgentStore.setState({
      selectedProvider: "goose",
      providers: [],
    });
    useProviderInventoryStore.setState({
      entries: new Map(),
      loading: false,
    });
    useProviderCatalogStore.getState().setEntries([
      {
        id: "anthropic",
        displayName: "Anthropic",
        category: "model",
        description: "",
        setupMethod: "single_api_key",
        group: "default",
      },
      {
        id: "claude-acp",
        displayName: "Claude Code",
        category: "agent",
        description: "",
        setupMethod: "cli_auth",
        group: "default",
      },
    ]);
    useProviderInventoryStore.getState().setEntries([providerEntry({})]);
  });

  it("saves Goose defaults before continuing with the current model setup", async () => {
    const { result, onReady, onSelectedSetup } = renderProviderStep(
      readyReadiness(),
    );

    act(() => {
      result.current.onContinue();
    });

    await waitFor(() => expect(onReady).toHaveBeenCalledTimes(1));
    expect(mocks.saveDefaults).toHaveBeenCalledWith({
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
    });
    expect(onSelectedSetup).toHaveBeenCalledWith({
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
      modelName: "Claude Sonnet 4.5",
    });
    expect(useAgentStore.getState().selectedProvider).toBe("goose");
    expect(getStoredModelPreference("goose")).toMatchObject({
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
    });
  });

  it("continues with a current ACP agent without writing Goose model defaults", async () => {
    const { result, onReady, onSelectedSetup } = renderProviderStep(
      readyReadiness({
        providerId: "claude-acp",
        modelId: "default",
        modelName: "Claude Code",
      }),
    );

    act(() => {
      result.current.onContinue();
    });

    await waitFor(() => expect(onReady).toHaveBeenCalledTimes(1));
    expect(mocks.saveDefaults).not.toHaveBeenCalled();
    expect(onSelectedSetup).toHaveBeenCalledWith({
      providerId: "claude-acp",
      modelId: "default",
      modelName: "Claude Code",
    });
    expect(useAgentStore.getState().selectedProvider).toBe("claude-acp");
    expect(getStoredModelPreference("goose")).toBeNull();
  });
});
