import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_CHAT_TITLE } from "@/features/chat/lib/sessionTitle";
import { SidebarChatRow } from "../SidebarChatRow";

describe("SidebarChatRow", () => {
  it("starts inline rename on double-click and commits on Enter", async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();

    render(
      <SidebarChatRow
        id="session-1"
        title="Original Title"
        isActive={false}
        onRename={onRename}
      />,
    );

    await user.dblClick(screen.getByTitle("Double-click to rename"));

    const input = screen.getByRole("textbox");
    await user.clear(input);
    await user.type(input, "Renamed Chat{Enter}");

    expect(onRename).toHaveBeenCalledWith("session-1", "Renamed Chat");
  });

  it("opens rename from menu and cancels on Escape", async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();

    render(
      <SidebarChatRow
        id="session-1"
        title="Original Title"
        isActive={false}
        onRename={onRename}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /options for original title/i }),
    );
    await user.click(screen.getByRole("menuitem", { name: /rename/i }));

    const input = screen.getByRole("textbox");
    await user.clear(input);
    await user.type(input, "Should Not Save{Escape}");

    expect(onRename).not.toHaveBeenCalled();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("trims input and does not rename when empty or unchanged", async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();

    render(
      <SidebarChatRow
        id="session-1"
        title="Same Title"
        isActive={false}
        onRename={onRename}
      />,
    );

    await user.dblClick(screen.getByTitle("Double-click to rename"));
    const input = screen.getByRole("textbox");

    await user.clear(input);
    await user.type(input, "   {Enter}");

    expect(onRename).not.toHaveBeenCalled();

    await user.dblClick(screen.getByTitle("Double-click to rename"));
    const input2 = screen.getByRole("textbox");
    await user.clear(input2);
    await user.type(input2, "  Same Title  {Enter}");

    expect(onRename).not.toHaveBeenCalled();
  });

  it("shows a spinner when the chat is active", () => {
    render(
      <SidebarChatRow
        id="session-1"
        title="Busy Chat"
        isActive={false}
        isRunning
      />,
    );

    expect(screen.getByLabelText(/chat active/i)).toBeInTheDocument();
  });

  it("shows an unread dot when the chat has unread output", () => {
    render(
      <SidebarChatRow
        id="session-1"
        title="Unread Chat"
        isActive={false}
        hasUnread
      />,
    );

    expect(screen.getByLabelText(/unread messages/i)).toBeInTheDocument();
  });

  it("does not reserve activity space by default when idle", () => {
    const { container } = render(
      <SidebarChatRow id="session-1" title="Idle Chat" isActive={false} />,
    );

    expect(
      container.querySelector(".h-3.w-3.shrink-0.items-center.justify-center"),
    ).toBeNull();
  });

  it("reserves activity space only once activity exists", () => {
    const { container, rerender } = render(
      <SidebarChatRow id="session-1" title="Recent Chat" isActive={false} />,
    );

    expect(
      container.querySelector(".h-3.w-3.shrink-0.items-center.justify-center"),
    ).toBeNull();

    rerender(
      <SidebarChatRow
        id="session-1"
        title="Recent Chat"
        isActive={false}
        hasUnread
      />,
    );

    expect(screen.getByLabelText(/unread messages/i)).toBeInTheDocument();
  });

  it("keeps the localized default title in rename mode without persisting it", async () => {
    const user = userEvent.setup();
    const onRename = vi.fn();

    render(
      <SidebarChatRow
        id="session-1"
        title={DEFAULT_CHAT_TITLE}
        isActive={false}
        onRename={onRename}
      />,
    );

    await user.dblClick(screen.getByTitle("Double-click to rename"));

    const input = screen.getByRole("textbox");
    expect(input).toHaveValue("New chat");

    await user.tab();

    expect(onRename).not.toHaveBeenCalled();
  });
});
