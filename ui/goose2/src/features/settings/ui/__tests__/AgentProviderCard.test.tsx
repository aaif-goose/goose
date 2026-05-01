import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

  afterEach(() => {
    vi.useRealTimers();
  });

  it("delays the checking indicator and does not show sign in while auth status is checking", async () => {
    vi.useFakeTimers();
    let resolveAuth!: (authenticated: boolean) => void;
    const authPromise = new Promise<boolean>((resolve) => {
      resolveAuth = resolve;
    });

    checkAgentInstalled.mockResolvedValue(true);
    checkAgentAuth.mockReturnValue(authPromise);

    renderWithProviders(<AgentProviderCard provider={createProvider()} />);

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(checkAgentAuth).toHaveBeenCalled();

    expect(screen.queryByRole("status", { name: "Checking..." })).toBeNull();
    expect(screen.queryByText("Checking...")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /sign in/i }),
    ).not.toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(
      screen.getByRole("status", { name: "Checking..." }),
    ).toBeInTheDocument();

    await act(async () => {
      resolveAuth(false);
      await authPromise;
    });

    expect(
      screen.getByRole("button", { name: /sign in/i }),
    ).toBeInTheDocument();
  });

  it("does not show the checking indicator when auth resolves quickly", async () => {
    vi.useFakeTimers();
    checkAgentInstalled.mockResolvedValue(true);
    checkAgentAuth.mockResolvedValue(true);

    renderWithProviders(<AgentProviderCard provider={createProvider()} />);

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(screen.queryByRole("status", { name: "Checking..." })).toBeNull();
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
