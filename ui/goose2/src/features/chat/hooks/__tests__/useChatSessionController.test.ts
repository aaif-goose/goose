import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import { applyLatestSessionConfig } from "../../lib/sessionConfigRequests";

const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();
const mockSetSelectedProvider = vi.fn();
const mockResolveSessionCwd = vi.fn();
const mockGooseDefaultsRead = vi.fn();
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

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

vi.mock("@/shared/api/acp", () => ({
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

vi.mock("@/shared/api/acpConnection", () => ({
  getClient: async () => ({
    goose: {
      GooseDefaultsRead: (...args: unknown[]) => mockGooseDefaultsRead(...args),
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
    tokenState: null,
    sendMessage: vi.fn(),
    stopStreaming: vi.fn(),
    streamingMessageId: null,
  }),
}));

vi.mock("../useMessageQueue", () => ({
  useMessageQueue: () => ({
    queuedMessage: null,
    enqueue: vi.fn(),
    dismiss: vi.fn(),
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
    mockResolveSessionCwd.mockResolvedValue("/tmp/project");
    mockGooseDefaultsRead.mockResolvedValue({
      providerId: null,
      modelId: null,
    });
    mockUseProviderInventory.mockReturnValue({
      getEntry: () => undefined,
    });
    mockPickerState.pickerAgents = [{ id: "goose", label: "Goose" }];
    mockPickerState.availableModels = [];
    mockPickerState.modelsLoading = false;
    mockPickerState.modelStatusMessage = null;

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
      personaEditorMode: "create",
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
    mockGooseDefaultsRead.mockResolvedValue({
      providerId: "databricks",
      modelId: "goose-claude-4-6-opus",
    });
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

  it("moves pending Home queued messages when preparation is superseded", async () => {
    const firstPrepare = deferred();
    mockAcpPrepareSession.mockReturnValueOnce(firstPrepare.promise);

    const { result, rerender } = renderHook(
      ({ sessionId }: { sessionId: string | null }) =>
        useChatSessionController({ sessionId }),
      {
        initialProps: { sessionId: null as string | null },
      },
    );

    act(() => {
      result.current.handleModelChange("claude-sonnet-4");
      useChatStore
        .getState()
        .enqueueMessage("__home_pending__", { text: "queued from Home" });
    });

    useChatSessionStore.setState((state) => ({
      sessions: [
        {
          id: "session-superseded-home",
          title: "Chat",
          providerId: "openai",
          createdAt: "2026-04-21T00:00:00.000Z",
          updatedAt: "2026-04-21T00:00:00.000Z",
          messageCount: 0,
        },
        ...state.sessions,
      ],
    }));

    rerender({ sessionId: "session-superseded-home" });

    await waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-superseded-home",
        "anthropic",
        "/tmp/project",
      );
    });

    const latestConfig = applyLatestSessionConfig({
      sessionId: "session-superseded-home",
      providerId: "anthropic",
      workingDir: "/tmp/other-project",
      modelId: "claude-sonnet-4",
    });

    firstPrepare.resolve();

    await waitFor(() => {
      expect(mockAcpPrepareSession).toHaveBeenCalledWith(
        "session-superseded-home",
        "anthropic",
        "/tmp/other-project",
      );
    });
    await expect(latestConfig).resolves.toEqual({ applied: true });

    await waitFor(() => {
      expect(
        useChatStore.getState().queuedMessageBySession[
          "session-superseded-home"
        ],
      ).toEqual({ text: "queued from Home" });
    });
    expect(
      useChatStore.getState().queuedMessageBySession.__home_pending__,
    ).toBeUndefined();
    expect(
      window.localStorage.getItem("goose:preferredModelsByAgent"),
    ).toBeNull();
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
});
