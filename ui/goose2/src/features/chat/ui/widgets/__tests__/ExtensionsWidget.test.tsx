import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../../../stores/chatStore";
import { ExtensionsWidget } from "../ExtensionsWidget";

const { mockListExtensions, mockListSessionExtensions } = vi.hoisted(() => ({
  mockListExtensions: vi.fn(),
  mockListSessionExtensions: vi.fn(),
}));

vi.mock("@/features/extensions/api/extensions", () => ({
  listExtensions: mockListExtensions,
  listSessionExtensions: mockListSessionExtensions,
}));

function toolRequestMessage(extensionName: string): Message {
  return {
    id: "message-1",
    role: "assistant",
    created: 10,
    content: [
      {
        type: "toolRequest",
        id: "tool-1",
        name: "analyze",
        toolName: "analyze",
        extensionName,
        arguments: {},
        status: "completed",
      },
    ],
  };
}

function persistedToolRequestMessage(): Message {
  return {
    id: "message-2",
    role: "assistant",
    created: 20,
    content: [
      {
        type: "toolRequest",
        id: "tool-2",
        toolCall: {
          status: "success",
          value: {
            name: "context7__resolve-library-id",
            arguments: {
              libraryName: "React",
              query: "React docs useEffect cleanup",
            },
          },
        },
        _meta: {
          goose_extension: "context7",
        },
      },
    ],
  } as unknown as Message;
}

describe("ExtensionsWidget", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListExtensions.mockResolvedValue([]);
    mockListSessionExtensions.mockResolvedValue([]);
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      skillDraftsBySession: {},
      activeSessionId: null,
      isConnected: false,
      loadingSessionIds: new Set<string>(),
      scrollTargetMessageBySession: {},
    });
  });

  it("keeps showing historical usage when the extension is unavailable", async () => {
    useChatStore.setState({
      messagesBySession: {
        "session-1": [toolRequestMessage("analyze")],
      },
    });

    render(<ExtensionsWidget sessionId="session-1" />);

    expect(await screen.findByText("Analyze")).toBeInTheDocument();
    expect(screen.getByText("Not currently available")).toBeInTheDocument();
  });

  it("shows persisted Goose tool request usage", async () => {
    mockListSessionExtensions.mockResolvedValue([
      {
        type: "streamable_http",
        name: "Context7",
        description: "Up-to-date docs",
        uri: "https://mcp.context7.com/mcp",
        config_key: "context7",
        status: "connected",
        tools: ["context7__resolve-library-id", "context7__query-docs"],
      },
    ]);
    useChatStore.setState({
      messagesBySession: {
        "session-1": [persistedToolRequestMessage()],
      },
    });

    render(<ExtensionsWidget sessionId="session-1" />);

    expect(await screen.findByText("Context7")).toBeInTheDocument();
    expect(screen.getByText("Connected · 2 tools")).toBeInTheDocument();
  });
});
