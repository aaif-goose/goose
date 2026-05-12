import { describe, expect, it } from "vitest";
import { resolveSelectedAgentId } from "../agentProviderResolution";
import type { ProviderCatalogEntry } from "@/shared/types/providers";

const catalogEntries: ProviderCatalogEntry[] = [
  {
    id: "claude-acp",
    displayName: "Claude Code",
    category: "agent",
    description: "Claude Code",
    setupMethod: "cli_auth",
    group: "default",
    aliases: ["claude-acp", "claude_code", "claude"],
  },
  {
    id: "openai",
    displayName: "OpenAI",
    category: "model",
    description: "OpenAI",
    setupMethod: "single_api_key",
    group: "default",
  },
];

const noInventory = () => undefined;

describe("resolveSelectedAgentId", () => {
  it("returns goose when no provider is selected", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded: true,
        selectedProvider: undefined,
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("goose");
  });

  it("resolves known agent from catalog", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded: true,
        selectedProvider: "claude-acp",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("claude-acp");
  });

  it("returns goose for model providers with catalog loaded", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded: true,
        selectedProvider: "openai",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("goose");
  });

  it("preserves persisted claude-acp during empty inventory before catalog loads", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries: [],
        catalogLoaded: false,
        selectedProvider: "claude-acp",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("claude-acp");
  });

  it("preserves unknown provider before catalog loads when inventory is empty", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries: [],
        catalogLoaded: false,
        selectedProvider: "some-future-agent",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("some-future-agent");
  });

  it("falls back to goose for model provider identified by inventory before catalog", () => {
    const getEntry = (id: string) =>
      id === "openai"
        ? ({
            providerId: "openai",
            category: "model" as const,
            configured: true,
            refreshing: false,
            models: [],
          } as never)
        : undefined;

    expect(
      resolveSelectedAgentId({
        catalogEntries: [],
        catalogLoaded: false,
        selectedProvider: "openai",
        getProviderInventoryEntry: getEntry,
      }),
    ).toBe("goose");
  });

  it("preserves agent provider identified by inventory before catalog", () => {
    const getEntry = (id: string) =>
      id === "claude-acp"
        ? ({
            providerId: "claude-acp",
            category: "agent" as const,
            configured: true,
            refreshing: false,
            models: [],
          } as never)
        : undefined;

    expect(
      resolveSelectedAgentId({
        catalogEntries: [],
        catalogLoaded: false,
        selectedProvider: "claude-acp",
        getProviderInventoryEntry: getEntry,
      }),
    ).toBe("claude-acp");
  });

  it("falls back to goose after catalog validates provider as non-agent", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded: true,
        selectedProvider: "openai",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("goose");
  });

  it("falls back to goose after catalog validates unknown provider", () => {
    expect(
      resolveSelectedAgentId({
        catalogEntries,
        catalogLoaded: true,
        selectedProvider: "nonexistent-provider",
        getProviderInventoryEntry: noInventory,
      }),
    ).toBe("goose");
  });
});
