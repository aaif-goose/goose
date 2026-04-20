import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../../stores/chatStore";
import { clearReplayBuffer, ensureReplayBuffer } from "../replayBuffer";

const mockAcpSendMessage = vi.fn();
const mockAcpLoadSession = vi.fn();
const mockGetGooseSessionId = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: vi.fn(),
  acpLoadSession: (...args: unknown[]) => mockAcpLoadSession(...args),
  acpPrepareSession: vi.fn(),
  acpSetModel: vi.fn(),
}));

vi.mock("@/shared/api/acpSessionTracker", () => ({
  getGooseSessionId: (...args: unknown[]) => mockGetGooseSessionId(...args),
}));

import { useChat } from "../useChat";

function createDeferredPromise<T = void>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function createTextMessage(
  id: string,
  role: Message["role"],
  text: string,
): Message {
  return {
    id,
    role,
    created: 0,
    content: [{ type: "text", text }],
    metadata: {
      userVisible: true,
      agentVisible: role !== "system",
    },
  };
}

describe("useChat compaction", () => {
  beforeEach(() => {
    mockAcpSendMessage.mockReset();
    mockAcpLoadSession.mockReset();
    mockGetGooseSessionId.mockReset();
    clearReplayBuffer("session-1");
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: null,
      isConnected: true,
      loadingSessionIds: new Set<string>(),
    });
    mockAcpSendMessage.mockResolvedValue(undefined);
    mockAcpLoadSession.mockResolvedValue(undefined);
    mockGetGooseSessionId.mockReturnValue(null);
  });

  it("reloads compacted history after sending the compact command", async () => {
    mockGetGooseSessionId.mockReturnValue("goose-session-1");
    mockAcpLoadSession.mockImplementation(async (sessionId: string) => {
      const buffer = ensureReplayBuffer(sessionId);
      buffer.push(createTextMessage("user-1", "user", "Before compact"));
      buffer.push(createTextMessage("compact-1", "user", "/compact/compact"));
      buffer.push(
        createTextMessage("assistant-1", "assistant", "After compact"),
      );
    });

    useChatStore
      .getState()
      .setMessages("session-1", [
        createTextMessage("stale-1", "assistant", "Stale"),
      ]);

    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.compactConversation();
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/compact",
      undefined,
    );
    expect(mockAcpLoadSession).toHaveBeenCalledWith(
      "session-1",
      "goose-session-1",
      undefined,
    );

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toEqual([
      createTextMessage("user-1", "user", "Before compact"),
      createTextMessage("assistant-1", "assistant", "After compact"),
    ]);
    expect(runtime.chatState).toBe("idle");
    expect(runtime.error).toBeNull();
    expect(useChatStore.getState().loadingSessionIds.has("session-1")).toBe(
      false,
    );
  });

  it("blocks new sends while compaction is in flight", async () => {
    mockGetGooseSessionId.mockReturnValue("goose-session-1");
    const compactDeferred = createDeferredPromise();
    mockAcpSendMessage.mockImplementation(
      (_sessionId: string, prompt: string) =>
        prompt === "/compact" ? compactDeferred.promise : Promise.resolve(),
    );

    const { result } = renderHook(() => useChat("session-1"));

    let compactPromise!: Promise<void>;
    await act(async () => {
      compactPromise = result.current.compactConversation();
      await Promise.resolve();
    });

    expect(
      useChatStore.getState().getSessionRuntime("session-1").chatState,
    ).toBe("compacting");

    await act(async () => {
      await result.current.sendMessage("Hello during compact");
    });

    expect(mockAcpSendMessage).toHaveBeenCalledTimes(1);
    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/compact",
      undefined,
    );
    expect(
      useChatStore.getState().messagesBySession["session-1"],
    ).toBeUndefined();
    expect(
      useChatStore.getState().getSessionRuntime("session-1").chatState,
    ).toBe("compacting");

    compactDeferred.resolve();
    await act(async () => {
      await compactPromise;
    });

    expect(
      useChatStore.getState().getSessionRuntime("session-1").chatState,
    ).toBe("idle");
  });

  it("ignores a second compact request while the first one is still in flight", async () => {
    mockGetGooseSessionId.mockReturnValue("goose-session-1");
    const compactDeferred = createDeferredPromise();
    mockAcpSendMessage.mockImplementation(
      (_sessionId: string, prompt: string) =>
        prompt === "/compact" ? compactDeferred.promise : Promise.resolve(),
    );

    const { result } = renderHook(() => useChat("session-1"));

    let firstCompact!: Promise<void>;
    let secondCompact!: Promise<void>;
    await act(async () => {
      firstCompact = result.current.compactConversation();
      secondCompact = result.current.compactConversation();
      await Promise.resolve();
    });

    expect(mockAcpSendMessage).toHaveBeenCalledTimes(1);
    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "/compact",
      undefined,
    );
    expect(mockAcpLoadSession).not.toHaveBeenCalled();
    expect(
      useChatStore.getState().getSessionRuntime("session-1").chatState,
    ).toBe("compacting");

    compactDeferred.resolve();
    await act(async () => {
      await Promise.all([firstCompact, secondCompact]);
    });

    expect(mockAcpLoadSession).toHaveBeenCalledTimes(1);
    expect(
      useChatStore.getState().getSessionRuntime("session-1").chatState,
    ).toBe("idle");
  });

  it("surfaces an error when compacting before the session is prepared", async () => {
    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.compactConversation();
    });

    expect(mockAcpSendMessage).not.toHaveBeenCalled();
    expect(mockAcpLoadSession).not.toHaveBeenCalled();

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(1);
    expect(messages[0].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Session not prepared. Send a message before compacting.",
      },
    ]);
    expect(runtime.error).toBe(
      "Session not prepared. Send a message before compacting.",
    );
  });
});
