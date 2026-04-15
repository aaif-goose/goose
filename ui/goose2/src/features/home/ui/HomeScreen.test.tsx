import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { HomeScreen } from "./HomeScreen";

const setSelectedProvider = vi.fn();
const setSelectedProviderWithoutPersist = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  discoverAcpProviders: vi.fn().mockResolvedValue([
    { id: "goose", label: "Goose" },
    { id: "claude-acp", label: "Claude Code" },
  ]),
}));

vi.mock("@/features/providers/hooks/useAgentProviderStatus", () => ({
  useAgentProviderStatus: () => ({
    readyAgentIds: new Set(["goose", "claude-acp", "codex-acp"]),
    loading: false,
    refresh: vi.fn(),
  }),
}));

vi.mock("@/features/agents/hooks/useProviderSelection", () => ({
  useProviderSelection: () => ({
    providers: [
      { id: "goose", label: "Goose" },
      { id: "claude-acp", label: "Claude Code" },
    ],
    providersLoading: false,
    selectedProvider: "goose",
    setSelectedProvider,
    setSelectedProviderWithoutPersist,
  }),
}));

// HomeScreen now reads personas from the agent store, not from ACP providers
vi.mock("@/features/agents/stores/agentStore", async (importOriginal) => {
  const actual =
    await importOriginal<
      typeof import("@/features/agents/stores/agentStore")
    >();
  return {
    ...actual,
    useAgentStore: Object.assign(
      (selector?: (s: unknown) => unknown) => {
        const state = {
          personas: [
            {
              id: "builtin-solo",
              displayName: "Solo",
              systemPrompt: "You are Solo.",
              provider: "openai",
              description: null,
              avatar: null,
              createdBy: null,
              source: "custom",
              extensions: [],
              metadata: null,
              sortOrder: 0,
              isDefault: false,
            },
            {
              id: "builtin-goose",
              displayName: "Goosey",
              systemPrompt: "You are Goosey.",
              isBuiltin: true,
              description: null,
              avatar: null,
              createdBy: null,
              source: "custom",
              extensions: [],
              metadata: null,
              sortOrder: 1,
              isDefault: false,
              createdAt: "",
              updatedAt: "",
            },
          ],
          personasLoading: false,
          openPersonaEditor: vi.fn(),
        };
        return selector ? selector(state) : state;
      },
      { getState: () => ({ openPersonaEditor: vi.fn() }) },
    ),
  };
});

describe("HomeScreen", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 2, 29, 14, 30, 0)); // 2:30 PM
    localStorage.clear();
    localStorage.setItem("goose:defaultProvider", "goose");
    setSelectedProvider.mockReset();
    setSelectedProviderWithoutPersist.mockReset();
  });

  it("renders the clock", () => {
    render(<HomeScreen />);
    expect(screen.getByText("2:30")).toBeInTheDocument();
    expect(screen.getByText("PM")).toBeInTheDocument();
  });

  it("shows afternoon greeting at 2:30 PM", () => {
    render(<HomeScreen />);
    expect(screen.getByText("Good afternoon")).toBeInTheDocument();
  });

  it("renders the chat input placeholder with default agent name when no persona selected", () => {
    render(<HomeScreen />);
    expect(
      screen.getByPlaceholderText("Message Goose, @ to mention personas"),
    ).toBeInTheDocument();
  });

  it("renders the assistant chooser affordance", () => {
    render(<HomeScreen />);
    expect(
      screen.getByRole("button", { name: /choose assistant/i }),
    ).toBeInTheDocument();
  });

  it("renders the provider and project controls on the home screen", () => {
    render(<HomeScreen />);
    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /select project/i }),
    ).toBeInTheDocument();
  });

  it("reverts to the stored provider when a persona override is cleared", async () => {
    vi.useRealTimers();
    const user = userEvent.setup();

    render(<HomeScreen />);

    await user.click(screen.getByRole("button", { name: /choose assistant/i }));
    await user.click(screen.getByRole("menuitem", { name: /solo/i }));

    expect(setSelectedProviderWithoutPersist).toHaveBeenLastCalledWith(
      "openai",
    );

    await user.click(
      screen.getByRole("button", { name: /clear active assistant/i }),
    );

    expect(setSelectedProviderWithoutPersist).toHaveBeenLastCalledWith("goose");
  });
});
