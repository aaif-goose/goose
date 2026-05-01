import type { SessionExtensionStatusDto } from "@aaif/goose-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listSessionExtensions } from "./extensions";

const mocks = vi.hoisted(() => ({
  getClient: vi.fn(),
  sessionExtensionsStatus: vi.fn(),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: () => mocks.getClient(),
}));

describe("extension API", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getClient.mockResolvedValue({
      goose: {
        GooseSessionExtensionsStatus: mocks.sessionExtensionsStatus,
      },
    });
  });

  it("normalizes generated session extension status DTOs for the UI", async () => {
    const status: SessionExtensionStatusDto = {
      type: "frontend",
      name: "Artifacts",
      description: "Render artifacts",
      frontend_tools: [{ name: "render" }],
      config_key: "artifacts",
      status: "connected",
      error: null,
    };
    mocks.sessionExtensionsStatus.mockResolvedValue({ extensions: [status] });

    await expect(listSessionExtensions("session-1")).resolves.toEqual([
      {
        ...status,
        tools: [],
        error: undefined,
      },
    ]);

    expect(mocks.sessionExtensionsStatus).toHaveBeenCalledWith({
      sessionId: "session-1",
    });
  });
});
