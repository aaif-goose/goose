import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../../stores/chatStore";
import { clearReplayBuffer, ensureReplayBuffer } from "../replayBuffer";

const mockAcpSendMessage = vi.fn();
const mockAcpLoadSession = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: vi.fn(),
  acpLoadSession: (...args: unknown[]) => mockAcpLoadSession(...args),
  acpPrepareSession: vi.fn(),
  acpSetModel: vi.fn(),
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
  });

  it("reloads compacted history after sending the compact command", async () => {
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
    expect(mockAcpLoadSession).toHaveBeenCalledWith("session-1", undefined);

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(4);
    expect(messages[0]).toEqual(
      createTextMessage("user-1", "user", "Before compact"),
    );
    expect(messages[1]).toEqual(
      createTextMessage("compact-1", "user", "/compact/compact"),
    );
    expect(messages[2]).toEqual(
      createTextMessage("assistant-1", "assistant", "After compact"),
    );
    expect(messages[3]).toMatchObject({
      role: "system",
      content: [
        {
          type: "systemNotification",
          notificationType: "compaction",
          text: "Conversation compacted. Older context was summarized.",
        },
      ],
      metadata: {
        userVisible: true,
        agentVisible: false,
      },
    });
    expect(runtime.chatState).toBe("idle");
    expect(runtime.error).toBeNull();
    expect(useChatStore.getState().loadingSessionIds.has("session-1")).toBe(
      false,
    );
  });

  it("prepares and compacts the override persona session", async () => {
    let preparedPersonaId: string | undefined;
    const ensurePrepared = vi.fn(async (personaId?: string) => {
      preparedPersonaId = personaId;
      return undefined;
    });

    const { result } = renderHook(() =>
      useChat(
        "session-1",
        undefined,
        undefined,
        { id: "persona-b", name: "Persona B" },
        { ensurePrepared },
      ),
    );

    await act(async () => {
      await result.current.compactConversation({ id: "persona-a" });
    });

    expect(ensurePrepared).toHaveBeenCalledWith("persona-a");
    expect(mockAcpSendMessage).toHaveBeenCalledWith("session-1", "/compact", {
      personaId: "persona-a",
    });
    expect(mockAcpLoadSession).toHaveBeenCalledWith("session-1", undefined);
    expect(preparedPersonaId).toBe("persona-a");
  });

  it("blocks new sends while compaction is in flight", async () => {
    const compactDeferred = createDeferredPromise();
    mockAcpSendMessage.mockImplementation(
      (_sessionId: string, prompt: string) =>
        prompt === "/compact" ? compactDeferred.promise : Promise.resolve(),
    );

    const { result } = renderHook(() => useChat("session-1"));

    let compactPromise!: Promise<unknown>;
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
    const compactDeferred = createDeferredPromise();
    mockAcpSendMessage.mockImplementation(
      (_sessionId: string, prompt: string) =>
        prompt === "/compact" ? compactDeferred.promise : Promise.resolve(),
    );

    const { result } = renderHook(() => useChat("session-1"));

    let firstCompact!: Promise<unknown>;
    let secondCompact!: Promise<unknown>;
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

  it("surfaces an error when preparing for compaction fails", async () => {
    const ensurePrepared = vi
      .fn()
      .mockRejectedValue(new Error("prepare failed"));

    const { result } = renderHook(() =>
      useChat("session-1", undefined, undefined, undefined, {
        ensurePrepared,
      }),
    );

    await act(async () => {
      await result.current.compactConversation();
    });

    expect(ensurePrepared).toHaveBeenCalledWith(undefined);
    expect(mockAcpSendMessage).not.toHaveBeenCalled();
    expect(mockAcpLoadSession).not.toHaveBeenCalled();

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(1);
    expect(messages[0].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "prepare failed",
      },
    ]);
    expect(runtime.error).toBe("prepare failed");
    expect(runtime.chatState).toBe("idle");
  });

  it("does not compact when preparation is superseded", async () => {
    const ensurePrepared = vi.fn().mockResolvedValue(false);

    const { result } = renderHook(() =>
      useChat("session-1", undefined, undefined, undefined, {
        ensurePrepared,
      }),
    );

    let compactResult: unknown;
    await act(async () => {
      compactResult = await result.current.compactConversation();
    });

    expect(compactResult).toBe("failed");
    expect(ensurePrepared).toHaveBeenCalledWith(undefined);
    expect(mockAcpSendMessage).not.toHaveBeenCalled();
    expect(mockAcpLoadSession).not.toHaveBeenCalled();

    const messages = useChatStore.getState().messagesBySession["session-1"];
    const runtime = useChatStore.getState().getSessionRuntime("session-1");

    expect(messages).toHaveLength(1);
    expect(messages[0].content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "Session configuration changed while preparing. Try sending again.",
      },
    ]);
    expect(runtime.error).toBe(
      "Session configuration changed while preparing. Try sending again.",
    );
    expect(runtime.chatState).toBe("idle");
  });
});
