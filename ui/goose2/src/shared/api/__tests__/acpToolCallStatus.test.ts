import { beforeEach, describe, expect, it } from "vitest";
import {
  clearReplayBuffer,
  getReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { useChatStore } from "@/features/chat/stores/chatStore";
import {
  clearMessageTracking,
  handleSessionNotification,
  setActiveMessageId,
} from "../acpNotificationHandler";
import { registerSession } from "../acpSessionTracker";

describe("ACP tool call status handling", () => {
  beforeEach(() => {
    clearMessageTracking();
    clearReplayBuffer("replay-failed-tool-session");
    clearReplayBuffer("goose-session");
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

  it("marks failed replay tool updates as errors", async () => {
    const replaySessionId = "replay-failed-tool-session";
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "shell",
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "failed",
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Command failed.",
            },
          },
        ],
      },
    } as never);

    const assistant = getReplayBuffer(replaySessionId)?.[0];
    expect(assistant?.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-1",
      status: "error",
    });
    expect(assistant?.content[1]).toMatchObject({
      type: "toolResponse",
      id: "tool-1",
      isError: true,
      result: "Command failed.",
    });
  });

  it("marks failed live tool updates as errors", async () => {
    registerSession(
      "local-session",
      "goose-session",
      "goose",
      "/Users/aharvard/.goose/artifacts",
    );
    setActiveMessageId("goose-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "goose-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "shell",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "goose-session",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "failed",
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Command failed.",
            },
          },
        ],
      },
    } as never);

    const [message] =
      useChatStore.getState().messagesBySession["local-session"];
    expect(message.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-1",
      status: "error",
    });
    expect(message.content[1]).toMatchObject({
      type: "toolResponse",
      id: "tool-1",
      isError: true,
      result: "Command failed.",
    });
  });
});
