import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ChatLoadingSkeleton } from "../ChatLoadingSkeleton";

describe("ChatLoadingSkeleton", () => {
  it("renders with loading status role", () => {
    render(<ChatLoadingSkeleton />);

    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("has accessible label for screen readers", () => {
    render(<ChatLoadingSkeleton />);

    expect(
      screen.getByRole("status", { name: /loading conversation/i }),
    ).toBeInTheDocument();
  });

  it("renders skeleton elements", () => {
    const { container } = render(<ChatLoadingSkeleton />);

    const skeletons = container.querySelectorAll('[class*="animate-pulse"]');
    expect(skeletons.length).toBeGreaterThan(0);
  });
});
