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
      name: "gpt-5.4",
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
});
