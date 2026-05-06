import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { useResolvedAgentModelPicker } from "../useResolvedAgentModelPicker";

const mockUseProviderInventory = vi.fn();
const mockUseAgentModelPickerState = vi.fn();
const mockGetClient = vi.fn();
const mockAcpSetModel = vi.fn();

vi.mock("@/features/providers/hooks/useProviderInventory", () => ({
  useProviderInventory: () => mockUseProviderInventory(),
}));

vi.mock("../useAgentModelPickerState", () => ({
  useAgentModelPickerState: (args: unknown) =>
    mockUseAgentModelPickerState(args),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: (...args: unknown[]) => mockGetClient(...args),
}));

vi.mock("@/shared/api/acp", () => ({
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

describe("useResolvedAgentModelPicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    useProviderCatalogStore.getState().reset();
    useProviderCatalogStore.getState().setEntries([
      {
        id: "codex-acp",
        displayName: "Codex CLI",
        category: "agent",
        description: "Codex CLI",
        setupMethod: "cli_auth",
        group: "default",
        aliases: ["codex-acp", "codex_cli", "codex"],
      },
      {
        id: "claude-acp",
        displayName: "Claude Code",
        category: "agent",
        description: "Claude Code",
        setupMethod: "cli_auth",
        group: "default",
        aliases: ["claude-acp", "claude_code", "claude"],
      },
      {
        id: "openai",
        displayName: "OpenAI",
        category: "model",
        description: "OpenAI",
        setupMethod: "single_api_key",
        group: "default",
      },
    ]);

    mockGetClient.mockResolvedValue({
      goose: {
        GooseDefaultsRead: vi.fn().mockResolvedValue({
          providerId: null,
          modelId: null,
        }),
      },
    });
    mockAcpSetModel.mockResolvedValue(undefined);

    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "codex-acp"
          ? {
              providerId: "codex-acp",
              category: "agent",
              defaultModel: "gpt-5.4",
              models: [
                {
                  id: "gpt-5.4",
                  name: "GPT-5.4",
                  recommended: true,
                },
              ],
            }
          : undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onProviderSelected,
      }: {
        onProviderSelected: (providerId: string) => void;
      }) => ({
        pickerAgents: [
          { id: "goose", label: "Goose" },
          { id: "codex-acp", label: "Codex" },
        ],
        availableModels: [],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: (providerId: string) =>
          onProviderSelected(providerId),
        handleModelChange: vi.fn(),
      }),
    );
  });

  it("selects the agent default model when switching to a provider without a saved model", () => {
    const setPendingProviderId = vi.fn();
    const setPendingModelSelection = vi.fn();
    const setGlobalSelectedProvider = vi.fn();

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "codex-acp", label: "Codex" },
        ],
        selectedProvider: "goose",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId,
        setPendingModelSelection,
        setGlobalSelectedProvider,
        prepareSelectedProvider: vi.fn(),
      }),
    );

    act(() => {
      result.current.handleProviderChange("codex-acp");
    });

    expect(setGlobalSelectedProvider).toHaveBeenCalledWith("codex-acp");
    expect(setPendingProviderId).toHaveBeenCalledWith("codex-acp");
    expect(setPendingModelSelection).toHaveBeenCalledWith({
      id: "gpt-5.4",
      name: "GPT-5.4",
      providerId: "codex-acp",
      source: "default",
    });
  });

  it("selects the saved model when switching back to an agent", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        "codex-acp": {
          modelId: "gpt-5.4-mini",
          modelName: "GPT-5.4 mini",
          providerId: "codex-acp",
        },
      }),
    );

    const setPendingProviderId = vi.fn();
    const setPendingModelSelection = vi.fn();
    const setGlobalSelectedProvider = vi.fn();

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "codex-acp", label: "Codex" },
        ],
        selectedProvider: "goose",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId,
        setPendingModelSelection,
        setGlobalSelectedProvider,
        prepareSelectedProvider: vi.fn(),
      }),
    );

    act(() => {
      result.current.handleProviderChange("codex-acp");
    });

    expect(setGlobalSelectedProvider).toHaveBeenCalledWith("codex-acp");
    expect(setPendingProviderId).toHaveBeenCalledWith("codex-acp");
    expect(setPendingModelSelection).toHaveBeenCalledWith({
      id: "gpt-5.4-mini",
      name: "GPT-5.4 mini",
      providerId: "codex-acp",
      source: "explicit",
    });
  });

  it("keeps explicit concrete provider requests authoritative", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "claude-sonnet-4",
          modelName: "Claude Sonnet 4",
          providerId: "anthropic",
        },
      }),
    );

    const setPendingProviderId = vi.fn();
    const setPendingModelSelection = vi.fn();
    const setGlobalSelectedProvider = vi.fn();

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        selectedProvider: "anthropic",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId,
        setPendingModelSelection,
        setGlobalSelectedProvider,
        prepareSelectedProvider: vi.fn(),
      }),
    );

    act(() => {
      result.current.handleProviderChange("openai");
    });

    expect(setGlobalSelectedProvider).toHaveBeenCalledWith("openai");
    expect(setPendingProviderId).toHaveBeenCalledWith("openai");
    expect(setPendingModelSelection).toHaveBeenCalledWith(undefined);
  });

  it("resolves ACP alias defaults to a concrete model when switching agents", () => {
    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "claude-acp"
          ? {
              providerId: "claude-acp",
              defaultModel: "current",
              models: [
                {
                  id: "sonnet",
                  name: "Claude Sonnet",
                  recommended: true,
                },
                {
                  id: "opus",
                  name: "Claude Opus",
                  recommended: false,
                },
              ],
            }
          : undefined,
    });

    const setPendingProviderId = vi.fn();
    const setPendingModelSelection = vi.fn();
    const setGlobalSelectedProvider = vi.fn();

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "claude-acp", label: "Claude Code" },
        ],
        selectedProvider: "goose",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId,
        setPendingModelSelection,
        setGlobalSelectedProvider,
        prepareSelectedProvider: vi.fn(),
      }),
    );

    act(() => {
      result.current.handleProviderChange("claude-acp");
    });

    expect(setPendingModelSelection).toHaveBeenCalledWith({
      id: "sonnet",
      name: "Claude Sonnet",
      providerId: "claude-acp",
      source: "default",
    });
  });

  it("prefers a concrete default model over a session alias like current", () => {
    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "claude-acp"
          ? {
              providerId: "claude-acp",
              defaultModel: "current",
              models: [
                {
                  id: "sonnet",
                  name: "Claude Sonnet",
                  recommended: true,
                },
              ],
            }
          : undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onProviderSelected,
      }: {
        onProviderSelected: (providerId: string) => void;
      }) => ({
        pickerAgents: [
          { id: "goose", label: "Goose" },
          { id: "claude-acp", label: "Claude Code" },
        ],
        availableModels: [
          {
            id: "sonnet",
            name: "Claude Sonnet",
            recommended: true,
          },
        ],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: (providerId: string) =>
          onProviderSelected(providerId),
        handleModelChange: vi.fn(),
      }),
    );

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "claude-acp", label: "Claude Code" },
        ],
        selectedProvider: "claude-acp",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "claude-acp",
          modelId: "current",
          modelName: "current",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn(),
      }),
    );

    expect(result.current.effectiveModelSelection).toEqual({
      id: "sonnet",
      name: "Claude Sonnet",
      providerId: "claude-acp",
      source: "default",
    });
  });

  it("drops Goose fallback models that are incompatible with a concrete provider", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "claude-sonnet-4",
          modelName: "Claude Sonnet 4",
          providerId: "anthropic",
        },
      }),
    );

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onProviderSelected,
      }: {
        onProviderSelected: (providerId: string) => void;
      }) => ({
        pickerAgents: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        availableModels: [
          {
            id: "gpt-5.4",
            name: "GPT-5.4",
            providerId: "openai",
          },
          {
            id: "claude-sonnet-4",
            name: "Claude Sonnet 4",
            providerId: "anthropic",
          },
        ],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: (providerId: string) =>
          onProviderSelected(providerId),
        handleModelChange: vi.fn(),
      }),
    );

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        selectedProvider: "openai",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "openai",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn(),
      }),
    );

    expect(result.current.effectiveModelSelection).toBeNull();
  });

  it("enforces concrete provider compatibility from inventory before catalog loads", () => {
    useProviderCatalogStore.getState().reset();
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "claude-sonnet-4",
          modelName: "Claude Sonnet 4",
          providerId: "anthropic",
        },
      }),
    );

    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "openai"
          ? {
              providerId: "openai",
              category: "model",
              defaultModel: "gpt-5.4",
              models: [
                {
                  id: "gpt-5.4",
                  name: "GPT-5.4",
                  recommended: true,
                },
              ],
            }
          : undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(() => ({
      pickerAgents: [{ id: "goose", label: "Goose" }],
      availableModels: [],
      modelsLoading: true,
      modelStatusMessage: null,
      handleProviderChange: vi.fn(),
      handleModelChange: vi.fn(),
    }));

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        selectedProvider: "openai",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "openai",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn(),
      }),
    );

    expect(result.current.effectiveModelSelection).toBeNull();
  });

  it("preserves unresolved agent provider identity before catalog loads", async () => {
    useProviderCatalogStore.getState().reset();

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onProviderSelected,
        onModelSelected,
      }: {
        onProviderSelected: (providerId: string) => void;
        onModelSelected?: (model: {
          id: string;
          name: string;
          displayName?: string;
          providerId?: string;
        }) => void;
      }) => ({
        pickerAgents: [
          { id: "goose", label: "Goose" },
          { id: "codex-acp", label: "Codex" },
        ],
        availableModels: [
          {
            id: "gpt-5.4",
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "codex-acp",
          },
        ],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: (providerId: string) =>
          onProviderSelected(providerId),
        handleModelChange: (modelId: string) =>
          onModelSelected?.({
            id: modelId,
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "codex-acp",
          }),
      }),
    );

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "codex-acp", label: "Codex" },
        ],
        selectedProvider: "codex-acp",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "codex-acp",
          modelId: "current",
          modelName: "current",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn().mockResolvedValue(true),
      }),
    );

    act(() => {
      result.current.handleModelChange("gpt-5.4");
    });

    await waitFor(() => {
      expect(
        JSON.parse(
          localStorage.getItem("goose:preferredModelsByAgent") ?? "{}",
        ),
      ).toEqual({
        "codex-acp": {
          modelId: "gpt-5.4",
          modelName: "GPT-5.4",
          providerId: "codex-acp",
        },
      });
    });
  });

  it("routes unresolved model provider identity through Goose before catalog loads", async () => {
    useProviderCatalogStore.getState().reset();

    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "openai"
          ? {
              providerId: "openai",
              category: "model",
              defaultModel: "gpt-5.4",
              models: [
                {
                  id: "gpt-5.4",
                  name: "GPT-5.4",
                  recommended: true,
                },
              ],
            }
          : undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onModelSelected,
      }: {
        onModelSelected?: (model: {
          id: string;
          name: string;
          displayName?: string;
          providerId?: string;
        }) => void;
      }) => ({
        pickerAgents: [{ id: "goose", label: "Goose" }],
        availableModels: [
          {
            id: "gpt-5.4",
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "openai",
          },
        ],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: vi.fn(),
        handleModelChange: (modelId: string) =>
          onModelSelected?.({
            id: modelId,
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "openai",
          }),
      }),
    );

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        selectedProvider: "openai",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "openai",
          modelId: "current",
          modelName: "current",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn().mockResolvedValue(true),
      }),
    );

    expect(result.current.selectedAgentId).toBe("goose");

    act(() => {
      result.current.handleModelChange("gpt-5.4");
    });

    await waitFor(() => {
      expect(
        JSON.parse(
          localStorage.getItem("goose:preferredModelsByAgent") ?? "{}",
        ),
      ).toEqual({
        goose: {
          modelId: "gpt-5.4",
          modelName: "GPT-5.4",
          providerId: "openai",
        },
      });
    });
  });

  it("does not persist a superseded explicit model selection", async () => {
    const prepareSelectedProvider = vi.fn().mockResolvedValue(false);

    mockUseAgentModelPickerState.mockImplementation(
      ({
        onModelSelected,
      }: {
        onModelSelected?: (model: {
          id: string;
          name: string;
          displayName?: string;
          providerId?: string;
        }) => void;
      }) => ({
        pickerAgents: [{ id: "goose", label: "Goose" }],
        availableModels: [
          {
            id: "gpt-5.4",
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "openai",
          },
        ],
        modelsLoading: false,
        modelStatusMessage: null,
        handleProviderChange: vi.fn(),
        handleModelChange: (modelId: string) =>
          onModelSelected?.({
            id: modelId,
            name: "GPT-5.4",
            displayName: "GPT-5.4",
            providerId: "openai",
          }),
      }),
    );

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "openai", label: "OpenAI" },
        ],
        selectedProvider: "openai",
        sessionId: "session-1",
        session: {
          id: "session-1",
          title: "Chat",
          providerId: "openai",
          modelId: "current",
          modelName: "current",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider,
      }),
    );

    act(() => {
      result.current.handleModelChange("gpt-5.4");
    });

    await waitFor(() => {
      expect(prepareSelectedProvider).toHaveBeenCalledWith("openai", {
        id: "gpt-5.4",
        name: "GPT-5.4",
        providerId: "openai",
        source: "explicit",
      });
    });
    expect(localStorage.getItem("goose:preferredModelsByAgent")).toBeNull();
  });

  it("preserves persisted Claude Code / Opus during empty inventory and catalog", () => {
    useProviderCatalogStore.getState().reset();

    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        "claude-acp": {
          modelId: "opus",
          modelName: "Claude Opus",
          providerId: "claude-acp",
        },
      }),
    );

    mockUseProviderInventory.mockReturnValue({
      getEntry: () => undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(() => ({
      pickerAgents: [{ id: "goose", label: "Goose" }],
      availableModels: [],
      modelsLoading: true,
      modelStatusMessage: null,
      handleProviderChange: vi.fn(),
      handleModelChange: vi.fn(),
    }));

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [],
        selectedProvider: "claude-acp",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn(),
      }),
    );

    expect(result.current.selectedAgentId).toBe("claude-acp");
    expect(result.current.effectiveModelSelection).toEqual({
      id: "opus",
      name: "Claude Opus",
      providerId: "claude-acp",
      source: "explicit",
    });
  });

  it("retains selection after validated inventory confirms the agent", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        "claude-acp": {
          modelId: "opus",
          modelName: "Claude Opus",
          providerId: "claude-acp",
        },
      }),
    );

    mockUseProviderInventory.mockReturnValue({
      getEntry: (id: string) =>
        id === "claude-acp"
          ? {
              providerId: "claude-acp",
              category: "agent",
              models: [
                { id: "opus", name: "Claude Opus", recommended: false },
                { id: "sonnet", name: "Claude Sonnet", recommended: true },
              ],
            }
          : undefined,
    });

    mockUseAgentModelPickerState.mockImplementation(() => ({
      pickerAgents: [
        { id: "goose", label: "Goose" },
        { id: "claude-acp", label: "Claude Code" },
      ],
      availableModels: [
        { id: "opus", name: "Claude Opus", providerId: "claude-acp" },
        { id: "sonnet", name: "Claude Sonnet", providerId: "claude-acp" },
      ],
      modelsLoading: false,
      modelStatusMessage: null,
      handleProviderChange: vi.fn(),
      handleModelChange: vi.fn(),
    }));

    const { result } = renderHook(() =>
      useResolvedAgentModelPicker({
        providers: [
          { id: "goose", label: "Goose" },
          { id: "claude-acp", label: "Claude Code" },
        ],
        selectedProvider: "claude-acp",
        sessionId: null,
        session: undefined,
        pendingModelSelection: undefined,
        setPendingProviderId: vi.fn(),
        setPendingModelSelection: vi.fn(),
        setGlobalSelectedProvider: vi.fn(),
        prepareSelectedProvider: vi.fn(),
      }),
    );

    expect(result.current.selectedAgentId).toBe("claude-acp");
    expect(result.current.effectiveModelSelection).toEqual({
      id: "opus",
      name: "Claude Opus",
      providerId: "claude-acp",
      source: "explicit",
    });
  });
});
