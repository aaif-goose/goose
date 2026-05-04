import { beforeEach, describe, expect, it } from "vitest";
import type { SessionNotification } from "@agentclientprotocol/sdk";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  clearReplayBuffer,
  getAndDeleteReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { registerSession } from "./acpSessionTracker";
import {
  clearMessageTracking,
  handleSessionNotification,
} from "./acpNotificationHandler";

describe("acpNotificationHandler", () => {
  beforeEach(() => {
    clearMessageTracking();
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

  it("threads tool chain summary onto the streaming tool request (live)", async () => {
    registerSession("draft-session-1", "goose-session-1", "goose", "/tmp");

    await handleSessionNotification({
      sessionId: "goose-session-1",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-1",
        title: "running ls",
      },
    } as SessionNotification);

    await handleSessionNotification({
      sessionId: "goose-session-1",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-2",
        title: "running pwd",
      },
    } as SessionNotification);

    await handleSessionNotification({
      sessionId: "goose-session-1",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tc-1",
        _meta: {
          goose: {
            toolChainSummary: {
              summary: "inspected working directory",
              count: 2,
            },
          },
        },
      },
    } as SessionNotification);

    const messages =
      useChatStore.getState().messagesBySession["draft-session-1"];
    expect(messages).toBeTruthy();
    const toolReqs =
      messages?.flatMap((m) =>
        m.content.filter((c) => c.type === "toolRequest"),
      ) ?? [];
    const first = toolReqs.find(
      (c) => c.type === "toolRequest" && c.id === "tc-1",
    );
    const second = toolReqs.find(
      (c) => c.type === "toolRequest" && c.id === "tc-2",
    );
    expect(first?.type === "toolRequest" && first.chainSummary).toEqual({
      summary: "inspected working directory",
      count: 2,
    });
    expect(
      second?.type === "toolRequest" && second.chainSummary,
    ).toBeUndefined();
  });

  it("attaches tool chain summary on initial tool_call during replay", async () => {
    await handleSessionNotification({
      sessionId: "goose-session-2",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-1",
        title: "ran two things",
        _meta: {
          goose: {
            toolChainSummary: {
              summary: "applied dark mode polish",
              count: 4,
            },
          },
        },
      },
    } as SessionNotification);

    registerSession("draft-session-2", "goose-session-2", "goose", "/tmp");

    const buffered =
      useChatStore.getState().messagesBySession["draft-session-2"];
    expect(buffered).toBeTruthy();
    const tc = buffered
      ?.flatMap((m) => m.content)
      .find((c) => c.type === "toolRequest" && c.id === "tc-1");
    expect(tc?.type === "toolRequest" && tc.chainSummary).toEqual({
      summary: "applied dark mode polish",
      count: 4,
    });
  });
});
