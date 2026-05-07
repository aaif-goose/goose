import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Sidebar } from "../Sidebar";

const mockSessions: Array<{
  id: string;
  title: string;
  updatedAt: string;
  messageCount: number;
  projectId?: string;
  archivedAt?: string;
}> = [];

vi.mock("@/features/chat/stores/chatStore", () => ({
  useChatStore: (selector: (state: unknown) => unknown) =>
    selector({
      messagesBySession: {},
      sessionStateById: {},
    }),
}));

vi.mock("@/features/chat/stores/chatSessionStore", () => ({
  getVisibleSessions: (sessions: typeof mockSessions) =>
    sessions.filter((session) => session.messageCount > 0),
  useChatSessionStore: (selector: (state: unknown) => unknown) =>
    selector({
      sessions: mockSessions,
    }),
}));

vi.mock("@/features/agents/stores/agentStore", () => ({
  useAgentStore: (selector: (state: unknown) => unknown) =>
    selector({
      getPersonaById: () => undefined,
    }),
}));

vi.mock("@/features/projects/stores/projectStore", () => ({
  useProjectStore: (selector: (state: unknown) => unknown) =>
    selector({
      projects: [],
    }),
}));

describe("Sidebar", () => {
  it("shows sessions in recents when their project is not loaded", () => {
    mockSessions.splice(0, mockSessions.length, {
      id: "session-1",
      title: "Recovered Session",
      updatedAt: "2026-04-09T12:00:00.000Z",
      messageCount: 3,
      projectId: "missing-project",
    });

    render(
      <Sidebar
        collapsed={false}
        onCollapse={vi.fn()}
        onNavigate={vi.fn()}
        onSelectSession={vi.fn()}
        projects={[]}
      />,
    );

    expect(screen.getByText("Recovered Session")).toBeInTheDocument();

    mockSessions.splice(0, mockSessions.length);
  });

  it("hides zero-message sessions from recents", () => {
    mockSessions.splice(
      0,
      mockSessions.length,
      {
        id: "home-session",
        title: "New Chat",
        updatedAt: "2026-04-09T12:00:00.000Z",
        messageCount: 0,
      },
      {
        id: "session-1",
        title: "Recovered Session",
        updatedAt: "2026-04-09T12:01:00.000Z",
        messageCount: 3,
      },
    );

    render(
      <Sidebar
        collapsed={false}
        onCollapse={vi.fn()}
        onNavigate={vi.fn()}
        onSelectSession={vi.fn()}
        projects={[]}
      />,
    );

    expect(screen.queryByText("New Chat")).not.toBeInTheDocument();
    expect(screen.getByText("Recovered Session")).toBeInTheDocument();

    mockSessions.splice(0, mockSessions.length);
  });

  it("renders a home button in the sidebar header and navigates home", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();

    render(
      <Sidebar
        collapsed={false}
        onCollapse={vi.fn()}
        onNavigate={onNavigate}
        projects={[]}
      />,
    );

    await user.click(screen.getByRole("button", { name: /home/i }));

    expect(onNavigate).toHaveBeenCalledWith("home");
  });

  it("keeps the home button visible when the sidebar is collapsed", () => {
    render(
      <Sidebar
        collapsed
        onCollapse={vi.fn()}
        onNavigate={vi.fn()}
        projects={[]}
      />,
    );

    expect(screen.getByRole("button", { name: /home/i })).toBeInTheDocument();
  });
});
