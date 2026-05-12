import { renderHook } from "@testing-library/react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it } from "vitest";
import { useDistroStore } from "@/features/settings/stores/distroStore";
import { useProviderCatalogStore } from "../stores/providerCatalogStore";
import { useProviderInventoryStore } from "../stores/providerInventoryStore";
import { useProviderInventory } from "./useProviderInventory";

function providerEntry(
  overrides: Partial<ProviderInventoryEntryDto>,
): ProviderInventoryEntryDto {
  const providerId = overrides.providerId ?? "openai";

  return {
    providerId,
    providerName: overrides.providerName ?? providerId,
    description: "",
    defaultModel: "",
    configured: true,
    providerType: "Preferred",
    category: "model",
    configKeys: [],
    setupSteps: [],
    supportsRefresh: true,
    refreshing: false,
    models: [],
    stale: false,
    ...overrides,
  };
}

describe("useProviderInventory", () => {
  beforeEach(() => {
    useProviderCatalogStore.getState().setEntries([
      {
        id: "openai",
        displayName: "OpenAI",
        category: "model",
        description: "GPT and o-series models",
        setupMethod: "config_fields",
        group: "default",
      },
      {
        id: "custom_deepseek",
        displayName: "DeepSeek",
        category: "model",
        description: "DeepSeek chat and reasoning models",
        setupMethod: "single_api_key",
        group: "additional",
      },
    ]);
    useDistroStore.setState({ loaded: false, manifest: { present: false } });
    useProviderInventoryStore.setState({
      entries: new Map(),
      loading: false,
    });
  });

  it("shows configured static, custom, and curated declarative model providers", () => {
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "openai",
        providerName: "OpenAI",
        providerType: "Preferred",
      }),
      providerEntry({
        providerId: "custom_acme_openai",
        providerName: "Acme OpenAI",
        providerType: "Custom",
      }),
      providerEntry({
        providerId: "custom_deepseek",
        providerName: "DeepSeek",
        providerType: "Declarative",
      }),
      providerEntry({
        providerId: "internal_declarative",
        providerName: "Internal Declarative",
        providerType: "Declarative",
      }),
      providerEntry({
        providerId: "unconfigured_custom",
        providerName: "Unconfigured Custom",
        providerType: "Custom",
        configured: false,
      }),
      providerEntry({
        providerId: "local",
        providerName: "Local",
        providerType: "Custom",
      }),
      providerEntry({
        providerId: "local_inference",
        providerName: "Local Inference",
        providerType: "Custom",
      }),
    ]);

    const { result } = renderHook(() => useProviderInventory());

    expect(
      result.current.configuredModelProviderEntries.map(
        (entry) => entry.providerId,
      ),
    ).toEqual(["openai", "custom_acme_openai", "custom_deepseek"]);
  });

  it("falls back to configured inventory providers before the catalog loads", () => {
    useProviderCatalogStore.getState().reset();
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "openai",
        providerName: "OpenAI",
        providerType: "Preferred",
        models: [{ id: "gpt-4o", name: "GPT-4o", recommended: true }],
      }),
      providerEntry({
        providerId: "custom_acme_openai",
        providerName: "Acme OpenAI",
        providerType: "Custom",
      }),
      providerEntry({
        providerId: "codex-acp",
        providerName: "Codex",
        providerType: "Builtin",
        category: "agent",
        models: [{ id: "current", name: "Current", recommended: true }],
      }),
      providerEntry({
        providerId: "local",
        providerName: "Local",
        providerType: "Custom",
      }),
      providerEntry({
        providerId: "unconfigured_anthropic",
        providerName: "Anthropic",
        providerType: "Preferred",
        configured: false,
      }),
    ]);

    const { result } = renderHook(() => useProviderInventory());

    expect(
      result.current.configuredModelProviderEntries.map(
        (entry) => entry.providerId,
      ),
    ).toEqual(["openai", "custom_acme_openai"]);
    expect(result.current.getModelsForAgent("goose")).toEqual([
      {
        id: "gpt-4o",
        name: "GPT-4o",
        displayName: "GPT-4o",
        provider: undefined,
        providerId: "openai",
        providerName: "OpenAI",
        contextLimit: undefined,
        recommended: true,
      },
    ]);
  });

  it("applies the provider allowlist before the catalog loads", () => {
    useProviderCatalogStore.getState().reset();
    useDistroStore.setState({
      loaded: true,
      manifest: { present: true, providerAllowlist: "anthropic" },
    });
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "openai",
        providerName: "OpenAI",
        providerType: "Preferred",
        models: [{ id: "gpt-4o", name: "GPT-4o", recommended: true }],
      }),
      providerEntry({
        providerId: "anthropic",
        providerName: "Anthropic",
        providerType: "Preferred",
        models: [{ id: "claude-sonnet", name: "Claude Sonnet" }],
      }),
    ]);

    const { result } = renderHook(() => useProviderInventory());

    expect(
      result.current.configuredModelProviderEntries.map(
        (entry) => entry.providerId,
      ),
    ).toEqual(["anthropic"]);
    expect(result.current.getModelsForAgent("goose")).toEqual([
      {
        id: "claude-sonnet",
        name: "Claude Sonnet",
        displayName: "Claude Sonnet",
        provider: undefined,
        providerId: "anthropic",
        providerName: "Anthropic",
        contextLimit: undefined,
        recommended: false,
      },
    ]);
  });

  it("aggregates custom provider models under Goose", () => {
    useProviderInventoryStore.getState().setEntries([
      providerEntry({
        providerId: "custom_acme_openai",
        providerName: "Acme OpenAI",
        providerType: "Custom",
        models: [
          {
            id: "acme-gpt-5",
            name: "Acme GPT-5",
            family: "acme",
            contextLimit: 128000,
            recommended: true,
          },
        ],
      }),
    ]);

    const { result } = renderHook(() => useProviderInventory());

    expect(result.current.getModelsForAgent("goose")).toEqual([
      {
        id: "acme-gpt-5",
        name: "Acme GPT-5",
        displayName: "Acme GPT-5",
        provider: "acme",
        providerId: "custom_acme_openai",
        providerName: "Acme OpenAI",
        contextLimit: 128000,
        recommended: true,
      },
    ]);
  });
});
