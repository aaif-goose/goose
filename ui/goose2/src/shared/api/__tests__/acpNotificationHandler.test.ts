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
import { registerPreparedSession } from "../acpSessionRegistry";

function createMcpAppPayload(): McpAppPayload {
  return {
    sessionId: "acp-session",
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
    clearReplayBuffer("acp-session");
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
    registerPreparedSession("acp-session", "goose", "/Users/aharvard");
    setActiveMessageId("acp-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "mcp_app_bench__inspect_host_info",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
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
      sessionId: "acp-session",
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
        useChatStore.getState().messagesBySession["acp-session"]?.[0];
      expect(message?.content.some((block) => block.type === "mcpApp")).toBe(
        true,
      );
    });

    const [message] = useChatStore.getState().messagesBySession["acp-session"];
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
      toolName: "mcp_app_bench__inspect_host_info",
      extensionName: "mcp_app_bench",
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
      useChatStore.getState().getSessionRuntime("acp-session")
        .streamingMessageId,
    ).toBe("assistant-1");
  });

  it("preserves ACP tool kind and locations on tool requests", async () => {
    registerPreparedSession("acp-session", "goose", "/Users/test");
    setActiveMessageId("acp-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "write_file",
        kind: "edit",
        locations: [{ path: "/tmp/report.md", line: 7 }],
        rawInput: { path: "/tmp/report.md" },
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-1",
        status: "completed",
        locations: [{ path: "/tmp/report.md", line: 9 }],
      },
    } as never);

    const [message] = useChatStore.getState().messagesBySession["acp-session"];
    expect(message.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-1",
      arguments: { path: "/tmp/report.md" },
      toolKind: "edit",
      locations: [{ path: "/tmp/report.md", line: 9 }],
      status: "completed",
    });
  });

  it("attributes a completed live tool response to the matching request when a sibling is still executing", async () => {
    // Regression: with two sibling tool requests, completing the first
    // while the second is still unpaired must label the response with the
    // first request's name. Previously the live path used the latest
    // unpaired request, which could swap names across siblings.
    registerPreparedSession("acp-session", "goose", "/Users/test");
    setActiveMessageId("acp-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-a",
        title: "read_file",
        rawInput: { path: "/tmp/notes.md" },
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-b",
        title: "grep",
        rawInput: { pattern: "TODO" },
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-a",
        status: "completed",
        content: [
          {
            type: "content",
            content: { type: "text", text: "file contents" },
          },
        ],
      },
    } as never);

    const [message] = useChatStore.getState().messagesBySession["acp-session"];
    expect(message.content.map((block) => block.type)).toEqual([
      "toolRequest",
      "toolRequest",
      "toolResponse",
    ]);
    expect(message.content[0]).toMatchObject({
      type: "toolRequest",
      id: "tool-a",
      name: "read_file",
      status: "completed",
    });
    expect(message.content[1]).toMatchObject({
      type: "toolRequest",
      id: "tool-b",
      name: "grep",
      status: "in_progress",
    });
    expect(message.content[2]).toMatchObject({
      type: "toolResponse",
      id: "tool-a",
      name: "read_file",
      result: "file contents",
      isError: false,
    });
  });

  it("keeps a late live tool response from moving the streaming pointer back to its owner message", async () => {
    registerPreparedSession("acp-session", "goose", "/Users/test");
    setActiveMessageId("acp-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-a",
        title: "read_file",
        rawInput: { path: "/tmp/notes.md" },
      },
    } as never);

    const beforeMessages =
      useChatStore.getState().messagesBySession["acp-session"] ?? [];
    useChatStore.setState((state) => ({
      ...state,
      messagesBySession: {
        ...state.messagesBySession,
        "acp-session": [
          ...beforeMessages,
          {
            id: "assistant-2",
            role: "assistant",
            created: Date.now(),
            content: [],
            metadata: {
              userVisible: true,
              agentVisible: true,
              completionStatus: "inProgress",
            },
          },
        ],
      },
    }));
    useChatStore.getState().setStreamingMessageId("acp-session", "assistant-2");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call_update",
        toolCallId: "tool-a",
        status: "completed",
        content: [
          {
            type: "content",
            content: { type: "text", text: "file contents" },
          },
        ],
      },
    } as never);

    expect(
      useChatStore.getState().getSessionRuntime("acp-session")
        .streamingMessageId,
    ).toBe("assistant-2");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text: "Continuing with the answer.",
        },
      },
    } as never);

    const messages = useChatStore.getState().messagesBySession["acp-session"];
    const ownerMessage = messages.find((m) => m.id === "assistant-1");
    const currentMessage = messages.find((m) => m.id === "assistant-2");

    expect(ownerMessage?.content.map((block) => block.type)).toEqual([
      "toolRequest",
      "toolResponse",
    ]);
    expect(currentMessage?.content).toEqual([
      { type: "text", text: "Continuing with the answer." },
    ]);
  });

  it("preserves structured tool output when ACP provides rawOutput", async () => {
    registerPreparedSession(
      "acp-session",
      "goose",
      "/Users/aharvard/.goose/artifacts",
    );
    setActiveMessageId("acp-session", "assistant-1");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "mcp_app_bench__inspect_host_info",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
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
        rawOutput: {
          inspector: "host-info",
          supported: true,
        },
      },
    } as never);

    const [message] = useChatStore.getState().messagesBySession["acp-session"];
    expect(message.content[1]).toMatchObject({
      type: "toolResponse",
      id: "tool-1",
      result: "Opened the Host Info inspector.",
      structuredContent: {
        inspector: "host-info",
        supported: true,
      },
      isError: false,
    });
  });

  it("replay keeps tool and MCP app content on an assistant message when tool events arrive before text", async () => {
    const replaySessionId = "replay-acp-session";
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
    expect(buffer?.[1]?.content[0]).toMatchObject({
      type: "toolRequest",
      toolName: "mcp_app_bench__inspect_host_info",
      extensionName: "mcp_app_bench",
    });
    expect(buffer?.[1]?.content[2]).toMatchObject({
      type: "mcpApp",
      id: "tool-1",
      payload: {
        ...createMcpAppPayload(),
        sessionId: replaySessionId,
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

  it("replay preserves timestamps from goose metadata on user and assistant chunks", async () => {
    const replaySessionId = "replay-timestamp-session";
    const userCreated = 1_700_000_000;
    const assistantCreated = 1_700_000_120;
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "user_message_chunk",
        content: {
          type: "text",
          text: "what time was this sent?",
        },
        _meta: {
          goose: {
            messageId: "user-from-meta",
            created: userCreated,
          },
        },
      },
    } as never);

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text: "At the original replay time.",
        },
        _meta: {
          goose: {
            messageId: "assistant-from-meta",
            created: assistantCreated,
          },
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    expect(buffer?.[0]).toMatchObject({
      id: "user-from-meta",
      role: "user",
      created: userCreated * 1000,
    });
    expect(buffer?.[1]).toMatchObject({
      id: "assistant-from-meta",
      role: "assistant",
      created: assistantCreated * 1000,
    });
  });

  it("replay attaches MCP app payloads to tool-only assistant messages", async () => {
    const replaySessionId = "replay-acp-session-2";
    const replayCreated = 1_700_000_240;
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tool-1",
        title: "mcp_app_bench__inspect_host_info",
        _meta: {
          goose: {
            messageId: "assistant-tool-only",
            created: replayCreated,
          },
        },
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
            messageId: "assistant-tool-only",
            created: replayCreated,
          },
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    const assistant = buffer?.[0];
    expect(assistant).toMatchObject({
      id: "assistant-tool-only",
      created: replayCreated * 1000,
    });
    const mcpAppBlock = assistant?.content.find(
      (block) => block.type === "mcpApp",
    );
    expect(mcpAppBlock).toMatchObject({
      type: "mcpApp",
      payload: expect.objectContaining({
        sessionId: replaySessionId,
      }),
    });
  });

  it("replay falls back to tracked assistant when a tool update ID is not buffered", async () => {
    const replaySessionId = "replay-tool-response-id-session";
    const assistantCreated = 1_700_000_120;
    const toolResponseCreated = 1_700_000_240;
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
      update: {
        sessionUpdate: "agent_message_chunk",
        content: {
          type: "text",
          text: "I'll check that.",
        },
        _meta: {
          goose: {
            messageId: "assistant-1",
            created: assistantCreated,
          },
        },
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
              text: "Tool completed.",
            },
          },
        ],
        _meta: {
          goose: {
            messageId: "tool-response-user-message",
            created: toolResponseCreated,
          },
        },
      },
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    const assistant = buffer?.[0];
    expect(assistant).toMatchObject({
      id: "assistant-1",
      created: assistantCreated * 1000,
    });
    expect(assistant?.content.map((block) => block.type)).toEqual([
      "text",
      "toolResponse",
    ]);
    expect(assistant?.content[1]).toMatchObject({
      type: "toolResponse",
      id: "tool-1",
      result: "Tool completed.",
      isError: false,
    });
  });

  it("threads tool chain summary onto the streaming tool request (live)", async () => {
    registerPreparedSession("acp-session", "goose", "/tmp");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-1",
        title: "running ls",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-2",
        title: "running pwd",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
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
    } as never);

    const messages = useChatStore.getState().messagesBySession["acp-session"];
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

  it("threads tool chain summary onto the first tool call even when the agent has moved to the next assistant message (live)", async () => {
    registerPreparedSession("acp-session", "goose", "/tmp");

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-1",
        title: "running ls",
      },
    } as never);

    await handleSessionNotification({
      sessionId: "acp-session",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: "tc-2",
        title: "running pwd",
      },
    } as never);

    // Simulate the agent moving on to the next assistant message: the
    // streamingMessageId now points to a brand-new message that does not
    // contain the original tool requests. This is what happens in practice
    // by the time the chain summary task fires (after all tool responses
    // have been emitted and the next agent turn has begun).
    const beforeMessages =
      useChatStore.getState().messagesBySession["acp-session"] ?? [];
    const newAssistantId = "next-assistant-msg";
    useChatStore.setState((state) => ({
      ...state,
      messagesBySession: {
        ...state.messagesBySession,
        "acp-session": [
          ...beforeMessages,
          {
            id: newAssistantId,
            role: "assistant",
            created: Date.now(),
            content: [{ type: "text", text: "ok" }],
            metadata: {
              userVisible: true,
              agentVisible: true,
              completionStatus: "inProgress",
            },
          },
        ],
      },
    }));
    useChatStore
      .getState()
      .setStreamingMessageId("acp-session", newAssistantId);

    await handleSessionNotification({
      sessionId: "acp-session",
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
    } as never);

    const messages = useChatStore.getState().messagesBySession["acp-session"];
    const toolReqs =
      messages?.flatMap((m) =>
        m.content.filter((c) => c.type === "toolRequest"),
      ) ?? [];
    const first = toolReqs.find(
      (c) => c.type === "toolRequest" && c.id === "tc-1",
    );
    expect(first?.type === "toolRequest" && first.chainSummary).toEqual({
      summary: "inspected working directory",
      count: 2,
    });
    // The new assistant message must not have been mutated to absorb the
    // chain summary (regression guard: it doesn't own the tool request).
    const nextMsg = messages?.find((m) => m.id === newAssistantId);
    expect(nextMsg?.content.some((c) => c.type === "toolRequest")).toBe(false);
  });

  it("attaches tool chain summary on initial tool_call during replay", async () => {
    const replaySessionId = "replay-chain-summary-session";
    useChatStore.setState({
      loadingSessionIds: new Set<string>([replaySessionId]),
    });

    await handleSessionNotification({
      sessionId: replaySessionId,
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
    } as never);

    const buffer = getReplayBuffer(replaySessionId);
    expect(buffer).toBeTruthy();
    const tc = buffer
      ?.flatMap((m) => m.content)
      .find((c) => c.type === "toolRequest" && c.id === "tc-1");
    expect(tc?.type === "toolRequest" && tc.chainSummary).toEqual({
      summary: "applied dark mode polish",
      count: 4,
    });
  });
});
