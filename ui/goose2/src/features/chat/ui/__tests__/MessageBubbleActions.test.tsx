import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { MessageBubble } from "../MessageBubble";
import { useAgentStore } from "@/features/agents/stores/agentStore";
import type { Message } from "@/shared/types/messages";

const mockWriteText = vi.fn().mockResolvedValue(undefined);

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

describe("MessageBubble actions", () => {
  beforeEach(() => {
    useAgentStore.setState({ personas: [] });
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

  it("renders a reserved actions tray for pure assistant text messages", () => {
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

  it("hides the assistant actions tray when tool content is present", () => {
    const { container } = render(
      <MessageBubble
        message={assistantMessage([
          { type: "text", text: "Checking that now." },
          {
            type: "toolRequest",
            id: "tool-1",
            name: "readFile",
            arguments: { path: "/tmp/demo.txt" },
            status: "completed",
          },
          {
            type: "toolResponse",
            id: "tool-1",
            name: "readFile",
            result: "done",
            isError: false,
          },
        ])}
        onRetryMessage={vi.fn()}
      />,
    );

    expect(
      container.querySelector('[data-role="assistant-message"] .pb-8'),
    ).not.toBeInTheDocument();
    expect(
      container.querySelector(
        '[data-role="assistant-message"] [data-role="message-actions"]',
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /copy/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /retry/i }),
    ).not.toBeInTheDocument();
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
});
