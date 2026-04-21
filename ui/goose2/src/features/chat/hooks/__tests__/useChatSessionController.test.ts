import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";

const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();
const mockSendMessage = vi.fn();
const mockCompactConversation = vi.fn();
const mockSetSelectedProvider = vi.fn();
const mockResolveSessionCwd = vi.fn();
const mockGooseConfigRead = vi.fn();
const mockUseProviderInventory = vi.fn();
const mockPickerState = {
  pickerAgents: [{ id: "goose", label: "Goose" }],
  availableModels: [] as Array<{
    id: string;
    name: string;
    displayName?: string;
    providerId?: string;
  }>,
  modelsLoading: false,
  modelStatusMessage: null as string | null,
};
const mockSetAutoCompactThreshold = vi.fn();
const INITIAL_TOKEN_STATE = {
  inputTokens: 0,
  outputTokens: 0,
  totalTokens: 0,
  accumulatedInput: 0,
  accumulatedOutput: 0,
  accumulatedTotal: 0,
  contextLimit: 0,
};
let mockTokenState = { ...INITIAL_TOKEN_STATE };

vi.mock("@/shared/api/acp", () => ({
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: async () => ({
    goose: {
      GooseConfigRead: (...args: unknown[]) => mockGooseConfigRead(...args),
    },
  }),
}));

vi.mock("@/features/providers/hooks/useProviderInventory", () => ({
  useProviderInventory: () => mockUseProviderInventory(),
}));

vi.mock("../useChat", () => ({
  useChat: () => ({
    messages: [],
    chatState: "idle",
    tokenState: mockTokenState,
    sendMessage: (...args: unknown[]) => mockSendMessage(...args),
    compactConversation: (...args: unknown[]) =>
      mockCompactConversation(...args),
    stopStreaming: vi.fn(),
    streamingMessageId: null,
  }),
}));

vi.mock("../useMessageQueue", () => ({
  useMessageQueue: () => ({
    queuedMessage: null,
    enqueue: vi.fn(),
  }),
}));

vi.mock("../useAutoCompactPreferences", () => ({
  useAutoCompactPreferences: () => ({
    autoCompactThreshold: 0.8,
    isHydrated: true,
    setAutoCompactThreshold: (...args: unknown[]) =>
      mockSetAutoCompactThreshold(...args),
  }),
}));

vi.mock("@/features/agents/hooks/useProviderSelection", () => ({
  useProviderSelection: () => ({
    providers: [
      { id: "goose", label: "Goose" },
      { id: "openai", label: "OpenAI" },
      { id: "anthropic", label: "Anthropic" },
    ],
    providersLoading: false,
    selectedProvider: useAgentStore.getState().selectedProvider ?? "openai",
    setSelectedProvider: (...args: unknown[]) =>
      mockSetSelectedProvider(...args),
  }),
}));

vi.mock("@/features/projects/lib/sessionCwdSelection", () => ({
  resolveSessionCwd: (...args: unknown[]) => mockResolveSessionCwd(...args),
}));

vi.mock("../useAgentModelPickerState", () => ({
  useAgentModelPickerState: ({
    onModelSelected,
  }: {
    onModelSelected?: (model: {
      id: string;
      name: string;
      displayName?: string;
      providerId?: string;
      contextLimit?: number | null;
    }) => void;
  }) => ({
    selectedAgentId: "goose",
    pickerAgents: mockPickerState.pickerAgents,
    availableModels: mockPickerState.availableModels,
    modelsLoading: mockPickerState.modelsLoading,
    modelStatusMessage: mockPickerState.modelStatusMessage,
    handleProviderChange: vi.fn(),
    handleModelChange: (modelId: string) => {
      if (modelId === "claude-sonnet-4") {
        onModelSelected?.({
          id: modelId,
          name: modelId,
          displayName: "Claude Sonnet 4",
          providerId: "anthropic",
          contextLimit: 200_000,
        });
      }
    },
  }),
}));

import { useChatSessionController } from "../useChatSessionController";

