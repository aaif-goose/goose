import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { HomeScreen } from "./HomeScreen";

const setSelectedProvider = vi.fn();
const setSelectedProviderWithoutPersist = vi.fn();
const mockController = {
  handleSend: vi.fn(),
  projectMetadataPending: false,
  queue: { queuedMessage: null, dismiss: vi.fn() },
  stopStreaming: vi.fn(),
  chatState: "idle" as const,
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
  draftValue: "",
  handleDraftChange: vi.fn(),
  selectedPersonaId: null,
  handlePersonaChange: vi.fn(),
  handleCreatePersona: vi.fn(),
  pickerAgents: [
    { id: "goose", label: "Goose" },
    { id: "claude-acp", label: "Claude Code" },
  ],
  providersLoading: false,
  selectedProvider: "goose",
  handleProviderChange: setSelectedProvider,
  currentModelId: null,
  currentModelName: null,
  availableModels: [],
  modelsLoading: false,
  modelStatusMessage: null,
  handleModelChange: vi.fn(),
  selectedProjectId: null,
  availableProjects: [],
  handleProjectChange: vi.fn(),
  tokenState: { accumulatedTotal: 0, contextLimit: 0 },
  isContextUsageReady: false,
};

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

vi.mock("@/features/skills/api/skills", () => ({
  listSkills: vi.fn().mockResolvedValue([]),
}));

vi.mock("@/features/chat/hooks/useChatSessionController", () => ({
  useChatSessionController: () => mockController,
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
  const renderHome = () =>
    render(<HomeScreen sessionId="home-session" onActivateSession={vi.fn()} />);

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 2, 29, 14, 30, 0)); // 2:30 PM
    localStorage.clear();
    localStorage.setItem("goose:defaultProvider", "goose");
    setSelectedProvider.mockReset();
    setSelectedProviderWithoutPersist.mockReset();
  });

  it("renders the clock", () => {
    renderHome();
    expect(screen.getByText("2:30")).toBeInTheDocument();
    expect(screen.getByText("PM")).toBeInTheDocument();
  });

  it("shows afternoon greeting at 2:30 PM", () => {
    renderHome();
    expect(screen.getByText("Good afternoon")).toBeInTheDocument();
  });

  it("renders the chat input placeholder with default agent name when no persona selected", () => {
    renderHome();
    expect(
      screen.getByPlaceholderText("Chat with Goose or @ mention an agent"),
    ).toBeInTheDocument();
  });

  it("renders the agent/model chooser affordance", () => {
    renderHome();
    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toBeInTheDocument();
  });

  it("renders the provider and project controls on the home screen", () => {
    renderHome();
    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /select project/i }),
    ).toBeInTheDocument();
  });

  it("forwards agent selection through the shared session controller", async () => {
    vi.useRealTimers();
    const user = userEvent.setup();

    renderHome();

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );
    await user.click(screen.getByRole("button", { name: /claude code/i }));

    expect(setSelectedProvider).toHaveBeenLastCalledWith("claude-acp");
  });
});
