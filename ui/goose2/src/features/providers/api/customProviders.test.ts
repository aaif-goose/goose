import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createCustomProvider,
  deleteCustomProvider,
  getCustomProviderTemplate,
  listCustomProviderCatalog,
  readCustomProvider,
  updateCustomProvider,
} from "./customProviders";

const mocks = vi.hoisted(() => ({
  catalogList: vi.fn(),
  catalogTemplate: vi.fn(),
  customCreate: vi.fn(),
  customRead: vi.fn(),
  customUpdate: vi.fn(),
  customDelete: vi.fn(),
  getClient: vi.fn(),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mocks.getClient(),
}));

describe("custom provider API", () => {
  const input = {
    engine: "openai_compatible" as const,
    displayName: "Acme AI",
    apiUrl: "https://api.acme.test/v1",
    apiKey: "secret",
    models: ["acme-large"],
    supportsStreaming: true,
    headers: {
      "X-Acme": "goose",
    },
    requiresAuth: true,
    catalogProviderId: "acme",
    basePath: "/v1",
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getClient.mockResolvedValue({
      goose: {
        GooseProvidersCatalogList: mocks.catalogList,
        GooseProvidersCatalogTemplate: mocks.catalogTemplate,
        GooseProvidersCustomCreate: mocks.customCreate,
        GooseProvidersCustomRead: mocks.customRead,
        GooseProvidersCustomUpdate: mocks.customUpdate,
        GooseProvidersCustomDelete: mocks.customDelete,
      },
    });
  });

  it("lists catalog providers with the planned typed ACP method", async () => {
    const providers = [
      {
        providerId: "acme",
        name: "Acme AI",
        format: "openai",
        apiUrl: "https://api.acme.test/v1",
        modelCount: 1,
        docUrl: "https://acme.test/docs",
        envVar: "ACME_API_KEY",
      },
    ];
    mocks.catalogList.mockResolvedValue({ providers });

    await expect(listCustomProviderCatalog("openai")).resolves.toEqual(
      providers,
    );

    expect(mocks.catalogList).toHaveBeenCalledWith({ format: "openai" });
  });

  it("reads a catalog template through the planned typed ACP method", async () => {
    const template = {
      providerId: "acme",
      name: "Acme AI",
      format: "openai",
      apiUrl: "https://api.acme.test/v1",
      models: [],
      supportsStreaming: true,
      envVar: "ACME_API_KEY",
      docUrl: "https://acme.test/docs",
    };
    mocks.catalogTemplate.mockResolvedValue({ template });

    await expect(getCustomProviderTemplate("acme")).resolves.toEqual(template);

    expect(mocks.catalogTemplate).toHaveBeenCalledWith({
      providerId: "acme",
    });
  });

  it("creates, reads, updates, and deletes custom providers by generated method name", async () => {
    const createResponse = {
      providerId: "acme_ai",
      status: { providerId: "acme_ai", isConfigured: true },
      refresh: { started: ["acme_ai"], skipped: [] },
    };
    const readResponse = {
      provider: {
        providerId: "acme_ai",
        ...input,
        headers: input.headers ?? {},
        apiKeyEnv: "ACME_AI_API_KEY",
        apiKeySet: true,
      },
      editable: true,
      status: { providerId: "acme_ai", isConfigured: true },
    };
    const updateResponse = createResponse;
    const deleteResponse = {
      providerId: "acme_ai",
      refresh: { started: [], skipped: [] },
    };
    mocks.customCreate.mockResolvedValue(createResponse);
    mocks.customRead.mockResolvedValue(readResponse);
    mocks.customUpdate.mockResolvedValue(updateResponse);
    mocks.customDelete.mockResolvedValue(deleteResponse);

    await expect(createCustomProvider(input)).resolves.toEqual(createResponse);
    await expect(readCustomProvider("acme_ai")).resolves.toEqual(readResponse);
    await expect(updateCustomProvider("acme_ai", input)).resolves.toEqual(
      updateResponse,
    );
    await expect(deleteCustomProvider("acme_ai")).resolves.toEqual(
      deleteResponse,
    );

    expect(mocks.customCreate).toHaveBeenCalledWith(input);
    expect(mocks.customRead).toHaveBeenCalledWith({ providerId: "acme_ai" });
    expect(mocks.customUpdate).toHaveBeenCalledWith({
      ...input,
      providerId: "acme_ai",
    });
    expect(mocks.customDelete).toHaveBeenCalledWith({ providerId: "acme_ai" });
  });

  it("lets the explicit update target override a conflicting runtime provider id", async () => {
    mocks.customUpdate.mockResolvedValue({
      providerId: "acme_ai",
      status: { providerId: "acme_ai", isConfigured: true },
      refresh: { started: [], skipped: [] },
    });

    await updateCustomProvider("acme_ai", {
      ...input,
      providerId: "wrong_id",
    } as typeof input & { providerId: string });

    expect(mocks.customUpdate).toHaveBeenCalledWith({
      ...input,
      providerId: "acme_ai",
    });
  });
});
