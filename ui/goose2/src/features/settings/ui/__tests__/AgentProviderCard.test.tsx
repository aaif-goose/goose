import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "@/test/render";
import { AgentProviderCard } from "../AgentProviderCard";
import type { ProviderDisplayInfo } from "@/shared/types/providers";

const checkAgentInstalled = vi.fn();
const checkAgentAuth = vi.fn();
const installAgent = vi.fn();

vi.mock("@/features/providers/api/agentSetup", () => ({
  checkAgentInstalled: (...args: unknown[]) => checkAgentInstalled(...args),
  checkAgentAuth: (...args: unknown[]) => checkAgentAuth(...args),
  installAgent: (...args: unknown[]) => installAgent(...args),
  authenticateAgent: vi.fn(),
  onAgentSetupOutput: vi.fn(async () => vi.fn()),
}));

function createProvider(): ProviderDisplayInfo {
  return {
    id: "claude-acp",
    displayName: "Claude",
    category: "agent",
    description: "Claude provider",
    setupMethod: "cli_auth",
    binaryName: "claude",
    authCommand: "claude auth login",
    authStatusCommand: "claude auth status",
    tier: "standard",
    status: "not_installed",
  };
}

describe("AgentProviderCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("does not show sign in while auth status is checking", async () => {
    let resolveAuth!: (authenticated: boolean) => void;
    const authPromise = new Promise<boolean>((resolve) => {
      resolveAuth = resolve;
    });

    checkAgentInstalled.mockResolvedValue(true);
    checkAgentAuth.mockReturnValue(authPromise);

    renderWithProviders(<AgentProviderCard provider={createProvider()} />);

    expect(await screen.findByText("Checking...")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /sign in/i }),
    ).not.toBeInTheDocument();

    await act(async () => {
      resolveAuth(false);
      await authPromise;
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /sign in/i }),
      ).toBeInTheDocument();
    });
  });

  it("checks installation by provider id after installing", async () => {
    const user = userEvent.setup();
    checkAgentInstalled
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    installAgent.mockResolvedValue(undefined);

    renderWithProviders(
      <AgentProviderCard
        provider={{
          ...createProvider(),
          authCommand: undefined,
          authStatusCommand: undefined,
          installCommand: "npm install -g claude-agent-acp",
        }}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: /install claude/i }),
    );

    await waitFor(() => {
      expect(checkAgentInstalled).toHaveBeenNthCalledWith(2, "claude-acp");
    });
  });
});
