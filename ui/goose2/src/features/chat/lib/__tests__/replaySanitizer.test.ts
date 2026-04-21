import { describe, expect, it } from "vitest";
import type { Message } from "@/shared/types/messages";
import { sanitizeReplayMessages } from "../replaySanitizer";

function createTextMessage(
  id: string,
  role: Message["role"],
  text: string,
): Message {
  return {
    id,
    role,
    created: 0,
    content: [{ type: "text", text }],
    metadata: {
      userVisible: true,
      agentVisible: role !== "system",
    },
  };
}

describe("sanitizeReplayMessages", () => {
  it("removes manual compaction control messages from replayed history", () => {
    expect(
      sanitizeReplayMessages([
        createTextMessage("user-1", "user", "Before compact"),
        createTextMessage("compact-1", "user", "/compact"),
        createTextMessage("compact-2", "user", "/compact/compact"),
        createTextMessage("compact-4", "user", "/summarize"),
        createTextMessage("assistant-1", "assistant", "After compact"),
      ]),
    ).toEqual([
      createTextMessage("user-1", "user", "Before compact"),
      createTextMessage("assistant-1", "assistant", "After compact"),
    ]);
  });

  it("keeps natural-language requests to compact the conversation", () => {
    expect(
      sanitizeReplayMessages([
        createTextMessage("user-1", "user", "Please compact this conversation"),
      ]),
    ).toEqual([
      createTextMessage("user-1", "user", "Please compact this conversation"),
    ]);
  });

  it("keeps normal user messages that merely mention compact commands", () => {
    expect(
      sanitizeReplayMessages([
        createTextMessage(
          "user-1",
          "user",
          "Can you explain what /compact does?",
        ),
      ]),
    ).toEqual([
      createTextMessage(
        "user-1",
        "user",
        "Can you explain what /compact does?",
      ),
    ]);
  });
});
