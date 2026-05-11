import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { INITIAL_TOKEN_STATE } from "@/shared/types/chat";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../chatStore";

function makeMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: crypto.randomUUID(),
    role: "assistant",
    created: Date.now(),
    content: [{ type: "text", text: "hello" }],
    metadata: { userVisible: true },
    ...overrides,
  };
}

function getRuntime(sessionId: string) {
  return useChatStore.getState().getSessionRuntime(sessionId);
}

describe("chatStore", () => {
  beforeEach(() => {
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      skillDraftsBySession: {},
      activeSessionId: null,
      isConnected: false,
    });
  });

  it("starts with empty messages and no active session", () => {
    const state = useChatStore.getState();
    expect(state.messagesBySession).toEqual({});
    expect(state.sessionStateById).toEqual({});
    expect(state.activeSessionId).toBeNull();
  });

  it("stores messages per session", () => {
    const first = makeMessage({ id: "first" });
    const second = makeMessage({ id: "second" });

    useChatStore.getState().addMessage("s1", first);
    useChatStore.getState().addMessage("s2", second);

    expect(useChatStore.getState().messagesBySession.s1).toEqual([first]);
    expect(useChatStore.getState().messagesBySession.s2).toEqual([second]);
  });

  it("updates runtime state per session", () => {
    const store = useChatStore.getState();

    store.setChatState("s1", "streaming");
    store.setStreamingMessageId("s1", "stream-1");
    store.updateTokenState("s1", { inputTokens: 12, outputTokens: 8 });

    const runtime = getRuntime("s1");
    expect(runtime.chatState).toBe("streaming");
    expect(runtime.streamingMessageId).toBe("stream-1");
    expect(runtime.tokenState.totalTokens).toBe(20);
    expect(runtime.hasUsageSnapshot).toBe(true);

    expect(getRuntime("s2").chatState).toBe("idle");
    expect(getRuntime("s2").tokenState).toEqual(INITIAL_TOKEN_STATE);
    expect(getRuntime("s2").hasUsageSnapshot).toBe(false);
  });

  it("appends streamed text only within the targeted session", () => {
    const streaming = makeMessage({
      id: "stream-1",
      content: [{ type: "text", text: "" }],
    });

    useChatStore.getState().setMessages("s1", [streaming]);
    useChatStore.getState().setStreamingMessageId("s1", "stream-1");
    useChatStore.getState().updateStreamingText("s1", "Hello");
    useChatStore.getState().updateStreamingText("s1", " world");

    const updated = useChatStore.getState().messagesBySession.s1[0];
    expect(updated.content[0]).toEqual({ type: "text", text: "Hello world" });
    expect(getRuntime("s2").streamingMessageId).toBeNull();
  });

  it("transitions a session to error without affecting another session", () => {
    const store = useChatStore.getState();

    store.setChatState("s1", "streaming");
    store.setChatState("s2", "thinking");
    store.setError("s1", "boom");

    expect(getRuntime("s1").chatState).toBe("error");
    expect(getRuntime("s1").error).toBe("boom");
    expect(getRuntime("s2").chatState).toBe("thinking");
    expect(getRuntime("s2").error).toBeNull();
  });

  it("tracks unread state per session and clears it idempotently", () => {
    const store = useChatStore.getState();

    store.markSessionUnread("s1");
    expect(getRuntime("s1").hasUnread).toBe(true);
    expect(getRuntime("s2").hasUnread).toBe(false);

    store.markSessionRead("s1");
    store.markSessionRead("s1");
    expect(getRuntime("s1").hasUnread).toBe(false);
  });

  it("clears messages and runtime state for a single session", () => {
    useChatStore.getState().addMessage("s1", makeMessage());
    useChatStore.getState().setChatState("s1", "streaming");
    useChatStore.getState().setStreamingMessageId("s1", "stream-1");
    useChatStore.getState().markSessionUnread("s1");
    useChatStore.getState().clearMessages("s1");

    expect(useChatStore.getState().messagesBySession.s1).toEqual([]);
    expect(getRuntime("s1").chatState).toBe("idle");
    expect(getRuntime("s1").streamingMessageId).toBeNull();
    expect(getRuntime("s1").hasUnread).toBe(false);
  });

  it("enqueues and dismisses messages per session", () => {
    const store = useChatStore.getState();

    store.enqueueMessage("s1", { text: "follow up" });
    expect(useChatStore.getState().queuedMessageBySession.s1).toEqual({
      text: "follow up",
    });
    expect(useChatStore.getState().queuedMessageBySession.s2).toBeUndefined();

    store.dismissQueuedMessage("s1");
    expect(useChatStore.getState().queuedMessageBySession.s1).toBeUndefined();
  });

  it("persists and clears draft text per session", () => {
    const store = useChatStore.getState();

    store.setDraft("s1", "hello world");
    expect(useChatStore.getState().draftsBySession.s1).toBe("hello world");
    expect(useChatStore.getState().draftsBySession.s2).toBeUndefined();

    store.clearDraft("s1");
    expect(useChatStore.getState().draftsBySession.s1).toBeUndefined();
  });

  it("stores and clears skill draft chips per session", () => {
    const store = useChatStore.getState();

    store.setSkillDrafts("s1", [{ id: "skill-1", name: "code-review" }]);
    expect(useChatStore.getState().skillDraftsBySession.s1).toEqual([
      { id: "skill-1", name: "code-review" },
    ]);

    store.clearSkillDrafts("s1");
    expect(useChatStore.getState().skillDraftsBySession.s1).toBeUndefined();
  });

  it("removes session data during cleanup including queued messages and drafts", () => {
    const store = useChatStore.getState();

    store.addMessage("s1", makeMessage());
    store.setChatState("s1", "streaming");
    store.enqueueMessage("s1", { text: "queued" });
    store.setDraft("s1", "draft text");
    store.setSkillDrafts("s1", [{ id: "skill-1", name: "code-review" }]);
    store.setActiveSession("s1");
    store.cleanupSession("s1");

    expect(store.messagesBySession.s1).toBeUndefined();
    expect(store.sessionStateById.s1).toBeUndefined();
    expect(store.queuedMessageBySession.s1).toBeUndefined();
    expect(store.draftsBySession.s1).toBeUndefined();
    expect(useChatStore.getState().skillDraftsBySession.s1).toBeUndefined();
    expect(store.activeSessionId).toBeNull();
  });

  it("stores and clears scroll targets per session", () => {
    const store = useChatStore.getState();

    store.setScrollTargetMessage("s1", "message-1", "needle");
    expect(useChatStore.getState().scrollTargetMessageBySession.s1).toEqual({
      messageId: "message-1",
      query: "needle",
    });

    store.clearScrollTargetMessage("s1");
    expect(
      useChatStore.getState().scrollTargetMessageBySession.s1,
    ).toBeUndefined();
  });
});

