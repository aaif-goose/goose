import { describe, expect, it } from "vitest";
import type { Message } from "@/shared/types/messages";
import { findExistingDraft } from "./newChat";
import type { ChatSession } from "../stores/chatSessionStore";

function makeSession(
  id: string,
  overrides: Partial<ChatSession> = {},
): ChatSession {
  return {
    id,
    title: "New chat",
    createdAt: "2026-04-01T00:00:00.000Z",
    updatedAt: "2026-04-01T00:00:00.000Z",
    messageCount: 0,
    ...overrides,
  };
}

describe("findExistingDraft", () => {
  it("reuses a matching project draft with content", () => {
    const draft = makeSession("alpha-draft", {
      projectId: "alpha",
      providerId: "goose",
    });

    expect(
      findExistingDraft({
        sessions: [draft],
        activeSessionId: null,
        draftsBySession: { "alpha-draft": "alpha draft" },
        messagesBySession: {},
        request: {
          title: "New chat",
          projectId: "alpha",
        },
      }),
    ).toEqual(draft);
  });

  it("does not reuse a draft from a different project", () => {
    const draft = makeSession("alpha-draft", {
      projectId: "alpha",
      providerId: "goose",
    });

    expect(
      findExistingDraft({
        sessions: [draft],
        activeSessionId: null,
        draftsBySession: { "alpha-draft": "alpha draft" },
        messagesBySession: {},
        request: {
          title: "New Chat",
          projectId: "beta",
        },
      }),
    ).toBeUndefined();
  });

  it("does not reuse an abandoned empty draft", () => {
    const draft = makeSession("alpha-draft", {
      projectId: "alpha",
      providerId: "goose",
    });

    expect(
      findExistingDraft({
        sessions: [draft],
        activeSessionId: null,
        draftsBySession: {},
        messagesBySession: {},
        request: {
          title: "New Chat",
          projectId: "alpha",
        },
      }),
    ).toBeUndefined();
  });

  it("does not reuse the active empty draft without content", () => {
    const draft = makeSession("alpha-draft", {
      projectId: "alpha",
      providerId: "goose",
    });

    expect(
      findExistingDraft({
        sessions: [draft],
        activeSessionId: "alpha-draft",
        draftsBySession: {},
        messagesBySession: {},
        request: {
          title: "New Chat",
          projectId: "alpha",
        },
      }),
    ).toBeUndefined();
  });

  it("does not reuse a session with local messages even if messageCount is 0", () => {
    const session = makeSession("alpha-session", {
      projectId: "alpha",
      providerId: "goose",
      messageCount: 0,
    });

    expect(
      findExistingDraft({
        sessions: [session],
        activeSessionId: "alpha-session",
        draftsBySession: {},
        messagesBySession: {
          "alpha-session": [
            {
              id: "msg-1",
              role: "user",
              created: 1,
              content: [{ type: "text", text: "hello" }],
            } satisfies Message,
          ],
        },
        request: {
          title: "New Chat",
          projectId: "alpha",
        },
      }),
    ).toBeUndefined();
  });
});
