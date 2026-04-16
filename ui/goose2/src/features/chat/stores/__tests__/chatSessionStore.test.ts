import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AcpSessionInfo } from "@/shared/api/acp";
import { useChatSessionStore } from "../chatSessionStore";

vi.mock("@/shared/api/acp", () => ({
  acpListSessions: vi.fn(),
}));

import { acpListSessions } from "@/shared/api/acp";

const mockedAcpListSessions = vi.mocked(acpListSessions);

const LEGACY_SESSION_CACHE_KEY = "goose:chat-sessions";
const OVERLAY_CACHE_KEY = "goose:acp-session-metadata";
const DRAFT_SESSION_CACHE_KEY = "goose:chat-draft-sessions";

function resetStore() {
  useChatSessionStore.setState({
    sessions: [],
    activeSessionId: null,
    isLoading: false,
    contextPanelOpenBySession: {},
    activeWorkingContextBySession: {},
    modelsBySession: {},
    modelCacheByProvider: {},
  });
}

describe("chatSessionStore", () => {
  beforeEach(() => {
    resetStore();
    window.localStorage.removeItem(LEGACY_SESSION_CACHE_KEY);
    window.localStorage.removeItem(OVERLAY_CACHE_KEY);
    window.localStorage.removeItem(DRAFT_SESSION_CACHE_KEY);
    vi.clearAllMocks();
  });

  afterEach(() => {
    window.localStorage.removeItem(LEGACY_SESSION_CACHE_KEY);
    window.localStorage.removeItem(OVERLAY_CACHE_KEY);
    window.localStorage.removeItem(DRAFT_SESSION_CACHE_KEY);
  });

  describe("createDraftSession", () => {
    it("creates a draft session with default title", () => {
      const session = useChatSessionStore.getState().createDraftSession();

      expect(session.title).toBe("New Chat");
      expect(session.draft).toBe(true);
      expect(session.messageCount).toBe(0);
      expect(useChatSessionStore.getState().sessions).toContainEqual(session);
    });

    it("creates a draft session with custom options", () => {
      const session = useChatSessionStore.getState().createDraftSession({
        title: "My Custom Chat",
        projectId: "proj-1",
        providerId: "openai",
        personaId: "persona-1",
      });

      expect(session.title).toBe("My Custom Chat");
      expect(session.projectId).toBe("proj-1");
      expect(session.providerId).toBe("openai");
      expect(session.personaId).toBe("persona-1");
      expect(session.draft).toBe(true);
    });
  });

  describe("promoteDraft", () => {
    it("removes draft flag from a draft session", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      expect(session.draft).toBe(true);

      useChatSessionStore.getState().promoteDraft(session.id);

      const updated = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === session.id);
      expect(updated?.draft).toBeUndefined();
    });

    it("does nothing for non-draft sessions", () => {
      useChatSessionStore.setState({
        sessions: [
          {
            id: "non-draft",
            title: "Regular Session",
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            messageCount: 5,
          },
        ],
      });

      useChatSessionStore.getState().promoteDraft("non-draft");

      const session = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === "non-draft");
      expect(session?.draft).toBeUndefined();
    });
  });

  describe("removeDraft", () => {
    it("removes a draft session", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      expect(useChatSessionStore.getState().sessions).toHaveLength(1);

      useChatSessionStore.getState().removeDraft(session.id);

      expect(useChatSessionStore.getState().sessions).toHaveLength(0);
    });

    it("clears activeSessionId if removing the active draft", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      useChatSessionStore.getState().setActiveSession(session.id);

      useChatSessionStore.getState().removeDraft(session.id);

      expect(useChatSessionStore.getState().activeSessionId).toBeNull();
    });

    it("does not remove non-draft sessions", () => {
      useChatSessionStore.setState({
        sessions: [
          {
            id: "non-draft",
            title: "Regular Session",
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            messageCount: 5,
          },
        ],
      });

      useChatSessionStore.getState().removeDraft("non-draft");

      expect(useChatSessionStore.getState().sessions).toHaveLength(1);
    });
  });

  describe("loadSessions", () => {
    it("loads sessions from ACP and maps them correctly", async () => {
      mockedAcpListSessions.mockResolvedValue([
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
      expect(sessions[0].id).toBe("acp-2"); // Most recent first
      expect(sessions[0].title).toBe("Untitled"); // null title becomes "Untitled"
      expect(sessions[0].messageCount).toBe(7);
      expect(sessions[1].id).toBe("acp-1");
      expect(sessions[1].title).toBe("ACP Session 1");
      expect(sessions[1].messageCount).toBe(4);
    });

    it("preserves local drafts alongside ACP sessions", async () => {
      const draft = useChatSessionStore.getState().createDraftSession({
        title: "My Draft",
      });

      mockedAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: "ACP Session",
          updatedAt: "2026-04-01",
          messageCount: 3,
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions).toHaveLength(2);
      expect(sessions.find((s) => s.id === "acp-1")).toBeDefined();
      expect(sessions.find((s) => s.id === draft.id)).toBeDefined();
    });

    it("migrates promoted draft metadata onto the resolved ACP session id", async () => {
      const draft = useChatSessionStore.getState().createDraftSession({
        title: "Project Draft",
        projectId: "project-123",
        providerId: "goose",
      });

      useChatSessionStore.getState().promoteDraft(draft.id);
      useChatSessionStore.getState().setSessionAcpId(draft.id, "acp-1");

      mockedAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: "ACP Session",
          updatedAt: "2026-04-02",
          messageCount: 3,
        },
      ]);

      await useChatSessionStore.getState().loadSessions();

      const session = useChatSessionStore.getState().sessions[0];
      expect(session.id).toBe("acp-1");
      expect(session.acpSessionId).toBe("acp-1");
      expect(session.projectId).toBe("project-123");
      expect(session.providerId).toBe("goose");
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

      mockedAcpListSessions.mockResolvedValue([
        {
          sessionId: "acp-1",
          title: null,
          updatedAt: "2026-04-02",
          messageCount: 7,
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

    it("drops stale non-draft sessions that are no longer in ACP", async () => {
      useChatSessionStore.setState({
        sessions: [
          {
            id: "stale-session",
            title: "Stale Session",
            createdAt: "2026-04-01",
            updatedAt: "2026-04-01",
            messageCount: 2,
          },
        ],
        activeSessionId: "stale-session",
      });

      mockedAcpListSessions.mockResolvedValue([
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
      mockedAcpListSessions.mockReturnValue(
        new Promise((resolve) => {
          resolvePromise = resolve;
        }),
      );

      const loadPromise = useChatSessionStore.getState().loadSessions();
      expect(useChatSessionStore.getState().isLoading).toBe(true);

      resolvePromise([]);
      await loadPromise;

      expect(useChatSessionStore.getState().isLoading).toBe(false);
    });

    it("falls back to cached sessions on error", async () => {
      window.localStorage.setItem(
        LEGACY_SESSION_CACHE_KEY,
        JSON.stringify([
          {
            id: "cached-session",
            title: "Cached Session",
            projectId: "project-123",
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            messageCount: 8,
          },
          {
            id: "cached-draft",
            title: "Cached Draft",
            draft: true,
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            messageCount: 0,
          },
        ]),
      );

      mockedAcpListSessions.mockRejectedValue(new Error("Network error"));

      await useChatSessionStore.getState().loadSessions();

      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions).toHaveLength(2);
      expect(
        sessions.find((session) => session.id === "cached-session"),
      ).toMatchObject({
        projectId: "project-123",
      });
      expect(
        sessions.find((session) => session.id === "cached-draft")?.draft,
      ).toBe(true);
    });
  });

  describe("updateSession", () => {
    it("updates session properties", () => {
      const session = useChatSessionStore.getState().createDraftSession();

      useChatSessionStore.getState().updateSession(session.id, {
        title: "Updated Title",
        projectId: "new-project",
      });

      const updated = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === session.id);
      expect(updated?.title).toBe("Updated Title");
      expect(updated?.projectId).toBe("new-project");
    });

    it("preserves updatedAt when not explicitly provided in patch", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      const originalUpdatedAt = session.updatedAt;

      vi.useFakeTimers();
      vi.advanceTimersByTime(1000);

      useChatSessionStore.getState().updateSession(session.id, {
        title: "New Title",
      });

      vi.useRealTimers();

      const updated = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === session.id);
      expect(updated?.updatedAt).toBe(originalUpdatedAt);
    });

    it("updates updatedAt when explicitly provided in patch", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      const originalUpdatedAt = session.updatedAt;

      vi.useFakeTimers();
      vi.advanceTimersByTime(1000);

      const newTimestamp = new Date().toISOString();
      useChatSessionStore.getState().updateSession(session.id, {
        title: "New Title",
        updatedAt: newTimestamp,
      });

      vi.useRealTimers();

      const updated = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === session.id);
      expect(updated?.updatedAt).not.toBe(originalUpdatedAt);
      expect(updated?.updatedAt).toBe(newTimestamp);
    });
  });

  describe("session models", () => {
    it("stores models per session", () => {
      const session = useChatSessionStore.getState().createDraftSession();

      useChatSessionStore.getState().setSessionModels(session.id, [
        { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
        { id: "gpt-4o", name: "GPT-4o" },
      ]);

      expect(
        useChatSessionStore.getState().getSessionModels(session.id),
      ).toEqual([
        { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
        { id: "gpt-4o", name: "GPT-4o" },
      ]);
    });

    it("removes stored models when a draft session is removed", () => {
      const session = useChatSessionStore.getState().createDraftSession();
      useChatSessionStore
        .getState()
        .setSessionModels(session.id, [
          { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
        ]);

      useChatSessionStore.getState().removeDraft(session.id);

      expect(
        useChatSessionStore.getState().getSessionModels(session.id),
      ).toEqual([]);
    });

    it("removes stored models when a session is archived", async () => {
      const session = useChatSessionStore.getState().createDraftSession();
      useChatSessionStore
        .getState()
        .setSessionModels(session.id, [
          { id: "claude-sonnet-4", name: "Claude Sonnet 4" },
        ]);

      await useChatSessionStore.getState().archiveSession(session.id);

      expect(
        useChatSessionStore.getState().getSessionModels(session.id),
      ).toEqual([]);
    });
  });

  describe("archiveSession", () => {
    it("sets archivedAt on the session", async () => {
      const session = useChatSessionStore.getState().createDraftSession();

      await useChatSessionStore.getState().archiveSession(session.id);

      const archived = useChatSessionStore
        .getState()
        .sessions.find((s) => s.id === session.id);
      expect(archived?.archivedAt).toBeDefined();
    });

    it("clears activeSessionId if archiving the active session", async () => {
      const session = useChatSessionStore.getState().createDraftSession();
      useChatSessionStore.getState().setActiveSession(session.id);

      await useChatSessionStore.getState().archiveSession(session.id);

      expect(useChatSessionStore.getState().activeSessionId).toBeNull();
    });
  });

  describe("addSession", () => {
    it("prepends a new session to the list", () => {
      const { addSession } = useChatSessionStore.getState();
      addSession({
        id: "imported-1",
        title: "Imported Session",
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
        messageCount: 5,
      });
      const sessions = useChatSessionStore.getState().sessions;
      expect(sessions[0].id).toBe("imported-1");
      expect(sessions[0].title).toBe("Imported Session");
      expect(sessions[0].messageCount).toBe(5);
    });

    it("does not create a duplicate if session ID already exists", () => {
      const { addSession } = useChatSessionStore.getState();
      addSession({
        id: "dup-1",
        title: "First",
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
        messageCount: 1,
      });
      addSession({
        id: "dup-1",
        title: "Second",
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
        messageCount: 2,
      });
      const sessions = useChatSessionStore.getState().sessions;
      const matches = sessions.filter((s) => s.id === "dup-1");
      expect(matches).toHaveLength(1);
      expect(matches[0].title).toBe("Second");
    });
  });
});
