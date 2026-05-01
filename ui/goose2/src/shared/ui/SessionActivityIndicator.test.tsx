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

  it("renders nothing when the session is idle and read", () => {
    const { container } = render(<SessionActivityIndicator />);

    expect(container).toBeEmptyDOMElement();
  });
});