describe("chatStore draft localStorage persistence", () => {
  const STORAGE_KEY = "goose:chat-drafts";

  beforeEach(() => {
    window.localStorage.removeItem(STORAGE_KEY);
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      skillDraftsBySession: {},
      activeSessionId: null,
      isConnected: false,
    });
  });

  afterEach(() => {
    window.localStorage.removeItem(STORAGE_KEY);
  });

  it("persists non-empty drafts to localStorage on setDraft", () => {
    useChatStore.getState().setDraft("s1", "hello");

    const stored = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored).toEqual({ s1: "hello" });
  });

  it("removes empty drafts from localStorage", () => {
    useChatStore.getState().setDraft("s1", "hello");
    useChatStore.getState().setDraft("s1", "");

    const stored = window.localStorage.getItem(STORAGE_KEY);
    expect(stored).toBeNull();
  });

  it("removes draft from localStorage on clearDraft", () => {
    useChatStore.getState().setDraft("s1", "hello");
    useChatStore.getState().clearDraft("s1");

    const stored = window.localStorage.getItem(STORAGE_KEY);
    expect(stored).toBeNull();
  });

  it("removes draft from localStorage on cleanupSession", () => {
    useChatStore.getState().setDraft("s1", "hello");
    useChatStore.getState().setDraft("s2", "world");
    useChatStore.getState().cleanupSession("s1");

    const stored = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored).toEqual({ s2: "world" });
  });

  it("preserves other session drafts when one is cleared", () => {
    useChatStore.getState().setDraft("s1", "hello");
    useChatStore.getState().setDraft("s2", "world");
    useChatStore.getState().clearDraft("s1");

    const stored = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored).toEqual({ s2: "world" });
  });
});

describe("chatStore session loading state", () => {
  beforeEach(() => {
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      skillDraftsBySession: {},
      activeSessionId: null,
      isConnected: false,
      loadingSessionIds: new Set<string>(),
    });
  });

  it("starts with empty loadingSessionIds", () => {
    expect(useChatStore.getState().loadingSessionIds.size).toBe(0);
  });

  it("adds session to loadingSessionIds when setSessionLoading(true)", () => {
    useChatStore.getState().setSessionLoading("s1", true);

    expect(useChatStore.getState().loadingSessionIds.has("s1")).toBe(true);
  });

  it("removes session from loadingSessionIds when setSessionLoading(false)", () => {
    useChatStore.getState().setSessionLoading("s1", true);
    useChatStore.getState().setSessionLoading("s1", false);

    expect(useChatStore.getState().loadingSessionIds.has("s1")).toBe(false);
  });

  it("tracks multiple sessions independently", () => {
    useChatStore.getState().setSessionLoading("s1", true);
    useChatStore.getState().setSessionLoading("s2", true);

    expect(useChatStore.getState().loadingSessionIds.has("s1")).toBe(true);
    expect(useChatStore.getState().loadingSessionIds.has("s2")).toBe(true);

    useChatStore.getState().setSessionLoading("s1", false);

    expect(useChatStore.getState().loadingSessionIds.has("s1")).toBe(false);
    expect(useChatStore.getState().loadingSessionIds.has("s2")).toBe(true);
  });

  it("is idempotent for adding the same session", () => {
    useChatStore.getState().setSessionLoading("s1", true);
    useChatStore.getState().setSessionLoading("s1", true);

    expect(useChatStore.getState().loadingSessionIds.size).toBe(1);
  });

  it("is idempotent for removing a non-existent session", () => {
    useChatStore.getState().setSessionLoading("s1", false);

    expect(useChatStore.getState().loadingSessionIds.size).toBe(0);
  });
});
