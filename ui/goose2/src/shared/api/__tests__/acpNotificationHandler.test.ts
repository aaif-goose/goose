import { waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearReplayBuffer,
  getReplayBuffer,
} from "@/features/chat/hooks/replayBuffer";
import { useChatStore } from "@/features/chat/stores/chatStore";
import type { McpAppPayload } from "@/shared/types/messages";
import {
  clearMessageTracking,
  handleSessionNotification,
  setActiveMessageId,
} from "../acpNotificationHandler";
import { registerSession } from "../acpSessionTracker";

function createMcpAppPayload(): McpAppPayload {
  return {
    sessionId: "local-session",
    gooseSessionId: "goose-session",
    toolCallId: "tool-1",
    toolCallTitle: "mcp_app_bench__inspect_host_info",
    source: "toolCallUpdateMeta",
    tool: {
      name: "mcp_app_bench__inspect_host_info",
      extensionName: "mcp_app_bench",
      resourceUri: "ui://inspect-host-info",
    },
    resource: {
      result: null,
    },
  };
}

describe("acpNotificationHandler", () => {
  beforeEach(() => {
    clearMessageTracking();
    clearReplayBuffer("local-session");
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

  it("keeps tool calls that arrive before the first text chunk on the pending assistant message", async () => {
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
        title: "mcp_app_bench__inspect_host_info",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "goose-session",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "completed",
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Opened the Host Info inspector.",
            },
          },
        ],
        _meta: {
          goose: {
            mcpApp: {
              toolName: "mcp_app_bench__inspect_host_info",
              extensionName: "mcp_app_bench",
              resourceUri: "ui://inspect-host-info",
            },
          },
        },
      },
    } as never);

    await handleSessionNotification({
      sessionId: "goose-session",
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text: "The Host Info inspector is now open.",
        },
      },
    } as never);

    await waitFor(() => {
      const message =
        useChatStore.getState().messagesBySession["local-session"]?.[0];
      expect(message?.content.some((block) => block.type === "mcpApp")).toBe(
        true,
      );
    });

    const [message] =
      useChatStore.getState().messagesBySession["local-session"];
    expect(message.id).toBe("assistant-1");
    expect(message.content.map((block) => block.type)).toEqual([
      "toolRequest",
      "toolResponse",
      "mcpApp",
      "text",
    ]);
    expect(message.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-1",
      name: "mcp_app_bench__inspect_host_info",
      status: "completed",
    });
    expect(message.content[1]).toMatchObject({
      type: "toolResponse",
      id: "tool-1",
      name: "mcp_app_bench__inspect_host_info",
      result: "Opened the Host Info inspector.",
      isError: false,
    });
    expect(message.content[2]).toMatchObject({
      type: "mcpApp",
      id: "tool-1",
      payload: createMcpAppPayload(),
    });
    expect(message.content[3]).toMatchObject({
      type: "text",
      text: "The Host Info inspector is now open.",
    });
    expect(
      useChatStore.getState().getSessionRuntime("local-session")
        .streamingMessageId,
    ).toBe("assistant-1");
  });

  it("replay keeps tool and MCP app content on an assistant message when tool events arrive before text", async () => {
    const replaySessionId = "replay-goose-session";
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "user_message_chunk",
        messageId: "user-1",
        content: {
          type: "text",
          text: "run the app bench",
        },
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "mcp_app_bench__inspect_host_info",
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "completed",
        content: [
          {
            type: "content",
            content: {
              type: "text",
              text: "Opened the Host Info inspector.",
            },
          },
        ],
        _meta: {
          goose: {
            mcpApp: {
              toolName: "mcp_app_bench__inspect_host_info",
              extensionName: "mcp_app_bench",
              resourceUri: "ui://inspect-host-info",
            },
          },
        },
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "agent_message_chunk",
        messageId: "assistant-1",
        content: {
          type: "text",
          text: "The Host Info inspector is now open.",
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    expect(buffer).toHaveLength(2);
    expect(buffer?.[0]).toMatchObject({
      id: "user-1",
      role: "user",
      content: [{ type: "text", text: "run the app bench" }],
    });
    expect(
      buffer?.[0]?.content.some((block) => block.type === "toolRequest"),
    ).toBe(false);

    expect(buffer?.[1]?.id).toBe("assistant-1");
    expect(buffer?.[1]?.role).toBe("assistant");
    expect(buffer?.[1]?.content.map((block) => block.type)).toEqual([
      "toolRequest",
      "toolResponse",
      "mcpApp",
      "text",
    ]);
    expect(buffer?.[1]?.content[2]).toMatchObject({
      type: "mcpApp",
      id: "tool-1",
      payload: {
        ...createMcpAppPayload(),
        sessionId: replaySessionId,
        gooseSessionId: replaySessionId,
      },
    });
  });

  it("replay restores skill chips from assistant-only user chunks", async () => {
    const replaySessionId = "replay-skill-session";
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "user_message_chunk",
        messageId: "user-1",
        content: {
          type: "text",
          text: "Use these skills for this request: capture-task.",
          annotations: { audience: ["assistant"] },
        },
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "user_message_chunk",
        messageId: "user-1",
        content: {
          type: "text",
          text: "redo the settings modal",
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    expect(buffer).toHaveLength(1);
    expect(buffer?.[0]).toMatchObject({
      id: "user-1",
      role: "user",
      content: [{ type: "text", text: "redo the settings modal" }],
      metadata: {
        chips: [{ label: "capture-task", type: "skill" }],
      },
    });
  });

  it("replay preserves gooseSessionId in MCP app payloads before tracker registration", async () => {
    const replaySessionId = "replay-goose-session-2";
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "mcp_app_bench__inspect_host_info",
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "completed",
        _meta: {
          goose: {
            mcpApp: {
              toolName: "mcp_app_bench__inspect_host_info",
              extensionName: "mcp_app_bench",
              resourceUri: "ui://inspect-host-info",
            },
          },
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    const assistant = buffer?.[0];
    const mcpAppBlock = assistant?.content.find(
      (block) => block.type === "mcpApp",
    );
    expect(mcpAppBlock).toMatchObject({
      type: "mcpApp",
      payload: expect.objectContaining({
        gooseSessionId: replaySessionId,
      }),
    });
  });
});
