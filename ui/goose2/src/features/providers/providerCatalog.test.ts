import { beforeEach, describe, expect, it } from "vitest";
import type { ProviderCatalogEntry } from "@/shared/types/providers";
import {
  getAgentProviders,
  getCatalogEntry,
  getModelProviders,
  resolveAgentProviderCatalogId,
} from "./providerCatalog";
import { useProviderCatalogStore } from "./stores/providerCatalogStore";

const catalogEntries: ProviderCatalogEntry[] = [
  {
    id: "goose",
    displayName: "Goose",
    category: "agent",
    description: "Block's open-source coding agent",
    setupMethod: "none",
    group: "default",
    aliases: ["goose"],
  },
  {
    id: "claude-acp",
    displayName: "Claude Code",
    category: "agent",
    description: "Anthropic's agentic coding tool",
    setupMethod: "cli_auth",
    binaryName: "claude-agent-acp",
    group: "default",
    aliases: ["claude-acp", "claude_code", "claude"],
    supportsInstall: true,
    supportsAuth: true,
    supportsAuthStatus: true,
  },
  {
    id: "codex-acp",
    displayName: "Codex",
    category: "agent",
    description: "OpenAI's coding agent",
    setupMethod: "cli_auth",
    binaryName: "codex-acp",
    group: "default",
    aliases: ["codex-acp", "codex_cli", "codex"],
  },
  {
    id: "ollama",
    displayName: "Ollama",
    category: "model",
    description: "Run local or self-hosted models",
    setupMethod: "config_fields",
    fields: [
      {
        key: "OLLAMA_HOST",
        label: "Host",
        secret: false,
        required: true,
        placeholder: "localhost or http://localhost:11434",
        defaultValue: "http://localhost:11434",
      },
    ],
    docsUrl: "https://ollama.com",
    group: "default",
  },
];

describe("provider catalog selectors", () => {
  beforeEach(() => {
    useProviderCatalogStore.getState().reset();
  });

  it("returns the Goose fallback before the cache is loaded", () => {
    expect(getCatalogEntry("ollama")).toBeUndefined();
    expect(getAgentProviders().map((provider) => provider.id)).toEqual([
      "goose",
    ]);
    expect(getModelProviders()).toEqual([]);
  });

  it("uses loaded cache entries for provider selectors", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(getAgentProviders().map((provider) => provider.id)).toEqual([
      "goose",
      "claude-acp",
      "codex-acp",
    ]);
    expect(getModelProviders().map((provider) => provider.id)).toEqual([
      "ollama",
    ]);
    expect(getCatalogEntry("ollama")?.fields).toEqual([
      {
        key: "OLLAMA_HOST",
        label: "Host",
        secret: false,
        required: true,
        placeholder: "localhost or http://localhost:11434",
        defaultValue: "http://localhost:11434",
      },
    ]);
  });

  it("matches direct agent ids", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(resolveAgentProviderCatalogId("claude-acp", "Claude Code")).toBe(
      "claude-acp",
    );
  });

  it("matches backend-provided agent aliases", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(resolveAgentProviderCatalogId("codex-cli", "Codex CLI")).toBe(
      "codex-acp",
    );
    expect(
      resolveAgentProviderCatalogId("custom-id", "Claude Code (ACP)"),
    ).toBe("claude-acp");
  });

  it("matches suffixed agent labels from backend aliases", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(resolveAgentProviderCatalogId("custom-id", "Codex CLI (ACP)")).toBe(
      "codex-acp",
    );
  });

  it("does not match aliases embedded in unrelated labels", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(
      resolveAgentProviderCatalogId("custom-id", "Acme Claude Tools"),
    ).toBeNull();
    expect(
      resolveAgentProviderCatalogId("custom-id", "Codex compatible API"),
    ).toBeNull();
  });

  it("does not treat model providers as agents", () => {
    useProviderCatalogStore.getState().setEntries(catalogEntries);

    expect(resolveAgentProviderCatalogId("ollama", "Ollama")).toBeNull();
  });
});
