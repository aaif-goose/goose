import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MessageBubble } from "../MessageBubble";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import type { Message } from "@/shared/types/messages";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
}));

function assistantMessage(
  content: Message["content"],
  overrides: Partial<Message> = {},
): Message {
  return {
    id: "a1",
    role: "assistant",
    created: Date.now(),
    content,
    ...overrides,
  };
}

describe("MessageBubble MCP app rendering", () => {
  beforeEach(() => {
    useAgentStore.setState({ personas: [] });
  });

  it("renders MCP App blocks", () => {
    const msg = assistantMessage([
      {
        type: "toolRequest",
        id: "tool-1",
        name: "weather: open app",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-1",
        name: "weather: open app",
        result: "done",
        isError: false,
      },
      {
        type: "mcpApp",
        id: "tool-1",
        payload: {
          sessionId: "local-session",
          gooseSessionId: "goose-session",
          toolCallId: "tool-1",
          toolCallTitle: "weather: open app",
          source: "toolCallUpdateMeta",
          tool: {
            name: "weather__open_app",
            extensionName: "weather",
            resourceUri: "ui://weather/app",
          },
          resource: {
            result: {
              contents: [
                {
                  uri: "ui://weather/app",
                  mimeType: "text/html",
                  text: "<div>Hello</div>",
                },
              ],
            },
          },
        },
      },
    ]);

    render(<MessageBubble message={msg} />);

    const mcpAppView = screen.getByTestId("mcp-app-view");
    expect(mcpAppView).toBeInTheDocument();
    expect(mcpAppView).toHaveTextContent("ui://weather/app");
    expect(mcpAppView).toHaveTextContent("<div>Hello</div>");
  });
});
