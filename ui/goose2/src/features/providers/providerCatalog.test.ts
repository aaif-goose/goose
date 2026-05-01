import { describe, expect, it } from "vitest";
import {
  getCatalogEntry,
  getModelProviders,
  resolveAgentProviderCatalogId,
} from "./providerCatalog";

describe("provider catalog", () => {
  it("exposes Ollama host configuration", () => {
    const ollama = getCatalogEntry("ollama");

    expect(ollama?.setupMethod).toBe("config_fields");
    expect(ollama?.fields).toEqual([
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

  it("uses backend model provider ids for the curated catalog", () => {
    const ids = getModelProviders().map((provider) => provider.id);

    expect(ids).toEqual([
      "anthropic",
      "google",
      "chatgpt_codex",
      "openai",
      "mistral",
      "ollama",
      "openrouter",
      "databricks",
      "github_copilot",
      "custom_deepseek",
      "xai",
      "groq",
      "azure_openai",
      "aws_bedrock",
      "gcp_vertex_ai",
      "litellm",
      "lmstudio",
      "nvidia",
      "cerebras",
      "snowflake",
    ]);
    expect(ids).not.toContain("azure");
    expect(ids).not.toContain("bedrock");
    expect(ids).not.toContain("deepseek");
    expect(ids).not.toContain("local_inference");
  });

  it("marks the planned promoted model providers", () => {
    const promotedIds = getModelProviders()
      .filter((provider) => provider.tier === "promoted")
      .map((provider) => provider.id);

    expect(promotedIds).toEqual([
      "anthropic",
      "google",
      "chatgpt_codex",
      "openai",
      "mistral",
      "ollama",
      "openrouter",
      "databricks",
      "github_copilot",
    ]);
  });
});

describe("resolveAgentProviderCatalogId", () => {
  it("matches direct catalog ids", () => {
    expect(resolveAgentProviderCatalogId("cursor-agent", "Cursor Agent")).toBe(
      "cursor-agent",
    );
  });

  it("matches common agent aliases", () => {
    expect(resolveAgentProviderCatalogId("codex-cli", "Codex CLI")).toBe(
      "codex-acp",
    );
    expect(resolveAgentProviderCatalogId("claude-code", "Claude Code")).toBe(
      "claude-acp",
    );
  });

  it("does not treat model providers as agents", () => {
    expect(
      resolveAgentProviderCatalogId("databricks", "Databricks"),
    ).toBeNull();
  });

  it("matches fuzzy agent labels with extra suffixes", () => {
    expect(
      resolveAgentProviderCatalogId("custom-id", "Claude Code (ACP)"),
    ).toBe("claude-acp");
    expect(resolveAgentProviderCatalogId("custom-id", "Codex CLI (ACP)")).toBe(
      "codex-acp",
    );
    expect(
      resolveAgentProviderCatalogId("custom-id", "Cursor Agent Stable"),
    ).toBe("cursor-agent");
  });
});
