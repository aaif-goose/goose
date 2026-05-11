import { beforeEach, describe, expect, it, vi } from "vitest";
import { useChatSessionStore, type ChatSession } from "../chatSessionStore";
import {
  updateSessionProject,
  updateSessionTitle,
} from "../chatSessionOperations";

const mockRenameSession = vi.fn();
const mockUpdateSessionProject = vi.fn();

vi.mock("@/shared/api/acpApi", () => ({
  renameSession: (...args: unknown[]) => mockRenameSession(...args),
  updateSessionProject: (...args: unknown[]) =>
    mockUpdateSessionProject(...args),
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

function seedSession(overrides: Partial<ChatSession> = {}) {
  useChatSessionStore.setState({
    sessions: [
      {
        id: "session-1",
        title: "Original Title",
        createdAt: "2026-04-01T00:00:00.000Z",
        updatedAt: "2026-04-01T00:00:00.000Z",
        messageCount: 0,
        ...overrides,
      },
    ],
  });
}

describe("chatSessionOperations", () => {
  beforeEach(() => {
    resetStore();
    vi.clearAllMocks();
  });

  describe("updateSessionTitle", () => {
    it("renames in backend before patching local state", async () => {
      seedSession({ userSetName: false });
      mockRenameSession.mockResolvedValue(undefined);

      await updateSessionTitle("session-1", "Manual Title");

      expect(mockRenameSession).toHaveBeenCalledWith(
        "session-1",
        "Manual Title",
      );
      expect(
        useChatSessionStore.getState().getSession("session-1"),
      ).toMatchObject({
        title: "Manual Title",
        userSetName: true,
      });
    });

    it("does not patch local state when backend rename fails", async () => {
      seedSession({ userSetName: false });
      mockRenameSession.mockRejectedValue(new Error("rename failed"));

      await expect(
        updateSessionTitle("session-1", "Manual Title"),
      ).rejects.toThrow("rename failed");

      expect(
        useChatSessionStore.getState().getSession("session-1"),
      ).toMatchObject({
        title: "Original Title",
        userSetName: false,
      });
    });
  });

  describe("updateSessionProject", () => {
    it("updates project in backend before patching local state", async () => {
      seedSession({ projectId: "project-old" });
      mockUpdateSessionProject.mockResolvedValue(undefined);

      await updateSessionProject("session-1", "project-new");

      expect(mockUpdateSessionProject).toHaveBeenCalledWith(
        "session-1",
        "project-new",
      );
      expect(
        useChatSessionStore.getState().getSession("session-1")?.projectId,
      ).toBe("project-new");
    });

    it("does not patch local state when backend project update fails", async () => {
      seedSession({ projectId: "project-old" });
      mockUpdateSessionProject.mockRejectedValue(new Error("project failed"));

      await expect(
        updateSessionProject("session-1", "project-new"),
      ).rejects.toThrow("project failed");

      expect(
        useChatSessionStore.getState().getSession("session-1")?.projectId,
      ).toBe("project-old");
    });
  });
});
