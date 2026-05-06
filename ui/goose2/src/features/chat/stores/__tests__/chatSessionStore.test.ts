import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AcpSessionInfo } from "@/shared/api/acp";
import { useChatSessionStore, type ChatSession } from "../chatSessionStore";

const mockAcpCreateSession = vi.fn();
const mockAcpListSessions = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpCreateSession: (...args: unknown[]) => mockAcpCreateSession(...args),
  acpListSessions: (...args: unknown[]) => mockAcpListSessions(...args),
}));

vi.mock("@/shared/api/acpApi", () => ({
  archiveSession: vi.fn().mockResolvedValue(undefined),
  unarchiveSession: vi.fn().mockResolvedValue(undefined),
  renameSession: vi.fn().mockResolvedValue(undefined),
  updateSessionProject: vi.fn().mockResolvedValue(undefined),
}));

function resetStore() {
  useChatSessionStore.setState({
    sessions: [],
    activeSessionId: null,
    isLoading: false,
    hasHydratedSessions: false,
    contextPanelOpenBySession: {},
    activeWorkspaceBySession: {},
  });
}

function makeSession(overrides: Partial<ChatSession> = {}): ChatSession {
  return {
    id: "session-1",
    title: "Test Session",
    createdAt: "2026-04-01T00:00:00.000Z",
    updatedAt: "2026-04-01T00:00:00.000Z",
    messageCount: 0,
    ...overrides,
  };
}

function seedSession(overrides: Partial<ChatSession> = {}): ChatSession {
  const session = makeSession(overrides);
  useChatSessionStore.setState((state) => ({
    sessions: [session, ...state.sessions],
  }));
  return session;
}

