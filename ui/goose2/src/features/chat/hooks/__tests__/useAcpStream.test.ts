import { act, cleanup, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import { useChatStore } from "../../stores/chatStore";

type EventCallback = (event: { payload: Record<string, unknown> }) => void;

const listeners = new Map<string, EventCallback[]>();

function emit(eventName: string, payload: Record<string, unknown>) {
  const callbacks = listeners.get(eventName) ?? [];
  for (const callback of callbacks) {
    callback({ payload });
  }
}

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    (eventName: string, callback: EventCallback): Promise<() => void> => {
      const callbacks = listeners.get(eventName) ?? [];
      callbacks.push(callback);
      listeners.set(eventName, callbacks);
      return Promise.resolve(() => {
        const current = listeners.get(eventName) ?? [];
        listeners.set(
          eventName,
          current.filter((entry) => entry !== callback),
        );
      });
    },
  ),
}));

import { useAcpStream } from "../useAcpStream";

function makeStreamingMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: "msg-1",
    role: "assistant",
    created: Date.now(),
    content: [{ type: "text", text: "" }],
    metadata: { userVisible: true },
    ...overrides,
  };
}

function setupStreaming(sessionId: string, messageId = "msg-1") {
  const store = useChatStore.getState();
  store.addMessage(sessionId, makeStreamingMessage({ id: messageId }));
  store.setStreamingMessageId(sessionId, messageId);
  store.setChatState(sessionId, "streaming");
}

