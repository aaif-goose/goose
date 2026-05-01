import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAgentModelPickerState } from "../useAgentModelPickerState";

const mockUseProviderInventory = vi.fn();

vi.mock("@/features/providers/hooks/useProviderInventory", () => ({
  useProviderInventory: () => mockUseProviderInventory(),
}));

describe("useAgentModelPickerState", () => {
  it("switches to goose when the current provider is goose-backed", () => {
    const onProviderSelected = vi.fn();

    mockUseProviderInventory.mockReturnValue({
      entries: new Map([
        [
          "anthropic",
          {
            providerId: "anthropic",
            configured: true,
            refreshing: false,
            models: [],
          },
        ],
      ]),
      getEntry: (providerId: string) =>
        providerId === "anthropic"
          ? {
              providerId: "anthropic",
              configured: true,
              refreshing: false,
              models: [],
            }
          : undefined,
      configuredModelProviderEntries: [],
      getModelsForAgent: () => [],
      loading: false,
    });

    const { result } = renderHook(() =>
      useAgentModelPickerState({
        providers: [{ id: "anthropic", label: "Anthropic" }],
        selectedProvider: "anthropic",
        onProviderSelected,
      }),
    );

    act(() => {
      result.current.handleProviderChange("goose");
    });

    expect(onProviderSelected).toHaveBeenCalledWith("goose");
  });

  it("treats goose as a no-op only when goose is already selected", () => {
    const onProviderSelected = vi.fn();

    mockUseProviderInventory.mockReturnValue({
      entries: new Map(),
      getEntry: () => undefined,
      configuredModelProviderEntries: [],
      getModelsForAgent: () => [],
      loading: false,
    });

    const { result } = renderHook(() =>
      useAgentModelPickerState({
        providers: [],
        selectedProvider: "goose",
        onProviderSelected,
      }),
    );

    act(() => {
      result.current.handleProviderChange("goose");
    });

    expect(onProviderSelected).not.toHaveBeenCalled();
  });

  it("passes the selected model provider through for goose model picks", () => {
    const onModelSelected = vi.fn();

    mockUseProviderInventory.mockReturnValue({
      entries: new Map(),
      getEntry: () => undefined,
      configuredModelProviderEntries: [],
      getModelsForAgent: () => [
        {
          id: "claude-sonnet-4",
          name: "claude-sonnet-4",
          displayName: "Claude Sonnet 4",
          providerId: "anthropic",
          providerName: "Anthropic",
          recommended: true,
        },
      ],
      loading: false,
    });

    const { result } = renderHook(() =>
      useAgentModelPickerState({
        providers: [{ id: "goose", label: "Goose" }],
        selectedProvider: "openai",
        onProviderSelected: vi.fn(),
        onModelSelected,
      }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    expect(onModelSelected).toHaveBeenCalledWith({
      id: "claude-sonnet-4",
      name: "claude-sonnet-4",
      displayName: "Claude Sonnet 4",
      provider: undefined,
      providerId: "anthropic",
      providerName: "Anthropic",
      recommended: true,
    });
  });

  it("uses the clicked model when multiple providers expose the same model id", () => {
    const onModelSelected = vi.fn();
    const customModel = {
      id: "llama3.2",
      name: "llama3.2",
      displayName: "llama3.2",
      providerId: "custom_ollama",
      providerName: "Custom Ollama",
    };

    mockUseProviderInventory.mockReturnValue({
      entries: new Map(),
      getEntry: () => undefined,
      configuredModelProviderEntries: [],
      getModelsForAgent: () => [
        {
          id: "llama3.2",
          name: "llama3.2",
          displayName: "llama3.2",
          providerId: "ollama",
          providerName: "Ollama",
        },
        customModel,
      ],
      loading: false,
    });

    const { result } = renderHook(() =>
      useAgentModelPickerState({
        providers: [{ id: "goose", label: "Goose" }],
        selectedProvider: "ollama",
        onProviderSelected: vi.fn(),
        onModelSelected,
      }),
    );

    act(() => {
      result.current.handleModelChange("llama3.2", customModel);
    });

    expect(onModelSelected).toHaveBeenCalledWith({
      id: "llama3.2",
      name: "llama3.2",
      displayName: "llama3.2",
      provider: undefined,
      providerId: "custom_ollama",
      providerName: "Custom Ollama",
      recommended: undefined,
    });
  });
});
