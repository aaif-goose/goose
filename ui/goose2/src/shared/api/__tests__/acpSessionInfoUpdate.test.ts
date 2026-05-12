import { beforeEach, describe, expect, it } from "vitest";
import { useChatStore } from "@/features/chat/stores/chatStore";
import { useChatSessionStore } from "@/features/chat/stores/chatSessionStore";
import { handleSessionNotification } from "../acpNotificationHandler";

describe("ACP session info updates", () => {
  beforeEach(() => {
    useChatStore.setState({
      messagesBySession: {},
      sessionStateById: {},
      queuedMessageBySession: {},
      draftsBySession: {},
      activeSessionId: null,
      isConnected: false,
      loadingSessionIds: new Set<string>(),
      scrollTargetMessageBySession: {},
    });
    useChatSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      isLoading: false,
      hasHydratedSessions: false,
      contextPanelOpenBySession: {},
      activeWorkspaceBySession: {},
    });
  });

  it("applies generated session info updates to non-user-named sessions", async () => {
    useChatSessionStore.getState().addSession({
      id: "goose-session-title",
      title: "New Chat",
      createdAt: "2026-01-01T00:00:00.000Z",
      updatedAt: "2026-01-01T00:00:00.000Z",
      messageCount: 0,
      userSetName: false,
    });

    await handleSessionNotification({
      sessionId: "goose-session-title",
      update: {
        sessionUpdate: "session_info_update",
        title: "Generated Test Title",
        updatedAt: "2026-01-01T00:01:00.000Z",
        _meta: {
          messageCount: 1,
          userSetName: false,
        },
      },
    } as never);

    expect(
      useChatSessionStore.getState().getSession("goose-session-title"),
    ).toMatchObject({
      title: "Generated Test Title",
      updatedAt: "2026-01-01T00:01:00.000Z",
      messageCount: 1,
      userSetName: false,
    });
  });

  it("ignores generated titles for user-named sessions", async () => {
    useChatSessionStore.getState().addSession({
      id: "goose-session-user-title",
      title: "My Custom Title",
      createdAt: "2026-01-01T00:00:00.000Z",
      updatedAt: "2026-01-01T00:00:00.000Z",
      messageCount: 0,
      userSetName: true,
    });

    await handleSessionNotification({
      sessionId: "goose-session-user-title",
      update: {
        sessionUpdate: "session_info_update",
        title: "Generated Test Title",
        updatedAt: "2026-01-01T00:01:00.000Z",
        _meta: {
          messageCount: 1,
          userSetName: true,
        },
      },
    } as never);

    expect(
      useChatSessionStore.getState().getSession("goose-session-user-title"),
    ).toMatchObject({
      title: "My Custom Title",
      updatedAt: "2026-01-01T00:01:00.000Z",
      messageCount: 1,
      userSetName: true,
    });
  });
});
