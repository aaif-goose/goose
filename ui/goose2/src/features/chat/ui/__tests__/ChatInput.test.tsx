import { beforeEach, describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { ChatInput } from "../ChatInput";
import type { Persona } from "@/shared/types/agents";

vi.mock("@/features/providers/hooks/useAgentProviderStatus", () => ({
  useAgentProviderStatus: () => ({
    readyAgentIds: new Set(["goose", "claude-acp", "codex-acp"]),
    loading: false,
    refresh: vi.fn(),
  }),
}));

const mockListFilesForMentions = vi.fn<
  (roots: string[], maxResults?: number) => Promise<string[]>
>(async () => []);
vi.mock("@/shared/api/system", () => ({
  listFilesForMentions: (roots: string[], maxResults?: number) =>
    mockListFilesForMentions(roots, maxResults),
}));

const TEST_PERSONAS: Persona[] = [
  {
    id: "builtin-solo",
    displayName: "Solo",
    systemPrompt: "You are Solo.",
    isBuiltin: true,
    createdAt: "",
    updatedAt: "",
  },
  {
    id: "reviewer",
    displayName: "Reviewer",
    systemPrompt: "You are Reviewer, a code review specialist.",
    isBuiltin: true,
    createdAt: "",
    updatedAt: "",
  },
];

function StatefulChatInput({
  onSend = vi.fn(),
}: {
  onSend?: (text: string, personaId?: string) => void;
}) {
  const [selectedPersonaId, setSelectedPersonaId] = useState<string | null>(
    "builtin-solo",
  );

  return (
    <ChatInput
      onSend={onSend}
      personas={TEST_PERSONAS}
      selectedPersonaId={selectedPersonaId}
      onPersonaChange={setSelectedPersonaId}
    />
  );
}

describe("ChatInput", () => {
  beforeEach(() => {
    mockListFilesForMentions.mockClear();
    mockListFilesForMentions.mockResolvedValue([]);
  });

  it("renders with default placeholder", () => {
    render(<ChatInput onSend={vi.fn()} />);
    expect(
      screen.getByPlaceholderText("Message Goose, @ to mention personas"),
    ).toBeInTheDocument();
  });

  it("calls onSend when Enter is pressed", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("hello", undefined, undefined);
  });

  it("does not call onSend on Shift+Enter (newline)", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    await user.keyboard("{Shift>}{Enter}{/Shift}");

    expect(onSend).not.toHaveBeenCalled();
  });

  it("does not call onSend on Alt+Enter (newline)", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    const wasNotPrevented = fireEvent.keyDown(input, {
      altKey: true,
      key: "Enter",
    });

    expect(wasNotPrevented).toBe(true);
    expect(onSend).not.toHaveBeenCalled();
    expect(input).toHaveValue("hello");
  });

  it("shows current model name in model picker", () => {
    render(
      <ChatInput
        onSend={vi.fn()}
        currentModelId="gpt-4o"
        currentModel="GPT-4o"
        availableModels={[{ id: "gpt-4o", name: "GPT-4o" }]}
        providers={[{ id: "goose", label: "Goose" }]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("GPT-4o");
  });

  it("shows default model name in model picker", () => {
    render(
      <ChatInput
        onSend={vi.fn()}
        availableModels={[{ id: "claude-sonnet-4", name: "Claude Sonnet 4" }]}
        providers={[{ id: "goose", label: "Goose" }]}
      />,
    );
    expect(
      screen.getByRole("button", { name: /choose agent and model/i }),
    ).toHaveTextContent("Claude Sonnet 4");
  });

  it("shows default provider label", () => {
    render(
      <ChatInput
        onSend={vi.fn()}
        providers={[{ id: "goose", label: "Goose" }]}
        selectedProvider="goose"
      />,
    );
    const providerButton = screen.getByRole("button", {
      name: /choose agent and model/i,
    });
    expect(providerButton).toHaveTextContent("Goose");
  });

  it("opens the agent and model picker", async () => {
    const user = userEvent.setup();

    render(
      <ChatInput
        onSend={vi.fn()}
        providers={[
          { id: "goose", label: "Goose" },
          { id: "claude-acp", label: "Claude Code" },
        ]}
        selectedProvider="goose"
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /choose agent and model/i }),
    );

    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByText("Model")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
  });

  it("opens the project selector menu", async () => {
    const user = userEvent.setup();

    render(
      <ChatInput
        onSend={vi.fn()}
        selectedProjectId="project-1"
        availableProjects={[
          {
            id: "project-1",
            name: "goose2",
            workingDirs: ["/Users/wesb/dev/goose2"],
          },
        ]}
      />,
    );

    await user.click(screen.getByRole("button", { name: /select project/i }));

    expect(screen.getByText("Choose a project")).toBeInTheDocument();
    expect(screen.getByText("No project")).toBeInTheDocument();
  });

  it("shows no project in the toolbar when no project is selected", () => {
    render(<ChatInput onSend={vi.fn()} />);
    expect(screen.getByText("No project")).toBeInTheDocument();
  });

  it("shows the selected project name in the toolbar", () => {
    render(
      <ChatInput
        onSend={vi.fn()}
        selectedProjectId="project-1"
        availableProjects={[
          {
            id: "project-1",
            name: "goose2",
            workingDirs: ["/Users/wesb/dev/goose2"],
          },
        ]}
      />,
    );
    expect(screen.getByText("goose2")).toBeInTheDocument();
  });

  it("opens a context usage popover when token tracking is available", async () => {
    const user = userEvent.setup();
    render(
      <ChatInput onSend={vi.fn()} contextTokens={1536} contextLimit={8192} />,
    );

    await user.click(screen.getByRole("button", { name: /context usage/i }));

    expect(screen.getByText("Context window")).toBeInTheDocument();
    expect(screen.getByText("1.5K / 8.2K tokens used")).toBeInTheDocument();
    expect(screen.getByText("19%")).toBeInTheDocument();
  });

  it("runs compaction from the context usage popover", async () => {
    const user = userEvent.setup();
    const onCompactContext = vi.fn();

    render(
      <ChatInput
        onSend={vi.fn()}
        contextTokens={1536}
        contextLimit={8192}
        canCompactContext
        onCompactContext={onCompactContext}
      />,
    );

    await user.click(screen.getByRole("button", { name: /context usage/i }));
    await user.click(screen.getByRole("button", { name: "Compact" }));

    expect(onCompactContext).toHaveBeenCalledOnce();
  });

  it("hides the context usage control when the context limit is unavailable", () => {
    render(
      <ChatInput onSend={vi.fn()} contextTokens={1536} contextLimit={0} />,
    );

    expect(
      screen.queryByRole("button", { name: /context usage/i }),
    ).not.toBeInTheDocument();
  });

  it("shows stop button when streaming", () => {
    render(<ChatInput onSend={vi.fn()} onStop={vi.fn()} isStreaming />);
    expect(
      screen.getByRole("button", { name: /stop generation/i }),
    ).toBeInTheDocument();
  });

  it("calls onStop when stop button clicked", async () => {
    const onStop = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={vi.fn()} onStop={onStop} isStreaming />);

    await user.click(screen.getByRole("button", { name: /stop generation/i }));
    expect(onStop).toHaveBeenCalledOnce();
  });

  it("is disabled when disabled prop is true", () => {
    render(<ChatInput onSend={vi.fn()} disabled />);
    const input = screen.getByRole("textbox");
    expect(input).toBeDisabled();
  });

  it("clears input after sending", async () => {
    const user = userEvent.setup();
    render(<ChatInput onSend={vi.fn()} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "hello");
    await user.keyboard("{Enter}");

    expect(input).toHaveValue("");
  });

  it("selecting an @mention creates a sticky assistant chip without leaving inline text", async () => {
    const user = userEvent.setup();
    render(<StatefulChatInput />);

    const input = screen.getByRole("textbox");
    await user.type(input, "@Rev");
    await user.click(screen.getByRole("option", { name: /reviewer/i }));

    expect(input).toHaveValue("");
    expect(screen.getByText("@Reviewer")).toBeInTheDocument();
  });

  it("shows project files in @mention results and inserts the selected path", async () => {
    const user = userEvent.setup();
    mockListFilesForMentions.mockResolvedValue([
      "/Users/wesb/dev/goose2/README.md",
      "/Users/wesb/dev/goose2/src/features/chat/ui/ChatInput.tsx",
    ]);

    render(
      <ChatInput
        onSend={vi.fn()}
        selectedProjectId="project-1"
        availableProjects={[
          {
            id: "project-1",
            name: "goose2",
            workingDirs: ["/Users/wesb/dev/goose2"],
          },
        ]}
      />,
    );

    expect(mockListFilesForMentions).toHaveBeenCalledWith(
      ["/Users/wesb/dev/goose2"],
      undefined,
    );

    const input = screen.getByRole("textbox");
    await user.type(input, "@read");

    expect(await screen.findByText("Files")).toBeInTheDocument();

    const fileOption = await screen.findByRole("option", {
      name: /readme\.md/i,
    });
    await user.click(fileOption);

    expect(input).toHaveValue("/Users/wesb/dev/goose2/README.md ");
  });

  // ---------------------------------------------------------------------------
  // Message queue & streaming behavior
  // ---------------------------------------------------------------------------

  it("textarea is enabled during streaming", () => {
    render(<ChatInput onSend={vi.fn()} isStreaming />);
    expect(screen.getByRole("textbox")).not.toBeDisabled();
  });

  it("shows send button instead of stop when streaming with text entered", async () => {
    const user = userEvent.setup();
    render(<ChatInput onSend={vi.fn()} onStop={vi.fn()} isStreaming />);

    await user.type(screen.getByRole("textbox"), "follow up");

    expect(
      screen.getByRole("button", { name: /send message/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /stop generation/i }),
    ).not.toBeInTheDocument();
  });

  it("calls onSend during streaming when text is entered", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<ChatInput onSend={onSend} isStreaming />);

    await user.type(screen.getByRole("textbox"), "follow up");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("follow up", undefined, undefined);
  });

  it("shows disabled send button (not stop) when queue is full", () => {
    render(
      <ChatInput
        onSend={vi.fn()}
        onStop={vi.fn()}
        isStreaming
        queuedMessage={{ text: "queued msg" }}
      />,
    );

    const sendButton = screen.getByRole("button", { name: /send message/i });
    expect(sendButton).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: /stop generation/i }),
    ).not.toBeInTheDocument();
  });

  it("does not send when queue is full", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(
      <ChatInput
        onSend={onSend}
        isStreaming
        queuedMessage={{ text: "queued msg" }}
      />,
    );

    await user.type(screen.getByRole("textbox"), "another message");
    await user.keyboard("{Enter}");

    expect(onSend).not.toHaveBeenCalled();
  });

  it("keeps the selected assistant chip after sending subsequent messages", async () => {
    const onSend = vi.fn();
    const user = userEvent.setup();
    render(<StatefulChatInput onSend={onSend} />);

    const input = screen.getByRole("textbox");
    await user.type(input, "@Rev");
    await user.click(screen.getByRole("option", { name: /reviewer/i }));
    await user.click(input);
    await user.keyboard("{End}");
    await user.type(input, "hello");
    await user.keyboard("{Enter}");

    expect(onSend).toHaveBeenCalledWith("hello", "reviewer", undefined);
    expect(screen.getByText("@Reviewer")).toBeInTheDocument();
  });
});
