import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";
import { useChatStore } from "../../stores/chatStore";

const mockAcpSendMessage = vi.fn();
const mockAcpCancelSession = vi.fn();
const mockAcpLoadSession = vi.fn();
const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: (...args: unknown[]) => mockAcpCancelSession(...args),
  acpLoadSession: (...args: unknown[]) => mockAcpLoadSession(...args),
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

import { useChat } from "../useChat";

describe("useChat skill chips", () => {
  beforeEach(() => {
    mockAcpSendMessage.mockReset();
    mockAcpCancelSession.mockReset();
    mockAcpLoadSession.mockReset();
    mockAcpPrepareSession.mockReset();
    mockAcpSetModel.mockReset();
    mockAcpSendMessage.mockResolvedValue(undefined);
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      activeSessionId: null,
      isConnected: true,
    });
    useChatSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      isLoading: false,
      contextPanelOpenBySession: {},
      activeWorkspaceBySession: {},
    });
    useAgentStore.setState({
      personas: [],
      personasLoading: false,
      agents: [],
      agentsLoading: false,
      activeAgentId: null,
      isLoading: false,
      personaEditorOpen: false,
      editingPersona: null,
      personaEditorMode: "create",
    });
  });

  it("stores user-visible chips separately from the agent prompt", async () => {
    const { result } = renderHook(() => useChat("session-1"));

    await act(async () => {
      await result.current.sendMessage(
        "redo the settings modal",
        undefined,
        undefined,
        {
          displayText: "redo the settings modal",
          assistantPrompt: "Use these skills for this request: capture-task.",
          chips: [{ label: "capture-task", type: "skill" }],
        },
      );
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];
    expect(message.content).toEqual([
      { type: "text", text: "redo the settings modal" },
    ]);
    expect(message.metadata?.chips).toEqual([
      { label: "capture-task", type: "skill" },
    ]);
    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "redo the settings modal",
      {
        assistantPrompt: "Use these skills for this request: capture-task.",
        systemPrompt: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );
  });
});
