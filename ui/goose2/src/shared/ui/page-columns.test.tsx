import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PageColumns } from "./page-columns";

let mockIsMobile = false;

vi.mock("@/hooks/use-mobile", () => ({
  useIsMobile: () => mockIsMobile,
}));

describe("PageColumns", () => {
  beforeEach(() => {
    mockIsMobile = false;
  });

  it("renders a resizable desktop split layout", () => {
    render(
      <PageColumns sidebar={<div>Metadata</div>}>
        <div>Instructions</div>
      </PageColumns>,
    );

    expect(screen.getByText("Metadata")).toBeInTheDocument();
    expect(screen.getByText("Instructions")).toBeInTheDocument();
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("falls back to a stacked layout on mobile", () => {
    mockIsMobile = true;

    render(
      <PageColumns sidebar={<div>Metadata</div>}>
        <div>Instructions</div>
      </PageColumns>,
    );

    expect(screen.getByText("Metadata")).toBeInTheDocument();
    expect(screen.getByText("Instructions")).toBeInTheDocument();
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });
});
