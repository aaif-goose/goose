import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import { useChatStore } from "../../stores/chatStore";
import { useChatSessionStore } from "../../stores/chatSessionStore";

const mockAcpSendMessage = vi.fn();
const mockAcpCancelSession = vi.fn();
const mockAcpPrepareSession = vi.fn();
const mockAcpSetModel = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpSendMessage: (...args: unknown[]) => mockAcpSendMessage(...args),
  acpCancelSession: (...args: unknown[]) => mockAcpCancelSession(...args),
  acpPrepareSession: (...args: unknown[]) => mockAcpPrepareSession(...args),
  acpSetModel: (...args: unknown[]) => mockAcpSetModel(...args),
}));

import { useChat } from "../useChat";

describe("useChat attachments", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
      activeWorkingContextBySession: {},
      modelsBySession: {},
      modelCacheByProvider: {},
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
    });
    mockAcpCancelSession.mockResolvedValue(true);
    mockAcpPrepareSession.mockResolvedValue(undefined);
    mockAcpSetModel.mockResolvedValue(undefined);
  });

  it("stores non-image attachments in metadata and prepends path references to the prompt", async () => {
    const { result } = renderHook(() => useChat("session-1"));
    const attachments = [
      {
        id: "file-1",
        kind: "file" as const,
        name: "report.pdf",
        path: "/tmp/report.pdf",
        mimeType: "application/pdf",
      },
      {
        id: "dir-1",
        kind: "directory" as const,
        name: "screenshots",
        path: "/tmp/screenshots",
      },
    ];

    await act(async () => {
      await result.current.sendMessage(
        "Please review these",
        undefined,
        attachments,
      );
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];

    expect(message.metadata?.attachments).toEqual([
      {
        type: "file",
        name: "report.pdf",
        path: "/tmp/report.pdf",
        mimeType: "application/pdf",
      },
      {
        type: "directory",
        name: "screenshots",
        path: "/tmp/screenshots",
      },
    ]);
    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "goose",
      "Attached items:\n- [file] /tmp/report.pdf\n- [directory] /tmp/screenshots\nPlease review these",
      {
        systemPrompt: undefined,
        workingDir: undefined,
        personaId: undefined,
        personaName: undefined,
        images: undefined,
      },
    );
  });

  it("keeps image attachments in ACP images while preserving path metadata", async () => {
    const { result } = renderHook(() => useChat("session-1"));
    const attachments = [
      {
        id: "image-1",
        kind: "image" as const,
        name: "diagram.png",
        path: "/tmp/diagram.png",
        mimeType: "image/png",
        base64: "abc123",
        previewUrl: "tauri://localhost/tmp/diagram.png",
      },
    ];

    await act(async () => {
      await result.current.sendMessage("", undefined, attachments);
    });

    const message = useChatStore.getState().messagesBySession["session-1"][0];

    expect(message.metadata?.attachments).toEqual([
      {
        type: "file",
        name: "diagram.png",
        path: "/tmp/diagram.png",
        mimeType: "image/png",
      },
    ]);
    expect(message.content).toEqual([
      { type: "text", text: "" },
      {
        type: "image",
        source: {
          type: "base64",
          mediaType: "image/png",
          data: "abc123",
        },
      },
    ]);
    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "goose",
      "Attached items:\n- [image] diagram.png (image attached)\n ",
      {
        systemPrompt: undefined,
        workingDir: undefined,
        personaId: undefined,
        personaName: undefined,
        images: [["abc123", "image/png"]],
      },
    );
  });

  it("includes image attachments in the prompt summary for mixed sends", async () => {
    const { result } = renderHook(() => useChat("session-1"));
    const attachments = [
      {
        id: "file-1",
        kind: "file" as const,
        name: "mobile-confirmation.html",
        path: "/tmp/mobile-confirmation.html",
        mimeType: "text/html",
      },
      {
        id: "dir-1",
        kind: "directory" as const,
        name: "neighborhood block",
        path: "/tmp/neighborhood block",
      },
      {
        id: "image-1",
        kind: "image" as const,
        name: "Screenshot 2026-04-09 at 1.25.32 PM.png",
        path: "/tmp/Screenshot.png",
        mimeType: "image/png",
        base64: "abc123",
        previewUrl: "tauri://localhost/tmp/Screenshot.png",
      },
    ];

    await act(async () => {
      await result.current.sendMessage(
        "can you see the attachments i attached?",
        undefined,
        attachments,
      );
    });

    expect(mockAcpSendMessage).toHaveBeenCalledWith(
      "session-1",
      "goose",
      "Attached items:\n- [file] /tmp/mobile-confirmation.html\n- [directory] /tmp/neighborhood block\n- [image] Screenshot 2026-04-09 at 1.25.32 PM.png (image attached)\ncan you see the attachments i attached?",
      {
        systemPrompt: undefined,
        workingDir: undefined,
        personaId: undefined,
        personaName: undefined,
        images: [["abc123", "image/png"]],
      },
    );
  });
});
