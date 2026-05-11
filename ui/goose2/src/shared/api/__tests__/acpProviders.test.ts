import { beforeEach, describe, expect, it } from "vitest";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { discoverAcpProvidersFromEntries } from "../acp";

describe("discoverAcpProvidersFromEntries", () => {
  beforeEach(() => {
    useProviderCatalogStore.getState().reset();
  });

  it("preserves agent inventory entries when the setup catalog has not loaded", () => {
    expect(
      discoverAcpProvidersFromEntries([
        {
          providerId: "codex-acp",
          providerName: "Codex",
          category: "agent",
        },
        {
          providerId: "openai",
          providerName: "OpenAI",
          category: "model",
        },
      ]),
    ).toEqual([
      { id: "goose", label: "Goose" },
      { id: "codex-acp", label: "Codex" },
    ]);
  });
});
