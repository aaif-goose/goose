import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SessionActivityIndicator } from "./SessionActivityIndicator";

describe("SessionActivityIndicator", () => {
  it("renders an info-colored inline spinner for running sessions", () => {
    render(<SessionActivityIndicator isRunning />);

    expect(screen.getByLabelText(/chat active/i)).toHaveClass(
      "text-[var(--color-text-info)]",
    );
  });

  it("renders an info-colored inline dot for unread sessions", () => {
    render(<SessionActivityIndicator hasUnread />);

    expect(screen.getByLabelText(/unread messages/i)).toHaveClass(
      "bg-[var(--color-background-info)]",
    );
  });

  it("renders an overlay spinner variant for running sessions", () => {
    const { container } = render(
      <SessionActivityIndicator isRunning variant="overlay" />,
    );

    expect(screen.getByLabelText(/chat active/i)).toBeInTheDocument();
    expect(
      container.querySelector(".text-\\[var\\(--color-text-info\\)\\]"),
    ).toBeTruthy();
  });

  it("renders nothing when the session is idle and read", () => {
    const { container } = render(<SessionActivityIndicator />);

    expect(container).toBeEmptyDOMElement();
  });
});
