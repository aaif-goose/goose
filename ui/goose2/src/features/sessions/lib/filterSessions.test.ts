import { describe, expect, it } from "vitest";
import { filterSessions } from "./filterSessions";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";

const resolvers = {
  getPersonaName: (id: string) => (id === "p1" ? "Code Assistant" : undefined),
  getProjectName: (id: string) =>
    id === "proj1" ? "Goose2 Frontend" : undefined,
};

function makeSession(
  overrides: Partial<ChatSession> & { id: string },
): ChatSession {
  return {
    title: "Untitled",
    createdAt: "2026-04-07T12:00:00Z",
    updatedAt: "2026-04-07T12:00:00Z",
    messageCount: 3,
    ...overrides,
  };
}

describe("filterSessions", () => {
  const sessions: ChatSession[] = [
    makeSession({ id: "1", title: "Fix sidebar bug", personaId: "p1" }),
    makeSession({
      id: "2",
      title: "Add pagination",
      projectId: "proj1",
    }),
    makeSession({ id: "3", title: "Debug auth flow" }),
  ];

  it("returns all sessions for empty query", () => {
    expect(filterSessions(sessions, "", resolvers)).toEqual(sessions);
  });

  it("filters by title", () => {
    const result = filterSessions(sessions, "sidebar", resolvers);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("1");
  });

  it("filters by persona name", () => {
    const result = filterSessions(sessions, "code assistant", resolvers);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("1");
  });

  it("filters by project name", () => {
    const result = filterSessions(sessions, "frontend", resolvers);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("2");
  });

  it("is case-insensitive", () => {
    const result = filterSessions(sessions, "DEBUG", resolvers);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("3");
  });

  it("matches across multiple fields", () => {
    const result = filterSessions(sessions, "fix", resolvers);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("1");
  });
});
