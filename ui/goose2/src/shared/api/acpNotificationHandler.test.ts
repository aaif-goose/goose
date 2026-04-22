import { beforeEach, describe, expect, it } from "vitest";
import type { SessionNotification } from "@agentclientprotocol/sdk";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  clearReplayBuffer,
  getAndDeleteReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { registerSession } from "./acpSessionTracker";
import { handleSessionNotification } from "./acpNotificationHandler";

describe("acpNotificationHandler", () => {
  beforeEach(() => {
    clearReplayBuffer("draft-session-1");
    clearReplayBuffer("draft-session-2");
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      activeSessionId: null,
      isConnected: false,
      loadingSessionIds: new Set<string>(),
      scrollTargetMessageBySession: {},
    });
  });

  it("buffers usage updates until the local session mapping is registered", async () => {
    const notification = {
      sessionId: "goose-session-1",
      update: {
        sessionUpdate: "usage_update",
        used: 512,
        size: 8192,
      },
    } as SessionNotification;

    await handleSessionNotification(notification);

    expect(
      useChatStore.getState().sessionStateById["draft-session-1"],
    ).toBeUndefined();
    expect(
      useChatStore.getState().sessionStateById["goose-session-1"],
    ).toBeUndefined();

    registerSession("draft-session-1", "goose-session-1", "goose", "/tmp");

    const runtime = useChatStore
      .getState()
      .getSessionRuntime("draft-session-1");
    expect(runtime.tokenState.accumulatedTotal).toBe(512);
    expect(runtime.tokenState.contextLimit).toBe(8192);
    expect(runtime.hasUsageSnapshot).toBe(true);
  });

  it("does not buffer non-usage updates before the local session mapping exists", async () => {
    const notification = {
      sessionId: "goose-session-2",
      update: {
        sessionUpdate: "agent_message_chunk",
        messageId: "message-1",
        content: {
          type: "text",
          text: "hello from replay",
        },
      },
    } as SessionNotification;

    await handleSessionNotification(notification);
    registerSession("draft-session-2", "goose-session-2", "goose", "/tmp");

    expect(getAndDeleteReplayBuffer("draft-session-2")).toBeUndefined();
    expect(
      useChatStore.getState().messagesBySession["draft-session-2"],
    ).toBeUndefined();
  });
});
