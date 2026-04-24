import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { groupSessionsByDate } from "./groupSessionsByDate";
import type { ChatSession } from "@/features/chat/stores/chatSessionStore";

function makeSession(id: string, updatedAt: string): ChatSession {
  return {
    id,
    title: `Session ${id}`,
    createdAt: updatedAt,
    updatedAt,
    messageCount: 5,
  };
}

describe("groupSessionsByDate", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-04-07T12:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("groups sessions into Today, Yesterday, and dated buckets", () => {
    const sessions = [
      makeSession("a", "2026-04-07T10:00:00Z"),
      makeSession("b", "2026-04-07T08:00:00Z"),
      makeSession("c", "2026-04-06T15:00:00Z"),
      makeSession("d", "2026-03-28T12:00:00Z"),
    ];

    const groups = groupSessionsByDate(sessions);

    expect(groups).toHaveLength(3);
    expect(groups[0].label).toBe("Today");
    expect(groups[0].sessions).toHaveLength(2);
    expect(groups[0].sessions[0].id).toBe("a");
    expect(groups[1].label).toBe("Yesterday");
    expect(groups[1].sessions).toHaveLength(1);
    expect(groups[2].label).toBe("March 28, 2026");
    expect(groups[2].sessions).toHaveLength(1);
  });

  it("returns empty array for no sessions", () => {
    expect(groupSessionsByDate([])).toEqual([]);
  });

  it("sorts sessions within each group newest-first", () => {
    const sessions = [
      makeSession("early", "2026-04-07T06:00:00Z"),
      makeSession("late", "2026-04-07T11:00:00Z"),
      makeSession("mid", "2026-04-07T09:00:00Z"),
    ];

    const groups = groupSessionsByDate(sessions);
    const ids = groups[0].sessions.map((s) => s.id);
    expect(ids).toEqual(["late", "mid", "early"]);
  });
});
