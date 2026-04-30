import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  checkAllProviderStatus,
  deleteProviderConfig,
  getProviderConfig,
  saveProviderConfig,
} from "./credentials";

const mocks = vi.hoisted(() => ({
  configRead: vi.fn(),
  configSave: vi.fn(),
  configDelete: vi.fn(),
  configStatus: vi.fn(),
  getClient: vi.fn(),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mocks.getClient(),
}));

describe("provider credential API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getClient.mockResolvedValue({
      goose: {
        GooseProvidersConfigRead: mocks.configRead,
        GooseProvidersConfigSave: mocks.configSave,
        GooseProvidersConfigDelete: mocks.configDelete,
        GooseProvidersConfigStatus: mocks.configStatus,
      },
    });
  });

  it("reads provider config fields through the ACP provider config endpoint", async () => {
    const fields = [
      {
        key: "ANTHROPIC_API_KEY",
        value: "sk-ant-********",
        isSet: true,
        isSecret: true,
        required: true,
      },
    ];
    mocks.configRead.mockResolvedValue({ fields });

    await expect(getProviderConfig("anthropic")).resolves.toEqual(fields);

    expect(mocks.configRead).toHaveBeenCalledWith({
      providerId: "anthropic",
    });
  });

  it("saves provider config fields as one batch through ACP", async () => {
    const fields = [
      {
        key: "ANTHROPIC_API_KEY",
        value: "sk-ant-test",
      },
      {
        key: "ANTHROPIC_HOST",
        value: "https://api.anthropic.com",
      },
    ];
    const response = {
      status: {
        providerId: "anthropic",
        isConfigured: true,
      },
      refresh: {
        started: ["anthropic"],
        skipped: [],
      },
    };
    mocks.configSave.mockResolvedValue(response);

    await expect(saveProviderConfig("anthropic", fields)).resolves.toEqual(
      response,
    );

    expect(mocks.configSave).toHaveBeenCalledWith({
      providerId: "anthropic",
      fields,
    });
  });

  it("deletes provider config through ACP", async () => {
    const response = {
      status: {
        providerId: "anthropic",
        isConfigured: false,
      },
      refresh: {
        started: [],
        skipped: [
          {
            providerId: "anthropic",
            reason: "not_configured",
          },
        ],
      },
    };
    mocks.configDelete.mockResolvedValue(response);

    await expect(deleteProviderConfig("anthropic")).resolves.toEqual(response);

    expect(mocks.configDelete).toHaveBeenCalledWith({
      providerId: "anthropic",
    });
  });

  it("checks provider status through ACP", async () => {
    const statuses = [
      {
        providerId: "anthropic",
        isConfigured: true,
      },
    ];
    mocks.configStatus.mockResolvedValue({ statuses });

    await expect(checkAllProviderStatus()).resolves.toEqual(statuses);

    expect(mocks.configStatus).toHaveBeenCalledWith({
      providerIds: [],
    });
  });
});
