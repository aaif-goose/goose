import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SessionActivityIndicator } from "./SessionActivityIndicator";

describe("SessionActivityIndicator", () => {
  it("renders an inline spinner for running sessions", () => {
    render(<SessionActivityIndicator isRunning />);

    expect(screen.getByLabelText(/chat active/i)).toBeInTheDocument();
  });

  it("renders an inline dot for unread sessions", () => {
    render(<SessionActivityIndicator hasUnread />);

    expect(screen.getByLabelText(/unread messages/i)).toBeInTheDocument();
  });

  it("renders an overlay spinner variant for running sessions", () => {
    render(<SessionActivityIndicator isRunning variant="overlay" />);

    expect(screen.getByLabelText(/chat active/i)).toBeInTheDocument();
  });

  it("replaces the running spinner with unread state when activity ends", () => {
    const { rerender } = render(
      <SessionActivityIndicator isRunning hasUnread />,
    );

    expect(screen.getByLabelText(/chat active/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/unread messages/i)).not.toBeInTheDocument();

    rerender(<SessionActivityIndicator hasUnread />);

    expect(screen.queryByLabelText(/chat active/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText(/unread messages/i)).toBeInTheDocument();
  });

  it("renders nothing when the session is idle and read", () => {
    const { container } = render(<SessionActivityIndicator />);

    expect(container).toBeEmptyDOMElement();
  });
});
