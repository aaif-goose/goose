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
});
