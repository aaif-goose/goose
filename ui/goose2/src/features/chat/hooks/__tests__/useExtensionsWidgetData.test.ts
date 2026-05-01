import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ExtensionEntry } from "@/features/extensions/types";
import type { Message } from "@/shared/types/messages";
import { useChatStore } from "../../stores/chatStore";

const { mockListExtensions, mockListSessionExtensions } = vi.hoisted(() => ({
  mockListExtensions: vi.fn(),
  mockListSessionExtensions: vi.fn(),
}));

vi.mock("@/features/extensions/api/extensions", () => ({
  listExtensions: mockListExtensions,
  listSessionExtensions: mockListSessionExtensions,
}));

import { useExtensionsWidgetData } from "../useExtensionsWidgetData";

function toolRequestMessage(extensionName: string, id = "message-1"): Message {
  return {
    id,
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

function textMessage(id: string): Message {
  return {
    id,
    role: "user",
    created: 20,
    content: [{ type: "text", text: "hello" }],
  };
}

const configuredExtension: ExtensionEntry = {
  type: "stdio",
  name: "Analyze",
  description: "Analyze things",
  cmd: "analyze",
  args: [],
  config_key: "analyze",
  enabled: true,
};

describe("useExtensionsWidgetData", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListExtensions.mockResolvedValue([configuredExtension]);
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

  it("does not fetch extension status before any tool has been used", () => {
    renderHook(() => useExtensionsWidgetData("session-1"));

    expect(mockListSessionExtensions).not.toHaveBeenCalled();
    expect(mockListExtensions).not.toHaveBeenCalled();
  });

  it("does not refetch when non-tool messages change", async () => {
    useChatStore.setState({
      messagesBySession: {
        "session-1": [toolRequestMessage("analyze")],
      },
    });

    const { result } = renderHook(() => useExtensionsWidgetData("session-1"));

    await waitFor(() => {
      expect(result.current.usedExtensions).toHaveLength(1);
    });
    expect(mockListSessionExtensions).toHaveBeenCalledTimes(1);
    expect(mockListExtensions).toHaveBeenCalledTimes(1);

    act(() => {
      useChatStore.setState({
        messagesBySession: {
          "session-1": [toolRequestMessage("analyze"), textMessage("text-1")],
        },
      });
    });

    await waitFor(() => {
      expect(result.current.usedExtensions).toHaveLength(1);
    });
    expect(mockListSessionExtensions).toHaveBeenCalledTimes(1);
    expect(mockListExtensions).toHaveBeenCalledTimes(1);
  });
});
