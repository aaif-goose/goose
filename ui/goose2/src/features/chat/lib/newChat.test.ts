import { describe, expect, it } from "vitest";
import { findExistingDraft } from "./newChat";
import type { ChatSession } from "../stores/chatSessionStore";

function makeSession(
  id: string,
  overrides: Partial<ChatSession> = {},
): ChatSession {
  return {
    id,
    title: "New Chat",
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
          title: "New Chat",
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
});
