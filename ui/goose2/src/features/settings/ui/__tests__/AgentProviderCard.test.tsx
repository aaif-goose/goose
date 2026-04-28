import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "@/test/render";
import { AgentProviderCard } from "../AgentProviderCard";
import type { ProviderDisplayInfo } from "@/shared/types/providers";

const checkAgentInstalled = vi.fn();
const checkAgentAuth = vi.fn();

vi.mock("@/features/providers/api/agentSetup", () => ({
  checkAgentInstalled: (...args: unknown[]) => checkAgentInstalled(...args),
  checkAgentAuth: (...args: unknown[]) => checkAgentAuth(...args),
  installAgent: vi.fn(),
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
});