describe("useChatSessionController", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
    mockCompactConversation.mockResolvedValue("completed");
    mockResolveSessionCwd.mockResolvedValue("/tmp/project");
    mockGooseConfigRead.mockResolvedValue({ value: null });
    mockUseProviderInventory.mockReturnValue({
      getEntry: () => undefined,
    });
    mockPickerState.pickerAgents = [{ id: "goose", label: "Goose" }];
    mockPickerState.availableModels = [];
    mockPickerState.modelsLoading = false;
    mockPickerState.modelStatusMessage = null;
    mockTokenState = { ...INITIAL_TOKEN_STATE };

    useAgentStore.setState({
      personas: [],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      providers: [],
      providersLoading: false,
      selectedProvider: "openai",
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
    });

    useProjectStore.setState({
      projects: [],
      loading: false,
      activeProjectId: null,
    });

    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      draftsBySession: {},
      queuedMessageBySession: {},
      scrollTargetMessageBySession: {},
      activeSessionId: null,
      isConnected: true,
    });

    useChatSessionStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "Chat",
          providerId: "openai",
          modelId: "gpt-4o",
          modelName: "GPT-4o",
          createdAt: "2026-04-20T00:00:00.000Z",
          updatedAt: "2026-04-20T00:00:00.000Z",
          messageCount: 0,
        },
      ],
      activeSessionId: null,
      isLoading: false,
      hasHydratedSessions: true,
      contextPanelOpenBySession: {},
      activeWorkspaceBySession: {},
    });
  });

  it("prepares the selected model provider before setting a goose model", async () => {
    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    await waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-1",
        "anthropic",
        "/tmp/project",
        { personaId: undefined },
      );
    });

    await waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-1",
        "claude-sonnet-4",
      );
    });

    expect(mockAcpPrepareSession.mock.invocationCallOrder[0]).toBeLessThan(
      mockAcpSetModel.mock.invocationCallOrder[0],
    );
    expect(mockSetSelectedProvider).toHaveBeenCalledWith("anthropic");
    expect(
      useChatSessionStore.getState().getSession("session-1"),
    ).toMatchObject({
      providerId: "anthropic",
      modelId: "claude-sonnet-4",
      modelName: "Claude Sonnet 4",
    });
  });
  it("restores the previous stored model preference when setting a model fails", async () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "gpt-4o",
          modelName: "GPT-4o",
          providerId: "openai",
        },
      }),
    );
    mockAcpSetModel.mockRejectedValueOnce(new Error("set model failed"));

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    await waitFor(() => {
      expect(
        useChatSessionStore.getState().getSession("session-1"),
      ).toMatchObject({
        providerId: "openai",
        modelId: "gpt-4o",
        modelName: "GPT-4o",
      });
    });

    expect(
      JSON.parse(
        window.localStorage.getItem("goose:preferredModelsByAgent") ?? "{}",
      ),
    ).toEqual({
      goose: {
        modelId: "gpt-4o",
        modelName: "GPT-4o",
        providerId: "openai",
      },
    });
  });

  it("shows the stored explicit model for new chats", async () => {
    useAgentStore.setState({ selectedProvider: "goose" });
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

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: null }),
    );

    await waitFor(() => {
      expect(result.current.currentModelId).toBe("claude-sonnet-4");
    });
    expect(result.current.currentModelName).toBe("Claude Sonnet 4");
  });

  it("falls back to the configured goose default model when no explicit model is stored", async () => {
    useAgentStore.setState({ selectedProvider: "goose" });
    mockGooseConfigRead.mockImplementation(
      async ({ key }: { key: string }): Promise<{ value: string | null }> => {
        if (key === "GOOSE_PROVIDER") {
          return { value: "databricks" };
        }
        if (key === "GOOSE_MODEL") {
          return { value: "goose-claude-4-6-opus" };
        }
        return { value: null };
      },
    );
    mockPickerState.availableModels = [
      {
        id: "goose-claude-4-6-opus",
        name: "Claude 4.6 Opus",
        providerId: "databricks",
      },
    ];

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: null }),
    );

    await waitFor(() => {
      expect(result.current.currentModelId).toBe("goose-claude-4-6-opus");
    });
    expect(result.current.currentModelName).toBe("Claude 4.6 Opus");
  });

  it("applies the pending Home model to ACP when a real session becomes active", async () => {
    const { result, rerender } = renderHook(
      ({ sessionId }: { sessionId: string | null }) =>
        useChatSessionController({ sessionId }),
      {
        initialProps: { sessionId: null as string | null },
      },
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    useChatSessionStore.setState((state) => ({
      sessions: [
        {
          id: "session-2",
          title: "Chat",
          providerId: "openai",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        ...state.sessions,
      ],
    }));

    rerender({ sessionId: "session-2" });

    await waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-2",
        "anthropic",
        "/tmp/project",
        { personaId: undefined },
      );
    });

    await waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-2",
        "claude-sonnet-4",
      );
    });

    expect(
      useChatSessionStore.getState().getSession("session-2"),
    ).toMatchObject({
      providerId: "anthropic",
      modelId: "claude-sonnet-4",
      modelName: "Claude Sonnet 4",
    });
  });

  it("does not persist or record a pending Home model when ACP rejects it", async () => {
    mockAcpSetModel.mockRejectedValueOnce(new Error("set model failed"));

    const { result, rerender } = renderHook(
      ({ sessionId }: { sessionId: string | null }) =>
        useChatSessionController({ sessionId }),
      {
        initialProps: { sessionId: null as string | null },
      },
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    expect(
      window.localStorage.getItem("goose:preferredModelsByAgent"),
    ).toBeNull();

    useChatSessionStore.setState((state) => ({
      sessions: [
        {
          id: "session-3",
          title: "Chat",
          providerId: "openai",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        ...state.sessions,
      ],
    }));

    rerender({ sessionId: "session-3" });

    await waitFor(() => {
      expect(mockAcpSetModel).toHaveBeenCalledWith(
        "session-3",
        "claude-sonnet-4",
      );
    });

    await waitFor(() => {
      expect(
        useChatSessionStore.getState().getSession("session-3"),
      ).toMatchObject({
        providerId: "anthropic",
      });
    });

    expect(
      useChatSessionStore.getState().getSession("session-3"),
    ).not.toMatchObject({
      modelId: "claude-sonnet-4",
      modelName: "Claude Sonnet 4",
    });
    expect(
      window.localStorage.getItem("goose:preferredModelsByAgent"),
    ).toBeNull();
  });

  it("hides context usage until a fresh usage snapshot exists after switching models", () => {
    const store = useChatStore.getState();
    store.replaceTokenState(
      "session-1",
      {
        ...INITIAL_TOKEN_STATE,
        contextLimit: 400_000,
      },
      false,
    );

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    const runtime = useChatStore.getState().getSessionRuntime("session-1");
    expect(runtime.hasUsageSnapshot).toBe(false);
    expect(runtime.tokenState).toEqual(INITIAL_TOKEN_STATE);
  });

  it("hides context usage after switching models even when a snapshot existed", () => {
    const store = useChatStore.getState();
    store.replaceTokenState(
      "session-1",
      {
        ...INITIAL_TOKEN_STATE,
        accumulatedTotal: 12_000,
        contextLimit: 400_000,
      },
      true,
    );

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    const runtime = useChatStore.getState().getSessionRuntime("session-1");
    expect(runtime.hasUsageSnapshot).toBe(false);
    expect(runtime.tokenState).toEqual(INITIAL_TOKEN_STATE);
  });

  it("hides pending home context usage after switching models", () => {
    const store = useChatStore.getState();
    store.replaceTokenState(
      "__home_pending__",
      {
        ...INITIAL_TOKEN_STATE,
        accumulatedTotal: 12_000,
        contextLimit: 400_000,
      },
      true,
    );

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: null }),
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
    });

    const runtime = useChatStore
      .getState()
      .getSessionRuntime("__home_pending__");
    expect(runtime.hasUsageSnapshot).toBe(false);
    expect(runtime.tokenState).toEqual(INITIAL_TOKEN_STATE);
  });

  it("auto-compacts goose sessions before sending when the threshold is exceeded", async () => {
    mockTokenState = {
      ...INITIAL_TOKEN_STATE,
      accumulatedTotal: 8_500,
      contextLimit: 10_000,
    };
    useChatStore
      .getState()
      .replaceTokenState("session-1", mockTokenState, true);
    useChatSessionStore.getState().updateSession("session-1", {
      providerId: "goose",
    });

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    await act(async () => {
      await result.current.handleSend("hello");
    });

    expect(mockCompactConversation).toHaveBeenCalledOnce();
    expect(mockSendMessage).toHaveBeenCalledWith("hello", undefined, undefined);
    expect(mockCompactConversation.mock.invocationCallOrder[0]).toBeLessThan(
      mockSendMessage.mock.invocationCallOrder[0],
    );
  });

  it("keeps compaction enabled for goose agent sessions backed by model providers", async () => {
    mockTokenState = {
      ...INITIAL_TOKEN_STATE,
      accumulatedTotal: 8_500,
      contextLimit: 10_000,
    };
    useChatStore
      .getState()
      .replaceTokenState("session-1", mockTokenState, true);

    const { result } = renderHook(() =>
      useChatSessionController({ sessionId: "session-1" }),
    );

    expect(result.current.selectedProvider).toBe("goose");
    expect(result.current.supportsAutoCompactContext).toBe(true);
    expect(result.current.supportsCompactionControls).toBe(true);

    await act(async () => {
      await result.current.handleSend("hello");
    });

    expect(mockCompactConversation).toHaveBeenCalledOnce();
    expect(mockSendMessage).toHaveBeenCalledWith("hello", undefined, undefined);
  });
});
