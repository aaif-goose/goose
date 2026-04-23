import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MessageBubble } from "../MessageBubble";
import type { Message } from "@/shared/types/messages";

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

describe("MessageBubble tool chains", () => {
  it("groups multi-step ACP tool chains behind a parent card", async () => {
    const user = userEvent.setup();
    const msg = assistantMessage([
      {
        type: "toolRequest",
        id: "tool-1",
        chainId: "chain-1",
        chainSummary: "working",
        name: "python3 create_whales.py",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-1",
        chainId: "chain-1",
        chainSummary: "working",
        name: "python3 create_whales.py",
        result: "generated draft",
        isError: false,
      },
      {
        type: "toolRequest",
        id: "tool-2",
        chainId: "chain-1",
        chainSummary: "working",
        name: "Create PDF about whales",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-2",
        chainId: "chain-1",
        chainSummary: "working",
        name: "Create PDF about whales",
        result: "saved whales.pdf",
        isError: false,
      },
      {
        type: "toolRequest",
        id: "tool-3",
        chainId: "chain-1",
        chainSummary: "working",
        name: "ls -lh whales.pdf",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-3",
        chainId: "chain-1",
        chainSummary: "working",
        name: "ls -lh whales.pdf",
        result: "12K whales.pdf",
        isError: false,
      },
      {
        type: "toolRequest",
        id: "tool-4",
        chainId: "chain-1",
        chainSummary: "reviewing files",
        name: "Write whales.pdf",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-4",
        chainId: "chain-1",
        chainSummary: "reviewing files",
        name: "Write whales.pdf",
        result: "done",
        isError: false,
      },
    ]);

    render(<MessageBubble message={msg} />);

    expect(screen.getByText("reviewing files (4 steps)")).toBeInTheDocument();
    expect(
      screen.queryByText("python3 create_whales.py"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Write whales.pdf")).not.toBeInTheDocument();

    await user.click(screen.getByText("reviewing files (4 steps)"));

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

  it("wraps a single ACP tool call in a parent chain and collapses it when complete", async () => {
    const user = userEvent.setup();
    const runningMessage = assistantMessage([
      {
        type: "toolRequest",
        id: "tool-1",
        chainId: "chain-1",
        chainSummary: "working",
        name: "Read main.js",
        arguments: {},
        status: "executing",
      },
    ]);

    const { rerender } = render(<MessageBubble message={runningMessage} />);

    expect(screen.getByText("working through 1 step")).toBeInTheDocument();
    expect(screen.getByText("Read main.js")).toBeInTheDocument();

    const completedMessage = assistantMessage([
      {
        type: "toolRequest",
        id: "tool-1",
        chainId: "chain-1",
        chainSummary: "reviewing files",
        name: "Read main.js",
        arguments: {},
        status: "completed",
      },
      {
        type: "toolResponse",
        id: "tool-1",
        chainId: "chain-1",
        chainSummary: "reviewing files",
        name: "Read main.js",
        result: "done",
        isError: false,
      },
    ]);

    rerender(<MessageBubble message={completedMessage} />);

    expect(screen.getByText("reviewing files (1 step)")).toBeInTheDocument();
    expect(screen.queryByText("Read main.js")).not.toBeInTheDocument();

    await user.click(screen.getByText("reviewing files (1 step)"));

    expect(screen.getByText("Read main.js")).toBeInTheDocument();
  });
});
