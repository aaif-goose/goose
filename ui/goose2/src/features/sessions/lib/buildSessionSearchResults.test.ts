import { describe, expect, it } from "vitest";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";
import type { AcpSessionSearchResult } from "@/shared/api/acp";
import { buildSessionSearchResults } from "./buildSessionSearchResults";

const resolvers = {
  getPersonaName: (id: string) => (id === "persona-1" ? "Builder" : undefined),
  getProjectName: (id: string) => (id === "project-1" ? "Goose2" : undefined),
};

function makeSession(
  overrides: Partial<ChatSession> & { id: string; updatedAt: string },
): ChatSession {
  return {
    title: "Untitled",
    createdAt: "2026-04-10T12:00:00Z",
    messageCount: 1,
    ...overrides,
  };
}

describe("buildSessionSearchResults", () => {
  it("merges metadata and message matches, preferring message details", () => {
    const sessions = [
      makeSession({
        id: "acp-1",
        title: "Needle session",
        updatedAt: "2026-04-10T12:00:00Z",
      }),
      makeSession({
        id: "session-2",
        title: "Builder notes",
        personaId: "persona-1",
        updatedAt: "2026-04-09T12:00:00Z",
      }),
    ];
    const messageMatches: AcpSessionSearchResult[] = [
      {
        sessionId: "acp-1",
        snippet: "a matching snippet",
        messageId: "message-1",
        messageRole: "assistant",
        matchCount: 2,
      },
    ];

    const results = buildSessionSearchResults(
      sessions,
      "needle",
      messageMatches,
      resolvers,
    );

    expect(results).toHaveLength(1);
    expect(results[0]).toMatchObject({
      matchType: "message",
      snippet: "a matching snippet",
      messageId: "message-1",
      matchCount: 2,
    });
    expect(results[0].session.id).toBe("acp-1");
  });

  it("includes metadata-only matches and sorts by updatedAt descending", () => {
    const sessions = [
      makeSession({
        id: "session-1",
        title: "Draft architecture",
        updatedAt: "2026-04-08T12:00:00Z",
      }),
      makeSession({
        id: "session-2",
        projectId: "project-1",
        title: "Unrelated title",
        updatedAt: "2026-04-10T12:00:00Z",
      }),
    ];

    const results = buildSessionSearchResults(
      sessions,
      "goose2",
      [],
      resolvers,
    );

    expect(results).toHaveLength(1);
    expect(results[0].matchType).toBe("metadata");
    expect(results[0].session.id).toBe("session-2");
  });

  it("includes message-only matches even when metadata does not match", () => {
    const sessions = [
      makeSession({
        id: "session-1",
        title: "Unrelated title",
        updatedAt: "2026-04-10T12:00:00Z",
      }),
    ];
    const messageMatches: AcpSessionSearchResult[] = [
      {
        sessionId: "session-1",
        snippet: "found in body",
        messageId: "message-1",
        messageRole: "user",
        matchCount: 1,
      },
    ];

    const results = buildSessionSearchResults(
      sessions,
      "body text",
      messageMatches,
      resolvers,
    );

    expect(results).toHaveLength(1);
    expect(results[0]).toMatchObject({
      matchType: "message",
      snippet: "found in body",
    });
  });
});
