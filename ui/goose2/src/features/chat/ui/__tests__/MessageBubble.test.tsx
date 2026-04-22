import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MessageBubble } from "../MessageBubble";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import type { Message } from "@/shared/types/messages";
import { openPath } from "@tauri-apps/plugin-opener";
const mockWriteText = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn(),
}));

function userMessage(text: string, overrides: Partial<Message> = {}): Message {
  return {
    id: "u1",
    role: "user",
    created: Date.now(),
    content: [{ type: "text", text }],
    ...overrides,
  };
}

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

describe("MessageBubble", () => {
  beforeEach(() => {
    useAgentStore.setState({ personas: [] });
    vi.mocked(openPath).mockClear();
    mockWriteText.mockClear();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: mockWriteText,
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders user message with correct alignment", () => {
    const { container } = render(
      <MessageBubble message={userMessage("hey")} />,
    );
    const el = container.querySelector('[data-role="user-message"]');
    expect(el).toBeInTheDocument();
    // User messages use flex-row-reverse
    expect(el?.className).toContain("flex-row-reverse");
  });

  it("renders assistant message with avatar", () => {
    const { container } = render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "hi" }])}
      />,
    );
    const el = container.querySelector('[data-role="assistant-message"]');
    expect(el).toBeInTheDocument();
    expect(el?.className).toContain("flex-row");
    expect(el?.className).not.toContain("flex-row-reverse");
  });

  it("renders text content", () => {
    render(<MessageBubble message={userMessage("hello world")} />);
    expect(screen.getByText("hello world")).toBeInTheDocument();
  });

  it("renders compaction notifications as centered success messages", () => {
    const { container } = render(
      <MessageBubble
        message={{
          id: "s1",
          role: "system",
          created: Date.now(),
          content: [
            {
              type: "systemNotification",
              notificationType: "compaction",
              text: "Conversation compacted.",
            },
          ],
          metadata: {
            userVisible: true,
            agentVisible: false,
          },
        }}
      />,
    );

    expect(screen.getByText("Conversation compacted.")).toBeInTheDocument();
    expect(container.querySelector(".text-success")).toBeInTheDocument();
  });

  it("renders user text inside a muted bubble shell", () => {
    const { container } = render(
      <MessageBubble message={userMessage("hello world")} />,
    );

    expect(
      container.querySelector(
        '[data-role="user-message"] .rounded-2xl.bg-muted',
      ),
    ).toBeInTheDocument();
  });

  it("renders multiple content blocks", () => {
    const msg = assistantMessage([
      { type: "text", text: "first block" },
      { type: "text", text: "second block" },
    ]);
    render(<MessageBubble message={msg} />);
    expect(screen.getByText("first block")).toBeInTheDocument();
    expect(screen.getByText("second block")).toBeInTheDocument();
  });

  it("renders a reserved actions tray for assistant messages", () => {
    const onRetryMessage = vi.fn();
    const { container } = render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "response" }])}
        onRetryMessage={onRetryMessage}
      />,
    );

    expect(
      container.querySelector('[data-role="assistant-message"] .pb-8'),
    ).toBeInTheDocument();
    expect(
      container.querySelector(
        '[data-role="assistant-message"] [data-role="message-actions"]',
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });

  it("keeps the action tray timestamp on one line", () => {
    const { container } = render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "response" }])}
      />,
    );

    const timestamp = container.querySelector(
      '[data-role="assistant-message"] [data-role="message-timestamp"]',
    );
    expect(timestamp).toHaveClass("whitespace-nowrap");
    expect(timestamp).toHaveClass("shrink-0");
  });

  it("anchors assistant and user actions on opposite sides of the timestamp", () => {
    const { container } = render(
      <>
        <MessageBubble
          message={assistantMessage([{ type: "text", text: "response" }])}
          onRetryMessage={vi.fn()}
        />
        <MessageBubble message={userMessage("draft")} onEditMessage={vi.fn()} />
      </>,
    );

    const assistantActions = container.querySelector(
      '[data-role="assistant-message"] [data-role="message-actions"]',
    );
    const userActions = container.querySelector(
      '[data-role="user-message"] [data-role="message-actions"]',
    );

    expect(
      Array.from(assistantActions?.firstElementChild?.children ?? []).map(
        (element) => element.tagName,
      ),
    ).toEqual(["BUTTON", "BUTTON", "SPAN"]);
    expect(
      Array.from(userActions?.firstElementChild?.children ?? []).map(
        (element) => element.tagName,
      ),
    ).toEqual(["SPAN", "BUTTON", "BUTTON"]);
  });

  it("keeps copy confirmation visible until it resets", async () => {
    vi.useFakeTimers();
    const { container } = render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "response" }])}
      />,
    );

    const actions = container.querySelector(
      '[data-role="assistant-message"] [data-role="message-actions"]',
    );
    expect(actions).toHaveAttribute("data-copy-confirmed", "false");
    const copyButton = screen.getByRole("button", { name: /copy/i });
    expect(copyButton).not.toHaveClass("bg-accent");

    await act(async () => {
      fireEvent.click(copyButton);
      await Promise.resolve();
    });

    expect(mockWriteText).toHaveBeenCalledWith("response");
    expect(actions).toHaveAttribute("data-copy-confirmed", "true");
    expect(copyButton).toHaveClass("bg-accent");

    await act(async () => {
      vi.advanceTimersByTime(1999);
    });
    expect(actions).toHaveAttribute("data-copy-confirmed", "true");
    expect(copyButton).toHaveClass("bg-accent");

    await act(async () => {
      vi.advanceTimersByTime(1);
    });
    expect(actions).toHaveAttribute("data-copy-confirmed", "false");
    expect(copyButton).not.toHaveClass("bg-accent");
  });

  it("renders tool request content as ToolCallCard", () => {
    const msg = assistantMessage([
      {
        type: "toolRequest",
        id: "tr-1",
        name: "readFile",
        arguments: { path: "/tmp" },
        status: "completed",
      },
    ]);
    render(<MessageBubble message={msg} />);
    expect(screen.getByText("readFile")).toBeInTheDocument();
  });

  it("renders metadata attachments and opens them on click", async () => {
    const user = userEvent.setup();

    render(
      <MessageBubble
        message={userMessage("See attached", {
          metadata: {
            attachments: [
              {
                type: "file",
                name: "report.pdf",
                path: "/Users/test/report.pdf",
              },
              {
                type: "directory",
                name: "screenshots",
                path: "/Users/test/screenshots",
              },
            ],
          },
        })}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /open attachment report\.pdf/i }),
    );
    expect(vi.mocked(openPath)).toHaveBeenCalledWith("/Users/test/report.pdf");
    expect(
      screen.getByRole("button", { name: /open attachment screenshots/i }),
    ).toBeInTheDocument();
  });

  it("renders standalone tool responses without dropping surrounding text", () => {
    const msg = assistantMessage([
      { type: "text", text: "Working on it." },
      {
        type: "toolResponse",
        id: "tool-result-1",
        name: "readFile",
        result: "file contents here",
        isError: false,
      },
      { type: "text", text: "Done." },
    ]);

    render(<MessageBubble message={msg} />);

    expect(screen.getByText("Working on it.")).toBeInTheDocument();
    expect(screen.getByText("readFile")).toBeInTheDocument();
    expect(screen.getByText("Done.")).toBeInTheDocument();
  });

  it("merges matched tool requests and responses into one tool card", () => {
    const msg = assistantMessage([
      { type: "text", text: "Checking that now." },
      {
        type: "toolRequest",
        id: "tool-1",
        name: "readFile",
        arguments: { path: "/tmp/demo.txt" },
        status: "executing",
      },
      {
        type: "toolResponse",
        id: "tool-1",
        name: "readFile",
        result: "done",
        isError: false,
      },
    ]);

    render(<MessageBubble message={msg} />);

    expect(screen.getByText("Checking that now.")).toBeInTheDocument();
    expect(screen.getAllByText("readFile")).toHaveLength(1);
  });

  it("renders tool cards inline between surrounding assistant text blocks", () => {
    const msg = assistantMessage([
      { type: "text", text: "Lemme check..." },
      {
        type: "toolRequest",
        id: "tool-1",
        name: "readFile",
        arguments: {},
        status: "executing",
      },
      {
        type: "toolResponse",
        id: "tool-1",
        name: "readFile",
        result: "done",
        isError: false,
      },
      { type: "text", text: "Results from checking." },
    ]);

    const { container } = render(<MessageBubble message={msg} />);
    const bubbleText = container.querySelector(
      '[data-role="assistant-message"]',
    )?.textContent;

    expect(bubbleText).toContain("Lemme check...");
    expect(bubbleText).toContain("readFile");
    expect(bubbleText).toContain("Results from checking.");
    expect(bubbleText?.indexOf("Lemme check...")).toBeLessThan(
      bubbleText?.indexOf("readFile") ?? Number.POSITIVE_INFINITY,
    );
    expect(bubbleText?.indexOf("readFile")).toBeLessThan(
      bubbleText?.indexOf("Results from checking.") ?? Number.POSITIVE_INFINITY,
    );
  });

  it("does not render a duplicate blank tool card for fallback responses", () => {
    const msg = assistantMessage([
      { type: "text", text: "Lemme check..." },
      {
        type: "toolRequest",
        id: "tool-1",
        name: "readFile",
        arguments: {},
        status: "executing",
      },
      {
        type: "toolResponse",
        id: "tool-response-1",
        name: "",
        result: "done",
        isError: false,
      },
      { type: "text", text: "Results from checking." },
    ]);

    render(<MessageBubble message={msg} />);

    expect(screen.getAllByText("readFile")).toHaveLength(1);
    expect(screen.queryByText("Tool result")).not.toBeInTheDocument();
  });

  it("renders thinking content as Reasoning block", () => {
    const msg = assistantMessage([{ type: "thinking", text: "deep thoughts" }]);
    render(<MessageBubble message={msg} />);
    expect(screen.getByText(/thought for/i)).toBeInTheDocument();
  });

  it("prefers the message persona name over the provider identity", () => {
    render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "hi" }], {
          metadata: { personaName: "Builder", providerId: "codex-acp" },
        })}
      />,
    );

    expect(screen.getByText("Builder")).toBeInTheDocument();
    expect(
      screen.queryByText(
        (text, el) => el?.tagName === "SPAN" && text === "Codex",
      ),
    ).not.toBeInTheDocument();
  });

  it("does not render an assistant name when message identity metadata is missing", () => {
    render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "hi" }])}
      />,
    );

    const nameSpans = screen.queryAllByText((_text, el) => {
      if (el?.tagName !== "SPAN") return false;
      return el.classList.contains("font-normal");
    });
    expect(nameSpans).toHaveLength(0);
  });

  it("uses the message provider identity for the assistant label and icon", () => {
    render(
      <MessageBubble
        message={assistantMessage([{ type: "text", text: "hi" }], {
          metadata: { providerId: "claude-acp" },
        })}
      />,
    );

    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByTitle("Claude")).toBeInTheDocument();
  });

  it("renders identity for an in-progress assistant message with a provider", () => {
    render(
      <MessageBubble
        message={assistantMessage([], {
          metadata: { completionStatus: "inProgress", providerId: "codex-acp" },
        })}
        isStreaming
      />,
    );

    expect(
      screen.getByText(
        (text, el) => el?.tagName === "SPAN" && text === "Codex",
      ),
    ).toBeInTheDocument();
  });

  it("collapses low-signal internal tool steps behind a toggle", async () => {
    const user = userEvent.setup();
    const msg = assistantMessage([
      {
        type: "toolRequest",
        id: "tool-1",
        name: "Create PDF about whales",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolRequest",
        id: "tool-2",
        name: "Write whales.pdf",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolRequest",
        id: "tool-3",
        name: "python3 create_whales.py",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolRequest",
        id: "tool-4",
        name: "ls -lh whales.pdf",
        arguments: {},
        status: "completed",
      },
    ]);

    render(<MessageBubble message={msg} />);

    expect(screen.getByText("Create PDF about whales")).toBeInTheDocument();
    expect(screen.getByText("Write whales.pdf")).toBeInTheDocument();
    expect(
      screen.queryByText("python3 create_whales.py"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("ls -lh whales.pdf")).not.toBeInTheDocument();
    expect(screen.getByText("Show internal steps (2)")).toBeInTheDocument();

    await user.click(screen.getByText("Show internal steps (2)"));

    expect(screen.getByText("python3 create_whales.py")).toBeInTheDocument();
    expect(screen.getByText("ls -lh whales.pdf")).toBeInTheDocument();
  });
});
