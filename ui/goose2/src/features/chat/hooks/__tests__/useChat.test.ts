import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import type { Message } from "@/shared/types/messages";
import { clearReplayBuffer } from "../replayBuffer";

const mockAcpSendMessage = vi.fn();
const mockAcpCancelSession = vi.fn();
const mockAcpLoadSession = vi.fn();
const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();
const mockGetGooseSessionId = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: (...args: unknown[]) => mockAcpCancelSession(...args),
  acpLoadSession: (...args: unknown[]) => mockAcpLoadSession(...args),
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

vi.mock("@/shared/api/acpSessionTracker", () => ({
  getGooseSessionId: (...args: unknown[]) => mockGetGooseSessionId(...args),
}));

import { useChat } from "../useChat";

function addStreamingAssistantMessage(
  sessionId: string,
  messageId: string,
  personaId: string,
  personaName: string,
) {
  const message: Message = {
    id: messageId,
    role: "assistant",
    created: Date.now(),
    content: [],
    metadata: {
      userVisible: true,
      agentVisible: true,
      personaId,
      personaName,
      completionStatus: "inProgress",
    },
  };

  useChatStore.getState().addMessage(sessionId, message);
  useChatStore.getState().setStreamingMessageId(sessionId, messageId);
}

function createDeferredPromise<T = void>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useChat", () => {
  beforeEach(() => {
    mockAcpSendMessage.mockReset();
    mockAcpCancelSession.mockReset();
    mockAcpLoadSession.mockReset();
    mockAcpPrepareSession.mockReset();
    mockAcpSetModel.mockReset();
    mockGetGooseSessionId.mockReset();
    clearReplayBuffer("session-1");
    clearReplayBuffer("session-2");
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: null,
      isConnected: true,
    });
    useChatSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      isLoading: false,
      contextPanelOpenBySession: {},
      activeWorkspaceBySession: {},
      modelsBySession: {},
      modelCacheByProvider: {},
    });
    useAgentStore.setState({
      personas: [
        {
          id: "persona-a",
          displayName: "Persona A",
          systemPrompt: "",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
        {
          id: "persona-b",
          displayName: "Persona B",
          systemPrompt: "",
          isBuiltin: false,
          createdAt: "",
          updatedAt: "",
        },
      ],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
    });
    mockAcpSendMessage.mockResolvedValue(undefined);
    mockAcpCancelSession.mockResolvedValue(true);
    mockAcpLoadSession.mockResolvedValue(undefined);
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
    mockGetGooseSessionId.mockReturnValue(null);
  });

  it("cancels the active override persona instead of the hook default persona", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage.mockReturnValue(deferred.promise);

    const { result } = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    let sendPromise!: Promise<void>;
    await act(async () => {
      sendPromise = result.current.sendMessage("Hello", {
        id: "persona-b",
        name: "Persona B",
      });
      await Promise.resolve();
    });

    act(() => {
      result.current.stopGeneration();
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "Hello", {
      systemPrompt: undefined,
      personaId: "persona-b",
      personaName: "Persona B",
      images: undefined,
    });
    expect(mockAcpCancelSession).toHaveBeenCalledWith("session-1", "persona-b");

    deferred.resolve();
    await act(async () => {
      await sendPromise;
    });
  });

  it("keeps persona-aware cancellation working after remount", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage.mockReturnValue(deferred.promise);

    const firstMount = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    let sendPromise!: Promise<void>;
    await act(async () => {
      sendPromise = firstMount.result.current.sendMessage("Hello", {
        id: "persona-b",
        name: "Persona B",
      });
      await Promise.resolve();
    });
    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-b",
        "Persona B",
      );
    });

    act(() => {
      firstMount.unmount();
    });

    const secondMount = renderHook(() =>
      useChat("session-1", undefined, undefined, {
        id: "persona-a",
        name: "Persona A",
      }),
    );

    act(() => {
      secondMount.result.current.stopGeneration();
    });

    expect(mockAcpCancelSession).toHaveBeenCalledWith("session-1", "persona-b");

    deferred.resolve();
    await act(async () => {
      await sendPromise;
    });
  });

  it("marks the streaming message stopped only after cancellation succeeds", async () => {
    const cancelDeferred = createDeferredPromise<boolean>();
    mockAcpCancelSession.mockReturnValue(cancelDeferred.promise);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    act(() => {
      result.current.stopGeneration();
    });

    let message = useChatStore.getState().messagesBySession["session-1"][0];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(message.metadata?.completionStatus).toBe("inProgress");
    expect(runtime.chatState).toBe("idle");
    expect(runtime.streamingMessageId).toBeNull();

    await act(async () => {
      cancelDeferred.resolve(true);
      await cancelDeferred.promise;
    });

    message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("stopped");
  });

  it("does not overwrite a completed message when stop loses the race", async () => {
    const cancelDeferred = createDeferredPromise<boolean>();
    mockAcpCancelSession.mockReturnValue(cancelDeferred.promise);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    act(() => {
      result.current.stopGeneration();
      useChatStore
        .getState()
        .updateMessage("session-1", "assistant-1", (message) => ({
          ...message,
          metadata: {
            ...message.metadata,
            completionStatus: "completed",
          },
        }));
    });

    await act(async () => {
      cancelDeferred.resolve(true);
      await cancelDeferred.promise;
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("completed");
  });

  it("does not mark the message stopped when cancellation reports no active session", async () => {
    mockAcpCancelSession.mockResolvedValue(false);

    const { result } = renderHook(() => useChat("session-1"));

    act(() => {
      addStreamingAssistantMessage(
        "session-1",
        "assistant-1",
        "persona-a",
        "Persona A",
      );
      useChatStore.getState().setChatState("session-1", "streaming");
    });

    await act(async () => {
      result.current.stopGeneration();
      await Promise.resolve();
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.metadata?.completionStatus).toBe("inProgress");
  });

  it("allows another session to send while a different session is streaming", async () => {
    const deferred = createDeferredPromise();
    mockAcpSendMessage
      .mockReturnValueOnce(deferred.promise)
      .mockResolvedValueOnce(undefined);

    const firstSession = renderHook(() => useChat("session-1"));
    const secondSession = renderHook(() => useChat("session-2"));

    let firstPromise!: Promise<void>;
    await act(async () => {
      firstPromise = firstSession.result.current.sendMessage("First");
      await Promise.resolve();
    });

    await act(async () => {
      await secondSession.result.current.sendMessage("Second");
    });

    expect(mockAcpSendMessage).toHaveBeenNthCalledWith(
      1,
      "session-1",
      "First",
      {
        systemPrompt: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );
    expect(mockAcpSendMessage).toHaveBeenNthCalledWith(
      2,
      "session-2",
      "Second",
      {
        systemPrompt: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );

    deferred.resolve();
    await act(async () => {
      await firstPromise;
    });
  });

  it("prepares draft sessions before applying a selected model on first send", async () => {
    useChatSessionStore.setState({
      sessions: [
        {
          id: "session-1",
          title: "New Chat",
          providerId: "openai",
          modelId: "gpt-4.1",
          modelName: "GPT-4.1",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messageCount: 0,
          draft: true,
        },
      ],
    });

    const { result } = renderHook(() =>
      useChat("session-1", "openai", undefined, undefined, async () => "/tmp"),
    );

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    expect(mockAcpPrepareSession).toHaveBeenCalledWith(
      "session-1",
      "openai",
      "/tmp",
      {
        personaId: undefined,
      },
    );
    expect(mockAcpSetModel).toHaveBeenCalledWith("session-1", "gpt-4.1");
    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "Hello", {
      systemPrompt: undefined,
      personaId: undefined,
      personaName: undefined,
      images: undefined,
    });
  });

  it("appends an error message and removes the empty assistant placeholder when send fails", async () => {
    mockAcpSendMessage.mockRejectedValue(
      new Error("Working directory missing"),
    );

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe("user");
    expect(messages[1].role).toBe("system");
    expect(messages[1].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Working directory missing",
      },
    ]);
    expect(runtime.error).toBe("Working directory missing");
    expect(runtime.streamingMessageId).toBeNull();
    expect(runtime.chatState).toBe("idle");
  });

  it("shows string-shaped invoke errors instead of falling back to unknown error", async () => {
    mockAcpSendMessage.mockRejectedValue("Working directory missing");

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage("Hello");
    });

    const messages = useChatStore.getState().messagesBySession["session-1"];
    expect(messages[1].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Working directory missing",
      },
    ]);
  });
});
