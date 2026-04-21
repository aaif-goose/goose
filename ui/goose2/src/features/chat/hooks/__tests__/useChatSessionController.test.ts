import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useProjectStore } from "@/features/projects/stores/projectStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";

const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();
const mockSetSelectedProvider = vi.fn();
const mockResolveSessionCwd = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
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
    selectedProvider: "openai",
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
    pickerAgents: [{ id: "goose", label: "Goose" }],
    availableModels: [],
    modelsLoading: false,
    modelStatusMessage: null,
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
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
    mockResolveSessionCwd.mockResolvedValue("/tmp/project");

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
});
