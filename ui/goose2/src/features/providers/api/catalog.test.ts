import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  listProviderSetupCatalog,
  mapProviderCatalogEntryDto,
} from "./catalog";

const mocks = vi.hoisted(() => ({
  catalogList: vi.fn(),
  getClient: vi.fn(),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mocks.getClient(),
}));

describe("provider setup catalog API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getClient.mockResolvedValue({
      goose: {
        GooseProvidersCatalogList: mocks.catalogList,
      },
    });
  });

  it("maps setup catalog DTO fields to provider catalog entries", () => {
    expect(
      mapProviderCatalogEntryDto({
        providerId: "claude-acp",
        name: "Claude Code",
        format: "",
        apiUrl: "",
        modelCount: 0,
        docUrl: "https://docs.anthropic.com/en/docs/claude-code",
        envVar: "",
        kind: "setup",
        category: "agent",
        description: "Anthropic's agentic coding tool",
        setupMethod: "cli_auth",
        binaryName: "claude-agent-acp",
        tier: "promoted",
        showOnlyWhenInstalled: false,
        aliases: ["claude-code", "Claude Code"],
        supportsInstall: true,
        supportsAuth: true,
        supportsAuthStatus: true,
      } as unknown as Parameters<typeof mapProviderCatalogEntryDto>[0]),
    ).toEqual({
      id: "claude-acp",
      displayName: "Claude Code",
      category: "agent",
      description: "Anthropic's agentic coding tool",
      setupMethod: "cli_auth",
      binaryName: "claude-agent-acp",
      docsUrl: "https://docs.anthropic.com/en/docs/claude-code",
      tier: "promoted",
      showOnlyWhenInstalled: false,
      aliases: ["claude-code", "Claude Code"],
      supportsInstall: true,
      supportsAuth: true,
      supportsAuthStatus: true,
    });
  });

  it("requests the setup catalog through ACP", async () => {
    mocks.catalogList.mockResolvedValue({
      providers: [
        {
          providerId: "ollama",
          name: "Ollama",
          format: "",
          apiUrl: "",
          modelCount: 0,
          docUrl: "",
          envVar: "",
          kind: "setup",
          category: "model",
          description: "Run local models",
          setupMethod: "config_fields",
          fields: [
            {
              key: "OLLAMA_HOST",
              label: "Host",
              secret: false,
              required: true,
            },
          ],
          tier: "promoted",
        },
      ],
    });

    await expect(listProviderSetupCatalog()).resolves.toEqual([
      {
        id: "ollama",
        displayName: "Ollama",
        category: "model",
        description: "Run local models",
        setupMethod: "config_fields",
        fields: [
          {
            key: "OLLAMA_HOST",
            label: "Host",
            secret: false,
            required: true,
          },
        ],
        tier: "promoted",
      },
    ]);
    expect(mocks.catalogList).toHaveBeenCalledWith({ kind: "setup" });
  });

  it("ignores non-setup catalog entries without a provider category", async () => {
    mocks.catalogList.mockResolvedValue({
      providers: [
        {
          providerId: "acme",
          name: "Acme AI",
          format: "openai",
          apiUrl: "https://api.acme.test/v1",
          modelCount: 1,
          docUrl: "https://acme.test/docs",
          envVar: "ACME_API_KEY",
        },
      ],
    });

    await expect(listProviderSetupCatalog()).resolves.toEqual([]);
  });
});
