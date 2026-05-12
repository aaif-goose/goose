import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MessageBubble } from "../MessageBubble";
import type { Message } from "@/shared/types/messages";

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

describe("MessageBubble skill chips", () => {
  it("renders user message chips from metadata", () => {
    render(
      <MessageBubble
        message={userMessage("redo the settings modal", {
          metadata: {
            chips: [{ label: "capture-task", type: "skill" }],
          },
        })}
      />,
    );

    expect(screen.getByText("capture-task")).toBeInTheDocument();
    expect(screen.getByText("redo the settings modal")).toBeInTheDocument();
    expect(screen.queryByText(/Use the capture-task skill/i)).toBeNull();
  });
});
