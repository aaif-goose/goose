import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";

const mockSendMessage = vi.fn();
const mockCompactConversation = vi.fn();
const mockSetSelectedProvider = vi.fn();
const mockResolveSessionCwd = vi.fn();
const mockHandleProviderChange = vi.fn();
const mockHandleModelChange = vi.fn();
let mockSelectedAgentId = "goose";
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
let capturedQueuedSend:
  | ((
      text: string,
      overridePersona?: { id: string; name?: string },
      attachments?: unknown[],
    ) => boolean | Promise<boolean>)
  | null = null;

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
  useMessageQueue: (...args: unknown[]) => {
    capturedQueuedSend = args[2] as typeof capturedQueuedSend;
    return {
      queuedMessage: null,
      enqueue: vi.fn(),
      dismiss: vi.fn(),
    };
  },
}));

vi.mock("../useAutoCompactPreferences", () => ({
  useAutoCompactPreferences: () => ({
    autoCompactThreshold: 0.8,
    isHydrated: true,
    setAutoCompactThreshold: vi.fn(),
  }),
}));

vi.mock("../useResolvedAgentModelPicker", () => ({
  useResolvedAgentModelPicker: () => ({
    selectedAgentId: mockSelectedAgentId,
    pickerAgents: [{ id: "goose", label: "Goose" }],
    availableModels: [],
    modelsLoading: false,
    modelStatusMessage: null,
    handleProviderChange: (providerId: string) =>
      mockHandleProviderChange(providerId),
    handleModelChange: (modelId: string) => mockHandleModelChange(modelId),
    effectiveModelSelection: {
      id: "gpt-4o",
      name: "GPT-4o",
      providerId: "openai",
      source: "explicit" as const,
    },
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

import { useChatSessionController } from "../useChatSessionController";

describe("useChatSessionController compaction behavior", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCompactConversation.mockResolvedValue("completed");
    mockResolveSessionCwd.mockResolvedValue("/tmp/project");
    mockTokenState = { ...INITIAL_TOKEN_STATE };
    capturedQueuedSend = null;
    mockSelectedAgentId = "goose";

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

  it("compacts the queued persona session before sending", async () => {
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
      personaId: "persona-b",
    });

    renderHook(() => useChatSessionController({ sessionId: "session-1" }));

    expect(capturedQueuedSend).not.toBeNull();

    await act(async () => {
      await capturedQueuedSend?.("hello", { id: "persona-a" });
    });

    expect(mockCompactConversation).toHaveBeenCalledWith({ id: "persona-a" });
    expect(mockSendMessage).toHaveBeenCalledWith(
      "hello",
      { id: "persona-a" },
      undefined,
    );
  });

  it("auto-compacts queued messages for goose personas even after switching away", async () => {
    mockSelectedAgentId = "claude-acp";
    mockTokenState = {
      ...INITIAL_TOKEN_STATE,
      accumulatedTotal: 8_500,
      contextLimit: 10_000,
    };
    useChatStore
      .getState()
      .replaceTokenState("session-1", mockTokenState, true);
    useAgentStore.setState({
      personas: [
        {
          id: "persona-a",
          displayName: "Persona A",
          systemPrompt: "",
          provider: "openai",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
      ],
    });

    renderHook(() => useChatSessionController({ sessionId: "session-1" }));

    await act(async () => {
      await capturedQueuedSend?.("hello", { id: "persona-a" });
    });

    expect(mockCompactConversation).toHaveBeenCalledWith({ id: "persona-a" });
    expect(mockSendMessage).toHaveBeenCalledWith(
      "hello",
      { id: "persona-a" },
      undefined,
    );
  });

  it("skips auto-compaction for queued messages targeting unsupported personas", async () => {
    mockSelectedAgentId = "goose";
    mockTokenState = {
      ...INITIAL_TOKEN_STATE,
      accumulatedTotal: 8_500,
      contextLimit: 10_000,
    };
    useChatStore
      .getState()
      .replaceTokenState("session-1", mockTokenState, true);
    useAgentStore.setState({
      personas: [
        {
          id: "persona-a",
          displayName: "Persona A",
          systemPrompt: "",
          provider: "claude-acp",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
      ],
    });

    renderHook(() => useChatSessionController({ sessionId: "session-1" }));

    await act(async () => {
      await capturedQueuedSend?.("hello", { id: "persona-a" });
    });

    expect(mockCompactConversation).not.toHaveBeenCalled();
    expect(mockSendMessage).toHaveBeenCalledWith(
      "hello",
      { id: "persona-a" },
      undefined,
    );
  });
});