describe("chatSessionStore", () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe("createSession", () => {
    it("creates a real ACP-backed session", async () => {
      mockAcpCreateSession.mockResolvedValue({ sessionId: "acp-1" });

      const session = await useChatSessionStore.getState().createSession({
        title: "New Chat",
        providerId: "openai",
        projectId: "project-1",
        personaId: "persona-1",
        modelId: "gpt-4.1",
        modelName: "GPT-4.1",
        workingDir: "/tmp/project",
      });

      expect(mockAcpCreateSession).toHaveBeenCalledWith(
        "openai",
        "/tmp/project",
        {
          projectId: "project-1",
          personaId: "persona-1",
          modelId: "gpt-4.1",
        },
      );
      expect(session).toMatchObject({
        id: "acp-1",
        title: "New Chat",
        projectId: "project-1",
        providerId: "openai",
        personaId: "persona-1",
        modelId: "gpt-4.1",
        modelName: "GPT-4.1",
        workingDir: "/tmp/project",
      });
      expect(useChatSessionStore.getState().sessions).toContainEqual(session);
    });
  });

  describe("loadSessions", () => {
    it("loads sessions from ACP and maps them correctly", async () => {
      mockAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: "ACP Session 1",
          updatedAt: "2026-04-01",
          createdAt: "2026-03-31",
          archivedAt: null,
          userSetName: false,
          messageCount: 4,
          workingDir: "/tmp/acp-1",
          providerId: "openai",
          modelId: "gpt-4.1",
        },
        {
          sessionId: "acp-2",
          title: null,
          updatedAt: "2026-04-02",
          createdAt: "2026-04-02",
          archivedAt: null,
          userSetName: false,
          messageCount: 7,
          providerId: null,
          modelId: null,
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions).toHaveLength(2);
      expect(sessions[0].id).toBe("acp-2");
      expect(sessions[0].title).toBe("Untitled");
      expect(sessions[0].messageCount).toBe(7);
      expect(sessions[1].id).toBe("acp-1");
      expect(sessions[1].title).toBe("ACP Session 1");
      expect(sessions[1].messageCount).toBe(4);
      expect(sessions[1].providerId).toBe("openai");
      expect(sessions[1].modelId).toBe("gpt-4.1");
      expect(sessions[1].workingDir).toBe("/tmp/acp-1");
    });

    it("reads all metadata fields from backend response", async () => {
      mockAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: "Renamed Chat",
          updatedAt: "2026-04-02",
          createdAt: "2026-03-31",
          archivedAt: null,
          userSetName: true,
          messageCount: 7,
          workingDir: "/tmp/project-123",
          projectId: "project-123",
          providerId: "anthropic",
          personaId: "persona-1",
          modelId: "claude-sonnet-4",
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const session = useChatSessionStore.getState().sessions[0];
      expect(session.title).toBe("Renamed Chat");
      expect(session.projectId).toBe("project-123");
      expect(session.providerId).toBe("anthropic");
      expect(session.personaId).toBe("persona-1");
      expect(session.createdAt).toBe("2026-03-31");
      expect(session.updatedAt).toBe("2026-04-02");
      expect(session.messageCount).toBe(7);
      expect(session.userSetName).toBe(true);
      expect(session.modelId).toBe("claude-sonnet-4");
      expect(session.workingDir).toBe("/tmp/project-123");
    });

    it("drops stale sessions that are no longer in ACP", async () => {
      useChatSessionStore.setState({
        sessions: [
          makeSession({ id: "stale-session", title: "Stale Session" }),
        ],
        activeSessionId: "stale-session",
      });

      mockAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: "ACP Session",
          updatedAt: "2026-04-02",
          createdAt: "2026-04-02",
          archivedAt: null,
          userSetName: false,
          messageCount: 1,
          providerId: null,
          modelId: null,
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const state = useChatSessionStore.getState();
      expect(state.sessions).toHaveLength(1);
      expect(state.sessions[0].id).toBe("acp-1");
      expect(state.activeSessionId).toBeNull();
    });

    it("sets isLoading during fetch", async () => {
      let resolvePromise: (value: AcpSessionInfo[]) => void = () => {};
      mockAcpListSessions.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );

      const loadPromise = useChatSessionStore.getState().loadSessions();
      expect(useChatSessionStore.getState().isLoading).toBe(true);
      expect(useChatSessionStore.getState().hasHydratedSessions).toBe(false);

      resolvePromise([]);
      await loadPromise;

      expect(useChatSessionStore.getState().isLoading).toBe(false);
      expect(useChatSessionStore.getState().hasHydratedSessions).toBe(true);
    });

    it("keeps empty sessions list on error", async () => {
      mockAcpListSessions.mockRejectedValue(new Error("Network error"));

      await useChatSessionStore.getState().loadSessions();

      expect(useChatSessionStore.getState().sessions).toEqual([]);
      expect(useChatSessionStore.getState().hasHydratedSessions).toBe(true);
    });
  });

  describe("patchSession", () => {
    it("patches session properties while preserving updatedAt when omitted", () => {
      const session = seedSession();
      const originalUpdatedAt = session.updatedAt;

      useChatSessionStore.getState().patchSession(session.id, {
        title: "Updated Title",
        projectId: "new-project",
      });

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated).toMatchObject({
        title: "Updated Title",
        projectId: "new-project",
        updatedAt: originalUpdatedAt,
      });
    });

    it("updates updatedAt when explicitly provided in patch", () => {
      const session = seedSession();
      const newTimestamp = "2026-04-01T00:01:00.000Z";
      useChatSessionStore.getState().patchSession(session.id, {
        updatedAt: newTimestamp,
      });

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated?.updatedAt).toBe(newTimestamp);
    });
  });

  describe("provider switching", () => {
    it("clears the selected model when switching providers", () => {
      const session = seedSession({
        providerId: "openai",
        modelId: "gpt-4o",
        modelName: "GPT-4o",
      });

      useChatSessionStore
        .getState()
        .switchSessionProvider(session.id, "anthropic");

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated?.providerId).toBe("anthropic");
      expect(updated?.modelId).toBeUndefined();
      expect(updated?.modelName).toBeUndefined();
    });
  });

  describe("archiveSession", () => {
    it("sets archivedAt on the session", async () => {
      const session = seedSession();

      await useChatSessionStore.getState().archiveSession(session.id);

      const archived = useChatSessionStore.getState().getSession(session.id);
      expect(archived?.archivedAt).toBeDefined();
    });

    it("clears activeSessionId if archiving the active session", async () => {
      const session = seedSession();
      useChatSessionStore.getState().setActiveSession(session.id);

      await useChatSessionStore.getState().archiveSession(session.id);

      expect(useChatSessionStore.getState().activeSessionId).toBeNull();
    });
  });
});
