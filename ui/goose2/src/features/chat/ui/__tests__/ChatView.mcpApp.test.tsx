import type { ReactNode } from "react";
import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatView } from "../ChatView";

const mocks = vi.hoisted(() => ({
  messageTimelineSpy: vi.fn(),
  chatInputSpy: vi.fn(),
  handleSend: vi.fn(() => true),
  useChatSessionController: vi.fn(),
}));

vi.mock("motion/react", () => ({
  AnimatePresence: ({ children }: { children: ReactNode }) => children,
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock("../MessageTimeline", () => ({
  MessageTimeline: (props: unknown) => {
    mocks.messageTimelineSpy(props);
    return <div data-testid="message-timeline" />;
  },
}));

vi.mock("../ChatInput", () => ({
  ChatInput: (props: unknown) => {
    mocks.chatInputSpy(props);
    return <div data-testid="chat-input" />;
  },
}));

vi.mock("../LoadingGoose", () => ({
  LoadingGoose: () => null,
}));

vi.mock("../ChatLoadingSkeleton", () => ({
  ChatLoadingSkeleton: () => <div data-testid="chat-loading-skeleton" />,
}));

vi.mock("../ChatContextPanel", () => ({
  ChatContextPanel: () => null,
}));

vi.mock("../../hooks/ArtifactPolicyContext", () => ({
  ArtifactPolicyProvider: ({ children }: { children: ReactNode }) => children,
}));

vi.mock("../../hooks/useChatSessionController", () => ({
  useChatSessionController: mocks.useChatSessionController,
}));

vi.mock("../../stores/chatSessionStore", () => ({
  useChatSessionStore: (selector: (state: unknown) => unknown) =>
    selector({
      contextPanelOpenBySession: {},
      setContextPanelOpen: vi.fn(),
    }),
}));

vi.mock("@/features/projects/lib/chatProjectContext", () => ({
  defaultGlobalArtifactRoot: vi.fn().mockResolvedValue(null),
}));

vi.mock("@/shared/lib/perfLog", () => ({
  perfLog: vi.fn(),
}));

describe("ChatView MCP app messaging", () => {
  beforeEach(() => {
    mocks.messageTimelineSpy.mockClear();
    mocks.chatInputSpy.mockClear();
    mocks.handleSend.mockClear();
    mocks.useChatSessionController.mockReturnValue({
      messages: [],
      streamingMessageId: null,
      scrollTarget: null,
      handleScrollTargetHandled: vi.fn(),
      handleSend: mocks.handleSend,
      isLoadingHistory: false,
      chatState: "idle",
      stopStreaming: vi.fn(),
      projectMetadataPending: false,
      isCompactingContext: false,
      queue: { queuedMessage: null, dismiss: vi.fn() },
      draftValue: "",
      handleDraftChange: vi.fn(),
      personas: [],
      selectedPersonaId: null,
      handlePersonaChange: vi.fn(),
      handleCreatePersona: vi.fn(),
      pickerAgents: [],
      providersLoading: false,
      selectedProvider: "goose",
      handleProviderChange: vi.fn(),
      currentModelId: null,
      currentModelName: null,
      availableModels: [],
      modelsLoading: false,
      modelStatusMessage: null,
      handleModelChange: vi.fn(),
      selectedProjectId: null,
      availableProjects: [],
      handleProjectChange: vi.fn(),
      tokenState: { accumulatedTotal: 0, contextLimit: 0 },
      isContextUsageReady: false,
      compactConversation: vi.fn(),
      canCompactContext: false,
      supportsCompactionControls: false,
      sessionArtifactCwd: null,
      project: null,
    });
  });

  it("passes handleSend through to MessageTimeline for MCP app messages", () => {
    render(<ChatView sessionId="session-1" />);

    expect(mocks.messageTimelineSpy).toHaveBeenCalled();
    const timelineProps = mocks.messageTimelineSpy.mock.calls.at(-1)?.[0] as {
      onSendMcpAppMessage?: unknown;
    };

    expect(timelineProps.onSendMcpAppMessage).toBe(mocks.handleSend);
    const chatInputProps = mocks.chatInputSpy.mock.calls.at(-1)?.[0] as {
      className?: string;
    };
    expect(chatInputProps.className).toBeUndefined();
  });

  it("overlaps the composer when the latest visible content is an MCP app", () => {
    mocks.useChatSessionController.mockReturnValue({
      ...mocks.useChatSessionController(),
      messages: [
        {
          id: "assistant-1",
          role: "assistant",
          created: Date.now(),
          content: [
            {
              type: "mcpApp",
              id: "mcp-app-1",
              payload: {},
            },
          ],
        },
      ],
    });

    render(<ChatView sessionId="session-1" />);

    expect(mocks.chatInputSpy).toHaveBeenCalled();
    const chatInputProps = mocks.chatInputSpy.mock.calls.at(-1)?.[0] as {
      className?: string;
    };
    expect(chatInputProps.className).toBe("-mt-4");
  });

  it("does not overlap the composer when reasoning is the latest visible content", () => {
    mocks.useChatSessionController.mockReturnValue({
      ...mocks.useChatSessionController(),
      messages: [
        {
          id: "assistant-1",
          role: "assistant",
          created: Date.now(),
          content: [
            {
              type: "reasoning",
              text: "Working through it",
            },
          ],
        },
      ],
    });

    render(<ChatView sessionId="session-1" />);

    expect(mocks.chatInputSpy).toHaveBeenCalled();
    const chatInputProps = mocks.chatInputSpy.mock.calls.at(-1)?.[0] as {
      className?: string;
    };
    expect(chatInputProps.className).toBeUndefined();
  });

  it("does not overlap the composer over the loading indicator", () => {
    mocks.useChatSessionController.mockReturnValue({
      ...mocks.useChatSessionController(),
      chatState: "thinking",
    });

    render(<ChatView sessionId="session-1" />);

    expect(mocks.chatInputSpy).toHaveBeenCalled();
    const chatInputProps = mocks.chatInputSpy.mock.calls.at(-1)?.[0] as {
      className?: string;
    };
    expect(chatInputProps.className).toBeUndefined();
  });
});