describe("useAcpStream", () => {
  const sessionId = "test-session";

  beforeEach(() => {
    listeners.clear();
    cleanup();
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: sessionId,
      isConnected: true,
    });
    useChatSessionStore.setState({
      sessions: [
        {
          id: sessionId,
          title: "New Chat",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messageCount: 0,
        },
      ],
      activeSessionId: sessionId,
      isLoading: false,
      modelsBySession: {},
    });
  });

  it("accumulates text chunks for the targeted session", async () => {
    setupStreaming(sessionId);

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() => expect(listeners.get("acp:text")).toBeDefined());

    act(() => {
      emit("acp:text", { sessionId, messageId: "msg-1", text: "Hello" });
      emit("acp:text", { sessionId, messageId: "msg-1", text: " world" });
    });

    const messages = useChatStore.getState().messagesBySession[sessionId];
    const text = messages[0].content.find((content) => content.type === "text");
    if (text && "text" in text) {
      expect(text.text).toBe("Hello world");
    }
  });

  it("clears runtime state on acp:done for the completed session", async () => {
    setupStreaming(sessionId);

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() => expect(listeners.get("acp:done")).toBeDefined());

    act(() => {
      emit("acp:text", { sessionId, messageId: "msg-1", text: "partial" });
      emit("acp:done", { sessionId, messageId: "msg-1" });
    });

    const runtime = useChatStore.getState().getSessionRuntime(sessionId);
    const message = useChatStore.getState().messagesBySession[sessionId][0];
    expect(runtime.chatState).toBe("idle");
    expect(runtime.streamingMessageId).toBeNull();
    expect(runtime.hasUnread).toBe(false);
    expect(message.metadata?.completionStatus).toBe("completed");
  });

  it("marks a background session unread when it completes", async () => {
    const backgroundSessionId = "background-session";

    setupStreaming(backgroundSessionId);
    useChatSessionStore.setState((state) => ({
      ...state,
      sessions: [
        ...state.sessions,
        {
          id: backgroundSessionId,
          title: "Background Chat",
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          messageCount: 0,
        },
      ],
      activeSessionId: sessionId,
    }));

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() => expect(listeners.get("acp:done")).toBeDefined());

    act(() => {
      emit("acp:done", { sessionId: backgroundSessionId, messageId: "msg-1" });
    });

    expect(
      useChatStore.getState().getSessionRuntime(backgroundSessionId).hasUnread,
    ).toBe(true);
    expect(useChatStore.getState().getSessionRuntime(sessionId).hasUnread).toBe(
      false,
    );
  });

  it("updates multiple sessions independently", async () => {
    const otherSessionId = "other-session";

    setupStreaming(sessionId, "msg-1");
    setupStreaming(otherSessionId, "msg-2");

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() => expect(listeners.get("acp:text")).toBeDefined());

    act(() => {
      emit("acp:text", { sessionId, messageId: "msg-1", text: "Alpha" });
      emit("acp:text", {
        sessionId: otherSessionId,
        messageId: "msg-2",
        text: "Beta",
      });
      emit("acp:done", { sessionId: otherSessionId, messageId: "msg-2" });
    });

    const primary = useChatStore.getState().messagesBySession[sessionId];
    const secondary = useChatStore.getState().messagesBySession[otherSessionId];
    const primaryText = primary[0].content.find(
      (content) => content.type === "text",
    );
    const secondaryText = secondary[0].content.find(
      (content) => content.type === "text",
    );

    if (primaryText && "text" in primaryText) {
      expect(primaryText.text).toBe("Alpha");
    }
    if (secondaryText && "text" in secondaryText) {
      expect(secondaryText.text).toBe("Beta");
    }

    expect(useChatStore.getState().getSessionRuntime(sessionId).chatState).toBe(
      "streaming",
    );
    expect(
      useChatStore.getState().getSessionRuntime(otherSessionId).chatState,
    ).toBe("idle");
  });

  it("does not register listeners when disabled", () => {
    renderHook(() => useAcpStream(false));
    expect(listeners.size).toBe(0);
  });

  it("flushes replayed history only after acp:replay_complete", async () => {
    useChatStore.getState().setSessionLoading(sessionId, true);

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:message_created")).toBeDefined(),
    );

    act(() => {
      emit("acp:message_created", {
        sessionId,
        messageId: "replay-msg",
      });
      emit("acp:text", {
        sessionId,
        messageId: "replay-msg",
        text: "replayed history",
      });
      emit("acp:done", {
        sessionId,
        messageId: "replay-msg",
      });
    });

    expect(
      useChatStore.getState().messagesBySession[sessionId],
    ).toBeUndefined();
    expect(useChatStore.getState().loadingSessionIds.has(sessionId)).toBe(true);

    act(() => {
      emit("acp:replay_complete", { sessionId });
    });

    const messages = useChatStore.getState().messagesBySession[sessionId];
    expect(useChatStore.getState().loadingSessionIds.has(sessionId)).toBe(
      false,
    );
    expect(messages).toHaveLength(1);
    const text = messages[0].content.find((content) => content.type === "text");
    if (text && "text" in text) {
      expect(text.text).toBe("replayed history");
    }
  });

  it("stores model state for the targeted session", async () => {
    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:model_state")).toBeDefined(),
    );

    act(() => {
      emit("acp:model_state", {
        sessionId,
        currentModelId: "claude-sonnet-4",
        currentModelName: "Claude Sonnet 4",
        availableModels: [
          { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
          { id: "gpt-4o", name: "GPT-4o" },
        ],
      });
    });

    expect(
      useChatSessionStore.getState().getSession(sessionId)?.modelName,
    ).toBe("Claude Sonnet 4");
    expect(useChatSessionStore.getState().getSession(sessionId)?.modelId).toBe(
      "claude-sonnet-4",
    );
    expect(useChatSessionStore.getState().getSessionModels(sessionId)).toEqual([
      { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
      { id: "gpt-4o", name: "GPT-4o" },
    ]);
  });

  it("accepts providerless model state when the session already has a provider", async () => {
    useChatSessionStore.setState((state) => ({
      ...state,
      sessions: state.sessions.map((session) =>
        session.id === sessionId
          ? { ...session, providerId: "openai" }
          : session,
      ),
    }));

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:model_state")).toBeDefined(),
    );

    act(() => {
      emit("acp:model_state", {
        sessionId,
        currentModelId: "gpt-4.1",
        currentModelName: "GPT-4.1",
        availableModels: [
          { id: "gpt-4.1", name: "GPT-4.1" },
          { id: "gpt-4o", name: "GPT-4o" },
        ],
      });
    });

    expect(
      useChatSessionStore.getState().getSession(sessionId)?.modelName,
    ).toBe("GPT-4.1");
    expect(useChatSessionStore.getState().getSession(sessionId)?.modelId).toBe(
      "gpt-4.1",
    );
    expect(useChatSessionStore.getState().getSessionModels(sessionId)).toEqual([
      { id: "gpt-4.1", name: "GPT-4.1" },
      { id: "gpt-4o", name: "GPT-4o" },
    ]);
  });

  it("creates the streaming assistant message from backend metadata", async () => {
    useChatStore.getState().setChatState(sessionId, "streaming");
    useChatStore
      .getState()
      .setPendingAssistantProvider(sessionId, "claude-acp");

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:message_created")).toBeDefined(),
    );

    act(() => {
      emit("acp:message_created", {
        sessionId,
        messageId: "msg-created",
        personaId: "persona-1",
        personaName: "Planner",
      });
    });

    const messages = useChatStore.getState().messagesBySession[sessionId];
    expect(messages).toHaveLength(1);
    expect(messages[0]).toMatchObject({
      id: "msg-created",
      role: "assistant",
      metadata: {
        personaId: "persona-1",
        personaName: "Planner",
        providerId: "claude-acp",
        completionStatus: "inProgress",
      },
    });
    expect(
      useChatStore.getState().getSessionRuntime(sessionId).streamingMessageId,
    ).toBe("msg-created");
  });

  it("keeps the original pending provider when the session provider changes before message creation", async () => {
    useChatStore.getState().setChatState(sessionId, "streaming");
    useChatStore
      .getState()
      .setPendingAssistantProvider(sessionId, "claude-acp");
    useChatSessionStore.setState((state) => ({
      ...state,
      sessions: state.sessions.map((session) =>
        session.id === sessionId
          ? { ...session, providerId: "codex-acp" }
          : session,
      ),
    }));

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:message_created")).toBeDefined(),
    );

    act(() => {
      emit("acp:message_created", {
        sessionId,
        messageId: "msg-provider-lock",
      });
    });

    const message = useChatStore.getState().messagesBySession[sessionId][0];
    const runtime = useChatStore.getState().getSessionRuntime(sessionId);
    expect(message.metadata?.providerId).toBe("claude-acp");
    expect(runtime.pendingAssistantProviderId).toBeNull();
  });

  it("ignores late message_created events after local streaming state is cleared", async () => {
    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:message_created")).toBeDefined(),
    );

    act(() => {
      emit("acp:message_created", {
        sessionId,
        messageId: "late-msg",
        personaId: "persona-1",
        personaName: "Planner",
      });
    });

    expect(
      useChatStore.getState().messagesBySession[sessionId],
    ).toBeUndefined();
    expect(
      useChatStore.getState().getSessionRuntime(sessionId).streamingMessageId,
    ).toBeNull();
  });

  it("unregisters listeners on unmount", async () => {
    const { unmount } = renderHook(() => useAcpStream(true));
    await vi.waitFor(() => expect(listeners.get("acp:text")).toBeDefined());

    expect(listeners.get("acp:text")?.length).toBe(1);

    unmount();

    await vi.waitFor(() =>
      expect(listeners.get("acp:text")?.length ?? 0).toBe(0),
    );
  });

  it("ignores late tool title updates after a message is completed", async () => {
    const store = useChatStore.getState();
    store.addMessage(sessionId, {
      id: "msg-1",
      role: "assistant",
      created: Date.now(),
      content: [
        {
          type: "toolRequest",
          id: "tool-1",
          name: "Original title",
          arguments: {},
          status: "executing",
        },
      ],
      metadata: {
        userVisible: true,
        completionStatus: "completed",
      },
    });

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:tool_title")).toBeDefined(),
    );

    act(() => {
      emit("acp:tool_title", {
        sessionId,
        messageId: "msg-1",
        toolCallId: "tool-1",
        title: "Late title",
      });
    });

    const message = useChatStore.getState().messagesBySession[sessionId][0];
    expect(message.content).toContainEqual({
      type: "toolRequest",
      id: "tool-1",
      name: "Original title",
      arguments: {},
      status: "executing",
    });
  });

  it("shows an error state when replay_complete is not received within the timeout", async () => {
    vi.useFakeTimers();
    try {
      renderHook(() => useAcpStream(true));
      await vi.waitFor(() =>
        expect(listeners.get("acp:replay_complete")).toBeDefined(),
      );

      // Start loading AFTER the hook subscribes so the subscription sees the transition
      act(() => {
        useChatStore.getState().setSessionLoading(sessionId, true);
      });

      // Simulate time passing without replay_complete
      act(() => {
        vi.advanceTimersByTime(30_000);
      });

      expect(useChatStore.getState().loadingSessionIds.has(sessionId)).toBe(
        false,
      );
      const runtime = useChatStore.getState().getSessionRuntime(sessionId);
      expect(runtime.error).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });

  it("clears the replay timeout when replay_complete arrives in time", async () => {
    vi.useFakeTimers();
    try {
      renderHook(() => useAcpStream(true));
      await vi.waitFor(() =>
        expect(listeners.get("acp:replay_complete")).toBeDefined(),
      );

      act(() => {
        useChatStore.getState().setSessionLoading(sessionId, true);
      });

      // replay_complete arrives before timeout
      act(() => {
        emit("acp:replay_complete", { sessionId });
      });

      // Advance past the timeout — should NOT set error
      act(() => {
        vi.advanceTimersByTime(30_000);
      });

      const runtime = useChatStore.getState().getSessionRuntime(sessionId);
      expect(runtime.error).toBeFalsy();
      expect(useChatStore.getState().loadingSessionIds.has(sessionId)).toBe(
        false,
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores late tool results after a message is stopped", async () => {
    const store = useChatStore.getState();
    store.addMessage(sessionId, {
      id: "msg-1",
      role: "assistant",
      created: Date.now(),
      content: [
        {
          type: "toolRequest",
          id: "tool-1",
          name: "Lookup",
          arguments: {},
          status: "executing",
        },
      ],
      metadata: {
        userVisible: true,
        completionStatus: "stopped",
      },
    });

    renderHook(() => useAcpStream(true));
    await vi.waitFor(() =>
      expect(listeners.get("acp:tool_result")).toBeDefined(),
    );

    act(() => {
      emit("acp:tool_result", {
        sessionId,
        messageId: "msg-1",
        content: "late result",
      });
    });

    const message = useChatStore.getState().messagesBySession[sessionId][0];
    expect(message.content).toHaveLength(1);
    expect(message.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-1",
      status: "executing",
    });
  });
});
