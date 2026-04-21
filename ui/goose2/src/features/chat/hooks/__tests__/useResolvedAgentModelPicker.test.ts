import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useResolvedAgentModelPicker } from "../useResolvedAgentModelPicker";

const mockUseProviderInventory = vi.fn();
const mockUseAgentModelPickerState = vi.fn();
const mockGetClient = vi.fn();

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

describe("useResolvedAgentModelPicker", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();

    mockGetClient.mockResolvedValue({
      goose: {
        GooseConfigRead: vi.fn().mockResolvedValue({ value: null }),
      },
    });

    mockUseProviderInventory.mockReturnValue({
      getEntry: (providerId: string) =>
        providerId === "codex-acp"
          ? {
              providerId: "codex-acp",
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
});
