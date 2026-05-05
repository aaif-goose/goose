import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  listProviderSetupCatalog,
  mapProviderSetupCatalogEntryDto,
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
        GooseProvidersSetupCatalogList: mocks.catalogList,
      },
    });
  });

  it("maps setup catalog DTO fields to provider catalog entries", () => {
    expect(
      mapProviderSetupCatalogEntryDto({
        providerId: "claude-acp",
        name: "Claude Code",
        docUrl: "https://docs.anthropic.com/en/docs/claude-code",
        category: "agent",
        description: "Anthropic's agentic coding tool",
        setupMethod: "cli_auth",
        binaryName: "claude-agent-acp",
        group: "default",
        showOnlyWhenInstalled: false,
        aliases: ["claude-acp", "claude_code", "claude"],
        supportsInstall: true,
        supportsAuth: true,
        supportsAuthStatus: true,
      }),
    ).toEqual({
      id: "claude-acp",
      displayName: "Claude Code",
      category: "agent",
      description: "Anthropic's agentic coding tool",
      setupMethod: "cli_auth",
      binaryName: "claude-agent-acp",
      docsUrl: "https://docs.anthropic.com/en/docs/claude-code",
      group: "default",
      showOnlyWhenInstalled: false,
      aliases: ["claude-acp", "claude_code", "claude"],
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
          group: "default",
          showOnlyWhenInstalled: false,
          supportsInstall: false,
          supportsAuth: false,
          supportsAuthStatus: false,
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
        group: "default",
        showOnlyWhenInstalled: false,
        supportsInstall: false,
        supportsAuth: false,
        supportsAuthStatus: false,
      },
    ]);
    expect(mocks.catalogList).toHaveBeenCalledWith({});
  });
});
