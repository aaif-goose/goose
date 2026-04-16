import { describe, expect, it } from "vitest";
import { findExistingDraft } from "./newChat";
import type { ChatSession } from "../stores/chatSessionStore";

function makeDraft(overrides: Partial<ChatSession> = {}): ChatSession {
  return {
    id: "session-1",
    title: "New Chat",
    createdAt: "2026-03-31T10:00:00.000Z",
    updatedAt: "2026-03-31T10:00:00.000Z",
    messageCount: 0,
    draft: true,
    ...overrides,
  };
}

describe("findExistingDraft", () => {
  it("reuses the active empty draft session", () => {
    const activeDraft = makeDraft({ id: "active-draft" });

    const result = findExistingDraft({
      sessions: [activeDraft],
      activeSessionId: activeDraft.id,
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result?.id).toBe(activeDraft.id);
  });

  it("does not reuse non-draft sessions", () => {
    const realSession = makeDraft({ id: "real", draft: undefined });

    const result = findExistingDraft({
      sessions: [realSession],
      activeSessionId: realSession.id,
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result).toBeUndefined();
  });

  it("does not reuse drafts that already have messages", () => {
    const result = findExistingDraft({
      sessions: [makeDraft({ id: "used-draft", messageCount: 1 })],
      activeSessionId: "used-draft",
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result).toBeUndefined();
  });

  it("does not reuse drafts with local in-memory messages", () => {
    const result = findExistingDraft({
      sessions: [makeDraft({ id: "streaming-draft" })],
      activeSessionId: "streaming-draft",
      draftsBySession: {},
      messagesBySession: {
        "streaming-draft": [
          {
            id: "msg-1",
            role: "user",
            created: Date.now(),
            content: [{ type: "text", text: "hello" }],
          },
        ],
      },
      request: { title: "New Chat" },
    });

    expect(result).toBeUndefined();
  });

  it("only reuses drafts for the same chat context", () => {
    const projectDraft = makeDraft({
      id: "project-draft",
      projectId: "project-1",
    });

    const result = findExistingDraft({
      sessions: [projectDraft],
      activeSessionId: projectDraft.id,
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "New Chat", projectId: "project-2" },
    });

    expect(result).toBeUndefined();
  });

  it("does not reuse drafts when creating a titled chat", () => {
    const result = findExistingDraft({
      sessions: [makeDraft()],
      activeSessionId: "session-1",
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "What day is it?" },
    });

    expect(result).toBeUndefined();
  });

  it("finds a non-active draft with content", () => {
    const draftWithContent = makeDraft({ id: "background-draft" });

    const result = findExistingDraft({
      sessions: [draftWithContent],
      activeSessionId: null,
      draftsBySession: { "background-draft": "some typed text" },
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result?.id).toBe("background-draft");
  });

  it("prefers active draft with content over non-active", () => {
    const activeDraft = makeDraft({ id: "active-draft" });
    const backgroundDraft = makeDraft({ id: "background-draft" });

    const result = findExistingDraft({
      sessions: [backgroundDraft, activeDraft],
      activeSessionId: "active-draft",
      draftsBySession: {
        "active-draft": "active text",
        "background-draft": "background text",
      },
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result?.id).toBe("active-draft");
  });

  it("finds an inactive empty draft when no draft has content", () => {
    const inactiveDraft = makeDraft({
      id: "inactive-draft",
      updatedAt: "2026-03-31T12:00:00.000Z",
    });

    const result = findExistingDraft({
      sessions: [inactiveDraft],
      activeSessionId: null,
      draftsBySession: {},
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result?.id).toBe("inactive-draft");
  });

  it("prefers drafts with content over empty drafts", () => {
    const emptyDraft = makeDraft({ id: "empty-draft" });
    const contentDraft = makeDraft({ id: "content-draft" });

    const result = findExistingDraft({
      sessions: [emptyDraft, contentDraft],
      activeSessionId: "empty-draft",
      draftsBySession: { "content-draft": "has text" },
      messagesBySession: {},
      request: { title: "New Chat" },
    });

    expect(result?.id).toBe("content-draft");
  });
});
