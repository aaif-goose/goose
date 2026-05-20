import { describe, expect, it } from "vitest";
import type { ChatSession } from "../stores/chatSessionStore";
import { resolveInheritedProjectWorkspace } from "./workspaceContext";

function makeSession(overrides: Partial<ChatSession> = {}): ChatSession {
  return {
    id: "session-1",
    title: "New chat",
    projectId: "project-1",
    workingDir: "/repo/main",
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
    messageCount: 0,
    ...overrides,
  };
}

describe("resolveInheritedProjectWorkspace", () => {
  it("inherits the active workspace for another chat in the same project", () => {
    expect(
      resolveInheritedProjectWorkspace({
        projectId: "project-1",
        sessions: [makeSession()],
        activeSessionId: "session-1",
        activeWorkspaceBySession: {
          "session-1": { path: "/repo/feature", branch: "feature" },
        },
      }),
    ).toEqual({ path: "/repo/feature", branch: "feature" });
  });

  it("falls back to the active session working directory after reload", () => {
    expect(
      resolveInheritedProjectWorkspace({
        projectId: "project-1",
        sessions: [makeSession({ workingDir: "/repo/feature" })],
        activeSessionId: "session-1",
        activeWorkspaceBySession: {},
      }),
    ).toEqual({ path: "/repo/feature", branch: null });
  });

  it("does not inherit workspace across projects", () => {
    expect(
      resolveInheritedProjectWorkspace({
        projectId: "project-2",
        sessions: [makeSession()],
        activeSessionId: "session-1",
        activeWorkspaceBySession: {
          "session-1": { path: "/repo/feature", branch: "feature" },
        },
      }),
    ).toBeUndefined();
  });
});
