import { renderHook } from "@testing-library/react";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it } from "vitest";
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
