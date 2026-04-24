import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SessionCard } from "../SessionCard";

describe("SessionCard", () => {
  const defaultProps = {
    id: "s1",
    title: "Fix sidebar bug",
    updatedAt: new Date().toISOString(),
    onSelect: vi.fn(),
  };

  it("renders title", () => {
    render(<SessionCard {...defaultProps} />);

    expect(screen.getByText("Fix sidebar bug")).toBeInTheDocument();
  });

  it("renders persona name when provided", () => {
    render(<SessionCard {...defaultProps} personaName="Code Assistant" />);

    expect(screen.getByText("Code Assistant")).toBeInTheDocument();
  });

  it("renders project name with color dot when provided", () => {
    render(
      <SessionCard
        {...defaultProps}
        projectName="My Project"
        projectColor="#3b82f6"
      />,
    );

    expect(screen.getByText("My Project")).toBeInTheDocument();
  });

  it("calls onSelect when clicked", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();

    render(<SessionCard {...defaultProps} onSelect={onSelect} />);

    await user.click(screen.getByLabelText("Open Fix sidebar bug"));

    expect(onSelect).toHaveBeenCalledWith("s1");
  });

  it("shows rename and archive in menu for active sessions", async () => {
    const user = userEvent.setup();

    render(
      <SessionCard {...defaultProps} onRename={vi.fn()} onArchive={vi.fn()} />,
    );

    await user.click(
      screen.getByRole("button", { name: /options for fix sidebar bug/i }),
    );

    expect(
      screen.getByRole("menuitem", { name: /rename/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: /archive/i }),
    ).toBeInTheDocument();
  });

  it("shows restore option for archived sessions", async () => {
    const user = userEvent.setup();

    render(
      <SessionCard
        {...defaultProps}
        archivedAt="2026-04-01T00:00:00Z"
        onUnarchive={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /options for fix sidebar bug/i }),
    );

    expect(
      screen.getByRole("menuitem", { name: /restore/i }),
    ).toBeInTheDocument();
  });
});
