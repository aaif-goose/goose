import { describe, it, expect } from "vitest";
import {
  isTextContent,
  isToolRequest,
  isThinking,
  isReasoning,
  isActionRequired,
  isSystemNotification,
  getTextContent,
  createSystemNotificationMessage,
  createUserMessage,
} from "../messages";
import type {
  TextContent,
  ToolRequestContent,
  ThinkingContent,
  ReasoningContent,
  ActionRequiredContent,
  SystemNotificationContent,
  ImageContent,
  Message,
} from "../messages";

// ── fixtures ──────────────────────────────────────────────────────────

const textBlock: TextContent = { type: "text", text: "hello" };
const imageBlock: ImageContent = {
  type: "image",
  source: { type: "url", url: "https://img.png" },
};
const toolRequestBlock: ToolRequestContent = {
  type: "toolRequest",
  id: "tr-1",
  name: "readFile",
  arguments: { path: "/tmp" },
  status: "pending",
};
const thinkingBlock: ThinkingContent = { type: "thinking", text: "hmm" };
const reasoningBlock: ReasoningContent = {
  type: "reasoning",
  text: "because...",
};
const actionRequiredBlock: ActionRequiredContent = {
  type: "actionRequired",
  id: "ar-1",
  actionType: "toolConfirmation",
  message: "confirm?",
};
const systemNotificationBlock: SystemNotificationContent = {
  type: "systemNotification",
  notificationType: "info",
  text: "compacted",
};

// ── type guards ───────────────────────────────────────────────────────

describe("isTextContent", () => {
  it("returns true for text content", () => {
    expect(isTextContent(textBlock)).toBe(true);
  });

  it("returns false for non-text content", () => {
    expect(isTextContent(imageBlock)).toBe(false);
    expect(isTextContent(toolRequestBlock)).toBe(false);
    expect(isTextContent(thinkingBlock)).toBe(false);
  });
});

describe("isToolRequest", () => {
  it("returns true for tool request content", () => {
    expect(isToolRequest(toolRequestBlock)).toBe(true);
  });

  it("returns false for other content", () => {
    expect(isToolRequest(textBlock)).toBe(false);
  });
});

describe("isThinking", () => {
  it("returns true for thinking content", () => {
    expect(isThinking(thinkingBlock)).toBe(true);
  });

  it("returns false for other content", () => {
    expect(isThinking(reasoningBlock)).toBe(false);
    expect(isThinking(textBlock)).toBe(false);
  });
});

describe("isReasoning", () => {
  it("returns true for reasoning content", () => {
    expect(isReasoning(reasoningBlock)).toBe(true);
  });

  it("returns false for other content", () => {
    expect(isReasoning(thinkingBlock)).toBe(false);
  });
});

describe("isActionRequired", () => {
  it("returns true for action required content", () => {
    expect(isActionRequired(actionRequiredBlock)).toBe(true);
  });

  it("returns false for other content", () => {
    expect(isActionRequired(textBlock)).toBe(false);
  });
});

describe("isSystemNotification", () => {
  it("returns true for system notification content", () => {
    expect(isSystemNotification(systemNotificationBlock)).toBe(true);
  });

  it("returns false for other content", () => {
    expect(isSystemNotification(textBlock)).toBe(false);
  });
});

// ── helpers ───────────────────────────────────────────────────────────

describe("getTextContent", () => {
  it("extracts text from messages with multiple content blocks", () => {
    const msg: Message = {
      id: "1",
      role: "assistant",
      created: Date.now(),
      content: [
        { type: "text", text: "line one" },
        thinkingBlock,
        { type: "text", text: "line two" },
      ],
    };
    expect(getTextContent(msg)).toBe("line one\nline two");
  });

  it("returns empty string for messages with no text", () => {
    const msg: Message = {
      id: "2",
      role: "assistant",
      created: Date.now(),
      content: [thinkingBlock, toolRequestBlock],
    };
    expect(getTextContent(msg)).toBe("");
  });
});

describe("createUserMessage", () => {
  it("creates proper message structure", () => {
    const msg = createUserMessage("hi there");
    expect(msg.role).toBe("user");
    expect(msg.id).toBeTruthy();
    expect(msg.created).toBeGreaterThan(0);
    expect(msg.content).toHaveLength(1);
    expect(msg.content[0]).toEqual({ type: "text", text: "hi there" });
    expect(msg.metadata).toEqual({
      userVisible: true,
      agentVisible: true,
    });
  });

  it("includes attachments when provided", () => {
    const attachments = [
      { type: "file" as const, name: "readme.md", path: "/readme.md" },
    ];
    const msg = createUserMessage("check this", attachments);
    expect(msg.metadata).toBeDefined();
    expect(msg.metadata?.userVisible).toBe(true);
    expect(msg.metadata?.agentVisible).toBe(true);
    expect(msg.metadata?.attachments).toEqual(attachments);
  });
});

describe("createSystemNotificationMessage", () => {
  it("creates a visible system notification message", () => {
    const message = createSystemNotificationMessage("boom", "error");

    expect(message.role).toBe("system");
    expect(message.content).toEqual([
      {
        type: "systemNotification",
        notificationType: "error",
        text: "boom",
      },
    ]);
    expect(message.metadata).toEqual({
      userVisible: true,
      agentVisible: false,
    });
  });
});
