import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AcpSessionInfo } from "@/shared/api/acp";
import { useChatSessionStore, type ChatSession } from "../chatSessionStore";

const mockAcpCreateSession = vi.fn();
const mockAcpListSessions = vi.fn();

vi.mock("@/shared/api/acp", () => ({
  acpCreateSession: (...args: unknown[]) => mockAcpCreateSession(...args),
  acpListSessions: (...args: unknown[]) => mockAcpListSessions(...args),
}));

const LEGACY_SESSION_CACHE_KEY = "goose:chat-sessions";
const OVERLAY_CACHE_KEY = "goose:acp-session-metadata";

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
    acpSessionId: "session-1",
    title: "Test Session",
    createdAt: "2026-04-01T00:00:00.000Z",
    updatedAt: "2026-04-01T00:00:00.000Z",
    messageCount: 0,
    ...overrides,
  };
}

function seedSession(overrides: Partial<ChatSession> = {}): ChatSession {
  const session = makeSession(overrides);
  useChatSessionStore.getState().addSession(session);
  return session;
}

describe("chatSessionStore", () => {
  beforeEach(() => {
    resetStore();
    window.localStorage.removeItem(LEGACY_SESSION_CACHE_KEY);
    window.localStorage.removeItem(OVERLAY_CACHE_KEY);
    vi.clearAllMocks();
  });

  afterEach(() => {
    window.localStorage.removeItem(LEGACY_SESSION_CACHE_KEY);
    window.localStorage.removeItem(OVERLAY_CACHE_KEY);
  });

  describe("createSession", () => {
    it("creates a real ACP-backed session", async () => {
      mockAcpCreateSession.mockResolvedValue({ sessionId: "acp-1" });

      const session = await useChatSessionStore.getState().createSession({
        title: "New Chat",
        providerId: "openai",
        personaId: "persona-1",
        modelId: "gpt-4.1",
        modelName: "GPT-4.1",
        workingDir: "/tmp/project",
      });

      expect(mockAcpCreateSession).toHaveBeenCalledWith(
        "openai",
        "/tmp/project",
        {
          personaId: "persona-1",
          modelId: "gpt-4.1",
        },
      );
      expect(session).toMatchObject({
        id: "acp-1",
        acpSessionId: "acp-1",
        title: "New Chat",
        providerId: "openai",
        personaId: "persona-1",
        modelId: "gpt-4.1",
        modelName: "GPT-4.1",
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
          messageCount: 4,
        },
        {
          sessionId: "acp-2",
          title: null,
          updatedAt: "2026-04-02",
          messageCount: 7,
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
    });

    it("rehydrates cached project metadata for ACP sessions", async () => {
      window.localStorage.setItem(
        LEGACY_SESSION_CACHE_KEY,
        JSON.stringify([
          {
            id: "acp-1",
            title: "Renamed Project Chat",
            projectId: "project-123",
            providerId: "openai",
            personaId: "persona-1",
            createdAt: "2026-03-31",
            updatedAt: "2026-04-01",
            messageCount: 4,
            userSetName: true,
          },
        ]),
      );

      mockAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: null,
          updatedAt: "2026-04-02",
          messageCount: 7,
          projectId: "project-123",
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const session = useChatSessionStore.getState().sessions[0];
      expect(session.title).toBe("Renamed Project Chat");
      expect(session.projectId).toBe("project-123");
      expect(session.providerId).toBe("openai");
      expect(session.personaId).toBe("persona-1");
      expect(session.createdAt).toBe("2026-03-31");
      expect(session.updatedAt).toBe("2026-04-02");
      expect(session.messageCount).toBe(7);
      expect(session.userSetName).toBe(true);
    });

    it("ignores legacy draft records while hydrating overlays", async () => {
      window.localStorage.setItem(
        LEGACY_SESSION_CACHE_KEY,
        JSON.stringify([
          {
            id: "cached-draft",
            title: "Cached Draft",
            draft: true,
            createdAt: "2026-04-01",
            updatedAt: "2026-04-01",
            messageCount: 0,
          },
        ]),
      );

      mockAcpListSessions.mockResolvedValue([]);

      await useChatSessionStore.getState().loadSessions();

      expect(useChatSessionStore.getState().sessions).toEqual([]);
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
          messageCount: 1,
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

    it("falls back to cached sessions on error", async () => {
      window.localStorage.setItem(
        LEGACY_SESSION_CACHE_KEY,
        JSON.stringify([
          {
            id: "cached-session",
            title: "Cached Session",
            projectId: "project-123",
            createdAt: "2026-04-01T00:00:00.000Z",
            updatedAt: "2026-04-01T00:00:00.000Z",
            messageCount: 8,
          },
          {
            id: "cached-draft",
            title: "Cached Draft",
            draft: true,
            createdAt: "2026-04-01T00:00:00.000Z",
            updatedAt: "2026-04-01T00:00:00.000Z",
            messageCount: 0,
          },
        ]),
      );

      mockAcpListSessions.mockRejectedValue(new Error("Network error"));

      await useChatSessionStore.getState().loadSessions();

      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions).toHaveLength(1);
      expect(sessions[0]).toMatchObject({
        id: "cached-session",
        projectId: "project-123",
      });
      expect(useChatSessionStore.getState().hasHydratedSessions).toBe(true);
    });
  });

  describe("updateSession", () => {
    it("updates session properties", () => {
      const session = seedSession();

      useChatSessionStore.getState().updateSession(session.id, {
        title: "Updated Title",
        projectId: "new-project",
      });

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated?.title).toBe("Updated Title");
      expect(updated?.projectId).toBe("new-project");
    });

    it("preserves updatedAt when not explicitly provided in patch", () => {
      const session = seedSession();
      const originalUpdatedAt = session.updatedAt;

      vi.useFakeTimers();
      vi.advanceTimersByTime(1000);

      useChatSessionStore.getState().updateSession(session.id, {
        title: "New Title",
      });

      vi.useRealTimers();

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated?.updatedAt).toBe(originalUpdatedAt);
    });

    it("updates updatedAt when explicitly provided in patch", () => {
      const session = seedSession();
      const originalUpdatedAt = session.updatedAt;

      vi.useFakeTimers();
      vi.advanceTimersByTime(1000);

      const newTimestamp = new Date().toISOString();
      useChatSessionStore.getState().updateSession(session.id, {
        title: "New Title",
        updatedAt: newTimestamp,
      });

      vi.useRealTimers();

      const updated = useChatSessionStore.getState().getSession(session.id);
      expect(updated?.updatedAt).not.toBe(originalUpdatedAt);
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

  describe("addSession", () => {
    it("prepends a new session to the list", () => {
      const { addSession } = useChatSessionStore.getState();
      addSession(
        makeSession({
          id: "imported-1",
          title: "Imported Session",
          messageCount: 5,
        }),
      );

      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions[0].id).toBe("imported-1");
      expect(sessions[0].title).toBe("Imported Session");
      expect(sessions[0].messageCount).toBe(5);
    });

    it("does not create a duplicate if session ID already exists", () => {
      const { addSession } = useChatSessionStore.getState();
      addSession(makeSession({ id: "dup-1", title: "First", messageCount: 1 }));
      addSession(
        makeSession({ id: "dup-1", title: "Second", messageCount: 2 }),
      );

      const sessions = useChatSessionStore.getState().sessions;
      const matches = sessions.filter((session) => session.id === "dup-1");
      expect(matches).toHaveLength(1);
      expect(matches[0].title).toBe("Second");
    });
  });
});
