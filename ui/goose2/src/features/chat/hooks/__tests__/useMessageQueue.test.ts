import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatState } from "@/shared/types/chat";
import { useChatStore } from "../../stores/chatStore";
import { useMessageQueue } from "../useMessageQueue";

describe("useMessageQueue", () => {
  beforeEach(() => {
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      activeSessionId: null,
      isConnected: false,
    });
  });

  it("starts with no queued message", () => {
    const sendMessage = vi.fn();
    const { result } = renderHook(() =>
      useMessageQueue("s1", "idle", sendMessage),
    );
    expect(result.current.queuedMessage).toBeNull();
  });

  it("enqueue stores a message in the Zustand store", () => {
    const sendMessage = vi.fn();
    const { result } = renderHook(() =>
      useMessageQueue("s1", "streaming", sendMessage),
    );

    act(() => result.current.enqueue("follow up"));

    expect(result.current.queuedMessage).toEqual({ text: "follow up" });
    expect(useChatStore.getState().queuedMessageBySession.s1).toEqual({
      text: "follow up",
    });
  });

  it("auto-sends queued message when chatState transitions to idle", () => {
    const sendMessage = vi.fn();
    // Start streaming with a queued message
    useChatStore.getState().enqueueMessage("s1", { text: "queued msg" });

    const { rerender } = renderHook(
      ({ chatState }: { chatState: ChatState }) =>
        useMessageQueue("s1", chatState, sendMessage),
      { initialProps: { chatState: "streaming" as ChatState } },
    );

    expect(sendMessage).not.toHaveBeenCalled();

    // Transition to idle
    rerender({ chatState: "idle" as const });

    expect(sendMessage).toHaveBeenCalledWith(
      "queued msg",
      undefined,
      undefined,
    );
    expect(useChatStore.getState().queuedMessageBySession.s1).toBeUndefined();
  });

  it("does not auto-send when chatState is not idle", () => {
    const sendMessage = vi.fn();
    useChatStore.getState().enqueueMessage("s1", { text: "queued" });

    renderHook(() => useMessageQueue("s1", "streaming", sendMessage));

    expect(sendMessage).not.toHaveBeenCalled();
    expect(useChatStore.getState().queuedMessageBySession.s1).toBeDefined();
  });

  it("dismiss clears the queued message without sending", () => {
    const sendMessage = vi.fn();
    useChatStore.getState().enqueueMessage("s1", { text: "queued" });

    const { result } = renderHook(() =>
      useMessageQueue("s1", "streaming", sendMessage),
    );

    act(() => result.current.dismiss());

    expect(result.current.queuedMessage).toBeNull();
    expect(sendMessage).not.toHaveBeenCalled();
  });

  it("queued messages are scoped to session", () => {
    const sendMessage = vi.fn();
    useChatStore.getState().enqueueMessage("s2", { text: "other session" });

    const { result } = renderHook(() =>
      useMessageQueue("s1", "idle", sendMessage),
    );

    expect(result.current.queuedMessage).toBeNull();
    expect(sendMessage).not.toHaveBeenCalled();
  });

  it("includes images when auto-sending", () => {
    const sendMessage = vi.fn();
    const attachments = [
      {
        id: "image-1",
        kind: "image" as const,
        name: "image.png",
        base64: "abc",
        mimeType: "image/png",
        previewUrl: "blob:image",
      },
    ];
    useChatStore.getState().enqueueMessage("s1", {
      text: "with image",
      attachments,
    });

    renderHook(
      ({ chatState }: { chatState: ChatState }) =>
        useMessageQueue("s1", chatState, sendMessage),
      { initialProps: { chatState: "idle" as ChatState } },
    );

    expect(sendMessage).toHaveBeenCalledWith(
      "with image",
      undefined,
      attachments,
    );
  });

  it("preserves personaId when auto-sending", () => {
    const sendMessage = vi.fn();
    useChatStore.getState().enqueueMessage("s1", {
      text: "for persona A",
      personaId: "persona-a",
    });

    renderHook(
      ({ chatState }: { chatState: ChatState }) =>
        useMessageQueue("s1", chatState, sendMessage),
      { initialProps: { chatState: "idle" as ChatState } },
    );

    expect(sendMessage).toHaveBeenCalledWith(
      "for persona A",
      { id: "persona-a" },
      undefined,
    );
  });
});
