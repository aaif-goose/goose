import { CANNED_REPLY } from "./data";
import type { Chat, Message } from "./types";

function truncate(s: string, n: number) {
  return s.length <= n ? s : s.slice(0, n - 1) + "…";
}

export { truncate };

export function generateFakeConversation(chat: Chat): Message[] {
  if (chat.id === "c1") {
    return [
      {
        role: "user",
        paragraphs: [
          "I'm seeing three WebSockets open on cold start — my code calls getClient() from three different modules in parallel. What's the cleanest fix?",
        ],
      },
      {
        role: "assistant",
        model: "Claude Opus 4.7",
        paragraphs: [
          "Classic init race. The fix is to cache the Promise, not the resolved client — so parallel callers all await the same in-flight connection.",
        ],
        tool: { name: "read_file", summary: "acpConnection.ts" },
      },
      {
        role: "assistant",
        model: "Claude Opus 4.7",
        paragraphs: ["Here's the pattern:"],
        bullets: [
          "Module-level variable: `let clientPromise: Promise<Client> | null = null`",
          "getClient() returns clientPromise ??= connect()",
          "On connection error, null the Promise so the next caller retries",
          "Use .closed to detect drops without active pings",
        ],
        followup: "Want me to draft the patch against your acpConnection.ts?",
      },
      { role: "user", paragraphs: ["Yes, and also explain why null-on-error instead of keeping the rejection."] },
    ];
  }
  if (chat.id === "c2") {
    return [
      {
        role: "user",
        paragraphs: [
          "Reviewing the PRD — the four-region layout feels right, but I'm worried the composer footer is doing too much. Six controls in one ribbon?",
        ],
      },
      {
        role: "assistant",
        model: "Claude Sonnet 4.6",
        paragraphs: ["Fair concern. Let me count what's load-bearing vs. decorative."],
        bullets: [
          "Context folder — load-bearing, changes model behavior",
          "Attach — load-bearing but used rarely, could move into a `/` command",
          "Token counter — load-bearing signal, but hover-only popover",
          "Model selector — load-bearing, users switch often",
          "MCP — load-bearing, same",
          "Bug reporter — could live in settings or status bar",
        ],
        followup: "If you pull Attach into slash commands and Bug into the status bar, you're down to 4. Still dense but scannable. Want mockups?",
      },
    ];
  }
  return [
    { role: "user", paragraphs: ["Let's pick this back up."] },
    {
      role: "assistant",
      model: "Claude Sonnet 4.6",
      paragraphs: [
        `Picking up the thread on "${chat.title}". Where were we — do you want me to summarize the last turn, or jump straight to next steps?`,
      ],
    },
  ];
}

export function makeCannedAssistantReply(modelName: string): Message {
  return {
    role: "assistant",
    model: modelName,
    paragraphs: CANNED_REPLY.paragraphs,
    bullets: CANNED_REPLY.bullets,
    followup: CANNED_REPLY.followup,
    tool: { name: "memory_search", summary: "Found 3 relevant notes" },
  };
}
