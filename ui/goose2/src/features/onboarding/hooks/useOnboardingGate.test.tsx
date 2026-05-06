import { act, renderHook } from "@testing-library/react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { setStoredModelPreference } from "@/features/chat/lib/modelPreferences";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { ONBOARDING_STORAGE_KEY } from "../types";
import { useOnboardingGate } from "./useOnboardingGate";

function providerEntry(
  overrides: Partial<ProviderInventoryEntryDto>,
): ProviderInventoryEntryDto {
  const providerId = overrides.providerId ?? "anthropic";

  return {
    providerId,
    providerName: overrides.providerName ?? providerId,
    description: "",
    defaultModel: "claude-sonnet-4-5",
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

function writeCompletedOnboarding(providerId = "anthropic", modelId?: string) {
  window.localStorage.setItem(
    ONBOARDING_STORAGE_KEY,
    JSON.stringify({
      completedAt: "2026-05-05T12:00:00.000Z",
      providerId,
      modelId,
    }),
  );
}

describe("useOnboardingGate", () => {
  beforeEach(() => {
    window.localStorage.clear();
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
        group: "additional",
      },
    ]);
  });

  it("shows onboarding after startup when there is no completed state", () => {
    useProviderInventoryStore.getState().setEntries([providerEntry({})]);

    const { result } = renderHook(() => useOnboardingGate(true));

    expect(result.current.shouldShowOnboarding).toBe(true);
    expect(result.current.readiness.reason).toBe("ready");
  });

  it("skips onboarding when completion and the selected Goose model are usable", () => {
    writeCompletedOnboarding("anthropic", "claude-sonnet-4-5");
    setStoredModelPreference("goose", {
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
      modelName: "Claude Sonnet 4.5",
    });
    useProviderInventoryStore.getState().setEntries([providerEntry({})]);

    const { result } = renderHook(() => useOnboardingGate(true));

    expect(result.current.shouldShowOnboarding).toBe(false);
    expect(result.current.readiness.isUsable).toBe(true);
    expect(result.current.readiness.providerId).toBe("anthropic");
  });

  it("reopens onboarding when the completed Goose provider is no longer usable", () => {
    writeCompletedOnboarding("anthropic", "claude-sonnet-4-5");
    setStoredModelPreference("goose", {
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
      modelName: "Claude Sonnet 4.5",
    });
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        configured: false,
        models: [],
      }),
    ]);

    const { result } = renderHook(() => useOnboardingGate(true));

    expect(result.current.shouldShowOnboarding).toBe(true);
    expect(result.current.readiness.isUsable).toBe(false);
    expect(result.current.readiness.reason).toBe("missing_provider");
  });

  it("treats a completed ACP agent provider with models as usable", () => {
    writeCompletedOnboarding("claude-acp", "claude-acp-session");
    useAgentStore.setState({ selectedProvider: "claude-acp" });
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "claude-acp",
        providerName: "Claude Code",
        providerType: "Acp",
        category: "agent",
        defaultModel: "claude-acp-session",
        models: [
          {
            id: "claude-acp-session",
            name: "Claude Code",
            family: "acp",
            contextLimit: null,
            recommended: true,
          },
        ],
      }),
    ]);

    const { result } = renderHook(() => useOnboardingGate(true));

    expect(result.current.shouldShowOnboarding).toBe(false);
    expect(result.current.readiness.providerId).toBe("claude-acp");
    expect(result.current.readiness.reason).toBe("ready");
  });

  it("falls back to a usable Goose model when the selected ACP agent is unusable", () => {
    writeCompletedOnboarding("claude-acp", "claude-acp-session");
    setStoredModelPreference("goose", {
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
      modelName: "Claude Sonnet 4.5",
    });
    useAgentStore.setState({ selectedProvider: "claude-acp" });
    useProviderInventoryStore.getState().setEntries([
      providerEntry({}),
      providerEntry({
        providerId: "claude-acp",
        providerName: "Claude Code",
        providerType: "Acp",
        category: "agent",
        defaultModel: "claude-acp-session",
        configured: false,
        models: [],
      }),
    ]);

    const { result } = renderHook(() => useOnboardingGate(true));

    expect(result.current.shouldShowOnboarding).toBe(false);
    expect(result.current.readiness.isUsable).toBe(true);
    expect(result.current.readiness.providerId).toBe("anthropic");
    expect(result.current.readiness.reason).toBe("ready");
  });

  it("persists completion from the onboarding flow", () => {
    const { result } = renderHook(() => useOnboardingGate(true));

    act(() => {
      result.current.completeOnboarding({
        providerId: "anthropic",
        modelId: "claude-sonnet-4-5",
      });
    });

    expect(
      JSON.parse(window.localStorage.getItem(ONBOARDING_STORAGE_KEY) ?? "{}"),
    ).toMatchObject({
      providerId: "anthropic",
      modelId: "claude-sonnet-4-5",
    });
  });
});
